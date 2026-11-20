//! Text pipeline (M2-T2): [`shape`] (cosmic-text shaping + bundled-font
//! fallback), [`layout`] (wrap/line-clamp/ellipsis rules) and [`cache`]
//! (LRU raster cache — no per-frame text shaping per §7.17a).
//!
//! The M0 `label`/`draw_label`/`measure` surface stays as thin wrappers.

use de_api::{Point, Size};
use std::sync::{Mutex, OnceLock};
use tiny_skia::{Pixmap, PixmapPaint, Transform};

use cosmic_text::SwashCache;

use crate::fonts::{font_system, FAMILY_ICONS, FAMILY_NOTO};

pub mod cache;
pub mod layout;
pub mod shape;

pub use layout::{ellipsize, layout_clamped, measure_text};

pub(crate) fn swash_cache() -> &'static Mutex<SwashCache> {
    static C: OnceLock<Mutex<SwashCache>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(SwashCache::new()))
}

/// Wrap mode for layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextWrap {
    /// Single line, no wrapping.
    None,
    /// Wrap at word boundaries (labels, cards).
    Word,
    /// Wrap anywhere (Thai has no spaces — long Thai strings need this).
    Glyph,
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
    /// Tabular numerals (clocks, timers, percentages). Informational on
    /// cosmic-text 0.12 — bundled Noto digits are already tabular-width.
    pub tnum: bool,
}

impl Default for LabelStyle {
    fn default() -> Self {
        Self {
            size: 26.0,
            line_height: 34.0,
            weight: 400,
            color: [255, 255, 255, 255],
            family: FAMILY_NOTO,
            tnum: false,
        }
    }
}

impl LabelStyle {
    /// From a `de-design` type-scale style plus a color.
    #[must_use]
    pub fn from_type(ts: de_design::type_scale::TypeStyle, color: [u8; 4]) -> Self {
        Self {
            size: ts.size,
            line_height: ts.line_height,
            weight: ts.weight,
            color,
            tnum: ts.tnum,
            ..Self::default()
        }
    }

    /// Material Icons Round style (icon glyphs by codepoint).
    #[must_use]
    pub fn icon(size: f32, color: [u8; 4]) -> Self {
        Self {
            size,
            line_height: size,
            family: FAMILY_ICONS,
            color,
            ..Self::default()
        }
    }
}

/// Rasterize `text` into a tight pixmap at the given `style`.
///
/// Returns `None` when the text shapes to nothing. The pixmap is premultiplied
/// RGBA8, ready for `Canvas::blit`.
/// Bounding box of the visible (non-transparent) pixels as
/// `(x0, y0, x1, y1)`, exclusive at the far end. Glyph boxes carry line
/// height and side bearings, so centring a box centres the padding, not
/// the glyph: centre the ink instead.
#[must_use]
pub fn ink_bounds(px: &Pixmap) -> Option<(i32, i32, i32, i32)> {
    let (w, h) = (px.width() as i32, px.height() as i32);
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, -1, -1);
    for y in 0..h {
        for x in 0..w {
            if px.data()[((y * w + x) * 4 + 3) as usize] > 8 {
                x0 = x0.min(x);
                x1 = x1.max(x);
                y0 = y0.min(y);
                y1 = y1.max(y);
            }
        }
    }
    (x1 >= 0).then_some((x0, y0, x1 + 1, y1 + 1))
}

#[must_use]
pub fn label(text: &str, style: LabelStyle, max_width: Option<i32>) -> Option<Pixmap> {
    if text.is_empty() {
        return None;
    }
    let mut fs = font_system().lock().ok()?;
    let shaped = shape::Shaped::shape(
        &mut fs,
        text,
        &style,
        max_width.map(|w| w.max(1) as f32),
        TextWrap::Glyph,
    );
    shaped.rasterize(&mut fs, style.color)
}

/// Rasterize a Material icon codepoint (e.g. `""` from `icons::GLYPH`).
#[must_use]
pub fn icon(codepoint: &str, size: f32, color: [u8; 4]) -> Option<Pixmap> {
    label(codepoint, LabelStyle::icon(size, color), None)
}

/// Clamped label (M2-T2 contract): wrap to `max_width`, clamp to
/// `max_lines`, ellipsize the last line when it doesn't fit.
#[must_use]
pub fn label_clamped(
    text: &str,
    style: LabelStyle,
    max_width: i32,
    max_lines: usize,
) -> Option<Pixmap> {
    let mut fs = font_system().lock().ok()?;
    layout::layout_clamped(&mut fs, text, &style, max_width as f32, max_lines)
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
    let offset = canvas.origin();
    canvas.pixmap_mut().draw_pixmap(
        0,
        0,
        px.as_ref(),
        &PixmapPaint::default(),
        Transform::from_translate((origin.x - offset.x) as f32, (origin.y - offset.y) as f32),
        None,
    );
    Some(size)
}

/// Measure a label without drawing it.
#[must_use]
pub fn measure(font_system: &mut cosmic_text::FontSystem, text: &str, style: LabelStyle) -> Size {
    layout::measure_text(font_system, text, &style)
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

    #[test]
    fn icon_glyph_renders() {
        // Material "wifi" codepoint. Must contain real ink — a shaped-but-
        // unrasterizable glyph (e.g. CFF outlines under swash) still returns
        // a pixmap with the font's advance box, so `is_some` alone is not
        // proof it drew anything.
        let px = icon("\u{e63e}", 24.0, [255, 255, 255, 255]).expect("icon pixmap");
        assert!(px.width() >= 20);
        let ink = px.data().chunks(4).filter(|c| c[3] > 10).count();
        assert!(ink > 20, "icon glyph rasterized with no ink");
    }
}
