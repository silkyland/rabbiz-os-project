//! Material Design 3 drawing & layout primitives on `de_paint::Canvas`.

use std::time::Duration;

use de_api::{Point, Rect, Size};
use de_design::motion::{bezier_eval, Bezier};
use de_design::tokens::{self, Theme};
use de_paint::{text, Canvas};

pub const TOP_SAFE: i32 = tokens::STATUS_BAR_HEIGHT;
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
        let (x0, y0, x1, y1) = text::ink_bounds(&px).unwrap_or((0, 0, w, h));
        let dx = r.x + (r.w - (x1 - x0)) / 2 - x0;
        let dy = r.y + (r.h - (y1 - y0)) / 2 - y0;
        c.blit(
            &de_api::Pixels::new(Size::new(w, h), px.data().to_vec()),
            Point::new(dx, dy),
        );
    }
}

/// Rasterize and draw a Google Material Icon glyph at `(x, y)` (top-left of ink).
pub fn draw_icon(c: &mut Canvas, codepoint: &str, size: f32, color: [u8; 4], x: i32, y: i32) {
    if let Some(px) = text::icon(codepoint, size, color) {
        let w = px.width() as i32;
        let h = px.height() as i32;
        let (x0, y0, _, _) = text::ink_bounds(&px).unwrap_or((0, 0, w, h));
        c.blit(
            &de_api::Pixels::new(Size::new(w, h), px.data().to_vec()),
            Point::new(x - x0, y - y0),
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

/// `color` with its alpha channel replaced by `a` (0–255).
#[must_use]
pub fn with_alpha(color: [u8; 4], a: u8) -> [u8; 4] {
    [color[0], color[1], color[2], a]
}

/// Per-channel linear interpolation `a → b` by `t` (unclamped — pass 0.0–1.0).
#[must_use]
pub fn mix(a: [u8; 4], b: [u8; 4], t: f32) -> [u8; 4] {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t) as u8;
    [l(a[0], b[0]), l(a[1], b[1]), l(a[2], b[2]), l(a[3], b[3])]
}

/// Rounded press/hover overlay — a [`state_layer`] fill at `alpha` (0.0–1.0,
/// pressed ≈ 0.10). One-liner for the pattern apps hand-roll per button.
pub fn press_overlay(c: &mut Canvas, t: &Theme, rect: Rect, radius: f32, alpha: f32) {
    if rect.is_empty() {
        return;
    }
    c.fill_rounded_rect(rect, radius, state_layer(t, alpha));
}

/// 1 px `t.line` hairline vertically centered inside `rect` (pass the row or
/// band the divider belongs to).
pub fn divider(c: &mut Canvas, t: &Theme, rect: Rect) {
    if rect.w <= 0 {
        return;
    }
    c.fill_rect(Rect::new(rect.x, rect.y + rect.h / 2, rect.w, 1), t.line);
}

/// 1 px `t.line` hairline from `x0` to `x1` at `y` (for inset list dividers).
pub fn divider_inset(c: &mut Canvas, t: &Theme, x0: i32, x1: i32, y: i32) {
    if x1 <= x0 {
        return;
    }
    c.fill_rect(Rect::new(x0, y, x1 - x0, 1), t.line);
}

/// Fake stroke: paints `color` on the outer edge and re-fills the inset
/// interior with `t.surface1`. Only reads as a border when the row/card sits
/// on a `surface1` (or matching) background — composite-aware callers should
/// pick `color`/`rect` accordingly.
pub fn stroke_rounded(
    c: &mut Canvas,
    t: &Theme,
    rect: Rect,
    radius: f32,
    color: [u8; 4],
    width: i32,
) {
    if rect.is_empty() {
        return;
    }
    c.fill_rounded_rect(rect, radius, color);
    let inner = rect.inset(width.max(0));
    if !inner.is_empty() {
        c.fill_rounded_rect(inner, (radius - width as f32).max(0.0), t.surface1);
    }
}

/// Thin scrollbar thumb inside `track` (the full-height lane). `offset` is
/// the scroll offset in content px, `content_h` the total scrollable height;
/// no thumb is drawn when the content fits the viewport.
pub fn scrollbar(c: &mut Canvas, t: &Theme, track: Rect, offset: i32, content_h: i32) {
    if track.is_empty() || content_h <= track.h {
        return;
    }
    let thumb_w = track.w.clamp(2, 6);
    let thumb_h = ((track.h as i64 * track.h as i64) / content_h as i64)
        .clamp(i64::from(32.min(track.h)), i64::from(track.h)) as i32;
    let max_off = (content_h - track.h).max(1);
    let travel = (track.h - thumb_h).max(0);
    let thumb_y =
        track.y + ((offset.clamp(0, max_off) as i64 * travel as i64) / max_off as i64) as i32;
    let thumb = Rect::new(track.right() - thumb_w, thumb_y, thumb_w, thumb_h);
    c.fill_rounded_rect(thumb, thumb_w as f32 / 2.0, with_alpha(t.text3, 0x66));
}

// ---------------------------------------------------------------------------
// Avatar tint palette
// ---------------------------------------------------------------------------

/// Stable avatar tint palette shared by dialer/contacts/sms — a peer gets
/// the same color in every app. Six hues chosen for contrast on both cream
/// and plum surfaces.
pub const AVATAR_TINTS: &[[u8; 4]] = &[
    [0x3B, 0x82, 0xF6, 0xFF], // blue
    [0x10, 0xB9, 0x81, 0xFF], // emerald
    [0xF5, 0x9E, 0x0B, 0xFF], // amber
    [0xEC, 0x48, 0x99, 0xFF], // pink
    [0x8B, 0x5C, 0xF6, 0xFF], // violet
    [0x06, 0xB6, 0xD4, 0xFF], // cyan
];

/// Deterministic tint from [`AVATAR_TINTS`] for a peer name/number seed
/// (djb-style `h*31 + b` hash, same contract the apps shared before
/// centralisation — existing avatars keep their colors).
#[must_use]
pub fn avatar_tint(seed: &str) -> [u8; 4] {
    let mut h: u32 = 0;
    for b in seed.bytes() {
        h = h.wrapping_mul(31).wrapping_add(u32::from(b)) & 0xFFFF;
    }
    AVATAR_TINTS[(h as usize) % AVATAR_TINTS.len()]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn avatar_tint_is_deterministic_and_in_palette() {
        for seed in ["Alice", "+6681234", "", "Zoë", "สมชาย"] {
            let tint = avatar_tint(seed);
            assert!(AVATAR_TINTS.contains(&tint), "seed {seed:?} out of palette");
            assert_eq!(tint, avatar_tint(seed), "seed {seed:?} not stable");
        }
        // Distinct seeds reach more than one palette slot.
        let hits: std::collections::BTreeSet<_> = ["a", "b", "c", "d", "e", "f"]
            .iter()
            .map(|s| avatar_tint(s))
            .collect();
        assert!(hits.len() > 1);
    }

    #[test]
    fn color_helpers() {
        assert_eq!(with_alpha([1, 2, 3, 4], 0x80), [1, 2, 3, 0x80]);
        assert_eq!(mix([0, 0, 0, 0], [100, 200, 50, 255], 0.0), [0, 0, 0, 0]);
        assert_eq!(
            mix([0, 0, 0, 0], [100, 200, 50, 255], 1.0),
            [100, 200, 50, 255]
        );
        let mid = mix([0, 0, 0, 255], [100, 200, 50, 255], 0.5);
        assert_eq!(mid[0], 50);
        assert_eq!(mid[1], 100);
    }

    #[test]
    fn scrollbar_draws_inside_short_viewport() {
        let track = Rect::new(0, 0, 6, 12);
        let mut canvas = Canvas::new(Size::new(track.w, track.h));
        scrollbar(&mut canvas, &tokens::LIGHT, track, 20, 100);
        assert!(canvas.pixmap().data().iter().any(|&byte| byte != 0));
    }

    #[test]
    fn scrollbar_skips_when_content_fits() {
        let track = Rect::new(700, 0, 6, 800);
        let mut c = Canvas::new(Size::new(720, 800));
        scrollbar(&mut c, &tokens::LIGHT, track, 0, 800);
        assert!(c.pixmap().data().iter().all(|&b| b == 0));

        scrollbar(&mut c, &tokens::LIGHT, track, 400, 2400);
        assert!(c.pixmap().data().iter().any(|&b| b != 0));
    }
}
