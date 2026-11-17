//! Minimal text rasterization via cosmic-text (M0-T5).
//!
//! M0 needs `label()` only; the full pipeline (layout cache, ellipsis rules,
//! Thai fallback policy) is M2-T2's job and lives in `layout`/`shape`/`cache`.

use cosmic_text::{
    Attrs, Buffer, Color as CosmicColor, Family, FontSystem, Metrics, Shaping, SwashCache, Weight,
    Wrap,
};
use de_api::{Point, Size};
use std::sync::{Mutex, OnceLock};
use tiny_skia::Pixmap;

use crate::fonts::{font_system, FAMILY_NOTO};

fn swash_cache() -> &'static Mutex<SwashCache> {
    static C: OnceLock<Mutex<SwashCache>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(SwashCache::new()))
}

/// Text style for a single-weight label.
#[derive(Debug, Clone, Copy)]
pub struct LabelStyle {
    /// Font size in px.
    pub size: f32,
    /// Line height in px.
    pub line_height: f32,
    /// 400 or 600 for the bundled faces.
    pub weight: u16,
    /// RGBA color.
    pub color: [u8; 4],
    /// Font family (defaults to Noto Sans; icon glyphs use `FAMILY_ICONS`).
    pub family: &'static str,
}

impl Default for LabelStyle {
    fn default() -> Self {
        Self {
            size: 26.0,
            line_height: 34.0,
            weight: 400,
            color: [255, 255, 255, 255],
            family: FAMILY_NOTO,
        }
    }
}

/// Rasterize `text` into a tight pixmap at the given `style`.
///
/// Returns `None` when the text shapes to nothing. The pixmap is premultiplied
/// RGBA8, ready for `Canvas::blit`.
#[must_use]
pub fn label(text: &str, style: LabelStyle, max_width: Option<i32>) -> Option<Pixmap> {
    if text.is_empty() {
        return None;
    }
    let mut fs = font_system().lock().ok()?;
    let mut cache = swash_cache().lock().ok()?;

    let metrics = Metrics::new(style.size, style.line_height);
    let mut buf = Buffer::new(&mut fs, metrics);
    let w = max_width.map_or(f32::MAX, |w| w.max(1) as f32);
    buf.set_size(&mut fs, Some(w), None);
    buf.set_wrap(&mut fs, Wrap::Glyph);
    let attrs = Attrs::new()
        .family(Family::Name(style.family))
        .weight(Weight(style.weight));
    buf.set_text(&mut fs, text, attrs, Shaping::Advanced);
    buf.shape_until_scroll(&mut fs, false);

    // Measure extents.
    let (mut width, mut height) = (0f32, 0i32);
    for run in buf.layout_runs() {
        width = width.max(run.line_w);
        height = height.max((run.line_y + run.line_height) as i32);
    }
    let (width, height) = (width.ceil() as i32 + 1, height.max(1));
    if width <= 0 || height <= 0 {
        return None;
    }
    let mut px = Pixmap::new(width as u32, height as u32)?;
    let color = CosmicColor::rgba(
        style.color[0],
        style.color[1],
        style.color[2],
        style.color[3],
    );
    let (pw, data_w) = (px.width() as i32, px.height() as i32);
    buf.draw(&mut fs, &mut cache, color, |x, y, _w, _h, c| {
        if x < 0 || y < 0 || x >= pw || y >= data_w {
            return;
        }
        let i = ((y * pw + x) * 4) as usize;
        let d = px.data_mut();
        let sa = c.a() as u32;
        let inv = 255 - sa;
        // `c` is straight (non-premultiplied) rgba; premultiply on blend.
        d[i] = ((c.r() as u32 * sa + 127) / 255 + (d[i] as u32 * inv + 127) / 255).min(255) as u8;
        d[i + 1] =
            ((c.g() as u32 * sa + 127) / 255 + (d[i + 1] as u32 * inv + 127) / 255).min(255) as u8;
        d[i + 2] =
            ((c.b() as u32 * sa + 127) / 255 + (d[i + 2] as u32 * inv + 127) / 255).min(255) as u8;
        d[i + 3] = (sa + (d[i + 3] as u32 * inv + 127) / 255).min(255) as u8;
    });
    Some(px)
}

/// Convenience: rasterize a label and blit it onto `canvas` at `origin`.
pub fn draw_label(
    canvas: &mut crate::canvas::Canvas,
    text: &str,
    origin: Point,
    style: LabelStyle,
    max_width: Option<i32>,
) -> Option<Size> {
    let px = label(text, style, max_width)?;
    let size = Size::new(px.width() as i32, px.height() as i32);
    canvas.pixmap_mut().draw_pixmap(
        0,
        0,
        px.as_ref(),
        &tiny_skia::PixmapPaint::default(),
        tiny_skia::Transform::from_translate(origin.x as f32, origin.y as f32),
        None,
    );
    Some(size)
}

/// Measure a label without drawing it.
#[must_use]
pub fn measure(font_system: &mut FontSystem, text: &str, style: LabelStyle) -> Size {
    let metrics = Metrics::new(style.size, style.line_height);
    let mut buf = Buffer::new(font_system, metrics);
    buf.set_size(font_system, None, None);
    let attrs = Attrs::new()
        .family(Family::Name(style.family))
        .weight(Weight(style.weight));
    buf.set_text(font_system, text, attrs, Shaping::Advanced);
    buf.shape_until_scroll(font_system, false);
    let (mut w, mut h) = (0f32, 0i32);
    for run in buf.layout_runs() {
        w = w.max(run.line_w);
        h = h.max((run.line_y + run.line_height) as i32);
    }
    Size::new(w.ceil() as i32 + 1, h.max(1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::Canvas;

    #[test]
    fn renders_latin_and_thai() {
        let style = LabelStyle {
            size: 28.0,
            line_height: 36.0,
            ..LabelStyle::default()
        };
        let latin = label("RabbizOS", style, None).expect("latin");
        assert!(latin.width() > 50);
        let thai = label("สวัสดี", style, None).expect("thai");
        assert!(thai.width() > 20);
        // Thai has marks above/below the line — pixmap must be tall enough.
        assert!(thai.height() >= 30);
    }

    #[test]
    fn draw_label_blends() {
        let mut c = Canvas::new(Size::new(200, 60));
        c.clear([0, 0, 0, 255]);
        let s = draw_label(&mut c, "Hi", Point::new(4, 4), LabelStyle::default(), None);
        assert!(s.is_some());
    }
}
