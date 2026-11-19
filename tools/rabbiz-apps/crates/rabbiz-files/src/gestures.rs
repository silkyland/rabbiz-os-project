//! Multi-touch gesture tracker for RabbizOS Files. Pure logic — fed
//! `TouchDown/Move/Up` positions, emits semantic events the app maps to
//! scroll / tap / long-press / edge-back / two-finger-swipe.
//!
//! Design notes (researched against Dolphin's touch MR !7 and M3 gestures):
//! - Long-press is decided at finger-UP: Wayland touch gives no timer events
//!   while a finger rests, so "held still ≥ LONG_PRESS_MS then lifted" is the
//!   only reliable signal without a frame timer in the kit.
//! - Two-finger horizontal swipe navigates back (Dolphin parity). Two-finger
//!   tap toggles selection on the item under the midpoint.
//! - Edge swipe: a touch that starts within `EDGE_PX` of the left edge and
//!   moves right becomes `EdgeBack`; everything else is `Scroll` once past
//!   `TOUCH_SLOP`.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use de_api::Point;

/// Movement threshold before a touch is classified (px).
pub const TOUCH_SLOP: i32 = 14;
/// Hold duration that turns a still press into a long-press (ms). Matches the
/// shell's ~450 ms convention.
pub const LONG_PRESS_MS: u64 = 450;
/// Left-edge band that arms the back-swipe (px).
pub const EDGE_PX: i32 = 30;
/// Horizontal travel (or fling) that commits an edge back-swipe.
pub const EDGE_COMMIT_PX: i32 = 110;
/// px/ms fling velocity that commits an edge back-swipe.
pub const EDGE_COMMIT_VX: f32 = 0.9;
/// Two-finger swipe commit distance.
pub const TWO_FINGER_COMMIT_PX: i32 = 80;
/// Two fingers lifting within this window = two-finger tap.
pub const TWO_FINGER_TAP_MS: u64 = 350;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Undecided,
    Scroll,
    EdgeBack,
    /// Two or more fingers down — horizontal avg displacement drives nav.
    TwoFinger,
}

#[derive(Debug, Clone, Copy)]
struct Sample {
    t: Instant,
    pos: Point,
}

#[derive(Debug, Clone)]
struct Touch {
    down: Point,
    prev: Point,
    mode: Mode,
    start: Instant,
    /// Recent move samples for release velocity (oldest→newest, ≤8).
    samples: Vec<Sample>,
}

/// Semantic events emitted by [`Tracker`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GestureEvent {
    /// Finger moved while in scroll mode; `dy` is this move's delta.
    ScrollDelta { id: i32, dy: i32 },
    /// Edge back-swipe in progress; total horizontal displacement.
    EdgeDrag { dx: i32 },
    /// Quick tap (moved < slop, held < LONG_PRESS_MS).
    Tap { pos: Point },
    /// Held still ≥ LONG_PRESS_MS and lifted — enter selection.
    LongPress { pos: Point },
    /// Scroll-mode release with vertical velocity px/ms (positive = down).
    ScrollFlick { vy: f32 },
    /// Edge back-swipe released: total dx and px/ms horizontal velocity.
    EdgeRelease { dx: i32, vx: f32 },
    /// All fingers lifted after a two-finger horizontal drag.
    TwoFingerRelease { dx: i32 },
    /// Two fingers tapped quickly without moving.
    TwoFingerTap { pos: Point },
}

#[derive(Debug, Default)]
pub struct Tracker {
    touches: HashMap<i32, Touch>,
    /// Latest average horizontal displacement across all touches (two-finger
    /// swipe progress), updated on every TwoFinger move.
    tf_dx: i32,
}

fn dist(a: Point, b: Point) -> i32 {
    let dx = (a.x - b.x).abs();
    let dy = (a.y - b.y).abs();
    dx.max(dy)
}

/// Velocity (px/ms) over the sample window ending at `up`, axis selected by
/// `horizontal`. Returns 0 with <2 samples.
fn velocity(t: &Touch, horizontal: bool) -> f32 {
    if t.samples.len() < 2 {
        return 0.0;
    }
    let first = t.samples.first().unwrap();
    let last = t.samples.last().unwrap();
    let dt = last.t.duration_since(first.t).as_secs_f32() * 1000.0;
    if dt <= 0.5 {
        return 0.0;
    }
    let d = if horizontal {
        (last.pos.x - first.pos.x) as f32
    } else {
        (last.pos.y - first.pos.y) as f32
    };
    (d / dt).clamp(-4.0, 4.0)
}

