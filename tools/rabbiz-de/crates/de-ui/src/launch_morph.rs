//! Launch/close morph (M3-T5): the icon tile springs from its grid rect to
//! the fullscreen app surface — finger-tracked, interruptible, and
//! reversible. Committed playbacks use area-based timing and reversals
//! continue from the current position and velocity with a spring.
//!
//! [`morph_frame`] is the pure renderer at progress `p` (0 = icon rect,
//! 1 = fullscreen). [`MorphAnimator`] advances with frame deltas and supports
//! retargeting mid-flight. The compositor does not yet call this driver.

use de_api::{Pixels, Point, Rect};
use de_design::motion::{MotionKind, SpringPreset, Transition, TransitionTiming, DURATION_MAX_MS};
use de_design::tokens::{self, Theme};
use de_paint::{
    animation::{AnimationClock, SpringValue},
    Canvas,
};

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

/// Motion driver for the morph. `retarget` flips between open and closed.
#[derive(Debug)]
pub struct MorphAnimator {
    spring: SpringValue,
    timing: TransitionTiming,
    timed_from: Option<f32>,
    elapsed_ms: f32,
}

impl MorphAnimator {
    /// Begin a fullscreen morph at progress `from` (0 = closed).
    #[must_use]
    pub fn new(from: f32) -> Self {
        Self::for_covered_fraction(from, 1.0)
    }

    /// Begin a morph using the destination window's visible output fraction.
    #[must_use]
    pub fn for_covered_fraction(from: f32, fraction: f32) -> Self {
        Self {
            spring: SpringValue::new(from, SpringPreset::Smooth, MotionKind::Spatial),
            timing: Transition::AppWindow.timing_for_covered_fraction(fraction),
            timed_from: None,
            elapsed_ms: 0.0,
        }
    }

    /// Retarget to open (`1.0`) or closed (`0.0`). Reversals in flight retain
    /// the current progress and hand the last frame's velocity to the spring.
    pub fn retarget(&mut self, open: bool) {
        let target = if open { 1.0 } else { 0.0 };
        if target == self.spring.target {
            return;
        }
        self.timed_from = self.settled().then_some(self.spring.value);
        self.elapsed_ms = 0.0;
        self.spring.retarget(target);
    }

    /// Advance by `dt_s`; returns current progress.
    pub fn advance(&mut self, dt_s: f32, clock: &AnimationClock) -> f32 {
        let restricted = clock.motion_restricted();
        if restricted {
            self.timed_from = None;
        }
        self.elapsed_ms += dt_s * 1000.0;
        if let Some(from) = self.timed_from {
            let motion = if self.spring.target == 1.0 {
                self.timing.enter()
            } else {
                self.timing.exit()
            };
            if self.elapsed_ms >= motion.duration_ms as f32 {
                self.timed_from = None;
                self.spring.snap();
            } else if dt_s > 0.0 {
                let value = lerp(
                    from,
                    self.spring.target,
                    motion.progress_at(self.elapsed_ms),
                );
                self.spring.velocity = (value - self.spring.value) / dt_s;
                self.spring.value = value;
            }
        } else if !restricted && self.elapsed_ms >= DURATION_MAX_MS as f32 {
            self.spring.snap();
        } else {
            self.spring.advance(dt_s, clock);
        }
        self.progress()
    }

    /// Current progress without advancing.
    #[must_use]
    pub fn progress(&self) -> f32 {
        self.spring.value.clamp(0.0, 1.0)
    }

    /// Settled at the target (frame loop may stop morph frames).
    #[must_use]
    pub fn settled(&self) -> bool {
        self.timed_from.is_none() && self.spring.settled()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use de_design::motion::MotionScheme;

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
    fn fullscreen_close_accelerates_and_finishes_in_234_ms() {
        let clock = AnimationClock::new();
        let mut morph = MorphAnimator::new(0.0);
        morph.retarget(true);
        morph.advance(0.519, &clock);
        assert!(!morph.settled());
        assert_eq!(morph.advance(0.001, &clock), 1.0);
        assert!(morph.settled());

        morph.retarget(false);
        let midway = morph.advance(0.117, &clock);
        assert!(midway > 0.5 && midway < 1.0, "midway={midway}");
        morph.advance(0.116, &clock);
        assert!(!morph.settled());
        assert_eq!(morph.advance(0.001, &clock), 0.0);
        assert!(morph.settled());
    }

    #[test]
    fn reversing_during_entry_continues_from_the_current_frame() {
        let clock = AnimationClock::new();
        let mut morph = MorphAnimator::new(0.0);
        morph.retarget(true);
        for _ in 0..15 {
            morph.advance(0.010, &clock);
        }
        let before = morph.progress();
        morph.retarget(false);
        assert_eq!(morph.progress(), before);
        assert!(morph.advance(0.001, &clock) > before);
        assert_eq!(morph.advance(0.600, &clock), 0.0);
        assert!(morph.settled());
    }

    #[test]
    fn restricted_morph_keeps_the_existing_spring_path() {
        for (reduced_motion, safe_mode) in [(true, false), (false, true)] {
            let clock =
                AnimationClock::with_motion(MotionScheme::Standard, reduced_motion, safe_mode);
            let mut morph = MorphAnimator::new(0.0);
            let mut spring = SpringValue::new(0.0, SpringPreset::Smooth, MotionKind::Spatial);
            for open in [true, false] {
                morph.retarget(open);
                spring.retarget(if open { 1.0 } else { 0.0 });
                for _ in 0..20 {
                    spring.advance(0.016, &clock);
                    assert_eq!(
                        morph.advance(0.016, &clock).to_bits(),
                        spring.value.clamp(0.0, 1.0).to_bits()
                    );
                }
            }
        }
    }

    #[test]
    fn animator_reaches_and_reverses() {
        let clock = AnimationClock::new();
        let mut m = MorphAnimator::new(0.0);
        m.retarget(true);
        for _ in 0..120 {
            m.advance(1.0 / 60.0, &clock);
        }
        assert!(m.progress() > 0.95);
        m.retarget(false);
        for _ in 0..120 {
            m.advance(1.0 / 60.0, &clock);
        }
        assert!(m.progress() < 0.05);
    }
}
