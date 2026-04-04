//! extract-transforms — Extracts per-frame container positions from Avatar.swf.
//!
//! The Glitch avatar uses a nested MovieClip hierarchy with named containers
//! (e.g., sideHeadContainer_mc, sideEyeCloseContainer_mc) that move per-frame
//! to animate the body. This tool extracts those positions so the extraction
//! pipeline can render components in the correct coordinate space.
//!
//! Usage:
//!   extract-transforms <Avatar.swf> [--frames 0,3,6,...]
//!
//! Outputs JSON with per-frame (x, y) positions for each named container,
//! accumulated through the hierarchy so positions are absolute (stage-relative).

use std::collections::HashMap;
use std::env;
use std::fs;
use swf::PlaceObjectAction;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: extract-transforms <Avatar.swf> [--frames 0,3,6,...]");
        std::process::exit(1);
    }

    let input_path = &args[1];

    // Optional: only output specific frames
    let frame_filter: Option<Vec<u16>> = args
        .iter()
        .position(|a| a == "--frames")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.split(',').filter_map(|n| n.parse().ok()).collect());

    let data = fs::read(input_path).expect("Cannot read input SWF");
    let buf = swf::decompress_swf(&data[..]).expect("Cannot decompress SWF");
    let parsed = swf::parse_swf(&buf).expect("Cannot parse SWF");

    // Build character ID → DefineSprite map
    let mut sprites: HashMap<u16, &swf::Sprite> = HashMap::new();
    for tag in &parsed.tags {
        if let swf::Tag::DefineSprite(sprite) = tag {
            sprites.insert(sprite.id, sprite);
        }
    }

    // List all DefineSprites with significant frame counts
    let mut high_frame_sprites: Vec<_> = sprites
        .values()
        .filter(|s| s.num_frames > 100)
        .collect();
    high_frame_sprites.sort_by_key(|s| std::cmp::Reverse(s.num_frames));

    eprintln!("High-frame-count DefineSprites:");
    for s in &high_frame_sprites {
        // Check which other sprites reference this one
        let referenced_by: Vec<u16> = sprites
            .iter()
            .filter(|(_, parent)| {
                parent.tags.iter().any(|t| {
                    if let swf::Tag::PlaceObject(p) = t {
                        matches!(&p.action,
                            PlaceObjectAction::Place(id) | PlaceObjectAction::Replace(id) if *id == s.id
                        )
                    } else {
                        false
                    }
                })
            })
            .map(|(id, _)| *id)
            .collect();
        eprintln!(
            "  id={}, frames={}, referenced_by={:?}",
            s.id, s.num_frames, referenced_by
        );
    }

    // Find the avatarContainer_mc — the one with 1233 frames that is placed by
    // the main timeline (not by another DefineSprite with 1233 frames).
    // It's the top-level animated sprite.
    let main_timeline_char_ids: Vec<u16> = parsed
        .tags
        .iter()
        .filter_map(|t| {
            if let swf::Tag::PlaceObject(p) = t {
                match &p.action {
                    PlaceObjectAction::Place(id) | PlaceObjectAction::Replace(id) => Some(*id),
                    _ => None,
                }
            } else {
                None
            }
        })
        .collect();
    eprintln!(
        "Main timeline places characters: {:?}",
        main_timeline_char_ids
    );

    // Show the main timeline placement transforms (including the avatar container's stage offset)
    for tag in &parsed.tags {
        if let swf::Tag::PlaceObject(p) = tag {
            let name = p
                .name
                .as_ref()
                .map(|n| n.to_str_lossy(swf::UTF_8).into_owned())
                .unwrap_or_default();
            let (tx, ty) = p
                .matrix
                .as_ref()
                .map(|m| (m.tx.to_pixels(), m.ty.to_pixels()))
                .unwrap_or((0.0, 0.0));
            let char_id = match &p.action {
                PlaceObjectAction::Place(id) | PlaceObjectAction::Replace(id) => Some(*id),
                _ => None,
            };
            eprintln!(
                "  Main timeline PlaceObject: depth={}, char_id={:?}, name={:?}, pos=({:.1}, {:.1})",
                p.depth, char_id, name, tx, ty
            );
        }
    }

    // The avatar container is the high-frame sprite placed on the main timeline,
    // or if none, the one that contains body-part containers as children.
    // Try main timeline first.
    let avatar_sprite = main_timeline_char_ids
        .iter()
        .filter_map(|id| sprites.get(id))
        .find(|s| s.num_frames > 1000)
        .or_else(|| {
            high_frame_sprites
                .iter()
                .copied()
                .find(|s| s.num_frames > 1000)
        })
        .expect("No high-frame-count DefineSprite found");

    eprintln!(
        "Selected avatar container: sprite id={}, frames={}",
        avatar_sprite.id, avatar_sprite.num_frames
    );

    // Extract per-frame display list from the avatar container
    let avatar_frames = extract_display_list_frames(&avatar_sprite.tags);

    eprintln!(
        "Extracted {} frames from avatar container",
        avatar_frames.len()
    );

    // For each frame, walk the display list and find named containers.
    // Then recursively extract child positions from nested DefineSprites.
    let mut output: Vec<serde_json::Value> = Vec::new();

    for (frame_idx, display_list) in avatar_frames.iter().enumerate() {
        if let Some(ref filter) = frame_filter {
            if !filter.contains(&(frame_idx as u16)) {
                continue;
            }
        }

        let mut frame_data = serde_json::Map::new();
        frame_data.insert("frame".into(), serde_json::json!(frame_idx));

        // For each placed object in this frame
        for (_, placement) in display_list {
            let tx = placement.tx;
            let ty = placement.ty;

            if let Some(ref name) = placement.name {
                let mut container_data = serde_json::Map::new();
                container_data.insert("x".into(), serde_json::json!(tx));
                container_data.insert("y".into(), serde_json::json!(ty));

                // If this is a DefineSprite, extract child positions too
                if let Some(char_id) = placement.character_id {
                    if let Some(child_sprite) = sprites.get(&char_id) {
                        let child_frames = extract_display_list_frames(&child_sprite.tags);
                        // Use the same frame index (clamped to child's frame count)
                        let child_frame_idx =
                            frame_idx.min(child_frames.len().saturating_sub(1));
                        if let Some(child_dl) = child_frames.get(child_frame_idx) {
                            let mut children = serde_json::Map::new();
                            for (_, child_placement) in child_dl {
                                if let Some(ref child_name) = child_placement.name {
                                    children.insert(
                                        child_name.clone(),
                                        serde_json::json!({
                                            "x": child_placement.tx,
                                            "y": child_placement.ty,
                                            "abs_x": tx + child_placement.tx,
                                            "abs_y": ty + child_placement.ty,
                                        }),
                                    );
                                }
                            }
                            if !children.is_empty() {
                                container_data.insert(
                                    "children".into(),
                                    serde_json::Value::Object(children),
                                );
                            }
                        }
                    }
                }

                frame_data.insert(name.clone(), serde_json::Value::Object(container_data));
            }
        }

        output.push(serde_json::Value::Object(frame_data));
    }

    println!("{}", serde_json::to_string_pretty(&output).unwrap());
}

