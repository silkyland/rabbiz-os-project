//! P10/P11 OSD pill: left-edge pill 96×420 @ x=24, vertically centered,
//! icon + 20 segments + percentage.

use de_api::{Pixels, Rect, Size};
use de_design::tokens::{self, Theme};
use de_paint::Canvas;

use crate::chrome::{self, style};
use crate::i18n::Strings;

/// Pill dimensions in logical pixels.
pub const PILL_W: i32 = 96;
pub const PILL_H: i32 = 420;
const PILL_MARGIN_X: i32 = 24;

/// Where the OSD pill sits on a `size` output (left-edge, vertically centered).
#[must_use]
pub fn pill_rect(size: Size) -> Rect {
    Rect::new(PILL_MARGIN_X, (size.h - PILL_H) / 2, PILL_W, PILL_H)
}

/// Render the standalone OSD pill on a transparent canvas for compositor
/// overlay placement (`DisplayArea::Overlay`).
#[must_use]
pub fn pill(t: &Theme, frac: f32, glyph: &str, muted: bool) -> Pixels {
    let mut c = Canvas::filled(Size::new(PILL_W, PILL_H), [0, 0, 0, 0]);
    let bounds = Rect::new(0, 0, PILL_W, PILL_H);
    c.fill_rounded_rect(bounds, tokens::RADIUS_PILL, t.surface2);
    let icon_col = if muted { t.text3 } else { t.text };
    chrome::draw_centered_x(
        &mut c,
        glyph,
        &de_paint::LabelStyle::icon(36.0, icon_col),
        PILL_W / 2,
        24,
    );
    let clamped = frac.clamp(0.0, 1.0);
    let seg_h = 12;
    let filled = if muted {
        0
    } else {
        (clamped * 20.0).round() as i32
    };
    for i in 0..20 {
        let y = 80 + (19 - i) * (seg_h + 4);
        let col = if i < filled { t.accent } else { t.surface3 };
        c.fill_rounded_rect(Rect::new(20, y, PILL_W - 40, seg_h), 6.0, col);
    }
    let pct = format!("{}%", (clamped * 100.0).round() as i32);
    chrome::draw_centered_x(
        &mut c,
        &pct,
        &style::label(icon_col),
        PILL_W / 2,
        PILL_H - 56,
    );
    c.into_pixels()
}

#[must_use]
pub fn render(
    size: Size,
    _s: &Strings,
    t: &Theme,
    volume: f32,
    label: &str,
    glyph: &str,
) -> Pixels {
    let mut c = Canvas::filled(size, t.bg);
    chrome::status_bar(&mut c, t, &chrome::StatusInfo::default());
    let rect = pill_rect(size);
    let card = pill(t, volume, glyph, false);
    c.blit(&card, de_api::Point::new(rect.x, rect.y));
    if !label.is_empty() {
        chrome::draw(
            &mut c,
            label,
            &style::caption(t.text2),
            rect.x + rect.w + 16,
            rect.y + rect.h / 2 - 14,
            Some(size.w - rect.x - rect.w - 40),
        );
    }
    chrome::nav_pill(&mut c, t);
    c.into_pixels()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pill_stays_on_screen_and_has_expected_size() {
        let r = pill_rect(Size::new(720, 1600));
        assert!(r.x >= 0 && r.y >= 0);
        assert!(r.x + r.w <= 720);
        assert!(r.y + r.h <= 1600);
        let px = pill(&tokens::DARK, 0.65, "\u{e050}", false);
        assert_eq!(px.size, Size::new(PILL_W, PILL_H));
    }
}
