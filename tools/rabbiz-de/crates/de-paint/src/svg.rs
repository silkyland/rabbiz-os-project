//! SVG rasterization via resvg (M0 brand assets; M2-T3 extends this into the
//! icon pipeline in `de-apps`).

use std::sync::Arc;

use de_api::Pixels;
use tiny_skia::{Pixmap, Transform};

/// SVG rasterization error.
#[derive(Debug)]
pub struct SvgError(pub String);

impl std::fmt::Display for SvgError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for SvgError {}

/// Rasterize `svg` scaled so the output is `target_width` px wide.
///
/// Fonts come from the bundled `fontdb` (same stack as text rendering), so
/// `<text>` in brand assets shapes identically on host and device.
pub fn rasterize(svg: &[u8], target_width: u32) -> Result<Pixels, SvgError> {
    // resvg 0.50 / usvg 0.50 uses fontdb 0.24, newer than cosmic-text's —
    // build a separate db of the same bundled faces.
    let mut fontdb = resvg::usvg::fontdb::Database::new();
    for face in crate::fonts::BUNDLED_FONTS {
        fontdb.load_font_data(face.to_vec());
    }
    let opt = resvg::usvg::Options {
        fontdb: Arc::new(fontdb),
        ..resvg::usvg::Options::default()
    };
    let tree =
        resvg::usvg::Tree::from_data(svg, &opt).map_err(|e| SvgError(format!("usvg: {e}")))?;
    let size = tree.size();
    let scale = target_width as f32 / size.width();
    let h = (size.height() * scale).ceil().max(1.0) as u32;
    let mut px =
        Pixmap::new(target_width.max(1), h).ok_or_else(|| SvgError("pixmap".to_string()))?;
    resvg::render(&tree, Transform::from_scale(scale, scale), &mut px.as_mut());
    Ok(Pixels {
        size: de_api::Size::new(px.width() as i32, px.height() as i32),
        data: px.take(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rasterizes_simple_svg() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="50" viewBox="0 0 100 50"><rect width="100" height="50" fill="#ff0000"/></svg>"##;
        let px = rasterize(svg, 200).unwrap();
        assert_eq!((px.size.w, px.size.h), (200, 100));
        assert_eq!(&px.data[0..4], &[255, 0, 0, 255]);
    }
}
