//! swf-wrapper — Makes wardrobe/vanity avatar SWFs renderable.
//!
//! Avatar component SWFs (eyes, hair, hats, etc.) define their art as
//! DefineSprite tags but never place them on the main timeline. This tool
//! creates a new SWF that places ALL sprites on stage so they can be rendered
//! by ruffle's exporter.
//!
//! Options:
//!   --info                   Print SWF header info (stage bounds etc.) and exit
//!   --match-stage <ref.swf>  Use ref SWF's stage bounds for the output

use std::env;
use std::fs;

fn main() {
    let args: Vec<String> = env::args().collect();

    // Parse flags and positional args
    let mut positional: Vec<String> = Vec::new();
    let mut match_stage_path: Option<String> = None;
    let mut info_mode = false;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--info" => {
                info_mode = true;
                i += 1;
            }
            "--match-stage" => {
                i += 1;
                if i < args.len() {
                    match_stage_path = Some(args[i].clone());
                } else {
                    eprintln!("--match-stage requires a path argument");
                    std::process::exit(1);
                }
                i += 1;
            }
            _ => {
                positional.push(args[i].clone());
                i += 1;
            }
        }
    }

    if positional.is_empty() {
        eprintln!(
            "Usage: swf-wrapper [--info] [--match-stage <ref.swf>] <input.swf> [<output.swf>]"
        );
        std::process::exit(1);
    }

    let input_path = &positional[0];

    let data = fs::read(input_path).expect("Cannot read input SWF");
    let buf = swf::decompress_swf(&data[..]).expect("Cannot decompress SWF");
    let parsed = swf::parse_swf(&buf).expect("Cannot parse SWF");

    // --info: print header info and exit
    if info_mode {
        let stage = parsed.header.stage_size();
        let w = (stage.x_max - stage.x_min).to_pixels();
        let h = (stage.y_max - stage.y_min).to_pixels();
        println!(
            r#"{{"version":{},"frame_rate":{},"num_frames":{},"stage_twips":{{"x_min":{},"x_max":{},"y_min":{},"y_max":{}}},"stage_px":{{"x_min":{:.1},"x_max":{:.1},"y_min":{:.1},"y_max":{:.1},"width":{:.1},"height":{:.1}}}}}"#,
            parsed.header.version(),
            parsed.header.frame_rate().to_f32(),
            parsed.header.num_frames(),
            stage.x_min.get(),
            stage.x_max.get(),
            stage.y_min.get(),
            stage.y_max.get(),
            stage.x_min.to_pixels(),
            stage.x_max.to_pixels(),
            stage.y_min.to_pixels(),
            stage.y_max.to_pixels(),
            w,
            h,
        );
        return;
    }

    if positional.len() < 2 {
        eprintln!(
            "Usage: swf-wrapper [--info] [--match-stage <ref.swf>] <input.swf> <output.swf>"
        );
        std::process::exit(1);
    }

    let output_path = &positional[1];

    // Determine stage size: use --match-stage reference or input's own stage
    let stage_size = if let Some(ref ref_path) = match_stage_path {
        let ref_data = fs::read(ref_path).expect("Cannot read reference SWF");
        let ref_buf =
            swf::decompress_swf(&ref_data[..]).expect("Cannot decompress reference SWF");
        let ref_parsed = swf::parse_swf(&ref_buf).expect("Cannot parse reference SWF");
        ref_parsed.header.stage_size().clone()
    } else {
        parsed.header.stage_size().clone()
    };

    // Collect ALL DefineSprite IDs and find max frame count
    let mut sprites: Vec<(u16, u16)> = Vec::new();
    let mut max_frames: u16 = 1;

    for tag in &parsed.tags {
        if let swf::Tag::DefineSprite(sprite) = tag {
            sprites.push((sprite.id, sprite.num_frames));
            if sprite.num_frames > max_frames {
                max_frames = sprite.num_frames;
            }
        }
    }

    if sprites.is_empty() {
        eprintln!("No DefineSprite found in SWF");
        std::process::exit(1);
    }

    // Parse raw tag bytes, filtering out background/showframe/end and all
    // main-timeline PlaceObject/RemoveObject tags to prevent double-placement.
    let tag_data = &buf.data;
    let mut raw_tags: Vec<u8> = Vec::new();
    let mut pos: usize = 0;

    while pos < tag_data.len() {
        if pos + 2 > tag_data.len() {
            break;
        }

        let tag_code_and_length = u16::from_le_bytes([tag_data[pos], tag_data[pos + 1]]);
        let tag_code = tag_code_and_length >> 6;
        let mut tag_length = (tag_code_and_length & 0x3F) as usize;
        let mut header_size = 2;

        if tag_length == 0x3F {
            if pos + 6 > tag_data.len() {
                break;
            }
            tag_length = u32::from_le_bytes([
                tag_data[pos + 2],
                tag_data[pos + 3],
                tag_data[pos + 4],
                tag_data[pos + 5],
            ]) as usize;
            header_size = 6;
        }

        let total_tag_size = header_size + tag_length;
        if pos + total_tag_size > tag_data.len() {
            break;
        }

        // Skip: End(0), ShowFrame(1), SetBackgroundColor(9)
        match tag_code {
            0 | 1 | 9 => {}
            _ => {
                raw_tags.extend_from_slice(&tag_data[pos..pos + total_tag_size]);
            }
        }

        pos += total_tag_size;
        if tag_code == 0 {
            break;
        }
    }

    // Place ALL sprites on stage at increasing depths
    for (i, (sprite_id, _)) in sprites.iter().enumerate() {
        let depth = (i + 1) as u16;
        // PlaceObject2 (tag code 26):
        //   flags=0x06 (HasCharacter | HasMatrix), depth, characterId, identity matrix
        let place_body = [
            0x06,
            (depth & 0xFF) as u8,
            ((depth >> 8) & 0xFF) as u8,
            (*sprite_id & 0xFF) as u8,
            ((*sprite_id >> 8) & 0xFF) as u8,
            0x00, // identity matrix (5-bit NTranslateBits=0, padded)
        ];
        let place_header = ((26u16 << 6) | place_body.len() as u16).to_le_bytes();
        raw_tags.extend_from_slice(&place_header);
        raw_tags.extend_from_slice(&place_body);
    }

    // ShowFrame * max_frames
    let show_frame_raw = ((1u16 << 6) | 0).to_le_bytes();
    for _ in 0..max_frames {
        raw_tags.extend_from_slice(&show_frame_raw);
    }

    // End tag
    raw_tags.extend_from_slice(&[0x00, 0x00]);

    // Write output SWF
    let header = swf::Header {
        compression: swf::Compression::Zlib,
        version: parsed.header.version(),
        stage_size,
        frame_rate: parsed.header.frame_rate(),
        num_frames: max_frames,
    };

    let output_file = fs::File::create(output_path).expect("Cannot create output file");
    swf::write::write_swf_raw_tags(&header, &raw_tags, output_file).expect("Cannot write SWF");

    // Output JSON for the pipeline to consume
    println!(
        r#"{{"sprites":{},"max_frames":{}}}"#,
        sprites.len(),
        max_frames
    );
}