/// A snapshot of a placed object's state.
#[derive(Clone)]
struct Placement {
    character_id: Option<u16>,
    name: Option<String>,
    tx: f64,
    ty: f64,
}

/// Extract per-frame display list snapshots from a sprite's tag stream.
///
/// Returns a Vec of frames, where each frame is a map of depth → Placement.
fn extract_display_list_frames(tags: &[swf::Tag]) -> Vec<HashMap<u16, Placement>> {
    let mut frames: Vec<HashMap<u16, Placement>> = Vec::new();
    let mut display_list: HashMap<u16, Placement> = HashMap::new();

    for tag in tags {
        match tag {
            swf::Tag::PlaceObject(place) => {
                let depth = place.depth;

                match &place.action {
                    PlaceObjectAction::Modify => {
                        // Update existing placement
                        if let Some(existing) = display_list.get_mut(&depth) {
                            if let Some(ref matrix) = place.matrix {
                                existing.tx = matrix.tx.to_pixels();
                                existing.ty = matrix.ty.to_pixels();
                            }
                            if let Some(ref name) = place.name {
                                existing.name = Some(name.to_str_lossy(swf::UTF_8).into_owned());
                            }
                        }
                    }
                    PlaceObjectAction::Replace(char_id) => {
                        // Replace character at depth
                        let mut placement = display_list
                            .get(&depth)
                            .cloned()
                            .unwrap_or(Placement {
                                character_id: None,
                                name: None,
                                tx: 0.0,
                                ty: 0.0,
                            });
                        if let Some(ref matrix) = place.matrix {
                            placement.tx = matrix.tx.to_pixels();
                            placement.ty = matrix.ty.to_pixels();
                        }
                        if let Some(ref name) = place.name {
                            placement.name = Some(name.to_str_lossy(swf::UTF_8).into_owned());
                        }
                        placement.character_id = Some(*char_id);
                        display_list.insert(depth, placement);
                    }
                    PlaceObjectAction::Place(char_id) => {
                        // Place new character
                        let tx = place
                            .matrix
                            .as_ref()
                            .map(|m| m.tx.to_pixels())
                            .unwrap_or(0.0);
                        let ty = place
                            .matrix
                            .as_ref()
                            .map(|m| m.ty.to_pixels())
                            .unwrap_or(0.0);
                        let name = place.name.as_ref().map(|n| n.to_str_lossy(swf::UTF_8).into_owned());

                        display_list.insert(
                            depth,
                            Placement {
                                character_id: Some(*char_id),
                                name,
                                tx,
                                ty,
                            },
                        );
                    }
                }
            }
            swf::Tag::RemoveObject(remove) => {
                display_list.remove(&remove.depth);
            }
            swf::Tag::ShowFrame => {
                frames.push(display_list.clone());
            }
            _ => {}
        }
    }

    frames
}
