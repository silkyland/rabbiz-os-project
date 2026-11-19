//! Material Design 3 drawing & layout primitives on `de_paint::Canvas`.

use std::time::Duration;

use de_api::{Point, Rect, Size};
use de_design::motion::{bezier_eval, Bezier};
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
    draw_text(
        c,
        heading,
        &title(t.text),
        MARGIN,
        y,
        Some(c.size().w - MARGIN * 2),
    );
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

/// Right-aligned text inside `r` (vertically centered, `pad` inset on the
/// right). Calculator-style readouts and numeric columns need this; mirror of
/// [`draw_centered`] on the opposite edge.
pub fn draw_text_right(c: &mut Canvas, s: &str, st: &text::LabelStyle, r: Rect, pad: i32) {
    if s.is_empty() {
        return;
    }
    let max_w = (r.w - pad * 2).max(16);
    let Some(px) = text::label(s, *st, Some(max_w)) else {
        return;
    };
    let w = px.width() as i32;
    let h = px.height() as i32;
    c.blit(
        &de_api::Pixels::new(Size::new(w, h), px.data().to_vec()),
        Point::new(r.right() - pad - w, r.y + (r.h - h).max(0) / 2),
    );
}

/// M3 state-layer tint derived from the theme's text color, for press/drag
/// overlays drawn with `fill_rounded_rect`. `alpha` is 0.0–1.0 (pressed ≈
/// 0.10, dragged ≈ 0.16); deriving from `text` keeps it legible on cream and
/// plum themes alike.
#[must_use]
pub fn state_layer(t: &Theme, alpha: f32) -> [u8; 4] {
    [
        t.text[0],
        t.text[1],
        t.text[2],
        (alpha.clamp(0.0, 1.0) * 255.0) as u8,
    ]
}

/// Bezier-eased scalar tween advanced by `AppEvent::Tick(dt)`.
/// `Tween::new(from, to, ..)` with `from == to` starts finished so it never
/// keeps `animating` alive for a no-op.
pub struct Tween {
    from: f32,
    to: f32,
    t: f32,
    dur: f32,
    curve: Bezier,
}

impl Tween {
    #[must_use]
    pub fn new(from: f32, to: f32, dur_s: f32, curve: Bezier) -> Self {
        let dur = dur_s.max(0.001);
        let t = if from == to { dur } else { 0.0 };
        Self {
            from,
            to,
            t,
            dur,
            curve,
        }
    }

    #[must_use]
    pub fn value(&self) -> f32 {
        let k = bezier_eval(self.curve, (self.t / self.dur).min(1.0));
        self.from + (self.to - self.from) * k
    }

    /// Advance by `dt`; returns true while still running.
    pub fn tick(&mut self, dt: Duration) -> bool {
        self.t += dt.as_secs_f32();
        self.t < self.dur
    }

    #[must_use]
    pub fn done(&self) -> bool {
        self.t >= self.dur
    }
}
