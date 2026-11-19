//! Launch/close morph (M3-T5): the icon tile springs from its grid rect to
//! the fullscreen app surface — finger-tracked, interruptible, and
//! reversible. Committed playbacks use area-based timing and reversals
//! continue from the current position and velocity with a spring.
//!
//! [`morph_frame`] is the pure renderer at progress `p` (0 = icon rect,
//! 1 = fullscreen). [`MorphAnimator`] advances with frame deltas and supports
//! retargeting mid-flight.

use de_api::{Pixels, Point, Rect};
use de_design::motion::{MotionKind, SpringPreset, Transition, TransitionTiming, DURATION_MAX_MS};
use de_design::tokens::Theme;
use de_paint::{
    animation::{AnimationClock, SpringValue},
    Canvas,
};

use crate::chrome::{self, IconEntry};
use crate::transitions::{fade_in, lerp, lerp_rect, with_opacity};

/// Geometry shared by the compositor and the host renderer.
#[derive(Debug, Clone, Copy)]
pub struct MorphPose {
    pub card: Rect,
    pub icon: Rect,
    pub radius: f32,
    pub card_alpha: f32,
    pub icon_alpha: f32,
    pub app_alpha: f32,
}

impl MorphPose {
    /// `content_ready` ramps in after the first committed app buffer.
    #[must_use]
    pub fn at(progress: f32, from: Rect, to: Rect, content_ready: f32) -> Self {
        let card = lerp_rect(from, to, progress);
        let ready = content_ready.clamp(0.0, 1.0);
        // M3 container transform: outgoing icon fades out in the first 35%,
        // incoming app content fades in over 30%..100% (gated by first buffer),
        // and corner radius morphs from tile radius to 0.
        let app_alpha = fade_in(progress, 0.30, 1.0).min(ready);
        let icon_alpha = (1.0 - fade_in(progress, 0.0, 0.35)).max(1.0 - ready);
        Self {
            card,
            icon: Rect::new(
                card.x + (card.w - from.w) / 2,
                card.y + (card.h - from.h) / 2,
                from.w,
                from.h,
            ),
            radius: lerp(de_design::tokens::RADIUS_CARD, 0.0, progress),
            card_alpha: fade_in(progress, 0.0, 0.12),
            icon_alpha,
            app_alpha,
        }
    }
}

/// Shared navigation trajectory: fullscreen → overview card → home icon.
/// Piecewise geometry keeps the overview endpoint identical to Recents.
#[must_use]
pub fn navigation_card(progress: f32, icon: Rect, output: Rect) -> Rect {
    let overview = crate::recents::card_rect(output.size());
    let p = progress.clamp(0.0, 1.0);
    if p >= 0.5 {
        lerp_rect(overview, output, (p - 0.5) * 2.0)
    } else {
        lerp_rect(icon, overview, p * 2.0)
    }
}

/// Rasterize the source tile once so the compositor only moves its texture.
#[must_use]
pub fn morph_icon(t: &Theme, entry: &IconEntry, bounds: Rect) -> Pixels {
    let mut canvas = Canvas::new(bounds.size());
    chrome::draw_app_icon(
        &mut canvas,
        t,
        entry,
        Rect::from_origin_size(Point::ZERO, bounds.size()),
    );
    canvas.into_pixels()
}

/// Paint the compositor's morph layers onto a host canvas for frame goldens.
#[must_use]
pub fn morph_frame(
    under: &Pixels,
    t: &Theme,
    pose: MorphPose,
    icon: &Pixels,
    app: &Pixels,
) -> Pixels {
    let mut c = Canvas::new(under.size);
    c.blit(under, Point::ZERO);
    c.fill_rounded_rect(
        pose.card,
        pose.radius,
        with_opacity(t.surface1, pose.card_alpha),
    );
    c.blit_scaled(app, pose.card, pose.app_alpha);
    c.blit_scaled(icon, pose.icon, pose.icon_alpha);
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
        if (target - self.spring.target).abs() < f32::EPSILON {
            return;
        }
        self.timed_from = self.settled().then_some(self.spring.value);
        self.elapsed_ms = 0.0;
        self.spring.retarget(target);
    }

    /// Settle a navigation drag into the overview card while preserving
    /// the position and velocity sampled under the finger.
    pub fn settle_recents(&mut self) {
        self.timed_from = None;
        self.elapsed_ms = 0.0;
        self.spring.retarget(0.5);
    }

    /// Spring toward `progress` from the current value and velocity, without
    /// a jump: used to rejoin the finger after the Recents preview.
    pub fn chase(&mut self, progress: f32) {
        self.timed_from = None;
        self.elapsed_ms = 0.0;
        self.spring.retarget(progress.clamp(0.0, 1.0));
    }

    /// Put the container under the user's finger. A later retarget starts
    /// its interruption spring with the gesture's last sampled velocity.
    pub fn track(&mut self, progress: f32, velocity: f32) {
        self.timed_from = None;
        self.elapsed_ms = 0.0;
        self.spring.value = progress.clamp(0.0, 1.0);
        self.spring
            .retarget_with_velocity(self.spring.value, velocity);
    }

    /// Advance by `dt_s`; returns current progress.
    pub fn advance(&mut self, dt_s: f32, clock: &AnimationClock) -> f32 {
        let restricted = clock.motion_restricted();
        if restricted {
            self.timed_from = None;
        }
        self.elapsed_ms += dt_s * 1000.0;
        if let Some(from) = self.timed_from {
            let timing = self.timing.for_form_factor(clock.form_factor());
            let motion = if self.spring.target > 0.5 {
                timing.enter()
            } else {
                timing.exit()
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
        let icon = morph_icon(&t, &IconEntry::new("app", "App"), src);
        let app = Canvas::filled(dst.size(), t.accent).into_pixels();
        let a = morph_frame(&under, &t, MorphPose::at(0.0, src, dst, 1.0), &icon, &app);
        let b = morph_frame(&under, &t, MorphPose::at(1.0, src, dst, 1.0), &icon, &app);
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
