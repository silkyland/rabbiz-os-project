//! Finger-tracked pager (Material 3 "lateral" pattern): the content follows
//! the finger 1:1, rubber-bands past the first/last page, and on release a
//! spring carries it to the page the *projected* position lands on, keeping
//! the finger's velocity. Pure state: no pixels, no I/O.
//!
//! Position is measured in pages (`1.0` = one page width), so the same
//! pager serves any output width.

use de_design::motion::{MotionKind, SpringPreset, RUBBER_FACTOR};
use de_paint::animation::{AnimationClock, SpringValue};

/// How far ahead (s) a release is projected: matches `SpringValue::projected_release`.
const PROJECT_S: f32 = 0.25;

#[derive(Debug, Clone)]
pub struct Pager {
    pages: usize,
    spring: SpringValue,
    /// Page the current drag started from, and the position at touch-down.
    origin_page: usize,
    origin_pos: f32,
    dragging: bool,
}

impl Pager {
    #[must_use]
    pub fn new(pages: usize) -> Self {
        Self {
            pages: pages.max(1),
            spring: SpringValue::new(0.0, SpringPreset::Smooth, MotionKind::Spatial),
            origin_page: 0,
            origin_pos: 0.0,
            dragging: false,
        }
    }

    /// Number of pages changed (apps added/removed); clamps the position.
    pub fn set_pages(&mut self, pages: usize) {
        self.pages = pages.max(1);
        let max = self.max_pos();
        if self.spring.target > max {
            self.spring.target = max;
        }
        if !self.dragging && self.spring.value > max {
            self.spring.value = max;
        }
    }

    /// Jump (no animation) to `page`.
    pub fn jump_to(&mut self, page: usize) {
        let p = page.min(self.pages - 1) as f32;
        self.spring.value = p;
        self.spring.target = p;
        self.spring.velocity = 0.0;
        self.dragging = false;
    }

    fn max_pos(&self) -> f32 {
        (self.pages - 1) as f32
    }

    /// Current (fractional) position in pages.
    #[must_use]
    pub fn pos(&self) -> f32 {
        self.spring.value
    }

    /// Page nearest to the current position.
    #[must_use]
    pub fn page(&self) -> usize {
        (self.spring.value.round().max(0.0) as usize).min(self.pages - 1)
    }

    /// Page the pager is heading to (equals `page()` at rest).
    #[must_use]
    pub fn target_page(&self) -> usize {
        (self.spring.target.round().max(0.0) as usize).min(self.pages - 1)
    }

    #[must_use]
    pub fn dragging(&self) -> bool {
        self.dragging
    }

    /// True while a drag or a settle is in progress (needs frames).
    #[must_use]
    pub fn animating(&self) -> bool {
        self.dragging || !self.spring.settled()
    }

    /// Finger down: stop any settle and grab the content where it is.
    pub fn begin(&mut self) {
        self.dragging = true;
        self.origin_page = self.page();
        self.origin_pos = self.spring.value;
        self.spring.velocity = 0.0;
        self.spring.target = self.spring.value;
    }

    /// Finger moved `dx_px` from where it touched down on a page `width_px` wide.
    pub fn drag(&mut self, dx_px: f32, width_px: f32) {
        if !self.dragging || width_px <= 0.0 {
            return;
        }
        let raw = self.origin_pos - dx_px / width_px;
        let max = self.max_pos();
        let pos = if raw < 0.0 {
            raw * RUBBER_FACTOR
        } else if raw > max {
            max + (raw - max) * RUBBER_FACTOR
        } else {
            raw
        };
        self.spring.value = pos;
        self.spring.target = pos;
    }

