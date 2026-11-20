//! Unified mobile touch scroll, overscroll, and inertial fling physics.
//!
//! Pure `std`, zero `unsafe`, 100% host-testable. Screen coordinates increase
//! downward (`y`), while scroll `offset` increases as content moves upward
//! (`0.0` at the top, `max_offset()` at the bottom).

/// Minimum pointer travel (px) before a touch gesture becomes a scroll drag.
pub const TOUCH_SLOP: f32 = 12.0;

/// Damping factor applied to drag deltas past `0.0` or `max_offset()`.
pub const OVERSCROLL_RESISTANCE: f32 = 0.35;

/// Minimum elapsed time (ms) between velocity samples to avoid jitter spikes.
pub const MIN_VELOCITY_DT_MS: u64 = 12;

/// Exponential velocity decay rate (1/s) during inertial fling.
pub const FRICTION_DECAY_PER_SEC: f32 = 4.2;

/// Velocity threshold (px/s) below which inertial fling stops.
pub const STOP_VELOCITY: f32 = 25.0;

/// Exponential spring-back rate (1/s) for settling overscroll back to bounds.
const OVERSCROLL_SPRING_RATE: f32 = 14.0;

/// Overscroll distance (px) within which we snap cleanly to the boundary.
const OVERSCROLL_STOP_EPSILON: f32 = 0.5;

/// Maximum fling velocity magnitude (px/s).
const MAX_FLING_VELOCITY: f32 = 6000.0;

/// Maximum pause (ms) before touch-up that still launches an inertial fling.
const VELOCITY_STALE_MS: u64 = 100;

/// Maximum tick delta (s) processed in a single step to guard against stalls.
const MAX_TICK_DT_SECS: f32 = 0.1;

/// Number of recent `(delta_offset, dt_ms)` samples averaged for fling velocity.
const VEL_WINDOW_CAP: usize = 4;

/// Mobile vertical scroll controller with touch slop, rubber-band overscroll,
/// and exponentially damped inertial fling.
#[derive(Debug, Clone)]
pub struct ScrollView {
    offset: f32,
    content_height: f32,
    viewport_height: f32,
    velocity: f32,
    dragging: bool,
    drag_started: bool,
    start_y: f32,
    last_y: f32,
    last_ms: u64,
    vel_anchor_y: f32,
    vel_anchor_ms: u64,
    vel_window: [(f32, u64); VEL_WINDOW_CAP],
    vel_len: usize,
    vel_cursor: usize,
}

impl Default for ScrollView {
    fn default() -> Self {
        Self::new()
    }
}

impl ScrollView {
    /// Creates a new `ScrollView` at offset `0.0` with empty bounds.
    #[must_use]
    pub fn new() -> Self {
        Self {
            offset: 0.0,
            content_height: 0.0,
            viewport_height: 0.0,
            velocity: 0.0,
            dragging: false,
            drag_started: false,
            start_y: 0.0,
            last_y: 0.0,
            last_ms: 0,
            vel_anchor_y: 0.0,
            vel_anchor_ms: 0,
            vel_window: [(0.0, 0); VEL_WINDOW_CAP],
            vel_len: 0,
            vel_cursor: 0,
        }
    }

    /// Updates content and viewport heights and clamps `offset` to the new
    /// bounds when not actively dragging.
    pub fn set_bounds(&mut self, content_height: f32, viewport_height: f32) {
        self.content_height = content_height.max(0.0);
        self.viewport_height = viewport_height.max(0.0);
        if !self.dragging {
            let max = self.max_offset();
            let clamped = self.offset.clamp(0.0, max);
            if clamped != self.offset {
                self.offset = clamped;
                self.velocity = 0.0;
            }
        }
    }

    /// Maximum in-bounds scroll offset (`(content_height - viewport_height).max(0.0)`).
    #[must_use]
    pub fn max_offset(&self) -> f32 {
        (self.content_height - self.viewport_height).max(0.0)
    }

