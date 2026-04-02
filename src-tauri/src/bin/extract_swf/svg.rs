/// Convert a parsed SWF to a minimal SVG string.
///
/// This is a stub — it returns a valid but empty SVG using the stage dimensions
/// as the viewBox. Full vector rendering will be added in subsequent tasks.
pub fn convert_swf_to_svg(swf: &swf::Swf) -> String {
    let stage = swf.header.stage_size();
    let width = (stage.x_max - stage.x_min).to_pixels();
    let height = (stage.y_max - stage.y_min).to_pixels();
    let x = stage.x_min.to_pixels();
    let y = stage.y_min.to_pixels();

    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="{x} {y} {width} {height}" width="{width}" height="{height}">
  <!-- TODO: vector rendering not yet implemented -->
</svg>"#
    )
}