impl Tracker {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Finger down. Returns events (a second finger arriving may convert the
    /// primary gesture to two-finger mode — no event for that; the app reads
    /// `mode` state via subsequent move/up outcomes).
    pub fn down(&mut self, id: i32, pos: Point, now: Instant) -> Vec<GestureEvent> {
        // A second simultaneous finger turns the gesture two-finger — for the
        // existing touches AND the newcomer (whose mode would otherwise stay
        // Undecided and misreport a tap on release).
        let multi = !self.touches.is_empty();
        for t in self.touches.values_mut() {
            if t.mode == Mode::Undecided || t.mode == Mode::Scroll || t.mode == Mode::EdgeBack {
                t.mode = Mode::TwoFinger;
            }
        }
        self.touches.insert(
            id,
            Touch {
                down: pos,
                prev: pos,
                mode: if multi {
                    Mode::TwoFinger
                } else {
                    Mode::Undecided
                },
                start: now,
                samples: vec![Sample { t: now, pos }],
            },
        );
        Vec::new()
    }

    /// Finger moved. Returns scroll deltas / edge drags.
    pub fn move_to(&mut self, id: i32, pos: Point, now: Instant) -> Vec<GestureEvent> {
        let mut ev = Vec::new();
        let multi = self.touches.len() >= 2;
        let Some(t) = self.touches.get_mut(&id) else {
            return ev;
        };
        if t.mode == Mode::Undecided {
            let dx = pos.x - t.down.x;
            let dy = pos.y - t.down.y;
            if dx.abs().max(dy.abs()) > TOUCH_SLOP {
                t.mode = if t.down.x <= EDGE_PX && dx > 0 && dx > dy.abs() * 6 / 5 {
                    Mode::EdgeBack
                } else if multi {
                    Mode::TwoFinger
                } else {
                    Mode::Scroll
                };
            }
        }
        match t.mode {
            Mode::Scroll => {
                ev.push(GestureEvent::ScrollDelta {
                    id,
                    dy: pos.y - t.prev.y,
                });
            }
            Mode::EdgeBack => {
                let dx = (pos.x - t.down.x).max(0);
                ev.push(GestureEvent::EdgeDrag { dx });
            }
            Mode::TwoFinger => {}
            _ => {}
        }
        // Keep a short velocity window.
        t.samples.push(Sample { t: now, pos });
        if t.samples.len() > 8 {
            t.samples.remove(0);
        }
        t.prev = pos;
        let mode = t.mode;
        // (borrow of `t` ends here)
        if mode == Mode::TwoFinger {
            // Average horizontal displacement across all active touches — a
            // real two-finger swipe moves both fingers together.
            let mut sum = 0i64;
            let mut n = 0i64;
            for o in self.touches.values() {
                sum += (o.prev.x - o.down.x) as i64;
                n += 1;
            }
            if n > 0 {
                self.tf_dx = (sum / n) as i32;
            }
        }
        ev
    }

    /// Finger up. Returns the terminal gesture event (if any).
    pub fn up(&mut self, id: i32, pos: Point, now: Instant) -> Vec<GestureEvent> {
        let Some(t) = self.touches.remove(&id) else {
            return Vec::new();
        };
        let mut ev = Vec::new();
        let held = now.duration_since(t.start);
        let still = dist(pos, t.down) <= TOUCH_SLOP;

        match t.mode {
            Mode::Undecided => {
                if still {
                    if held >= Duration::from_millis(LONG_PRESS_MS) {
                        ev.push(GestureEvent::LongPress { pos: t.down });
                    } else {
                        ev.push(GestureEvent::Tap { pos: t.down });
                    }
                }
                // moved-but-never-classified: ignore
            }
            Mode::Scroll => {
                ev.push(GestureEvent::ScrollFlick {
                    vy: velocity(&t, false),
                });
            }
            Mode::EdgeBack => {
                let dx = (pos.x - t.down.x).max(0);
                ev.push(GestureEvent::EdgeRelease {
                    dx,
                    vx: velocity(&t, true),
                });
            }
            Mode::TwoFinger => {
                if self.touches.is_empty() {
                    // All fingers up: decide commit vs tap. `tf_dx` is the
                    // average horizontal travel tracked during the moves.
                    let dx = self.tf_dx;
                    self.tf_dx = 0;
                    let quick = held <= Duration::from_millis(TWO_FINGER_TAP_MS) && still;
                    if quick {
                        ev.push(GestureEvent::TwoFingerTap { pos: t.down });
                    } else {
                        ev.push(GestureEvent::TwoFingerRelease { dx });
                    }
                }
            }
        }
        ev
    }
}

/// Decide whether an edge release commits back-navigation.
#[must_use]
pub fn edge_commits(dx: i32, vx: f32) -> bool {
    dx >= EDGE_COMMIT_PX || (dx > TOUCH_SLOP * 2 && vx >= EDGE_COMMIT_VX)
}

/// Two-finger swipe direction (Dolphin parity): leftward swipe = back,
/// rightward = forward. Returns -1 / +1 / 0.
#[must_use]
pub fn two_finger_dir(dx: i32) -> i32 {
    if dx <= -TWO_FINGER_COMMIT_PX {
        -1
    } else if dx >= TWO_FINGER_COMMIT_PX {
        1
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t0() -> Instant {
        Instant::now()
    }

    fn p(x: i32, y: i32) -> Point {
        Point::new(x, y)
    }

    #[test]
    fn tap_vs_long_press() {
        let mut tr = Tracker::new();
        let start = t0();
        tr.down(1, p(100, 100), start);
        let ev = tr.up(1, p(101, 100), start + Duration::from_millis(100));
        assert_eq!(ev, vec![GestureEvent::Tap { pos: p(100, 100) }]);

        let mut tr = Tracker::new();
        tr.down(1, p(100, 100), start);
        let ev = tr.up(1, p(100, 100), start + Duration::from_millis(600));
        assert_eq!(ev, vec![GestureEvent::LongPress { pos: p(100, 100) }]);
    }

    #[test]
    fn move_too_far_is_not_tap_or_longpress() {
        let mut tr = Tracker::new();
        let start = t0();
        tr.down(1, p(100, 100), start);
        let evs = tr.move_to(1, p(100, 160), start + Duration::from_millis(10));
        assert!(evs
            .iter()
            .any(|e| matches!(e, GestureEvent::ScrollDelta { .. })));
        let ev = tr.up(1, p(100, 160), start + Duration::from_millis(600));
        assert!(matches!(ev[0], GestureEvent::ScrollFlick { .. }));
    }

    #[test]
    fn edge_swipe_classification() {
        let mut tr = Tracker::new();
        let start = t0();
        tr.down(1, p(10, 400), start);
        let evs = tr.move_to(1, p(140, 405), start + Duration::from_millis(16));
        assert!(evs
            .iter()
            .any(|e| matches!(e, GestureEvent::EdgeDrag { dx: 130 })));
        let ev = tr.up(1, p(150, 405), start + Duration::from_millis(120));
        assert!(matches!(ev[0], GestureEvent::EdgeRelease { dx: 140, .. }));
        assert!(edge_commits(140, 0.0));
        assert!(!edge_commits(40, 0.0));
        assert!(edge_commits(40, 1.5)); // fling
    }

    #[test]
    fn non_edge_horizontal_is_scroll_not_edge() {
        let mut tr = Tracker::new();
        let start = t0();
        tr.down(1, p(200, 400), start);
        let evs = tr.move_to(1, p(340, 405), start + Duration::from_millis(16));
        // Horizontal-ish move away from edge → Scroll mode (dy small).
        assert!(evs
            .iter()
            .all(|e| matches!(e, GestureEvent::ScrollDelta { .. })));
    }

    #[test]
    fn scroll_delta_accumulates() {
        let mut tr = Tracker::new();
        let start = t0();
        tr.down(1, p(360, 400), start);
        let mut dy = 0;
        for (i, y) in [(16u64, 430), (32, 460)] {
            for e in tr.move_to(1, p(360, y), start + Duration::from_millis(i)) {
                if let GestureEvent::ScrollDelta { dy: d, .. } = e {
                    dy += d;
                }
            }
        }
        assert_eq!(dy, 60);
    }

    #[test]
    fn two_finger_swipe() {
        let mut tr = Tracker::new();
        let start = t0();
        tr.down(1, p(300, 400), start);
        tr.down(2, p(340, 420), start + Duration::from_millis(5));
        tr.move_to(1, p(200, 400), start + Duration::from_millis(30));
        tr.move_to(2, p(240, 420), start + Duration::from_millis(30));
        let ev = tr.up(1, p(200, 400), start + Duration::from_millis(60));
        assert!(ev.is_empty()); // one finger still down
        let ev = tr.up(2, p(240, 420), start + Duration::from_millis(80));
        assert_eq!(ev, vec![GestureEvent::TwoFingerRelease { dx: -100 }]);
        assert_eq!(two_finger_dir(-100), -1);
        assert_eq!(two_finger_dir(100), 1);
        assert_eq!(two_finger_dir(-30), 0);
    }

    #[test]
    fn two_finger_tap() {
        let mut tr = Tracker::new();
        let start = t0();
        tr.down(1, p(300, 400), start);
        tr.down(2, p(340, 420), start + Duration::from_millis(5));
        tr.up(1, p(301, 400), start + Duration::from_millis(80));
        let ev = tr.up(2, p(341, 420), start + Duration::from_millis(120));
        assert_eq!(ev, vec![GestureEvent::TwoFingerTap { pos: p(340, 420) }]);
    }

    #[test]
    fn flick_velocity_detected() {
        let mut tr = Tracker::new();
        let start = t0();
        tr.down(1, p(360, 800), start);
        // Simulate a fast upward fling: moves every ~8ms.
        for i in 1..=8u64 {
            tr.move_to(
                1,
                p(360, 800 - (i * 30) as i32),
                start + Duration::from_millis(i * 8),
            );
        }
        let ev = tr.up(1, p(360, 560), start + Duration::from_millis(80));
        match ev[0] {
            GestureEvent::ScrollFlick { vy } => {
                // 240px over ~64ms ≈ 3.75 px/ms, clamped to 4.
                assert!(vy <= -2.0, "vy={vy}");
            }
            other => panic!("expected ScrollFlick, got {other:?}"),
        }
    }
}
