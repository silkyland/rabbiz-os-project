//! Spacebar horizontal cursor scrubbing.
//!
//! Dragging a finger horizontally across the spacebar moves the text cursor
//! one character per `step_px` crossed and suppresses the Space character on
//! release. A short tap (movement below `step_px`) does not enter scrubbing
//! mode and commits Space normally.

/// Default horizontal drag distance in pixels per cursor step.
pub const DEFAULT_STEP_PX: f32 = 22.0;

/// Single cursor movement emitted while scrubbing on the spacebar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrubStep {
    Left,
    Right,
}

impl ScrubStep {
    /// Signed character delta (`-1` for [`ScrubStep::Left`], `+1` for [`ScrubStep::Right`]).
    pub const fn delta(self) -> i32 {
        match self {
            ScrubStep::Left => -1,
            ScrubStep::Right => 1,
        }
    }
}

/// Tracks a touch gesture on the spacebar to distinguish taps from cursor scrubs.
#[derive(Debug, Clone)]
pub struct SpacebarScrubber {
    step_px: f32,
    active: bool,
    anchor_x: f32,
    scrubbing: bool,
}

impl Default for SpacebarScrubber {
    fn default() -> Self {
        Self {
            step_px: DEFAULT_STEP_PX,
            active: false,
            anchor_x: 0.0,
            scrubbing: false,
        }
    }
}

impl SpacebarScrubber {
    /// Create a scrubber using [`DEFAULT_STEP_PX`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a scrubber with a custom horizontal step size in pixels.
    /// Falls back to [`DEFAULT_STEP_PX`] if `step_px` is not finite or not positive.
    pub fn with_step_px(step_px: f32) -> Self {
        let step_px = if step_px.is_finite() && step_px > 0.0 {
            step_px
        } else {
            DEFAULT_STEP_PX
        };
        Self {
            step_px,
            ..Self::default()
        }
    }

    /// Configured horizontal step size in pixels.
    pub fn step_px(&self) -> f32 {
        self.step_px
    }

    /// Whether the current touch has crossed the drag threshold and is scrubbing.
    pub fn is_scrubbing(&self) -> bool {
        self.scrubbing
    }

    /// Begin tracking a touch-down on the spacebar at horizontal position `x`.
    pub fn on_down(&mut self, x: f32) {
        self.active = x.is_finite();
        self.anchor_x = x;
        self.scrubbing = false;
    }

    /// Update the horizontal touch position.
    ///
    /// Once drag distance from the current anchor reaches `step_px`, enters
    /// scrubbing mode and emits one [`ScrubStep::Left`] or [`ScrubStep::Right`]
    /// per `step_px` crossed.
    pub fn on_move(&mut self, x: f32) -> Vec<ScrubStep> {
        if !self.active || !x.is_finite() {
            return Vec::new();
        }
        let dx = x - self.anchor_x;
        let count = (dx.abs() / self.step_px).floor() as usize;
        if count == 0 {
            return Vec::new();
        }
        self.scrubbing = true;
        let step = if dx > 0.0 {
            self.anchor_x += count as f32 * self.step_px;
            ScrubStep::Right
        } else {
            self.anchor_x -= count as f32 * self.step_px;
            ScrubStep::Left
        };
        vec![step; count]
    }

    /// Finish the touch gesture.
    ///
    /// Returns `true` if the gesture was a scrub and the normal Space character
    /// MUST be suppressed; returns `false` if it was a tap and Space should be
    /// committed.
    pub fn on_up(&mut self) -> bool {
        let was_scrubbing = self.scrubbing;
        self.active = false;
        self.scrubbing = false;
        was_scrubbing
    }

    /// Reset gesture state (e.g. on `wl_touch.cancel`).
    pub fn reset(&mut self) {
        self.active = false;
        self.scrubbing = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tap_below_threshold_commits_space() {
        let mut s = SpacebarScrubber::new();
        s.on_down(100.0);
        assert!(!s.is_scrubbing());
        assert!(s.on_move(110.0).is_empty());
        assert!(s.on_move(90.0).is_empty());
        assert!(!s.is_scrubbing());
        assert!(!s.on_up(), "tap must not suppress Space");
        assert!(!s.is_scrubbing());
    }

    #[test]
    fn drag_right_emits_steps_and_suppresses_space() {
        let mut s = SpacebarScrubber::new();
        s.on_down(100.0);
        assert!(s.on_move(121.5).is_empty());
        assert!(!s.is_scrubbing());

        assert_eq!(s.on_move(122.0), vec![ScrubStep::Right]);
        assert!(s.is_scrubbing());

        // Remainder preserved: 122 + 22 = 144, 144 + 22 = 166
        assert_eq!(s.on_move(170.0), vec![ScrubStep::Right, ScrubStep::Right]);
        assert!(s.is_scrubbing());
        assert!(s.on_up(), "scrub must suppress Space");
        assert!(!s.is_scrubbing());
    }

    #[test]
    fn drag_left_and_reverse_direction() {
        let mut s = SpacebarScrubber::new();
        s.on_down(200.0);
        assert_eq!(s.on_move(150.0), vec![ScrubStep::Left, ScrubStep::Left]);
        // Anchor is now 156.0; moving right by 22 reaches 178.0.
        assert!(s.on_move(177.0).is_empty());
        assert_eq!(s.on_move(178.0), vec![ScrubStep::Right]);
        assert!(s.is_scrubbing());
        assert!(s.on_up());
    }

    #[test]
    fn returning_to_origin_after_scrubbing_still_suppresses_space() {
        let mut s = SpacebarScrubber::new();
        s.on_down(100.0);
        assert_eq!(s.on_move(122.0), vec![ScrubStep::Right]);
        assert_eq!(s.on_move(100.0), vec![ScrubStep::Left]);
        assert!(s.is_scrubbing());
        assert!(s.on_up());
    }

    #[test]
    fn move_without_down_or_after_up_is_ignored() {
        let mut s = SpacebarScrubber::new();
        assert!(s.on_move(500.0).is_empty());
        assert!(!s.on_up());

        s.on_down(100.0);
        assert_eq!(s.on_move(150.0), vec![ScrubStep::Right, ScrubStep::Right]);
        assert!(s.on_up());
        assert!(s.on_move(300.0).is_empty());
        assert!(!s.on_up());
    }

    #[test]
    fn custom_and_invalid_step_px() {
        let mut s = SpacebarScrubber::with_step_px(10.0);
        assert_eq!(s.step_px(), 10.0);
        s.on_down(0.0);
        assert_eq!(s.on_move(35.0), vec![ScrubStep::Right; 3]);
        assert!(s.on_up());

        for bad in [0.0, -5.0, f32::NAN, f32::INFINITY] {
            assert_eq!(
                SpacebarScrubber::with_step_px(bad).step_px(),
                DEFAULT_STEP_PX
            );
        }
    }

    #[test]
    fn reset_clears_active_scrub() {
        let mut s = SpacebarScrubber::new();
        s.on_down(100.0);
        assert_eq!(s.on_move(130.0), vec![ScrubStep::Right]);
        assert!(s.is_scrubbing());
        s.reset();
        assert!(!s.is_scrubbing());
        assert!(s.on_move(200.0).is_empty());
        assert!(!s.on_up());
    }

    #[test]
    fn step_delta_matches_direction() {
        assert_eq!(ScrubStep::Left.delta(), -1);
        assert_eq!(ScrubStep::Right.delta(), 1);
    }
}
