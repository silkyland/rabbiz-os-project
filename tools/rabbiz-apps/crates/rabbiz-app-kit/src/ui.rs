//! Material Design 3 drawing & layout primitives on `de_paint::Canvas`.

use de_api::{Point, Rect, Size};
use de_design::tokens::{self, Theme};
use de_paint::{text, Canvas};

pub const TOP_SAFE: i32 = 44;
pub const BOTTOM_SAFE: i32 = tokens::GESTURE_BAR_HEIGHT + 12;
pub const MARGIN: i32 = 24;

#[must_use]
pub fn styled(size: f32, weight: u16, color: [u8; 4]) -> text::LabelStyle {
    text::LabelStyle {
        size,
        line_height: size * 1.25,
        weight,
        color,
        ..text::LabelStyle::default()
    }
}

#[must_use]
pub fn body(color: [u8; 4]) -> text::LabelStyle {
    styled(22.0, 400, color)
}

#[must_use]
pub fn label(color: [u8; 4]) -> text::LabelStyle {
    styled(20.0, 500, color)
}

#[must_use]
pub fn caption(color: [u8; 4]) -> text::LabelStyle {
    styled(17.0, 400, color)
}

#[must_use]
pub fn title(color: [u8; 4]) -> text::LabelStyle {
    styled(34.0, 600, color)
}

#[must_use]
pub fn display(color: [u8; 4]) -> text::LabelStyle {
    styled(56.0, 600, color)
}

pub fn draw_text(
    c: &mut Canvas,
    s: &str,
    st: &text::LabelStyle,
    x: i32,
    y: i32,
    max_w: Option<i32>,
) -> Rect {
    if s.is_empty() {
        return Rect::new(x, y, 0, 0);
    }
    if let Some(px) = text::label(s, *st, max_w) {
        let r = Rect::new(x, y, px.width() as i32, px.height() as i32);
        c.blit(
            &de_api::Pixels::new(Size::new(r.w, r.h), px.data().to_vec()),
            Point::new(x, y),
        );
        return r;
    }
    Rect::new(x, y, 0, 0)
}

pub fn draw_centered(c: &mut Canvas, s: &str, st: &text::LabelStyle, r: Rect) {
    if s.is_empty() {
        return;
    }
    if let Some(px) = text::label(s, *st, Some((r.w - 8).max(16))) {
        let w = px.width() as i32;
        let h = px.height() as i32;
        c.blit(
            &de_api::Pixels::new(Size::new(w, h), px.data().to_vec()),
            Point::new(r.x + (r.w - w) / 2, r.y + (r.h - h) / 2),
        );
    }
}

pub fn draw_header(c: &mut Canvas, t: &Theme, heading: &str, subtitle: &str) -> i32 {
    let y = TOP_SAFE + 16;
    draw_text(c, heading, &title(t.text), MARGIN, y, Some(c.size().w - MARGIN * 2));
    if !subtitle.is_empty() {
        draw_text(
            c,
            subtitle,
            &caption(t.text3),
            MARGIN,
            y + 44,
            Some(c.size().w - MARGIN * 2),
        );
        y + 80
    } else {
        y + 56
    }
}

pub fn draw_pill_button(
    c: &mut Canvas,
    t: &Theme,
    r: Rect,
    text_str: &str,
    primary: bool,
    pressed: bool,
) {
    let bg = if primary { t.accent } else { t.surface2 };
    let fg = if primary { t.text_on_accent } else { t.text };
    let rr = if pressed {
        Rect::new(r.x + 2, r.y + 2, (r.w - 4).max(1), (r.h - 4).max(1))
    } else {
        r
    };
    c.fill_rounded_rect(rr, (rr.h as f32) / 2.0, bg);
    if pressed {
        c.fill_rounded_rect(rr, (rr.h as f32) / 2.0, [255, 255, 255, 28]);
    }
    draw_centered(c, text_str, &label(fg), rr);
}

pub fn draw_card(c: &mut Canvas, t: &Theme, r: Rect, pressed: bool) {
    c.fill_rounded_rect(r, tokens::RADIUS_CARD, t.surface1);
    if pressed {
        c.fill_rounded_rect(r, tokens::RADIUS_CARD, [255, 255, 255, 18]);
    }
}

pub fn draw_switch(c: &mut Canvas, t: &Theme, x: i32, y: i32, on: bool) {
    let track = Rect::new(x, y, 56, 32);
    c.fill_rounded_rect(track, 16.0, if on { t.accent } else { t.surface2 });
    let kx = if on { x + 56 - 4 - 24 } else { x + 4 };
    c.fill_rounded_rect(
        Rect::new(kx, y + 4, 24, 24),
        12.0,
        if on { t.text_on_accent } else { t.text3 },
    );
}
