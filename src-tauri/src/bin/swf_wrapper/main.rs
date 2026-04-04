//! swf-wrapper — Makes wardrobe/vanity avatar SWFs renderable.
//!
//! Avatar component SWFs (eyes, hair, hats, etc.) define their art as
//! DefineSprite tags but never place them on the main timeline. This tool
//! creates a new SWF that places ALL sprites on stage so they can be rendered
//! by ruffle's exporter.

use std::env;
use std::fs;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: swf-wrapper <input.swf> <output.swf>");
        std::process::exit(1);
    }

    let input_path = &args[1];
    let output_path = &args[2];

    let data = fs::read(input_path).expect("Cannot read input SWF");
    let buf = swf::decompress_swf(&data[..]).expect("Cannot decompress SWF");
    let parsed = swf::parse_swf(&buf).expect("Cannot parse SWF");

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

    // Parse raw tag bytes, filtering out background/showframe/end
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
        stage_size: parsed.header.stage_size().clone(),
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