    /// Current scroll offset in logical pixels (may temporarily lie outside
    /// `[0.0, max_offset()]` during overscroll).
    #[must_use]
    pub fn offset(&self) -> f32 {
        self.offset
    }

    /// Current scroll offset rounded to the nearest integer pixel.
    #[must_use]
    pub fn offset_i32(&self) -> i32 {
        self.offset.round() as i32
    }

    /// Begins a touch gesture at screen coordinate `y` and timestamp `now_ms`.
    /// Immediately stops any ongoing inertial fling.
    pub fn on_touch_down(&mut self, y: f32, now_ms: u64) {
        self.velocity = 0.0;
        self.dragging = true;
        self.drag_started = false;
        self.start_y = y;
        self.last_y = y;
        self.last_ms = now_ms;
        self.vel_anchor_y = y;
        self.vel_anchor_ms = now_ms;
        self.vel_window = [(0.0, 0); VEL_WINDOW_CAP];
        self.vel_len = 0;
        self.vel_cursor = 0;
    }

    /// Processes a pointer move to screen coordinate `y` at timestamp `now_ms`.
    ///
    /// Returns `true` once the drag exceeds [`TOUCH_SLOP`]. While dragging
    /// beyond `0.0` or `max_offset()`, deltas are attenuated by
    /// [`OVERSCROLL_RESISTANCE`]. Smoothed velocity is updated only when the
    /// elapsed interval since the last velocity anchor is at least
    /// [`MIN_VELOCITY_DT_MS`].
    pub fn on_touch_move(&mut self, y: f32, now_ms: u64) -> bool {
        if !self.dragging {
            return false;
        }

        if !self.drag_started {
            let travel = y - self.start_y;
            if travel.abs() <= TOUCH_SLOP {
                self.last_ms = now_ms;
                return false;
            }
            self.drag_started = true;
            // Anchor right at the slop threshold so crossing slop does not jump.
            self.last_y = self.start_y + travel.signum() * TOUCH_SLOP;
        }

        let delta_offset = self.last_y - y;
        self.apply_drag_delta(delta_offset);
        self.last_y = y;
        self.last_ms = now_ms;

        let dt_ms = now_ms.saturating_sub(self.vel_anchor_ms);
        if dt_ms >= MIN_VELOCITY_DT_MS {
            let sample_delta = self.vel_anchor_y - y;
            self.record_velocity_sample(sample_delta, dt_ms);
            self.vel_anchor_y = y;
            self.vel_anchor_ms = now_ms;
        }

        true
    }

    /// Ends the active touch gesture at timestamp `now_ms`.
    ///
    /// Returns `true` if the gesture was a scroll drag (exceeded [`TOUCH_SLOP`]),
    /// or `false` if it should be treated as a tap. Launches inertial fling or
    /// overscroll snap-back as appropriate.
    pub fn on_touch_up(&mut self, now_ms: u64) -> bool {
        if !self.dragging {
            return false;
        }

        let was_drag = self.drag_started;
        self.dragging = false;
        self.drag_started = false;

        if !was_drag {
            self.velocity = 0.0;
            return false;
        }

        if now_ms.saturating_sub(self.last_ms) > VELOCITY_STALE_MS
            || self.is_overscrolled()
            || self.velocity.abs() < STOP_VELOCITY
        {
            self.velocity = 0.0;
        }

        true
    }

