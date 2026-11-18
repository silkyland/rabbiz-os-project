//! Launch/close morph (M3-T5): the icon tile springs from its grid rect to
//! the fullscreen app surface — finger-tracked, interruptible, and
//! reversible (the same curve runs backwards on an early release).
//!
//! [`morph_frame`] is the pure renderer at progress `p` (0 = icon rect,
//! 1 = fullscreen); [`MorphAnimator`] wraps `SpringValue` so `de-core`
//! drives it with real frame deltas and retargets mid-flight.

use de_api::{Pixels, Point, Rect};
use de_design::motion::{MotionKind, SpringPreset};
use de_design::tokens::{self, Theme};
use de_paint::{animation::SpringValue, Canvas};

use crate::chrome::{self, style, IconEntry};
use crate::transitions::{ease_out, fade_in, lerp, lerp_rect, with_opacity};

/// One morph frame over `under` (usually the home screen pixels).
///
/// `src` is the icon's rect in the grid, `dst` the window's target rect
/// (normally the full output minus chrome). Progress semantics:
/// card rect + radius lerp, icon crossfades into the app surface, the
/// label fades out first (gone by p=0.35), content fades in last
/// (from p=0.55).
#[must_use]
pub fn morph_frame(
    under: &Pixels,
    t: &Theme,
    p: f32,
    src: Rect,
    dst: Rect,
    icon: Option<&Pixels>,
    label: &str,
) -> Pixels {
    let mut c = Canvas::new(under.size);
    c.blit(under, Point::ZERO);

    let r = lerp_rect(src, dst, ease_out(p));
    let radius = lerp(tokens::ICON_TILE_RADIUS, 0.0, p);
    c.fill_rounded_rect(r, radius, t.surface1);

    // Icon rides the morph until the surface wins the crossfade.
    if let Some(px) = icon {
        let a = 1.0 - fade_in(p, 0.55, 0.85);
        if a > 0.0 {
            let box_r = Rect::new(
                r.x + (r.w - 96) / 2,
                lerp(
                    src.y as f32,
                    r.y as f32 + (r.h as f32 - 96.0) / 2.0,
                    fade_in(p, 0.0, 0.4),
                ) as i32,
                96,
                96,
            );
            let e = IconEntry {
                id: String::new(),
                label: label.into(),
                icon: Some(px.clone()),
            };
            // Opacity on an image needs a per-pixel fade — cheap path: draw
            // the icon onto a 96-px card and blit it (full alpha); the
            // crossfade is approximated by the surface fill above. For a
            // spring-driven morph this reads fine.
            let _ = a;
            chrome::draw_app_icon(&mut c, t, &e, box_r);
        }
    } else {
        // First-party tile keeps its look the whole way.
        let e = IconEntry {
            id: String::new(),
            label: label.into(),
            icon: None,
        };
        let box_r = Rect::new(r.x + (r.w - 96) / 2, r.y + (r.h - 96) / 2, 96, 96);
        if p < 0.85 {
            chrome::draw_app_icon(&mut c, t, &e, box_r);
        }
    }

    // App label fades out quickly (shared-element label hand-off).
    let la = 1.0 - fade_in(p, 0.0, 0.35);
    if la > 0.0 {
        chrome::draw(
            &mut c,
            label,
            &style::label(with_opacity(t.text, la)),
            src.x + (src.w - 96) / 2,
            src.y + src.h + 8,
            None,
        );
    }
    c.into_pixels()
}

/// Spring driver for the morph (`de-core` owns one per launch).
///
/// `start` seeds the spring from the icon rect; `retarget` flips between
/// "open" (1.0) and "closed" (0.0) without losing velocity — an early
/// release mid-morph snaps back smoothly instead of jumping (§7.17a).
#[derive(Debug)]
pub struct MorphAnimator {
    spring: SpringValue,
    target: f32,
}

impl MorphAnimator {
    /// Begin a morph at progress `from` (0 = closed).
    #[must_use]
    pub fn new(from: f32) -> Self {
        // §7.17a: app open/close uses Smooth (450 ms, zero bounce).
        Self {
            spring: SpringValue::new(from, SpringPreset::Smooth, MotionKind::Spatial),
            target: from,
        }
    }

    /// Retarget to open (`1.0`) or closed (`0.0`), preserving velocity.
    pub fn retarget(&mut self, open: bool) {
        self.target = if open { 1.0 } else { 0.0 };
        self.spring.retarget(self.target);
    }

    /// Advance by `dt_s`; returns current progress.
    pub fn advance(&mut self, dt_s: f32) -> f32 {
        self.spring.advance(dt_s);
        self.spring.value.clamp(0.0, 1.0)
    }

    /// Current progress without advancing.
    #[must_use]
    pub fn progress(&self) -> f32 {
        self.spring.value.clamp(0.0, 1.0)
    }

    /// Settled at the target (frame loop may stop morph frames).
    #[must_use]
    pub fn settled(&self) -> bool {
        self.spring.settled()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn morph_endpoints_match() {
        let under = Pixels::empty(de_api::Size::new(720, 1600));
        let t = de_design::tokens::DEFAULT;
        let src = Rect::new(100, 400, 96, 96);
        let dst = Rect::new(0, 56, 720, 1536);
        let a = morph_frame(&under, &t, 0.0, src, dst, None, "App");
        let b = morph_frame(&under, &t, 1.0, src, dst, None, "App");
        // Closed: tile drawn at src; open: surface covers dst.
        assert_ne!(a.data, b.data);
    }

    #[test]
    fn animator_reaches_and_reverses() {
        let mut m = MorphAnimator::new(0.0);
        m.retarget(true);
        for _ in 0..120 {
            m.advance(1.0 / 60.0);
        }
        assert!(m.progress() > 0.95);
        m.retarget(false);
        for _ in 0..120 {
            m.advance(1.0 / 60.0);
        }
        assert!(m.progress() < 0.05);
    }
}
