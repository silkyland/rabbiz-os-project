//! Layout rules: wrap modes, line clamping and ellipsis — the contract every
//! shell label obeys (labels never overflow; long text ends with "…").

use cosmic_text::FontSystem;
use de_api::Size;
use tiny_skia::Pixmap;

use super::shape::Shaped;
use super::{LabelStyle, TextWrap};

/// Rasterize `text` clamped to `max_lines` lines within `max_width`.
/// When the text doesn't fit, the last visible line is shortened and
/// terminated with an ellipsis glyph ("…").
#[must_use]
pub fn layout_clamped(
    fs: &mut FontSystem,
    text: &str,
    style: &LabelStyle,
    max_width: f32,
    max_lines: usize,
) -> Option<Pixmap> {
    if text.is_empty() || max_lines == 0 {
        return None;
    }
    let shaped = Shaped::shape(fs, text, style, Some(max_width), TextWrap::Word);
    if shaped.lines() <= max_lines {
        return shaped.rasterize(fs, style.color);
    }
    // Re-layout with an ellipsis appended — binary-ish shrink: try the
    // longest prefix that still fits `max_lines` lines including "…".
    let chars: Vec<char> = text.chars().collect();
    let (mut lo, mut hi) = (0usize, chars.len());
    let mut best: Option<String> = None;
    while lo < hi {
        let mid = (lo + hi) / 2;
        let cand: String = chars[..mid].iter().collect();
        let cand = format!("{}…", cand.trim_end());
        if Shaped::shape(fs, &cand, style, Some(max_width), TextWrap::Word).lines() <= max_lines {
            best = Some(cand);
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    let text = best.unwrap_or_else(|| "…".to_string());
    Shaped::shape(fs, &text, style, Some(max_width), TextWrap::Word).rasterize(fs, style.color)
}

/// Single-line truncation: fit `text` in `max_width`, else shorten + "…".
/// Returns the (possibly truncated) string — callers that need the pixmap
/// use [`layout_clamped`] with `max_lines = 1`.
#[must_use]
pub fn ellipsize(fs: &mut FontSystem, text: &str, style: &LabelStyle, max_width: f32) -> String {
    if Shaped::shape(fs, text, style, None, TextWrap::None).extent().0 as f32 <= max_width {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    let (mut lo, mut hi) = (0usize, chars.len());
    while lo < hi {
        let mid = (lo + hi + 1) / 2;
        let cand: String = chars[..mid].iter().collect();
        let cand = format!("{}…", cand.trim_end());
        if Shaped::shape(fs, &cand, style, None, TextWrap::None).extent().0 as f32 <= max_width {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    if lo == 0 {
        "…".to_string()
    } else {
        let s: String = chars[..lo].iter().collect();
        format!("{}…", s.trim_end())
    }
}

/// Measured extent of `text` at `style` (no rasterization).
#[must_use]
pub fn measure_text(fs: &mut FontSystem, text: &str, style: &LabelStyle) -> Size {
    let (w, h) = Shaped::shape(fs, text, style, None, TextWrap::None).extent();
    Size::new(w, h)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fonts::font_system;

    #[test]
    fn ellipsis_fits_width() {
        let mut fs = font_system().lock().unwrap();
        let style = LabelStyle::default();
        let out = ellipsize(&mut fs, "A very long application name that will not fit", &style, 120.0);
        assert!(out.ends_with('…'));
        let w = measure_text(&mut fs, &out, &style).w;
        assert!(w <= 120, "w={w} out={out:?}");
    }

    #[test]
    fn clamp_to_two_lines() {
        let mut fs = font_system().lock().unwrap();
        let style = LabelStyle::default();
        let px = layout_clamped(
            &mut fs,
            "word ".repeat(80).trim_end(),
            &style,
            160.0,
            2,
        )
        .expect("px");
        // Pixmap height = last baseline + line_height ≈ (lines+~0.8)*lh —
        // bound loosely at (max_lines+1) lines.
        let bound = (style.line_height * 3.0).ceil() as u32;
        assert!(px.height() <= bound, "h={}", px.height());
    }

    #[test]
    fn thai_ellipsis() {
        let mut fs = font_system().lock().unwrap();
        let style = LabelStyle::default();
        let out = ellipsize(&mut fs, "แอปพลิเคชันที่มีชื่อยาวมากเกินไปสำหรับพื้นที่", &style, 100.0);
        assert!(out.ends_with('…'));
    }
}