    /// Advances inertial fling and overscroll snap-back by `dt_secs` seconds.
    ///
    /// Returns `true` while the view is still animating and needs another frame.
    pub fn tick(&mut self, dt_secs: f32) -> bool {
        if self.dragging {
            return false;
        }
        if !dt_secs.is_finite() || dt_secs <= 0.0 {
            return self.is_animating();
        }

        let dt = dt_secs.min(MAX_TICK_DT_SECS);
        let max = self.max_offset();

        if self.is_overscrolled() {
            self.velocity = 0.0;
            let target = if self.offset < 0.0 { 0.0 } else { max };
            let remaining = (self.offset - target) * (-OVERSCROLL_SPRING_RATE * dt).exp();
            if remaining.abs() <= OVERSCROLL_STOP_EPSILON {
                self.offset = target;
            } else {
                self.offset = target + remaining;
            }
            return self.is_animating();
        }

        if self.velocity.abs() >= STOP_VELOCITY {
            let decay = (-FRICTION_DECAY_PER_SEC * dt).exp();
            let displacement = self.velocity * (1.0 - decay) / FRICTION_DECAY_PER_SEC;
            let next = self.offset + displacement;

            if next < 0.0 {
                self.offset = next * OVERSCROLL_RESISTANCE;
                self.velocity = 0.0;
            } else if next > max {
                self.offset = max + (next - max) * OVERSCROLL_RESISTANCE;
                self.velocity = 0.0;
            } else {
                self.offset = next;
                self.velocity *= decay;
                if self.velocity.abs() < STOP_VELOCITY {
                    self.velocity = 0.0;
                }
            }
        } else {
            self.velocity = 0.0;
        }

        self.is_animating()
    }

    /// Returns `true` if inertial fling or overscroll snap-back is active.
    #[must_use]
    pub fn is_animating(&self) -> bool {
        if self.dragging {
            return false;
        }
        self.velocity.abs() >= STOP_VELOCITY || self.is_overscrolled()
    }

    /// Jumps to `y` clamped to `[0.0, max_offset()]` and cancels fling velocity.
    pub fn scroll_to(&mut self, y: f32) {
        self.offset = y.clamp(0.0, self.max_offset());
        self.velocity = 0.0;
    }

    /// Shifts the current offset by `dy`, clamped to `[0.0, max_offset()]`,
    /// and cancels fling velocity.
    pub fn scroll_by(&mut self, dy: f32) {
        self.scroll_to(self.offset + dy);
    }

    fn is_overscrolled(&self) -> bool {
        self.offset < 0.0 || self.offset > self.max_offset()
    }

    fn apply_drag_delta(&mut self, mut delta: f32) {
        let max = self.max_offset();

        // If currently overscrolled above 0.0, consume resistance-scaled travel first.
        if self.offset < 0.0 {
            if delta <= 0.0 {
                self.offset += delta * OVERSCROLL_RESISTANCE;
                return;
            }
            let needed = -self.offset / OVERSCROLL_RESISTANCE;
            if delta <= needed {
                self.offset += delta * OVERSCROLL_RESISTANCE;
                return;
            }
            self.offset = 0.0;
            delta -= needed;
        } else if self.offset > max {
            if delta >= 0.0 {
                self.offset += delta * OVERSCROLL_RESISTANCE;
                return;
            }
            let needed = (self.offset - max) / OVERSCROLL_RESISTANCE;
            if -delta <= needed {
                self.offset += delta * OVERSCROLL_RESISTANCE;
                return;
            }
            self.offset = max;
            delta += needed;
        }

        // Now inside [0.0, max]; apply remaining delta with boundary resistance.
        let next = self.offset + delta;
        if next < 0.0 {
            self.offset = next * OVERSCROLL_RESISTANCE;
        } else if next > max {
            self.offset = max + (next - max) * OVERSCROLL_RESISTANCE;
        } else {
            self.offset = next;
        }
    }

