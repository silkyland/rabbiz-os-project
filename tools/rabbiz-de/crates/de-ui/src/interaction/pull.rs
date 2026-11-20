//! Finger-tracked pull-down sheet (notification panel, quick actions): the
//! sheet follows the finger 1:1 over `REACH` px, and on release a spring
//! carries it to open or closed according to where the *projected* position
//! lands, so a short fast flick opens it and a slow half-pull falls back.
//! Pure state: no pixels, no I/O.

use de_design::motion::{MotionKind, SpringPreset};
use de_paint::animation::{AnimationClock, SpringValue};

/// Finger travel (px) that opens the sheet completely on a 1600 px handset.
pub const REACH: f32 = 900.0;
/// Release projection time (s), as for the pager.
const PROJECT_S: f32 = 0.25;

/// Scale pull travel to the output height (0.5625× height, ≥ 320 px).
#[must_use]
pub fn reach_for(output_h: i32) -> f32 {
    (output_h as f32 * 0.5625).max(320.0)
}

#[derive(Debug, Clone)]
pub struct Pull {
    spring: SpringValue,
    dragging: bool,
    /// Fraction when the finger touched down (0 for an opening pull, 1 for a closing one).
    origin: f32,
    reach: f32,
}

impl Default for Pull {
    fn default() -> Self {
        Self::new()
    }
}

impl Pull {
    #[must_use]
    pub fn new() -> Self {
        Self::with_reach(REACH)
    }

    #[must_use]
    pub fn with_reach(reach: f32) -> Self {
        Self {
            spring: SpringValue::new(0.0, SpringPreset::Smooth, MotionKind::Spatial),
            dragging: false,
            origin: 0.0,
            reach: reach.max(1.0),
        }
    }

    pub fn set_reach(&mut self, reach: f32) {
        self.reach = reach.max(1.0);
    }

    /// 0.0 = closed, 1.0 = fully open.
    #[must_use]
    pub fn frac(&self) -> f32 {
        self.spring.value
    }

    /// Visible at all (being pulled, settling or open).
    #[must_use]
    pub fn shown(&self) -> bool {
        self.dragging || self.spring.value > 0.001 || self.spring.target > 0.5
    }

    /// Settled fully open.
    #[must_use]
    pub fn is_open(&self) -> bool {
        !self.dragging && self.spring.target > 0.5 && self.spring.settled()
    }

    /// Open or heading there (the finger is not on it).
    #[must_use]
    pub fn wants_open(&self) -> bool {
        !self.dragging && self.spring.target > 0.5
    }

    #[must_use]
    pub fn dragging(&self) -> bool {
        self.dragging
    }

    #[must_use]
    pub fn animating(&self) -> bool {
        self.dragging || !self.spring.settled()
    }

    /// Finger down on the sheet's edge (closed, or open and being grabbed).
    pub fn begin(&mut self) {
        self.dragging = true;
        self.origin = self.spring.value;
        self.spring.velocity = 0.0;
        self.spring.target = self.spring.value;
    }

    /// The finger is `dy_px` below (positive) or above where it touched down.
    pub fn drag(&mut self, dy_px: f32) {
        if !self.dragging {
            return;
        }
        let f = (self.origin + dy_px / self.reach).clamp(0.0, 1.0);
        self.spring.value = f;
        self.spring.target = f;
    }

    /// Finger lifted with vertical velocity `vy_px_s` (down is positive).
    /// Returns whether the sheet ends open.
    pub fn release(&mut self, vy_px_s: f32) -> bool {
        if !self.dragging {
            return self.spring.target > 0.5;
        }
        self.dragging = false;
        let v = vy_px_s / self.reach;
        let open = self.spring.value + v * PROJECT_S > 0.5;
        self.spring
            .retarget_with_velocity(if open { 1.0 } else { 0.0 }, v);
        open
    }

    /// Animate open programmatically (IPC, keyboard shortcut).
    pub fn open(&mut self) {
        self.dragging = false;
        self.spring.retarget(1.0);
    }

    /// Animate shut (back gesture, tapping outside, Home).
    pub fn close(&mut self) {
        self.dragging = false;
        self.spring.retarget(0.0);
    }

    /// Jump open or shut without animation (reduced motion, tests).
    pub fn jump(&mut self, open: bool) {
        let v = if open { 1.0 } else { 0.0 };
        self.spring.value = v;
        self.spring.target = v;
        self.spring.velocity = 0.0;
        self.dragging = false;
    }

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

    fn settle(p: &mut Pull) {
        let clock = AnimationClock::new();
        for _ in 0..600 {
            if !p.advance(1.0 / 60.0, &clock) {
                return;
            }
        }
        panic!("did not settle");
    }

    #[test]
    fn follows_the_finger_and_clamps() {
        let mut p = Pull::new();
        p.begin();
        p.drag(450.0);
        assert!((p.frac() - 0.5).abs() < 1e-4);
        p.drag(5000.0);
        assert_eq!(p.frac(), 1.0);
        p.drag(-100.0);
        assert_eq!(p.frac(), 0.0, "never below closed");
        assert!(p.shown());
    }

    #[test]
    fn slow_short_pull_falls_back_closed() {
        let mut p = Pull::new();
        p.begin();
        p.drag(250.0);
        assert!(!p.release(0.0));
        settle(&mut p);
        assert!(!p.shown());
    }

    #[test]
    fn past_half_opens() {
        let mut p = Pull::new();
        p.begin();
        p.drag(520.0);
        assert!(p.release(0.0));
        settle(&mut p);
        assert!(p.is_open());
        assert!((p.frac() - 1.0).abs() < 1e-3);
    }

    #[test]
    fn short_fast_flick_opens() {
        let mut p = Pull::new();
        p.begin();
        p.drag(150.0);
        assert!(p.release(3000.0));
    }

    #[test]
    fn upward_flick_closes_an_open_sheet() {
        let mut p = Pull::new();
        p.jump(true);
        p.begin();
        p.drag(-120.0);
        assert!(!p.release(-2500.0));
        settle(&mut p);
        assert!(!p.shown());
    }

    #[test]
    fn small_upward_drag_keeps_it_open() {
        let mut p = Pull::new();
        p.jump(true);
        p.begin();
        p.drag(-150.0);
        assert!(p.release(0.0));
        settle(&mut p);
        assert!(p.is_open());
    }

    #[test]
    fn close_animates_to_zero() {
        let mut p = Pull::new();
        p.jump(true);
        p.close();
        assert!(p.animating());
        settle(&mut p);
        assert!(!p.shown());
    }

    #[test]
    fn open_animates_to_one() {
        let mut p = Pull::new();
        p.open();
        assert!(p.shown());
        assert!(p.wants_open());
        assert!(p.animating());
        settle(&mut p);
        assert!(p.is_open());
        assert!((p.frac() - 1.0).abs() < 1e-3);
    }
}