    /// Finger lifted with horizontal velocity `vx_px_s` (px/s, negative = left).
    /// Returns the page it will settle on.
    pub fn release(&mut self, vx_px_s: f32, width_px: f32) -> usize {
        if !self.dragging {
            return self.target_page();
        }
        self.dragging = false;
        let v_pages = if width_px > 0.0 {
            -vx_px_s / width_px
        } else {
            0.0
        };
        let projected = self.spring.value + v_pages * PROJECT_S;
        let lo = self.origin_page.saturating_sub(1);
        let hi = (self.origin_page + 1).min(self.pages - 1);
        let target = (projected.round().max(0.0) as usize).clamp(lo, hi);
        self.spring.retarget_with_velocity(target as f32, v_pages);
        target
    }

    /// Advance the settle spring; returns `true` while it still moves.
    pub fn advance(&mut self, dt_s: f32, clock: &AnimationClock) -> bool {
        if self.dragging {
            return true;
        }
        self.spring.advance(dt_s, clock)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: f32 = 720.0;

    fn settle(p: &mut Pager) {
        let clock = AnimationClock::new();
        for _ in 0..600 {
            if !p.advance(1.0 / 60.0, &clock) {
                return;
            }
        }
        panic!("did not settle");
    }

    #[test]
    fn follows_the_finger_one_to_one() {
        let mut p = Pager::new(3);
        p.begin();
        p.drag(-360.0, W); // finger left half a page
        assert!((p.pos() - 0.5).abs() < 1e-4);
        assert!(p.dragging() && p.animating());
    }

    #[test]
    fn rubber_bands_past_the_first_page() {
        let mut p = Pager::new(3);
        p.begin();
        p.drag(200.0, W); // pull right on page 0
        let raw = -200.0 / W;
        assert!(p.pos() < 0.0 && p.pos() > raw, "damped, not 1:1");
        assert!((p.pos() - raw * RUBBER_FACTOR).abs() < 1e-4);
        assert_eq!(p.release(0.0, W), 0);
        settle(&mut p);
        assert!(p.pos().abs() < 1e-3);
    }

    #[test]
    fn slow_short_drag_snaps_back() {
        let mut p = Pager::new(3);
        p.begin();
        p.drag(-100.0, W);
        assert_eq!(p.release(0.0, W), 0);
        settle(&mut p);
        assert_eq!(p.page(), 0);
    }

    #[test]
    fn past_half_commits_to_next() {
        let mut p = Pager::new(3);
        p.begin();
        p.drag(-400.0, W);
        assert_eq!(p.release(0.0, W), 1);
        settle(&mut p);
        assert!((p.pos() - 1.0).abs() < 1e-3);
    }

    #[test]
    fn short_fast_flick_commits_by_velocity() {
        let mut p = Pager::new(3);
        p.begin();
        p.drag(-90.0, W);
        assert_eq!(p.release(-2500.0, W), 1);
    }

    #[test]
    fn never_skips_more_than_one_page() {
        let mut p = Pager::new(5);
        p.begin();
        p.drag(-300.0, W);
        assert_eq!(p.release(-20000.0, W), 1);
    }

    #[test]
    fn last_page_flick_stays_put() {
        let mut p = Pager::new(2);
        p.jump_to(1);
        p.begin();
        p.drag(-60.0, W);
        assert_eq!(p.release(-3000.0, W), 1);
    }

    #[test]
    fn grabbing_mid_settle_continues_from_there() {
        let mut p = Pager::new(3);
        p.begin();
        p.drag(-500.0, W);
        p.release(0.0, W);
        let clock = AnimationClock::new();
        p.advance(0.05, &clock);
        let mid = p.pos();
        assert!(mid > 0.0 && mid < 1.0);
        p.begin();
        assert!((p.pos() - mid).abs() < 1e-6);
        p.drag(0.0, W);
        assert!((p.pos() - mid).abs() < 1e-6);
    }

    #[test]
    fn shrinking_pages_clamps() {
        let mut p = Pager::new(3);
        p.jump_to(2);
        p.set_pages(2);
        assert_eq!(p.page(), 1);
    }
}