    fn record_velocity_sample(&mut self, delta_offset: f32, dt_ms: u64) {
        self.vel_window[self.vel_cursor] = (delta_offset, dt_ms);
        self.vel_cursor = (self.vel_cursor + 1) % VEL_WINDOW_CAP;
        if self.vel_len < VEL_WINDOW_CAP {
            self.vel_len += 1;
        }

        let mut total_delta = 0.0_f32;
        let mut total_dt_ms = 0_u64;
        for i in 0..self.vel_len {
            let (d, dt) = self.vel_window[i];
            total_delta += d;
            total_dt_ms += dt;
        }

        if total_dt_ms > 0 {
            let v = total_delta / (total_dt_ms as f32 / 1000.0);
            self.velocity = v.clamp(-MAX_FLING_VELOCITY, MAX_FLING_VELOCITY);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx_eq(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() <= eps
    }

    #[test]
    fn tap_below_touch_slop_does_not_scroll() {
        let mut sv = ScrollView::new();
        sv.set_bounds(2000.0, 600.0);

        sv.on_touch_down(300.0, 0);
        assert!(!sv.on_touch_move(294.0, 16));
        assert!(!sv.on_touch_move(288.0, 32)); // exactly TOUCH_SLOP (12.0), not beyond
        assert!(!sv.on_touch_up(48));

        assert_eq!(sv.offset(), 0.0);
        assert_eq!(sv.offset_i32(), 0);
        assert!(!sv.is_animating());
    }

    #[test]
    fn drag_beyond_touch_slop_scrolls_and_reports_drag() {
        let mut sv = ScrollView::new();
        sv.set_bounds(2000.0, 600.0);

        sv.on_touch_down(400.0, 0);
        assert!(!sv.on_touch_move(392.0, 16));
        assert!(sv.on_touch_move(350.0, 32)); // 50px up -> 38px past slop
        assert!(approx_eq(sv.offset(), 38.0, 1e-4));

        assert!(sv.on_touch_move(250.0, 48)); // +100px up
        assert!(approx_eq(sv.offset(), 138.0, 1e-4));
        assert_eq!(sv.offset_i32(), 138);

        assert!(sv.on_touch_up(48));
    }

    #[test]
    fn overscroll_top_applies_resistance_and_snaps_back() {
        let mut sv = ScrollView::new();
        sv.set_bounds(1500.0, 600.0);

        sv.on_touch_down(100.0, 0);
        assert!(sv.on_touch_move(112.0 + 100.0, 30)); // 100px past slop into top overscroll
        assert!(
            approx_eq(sv.offset(), -100.0 * OVERSCROLL_RESISTANCE, 1e-3),
            "expected -35.0, got {}",
            sv.offset()
        );

        assert!(sv.on_touch_up(30));
        assert!(sv.is_animating());

        let mut prev = sv.offset();
        let mut steps = 0;
        while sv.tick(1.0 / 60.0) {
            assert!(
                sv.offset() >= prev,
                "snap-back should move monotonically toward 0.0"
            );
            prev = sv.offset();
            steps += 1;
            assert!(steps < 300, "overscroll snap-back did not settle in time");
        }

        assert_eq!(sv.offset(), 0.0);
        assert!(!sv.is_animating());
    }

    #[test]
    fn overscroll_bottom_applies_resistance_and_snaps_back() {
        let mut sv = ScrollView::new();
        sv.set_bounds(1000.0, 600.0);
        assert_eq!(sv.max_offset(), 400.0);
        sv.scroll_to(400.0);

        sv.on_touch_down(500.0, 0);
        assert!(sv.on_touch_move(500.0 - TOUCH_SLOP - 100.0, 30));
        assert!(
            approx_eq(sv.offset(), 400.0 + 100.0 * OVERSCROLL_RESISTANCE, 1e-3),
            "expected 435.0, got {}",
            sv.offset()
        );

        assert!(sv.on_touch_up(30));
        assert!(sv.is_animating());

        let mut prev = sv.offset();
        let mut steps = 0;
        while sv.tick(1.0 / 60.0) {
            assert!(
                sv.offset() <= prev,
                "bottom snap-back should move monotonically toward max_offset"
            );
            prev = sv.offset();
            steps += 1;
            assert!(
                steps < 300,
                "bottom overscroll snap-back did not settle in time"
            );
        }

        assert_eq!(sv.offset(), 400.0);
        assert!(!sv.is_animating());
    }

    #[test]
    fn fling_decelerates_smoothly_and_stops() {
        let mut sv = ScrollView::new();
        sv.set_bounds(5000.0, 600.0);

        sv.on_touch_down(500.0, 0);
        assert!(sv.on_touch_move(460.0, 20));
        assert!(sv.on_touch_move(410.0, 40));
        assert!(sv.on_touch_move(360.0, 60));
        assert!(sv.on_touch_up(60));

        assert!(sv.is_animating());
        let start_offset = sv.offset();

        let mut prev_offset = start_offset;
        let mut prev_delta = f32::INFINITY;
        let mut steps = 0;
        while sv.tick(1.0 / 60.0) {
            let delta = sv.offset() - prev_offset;
            assert!(delta > 0.0, "fling should continue moving forward");
            assert!(
                delta <= prev_delta + 1e-4,
                "fling step should monotonically decelerate ({delta} > {prev_delta})"
            );
            prev_delta = delta;
            prev_offset = sv.offset();
            steps += 1;
            assert!(steps < 300, "fling did not stop in time");
        }

        assert!(sv.offset() > start_offset + 100.0);
        assert!(!sv.is_animating());
    }

    #[test]
    fn set_bounds_clamps_offset_when_content_shrinks() {
        let mut sv = ScrollView::new();
        sv.set_bounds(2000.0, 500.0);
        sv.scroll_to(1200.0);
        assert_eq!(sv.offset(), 1200.0);

        sv.set_bounds(800.0, 500.0);
        assert_eq!(sv.max_offset(), 300.0);
        assert_eq!(sv.offset(), 300.0);

        sv.set_bounds(300.0, 500.0);
        assert_eq!(sv.max_offset(), 0.0);
        assert_eq!(sv.offset(), 0.0);
    }

    #[test]
    fn velocity_ignores_sub_threshold_dt_and_accumulates_high_rate_events() {
        let mut sv = ScrollView::new();
        sv.set_bounds(4000.0, 600.0);

        // Single move with dt < MIN_VELOCITY_DT_MS produces no fling.
        sv.on_touch_down(500.0, 100);
        assert!(sv.on_touch_move(400.0, 105)); // dt = 5ms < 12ms
        assert!(sv.on_touch_up(105));
        assert!(!sv.is_animating());

        // High-frequency 5ms events accumulate until dt >= 12ms and do fling.
        sv.on_touch_down(500.0, 200);
        assert!(sv.on_touch_move(480.0, 205));
        assert!(sv.on_touch_move(460.0, 210));
        assert!(sv.on_touch_move(440.0, 215)); // 15ms elapsed -> velocity sample recorded
        assert!(sv.on_touch_up(215));
        assert!(sv.is_animating());
    }

    #[test]
    fn touch_down_interrupts_active_fling() {
        let mut sv = ScrollView::new();
        sv.set_bounds(4000.0, 600.0);

        sv.on_touch_down(500.0, 0);
        assert!(sv.on_touch_move(420.0, 20));
        assert!(sv.on_touch_up(20));
        assert!(sv.is_animating());

        sv.on_touch_down(420.0, 36);
        assert!(!sv.is_animating());
        assert!(!sv.on_touch_up(50));
        assert!(!sv.is_animating());
    }

    #[test]
    fn scroll_to_and_scroll_by_clamp_and_cancel_velocity() {
        let mut sv = ScrollView::new();
        sv.set_bounds(1000.0, 400.0);

        sv.on_touch_down(500.0, 0);
        assert!(sv.on_touch_move(400.0, 20));
        assert!(sv.on_touch_up(20));
        assert!(sv.is_animating());

        sv.scroll_to(9999.0);
        assert_eq!(sv.offset(), 600.0);
        assert!(!sv.is_animating());

        sv.scroll_by(-1000.0);
        assert_eq!(sv.offset(), 0.0);

        sv.scroll_by(123.6);
        assert!(approx_eq(sv.offset(), 123.6, 1e-4));
        assert_eq!(sv.offset_i32(), 124);
    }
}
