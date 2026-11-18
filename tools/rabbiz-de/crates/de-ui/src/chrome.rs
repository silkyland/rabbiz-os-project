//! Shared chrome pieces used by the screen skeletons (M2-T5): status bar,
//! nav pill, battery glyph, grouped-card rows. All geometry comes from
//! `de-design::tokens` safe-area constants.

use de_api::{Point, Rect, Size};
use de_design::tokens::{self, Theme};
use de_paint::{text, Canvas};

use crate::i18n::Strings;

/// Status bar height incl. cutout inset (§7.17: 56 px).
pub const STATUS_H: i32 = tokens::STATUS_BAR_HEIGHT;
/// Nav pill area (gesture bar 32 px).
pub const NAV_H: i32 = tokens::GESTURE_BAR_HEIGHT;
/// Page margin (§7.17: 24 px).
pub const MARGIN: i32 = tokens::PAGE_MARGIN;
/// Content width on 720 px output (672 px).
#[must_use]
pub fn content_w(size: Size) -> i32 {
    size.w - MARGIN * 2
}

/// Label style helpers bound to the type scale.
pub mod style {
    use de_paint::text::LabelStyle;

    pub fn body(color: [u8; 4]) -> LabelStyle {
        LabelStyle { color, ..LabelStyle::default() }
    }
    pub fn label(color: [u8; 4]) -> LabelStyle {
        LabelStyle { size: 22.0, line_height: 28.0, color, ..LabelStyle::default() }
    }
    pub fn caption(color: [u8; 4]) -> LabelStyle {
        LabelStyle { size: 20.0, line_height: 26.0, color, ..LabelStyle::default() }
    }
    pub fn h2(color: [u8; 4]) -> LabelStyle {
        LabelStyle { size: 32.0, line_height: 40.0, weight: 600, color, ..LabelStyle::default() }
    }
    pub fn display(color: [u8; 4]) -> LabelStyle {
        LabelStyle { size: 104.0, line_height: 112.0, weight: 600, tnum: true, color, ..LabelStyle::default() }
    }
}

/// Paint `px` centered horizontally at `y`.
pub fn blit_centered(c: &mut Canvas, px: &de_api::Pixels, y: i32) {
    let x = (c.size().w - px.size.w) / 2;
    c.blit(px, Point::new(x, y));
}

/// Draw a label; returns the rect the pixels occupy.
pub fn draw(c: &mut Canvas, s: &str, st: &text::LabelStyle, x: i32, y: i32, max_w: Option<i32>) -> Rect {
    if let Some(px) = text::label(s, *st, max_w) {
        let r = Rect::new(x, y, px.width() as i32, px.height() as i32);
        c.blit(&de_api::Pixels::new(de_api::Size::new(r.w, r.h), px.data().to_vec()), Point::new(x, y));
        return r;
    }
    Rect::new(x, y, 0, 0)
}

/// Centered label horizontally.
pub fn draw_centered(c: &mut Canvas, s: &str, st: &text::LabelStyle, y: i32) {
    draw_centered_x(c, s, st, c.size().w / 2, y);
}

/// Label centered on `cx` at `y`.
pub fn draw_centered_x(c: &mut Canvas, s: &str, st: &text::LabelStyle, cx: i32, y: i32) {
    if let Some(px) = text::label(s, *st, None) {
        let w = px.width() as i32;
        let h = px.height() as i32;
        c.blit(&de_api::Pixels::new(de_api::Size::new(w, h), px.data().to_vec()), Point::new(cx - w / 2, y));
    }
}

/// 56 px status bar: clock left (tnum), battery % + Wi-Fi right.
pub fn status_bar(c: &mut Canvas, s: &Strings, t: &Theme, clock: &str, battery: &str) {
    let w = c.size().w;
    // Left cutout inset 72 px — clock starts right of it.
    draw(c, clock, &style::label(t.text), tokens::CUTOUT_INSET, 14, None);
    // Battery pct right-aligned.
    if let Some(px) = text::label(battery, style::label(t.text), None) {
        let (bw, bh) = (px.width() as i32, px.height() as i32);
        c.blit(
            &de_api::Pixels::new(de_api::Size::new(bw, bh), px.data().to_vec()),
            Point::new(w - MARGIN - bw, 14),
        );
    }
    let _ = s;
}

/// 200×8 nav pill at the bottom (y = h - 24).
pub fn nav_pill(c: &mut Canvas, t: &Theme) {
    let size = c.size();
    let r = Rect::new((size.w - 200) / 2, size.h - 24, 200, 8);
    c.fill_rounded_rect(r, 4.0, t.text3);
}

/// Grouped-card row: 88 px row, `line` divider, label left, value+chevron right.
pub fn settings_row(c: &mut Canvas, t: &Theme, y: i32, name: &str, value: &str, first: bool, last: bool) {
    let w = c.size().w;
    let r = Rect::new(MARGIN, y, w - MARGIN * 2, 88);
    let radius = if first && last {
        tokens::RADIUS_CARD
    } else if first || last {
        tokens::RADIUS_CARD
    } else {
        0.0
    };
    c.fill_rounded_rect(r, radius, t.surface1);
    if !first {
        c.fill_rect(Rect::new(r.x + 16, y, r.w - 32, 1), t.line);
    }
    draw(c, name, &style::body(t.text), r.x + 24, y + 30, None);
    if let Some(px) = text::label(value, style::body(t.text3), None) {
        let (vw, vh) = (px.width() as i32, px.height() as i32);
        c.blit(
            &de_api::Pixels::new(de_api::Size::new(vw, vh), px.data().to_vec()),
            Point::new(r.x + r.w - 24 - vw - 24, y + 30),
        );
    }
    // chevron_right glyph.
    draw(c, "\u{e5cc}", &text::LabelStyle::icon(24.0, t.text3), r.x + r.w - 36, y + 32, None);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_width() {
        assert_eq!(content_w(Size::new(720, 1600)), 672);
        assert_eq!(content_w(Size::new(1080, 2400)), 1032);
    }
}
