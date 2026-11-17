//! Shell input policy: hit-testing, the home gesture recognizer, the focus
//! tracker and the power-key decision. Pure value logic — no smithay, no I/O —
//! so the same rules drive the seat wiring in `seat.rs` and can be unit
//! tested in the arm64 container (`cargo test -p de-core`).

/// Bottom gesture-bar height in physical px (P13: nav bar 32 px overlays the app).
pub const NAV_BAR_HEIGHT: f64 = 32.0;
/// Net upward travel, in px, a bar-origin swipe needs to fire `GoHome`.
pub const HOME_SWIPE_TRAVEL: f64 = 48.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OutputPoint {
    pub x: f64,
    pub y: f64,
}

/// Where a touch landing at `point` is routed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShellTarget {
    /// Forwarded to the focused client surface.
    AppSurface,
    /// Consumed by the compositor (gesture bar owns the touch).
    GestureBar,
}

/// Points inside the bottom `NAV_BAR_HEIGHT` strip belong to the shell.
pub fn hit_test(point: OutputPoint, output_height: f64) -> ShellTarget {
    if point.y >= output_height - NAV_BAR_HEIGHT {
        ShellTarget::GestureBar
    } else {
        ShellTarget::AppSurface
    }
}

/// What the compositor does in response to a finished input sequence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShellAction {
    /// Send `close` to the top toplevel (swipe-up home with an app mapped).
    GoHome,
    /// Release of XF86PowerOff: spawn `a21s-power-key toggle` detached.
    TogglePower,
}

/// Single-slot swipe-up-from-the-bar recognizer.
///
/// Claim: `begin` inside the bar zone. Fire: `end` after the finger travelled
/// at least `HOME_SWIPE_TRAVEL` px upward from where it landed. Any other slot
/// is ignored so a second finger never corrupts the measurement.
#[derive(Debug, Default)]
pub struct HomeGesture {
    slot: Option<i64>,
    start_x: f64,
    start_y: f64,
    last_y: f64,
}

impl HomeGesture {
    pub fn new() -> Self {
        Self::default()
    }

    /// Touch-down inside the bar zone. Returns `true` when this slot was claimed.
    pub fn begin(&mut self, slot: i64, point: OutputPoint) -> bool {
        if self.slot.is_some() {
            return false;
        }
        self.slot = Some(slot);
        self.start_x = point.x;
        self.start_y = point.y;
        self.last_y = point.y;
        true
    }

    /// Whether `slot` is the claimed gesture slot.
    pub fn is_tracking(&self, slot: i64) -> bool {
        self.slot == Some(slot)
    }

    /// Touch-motion on the claimed slot; other slots are ignored.
    pub fn update(&mut self, slot: i64, point: OutputPoint) {
        if self.is_tracking(slot) {
            self.last_y = point.y;
        }
    }

    /// Touch-up on the claimed slot. Fires `GoHome` when the net upward travel
    /// reached the threshold.
    pub fn end(&mut self, slot: i64) -> Option<ShellAction> {
        if !self.is_tracking(slot) {
            return None;
        }
        let travelled = self.start_y - self.last_y;
        self.slot = None;
        (travelled >= HOME_SWIPE_TRAVEL).then_some(ShellAction::GoHome)
    }

    /// Sequence cancelled (device noise, focus loss): drop the claim.
    pub fn cancel(&mut self) {
        self.slot = None;
    }
}

/// Keyboard/pointer focus bookkeeping.
///
/// The focus target follows the top of the window z-order, except while a
/// touch sequence (or pointer button grab) pins it to the surface the grab
/// began on — the client that owns a sequence must keep receiving it even if
/// another window maps mid-gesture.
#[derive(Debug)]
pub struct FocusTracker<T: Clone + Eq> {
    grab: Option<T>,
}

impl<T: Clone + Eq> FocusTracker<T> {
    pub fn new() -> Self {
        Self { grab: None }
    }

    /// A sequence started on `surface`: pin focus to it.
    pub fn begin_grab(&mut self, surface: T) {
        self.grab = Some(surface);
    }

    /// The last slot of the sequence lifted: release the pin.
    pub fn end_grab(&mut self) {
        self.grab = None;
    }

    /// The surface input should target, given the window z-order
    /// (bottom-to-top). The grab wins while its surface is still mapped;
    /// otherwise the top of the stack owns focus.
    pub fn focus<'a>(&self, z_order_top_last: &'a [T]) -> Option<&'a T> {
        if let Some(grab) = self.grab.as_ref() {
            if z_order_top_last.contains(grab) {
                return z_order_top_last.iter().find(|s| *s == grab);
            }
        }
        z_order_top_last.last()
    }
}

/// What a hardware key is, for routing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyKind {
    PowerKey,
    Other,
}

/// The power-key contract: the toggle spawns only on key *release*
/// (a21s-power-key owns the 300 ms combo guard); the press itself and every
/// other key pass through to the focused client.
pub fn power_key_action(kind: KeyKind, released: bool) -> Option<ShellAction> {
    match (kind, released) {
        (KeyKind::PowerKey, true) => Some(ShellAction::TogglePower),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const H: f64 = 1600.0;

    #[test]
    fn bottom_32px_is_the_bar_and_everything_above_is_the_app() {
        assert_eq!(
            hit_test(
                OutputPoint {
                    x: 360.0,
                    y: 1600.0
                },
                H
            ),
            ShellTarget::GestureBar
        );
        assert_eq!(
            hit_test(OutputPoint { x: 0.0, y: 1568.0 }, H),
            ShellTarget::GestureBar
        );
        assert_eq!(
            hit_test(
                OutputPoint {
                    x: 360.0,
                    y: 1567.9
                },
                H
            ),
            ShellTarget::AppSurface
        );
        assert_eq!(
            hit_test(OutputPoint { x: 5.0, y: 0.0 }, H),
            ShellTarget::AppSurface
        );
    }

    #[test]
    fn swipe_up_from_bar_fires_home_past_the_threshold() {
        let mut g = HomeGesture::new();
        assert!(g.begin(
            0,
            OutputPoint {
                x: 360.0,
                y: 1590.0
            }
        ));
        g.update(
            0,
            OutputPoint {
                x: 360.0,
                y: 1540.0,
            },
        );
        assert_eq!(g.end(0), Some(ShellAction::GoHome));
    }

    #[test]
    fn small_bar_drag_does_not_fire() {
        let mut g = HomeGesture::new();
        g.begin(
            0,
            OutputPoint {
                x: 360.0,
                y: 1590.0,
            },
        );
        g.update(
            0,
            OutputPoint {
                x: 360.0,
                y: 1560.0,
            },
        );
        assert_eq!(g.end(0), None);
    }

    #[test]
    fn downward_drag_never_fires() {
        let mut g = HomeGesture::new();
        g.begin(
            0,
            OutputPoint {
                x: 360.0,
                y: 1568.0,
            },
        );
        g.update(
            0,
            OutputPoint {
                x: 360.0,
                y: 1600.0,
            },
        );
        assert_eq!(g.end(0), None);
    }

    #[test]
    fn second_slot_cannot_steal_or_finish_the_gesture() {
        let mut g = HomeGesture::new();
        assert!(g.begin(
            0,
            OutputPoint {
                x: 360.0,
                y: 1590.0
            }
        ));
        assert!(!g.begin(
            1,
            OutputPoint {
                x: 100.0,
                y: 1595.0
            }
        ));
        g.update(
            1,
            OutputPoint {
                x: 100.0,
                y: 1000.0,
            },
        );
        assert_eq!(g.end(1), None);
        assert!(g.is_tracking(0));
        g.update(
            0,
            OutputPoint {
                x: 360.0,
                y: 1530.0,
            },
        );
        assert_eq!(g.end(0), Some(ShellAction::GoHome));
    }

    #[test]
    fn cancel_drops_the_claim() {
        let mut g = HomeGesture::new();
        g.begin(
            0,
            OutputPoint {
                x: 360.0,
                y: 1590.0,
            },
        );
        g.cancel();
        assert_eq!(g.end(0), None);
    }

    #[test]
    fn focus_follows_the_top_of_the_stack() {
        let tracker: FocusTracker<u32> = FocusTracker::new();
        assert_eq!(tracker.focus(&[]), None);
        assert_eq!(tracker.focus(&[1, 2]), Some(&2));
        assert_eq!(tracker.focus(&[3]), Some(&3));
    }

    #[test]
    fn grab_pins_focus_until_the_sequence_ends() {
        let mut tracker: FocusTracker<u32> = FocusTracker::new();
        tracker.begin_grab(1);
        // A second window maps mid-sequence: focus stays with the grab owner.
        assert_eq!(tracker.focus(&[1, 2]), Some(&1));
        tracker.end_grab();
        assert_eq!(tracker.focus(&[1, 2]), Some(&2));
    }

    #[test]
    fn grab_on_an_unmapped_surface_falls_back_to_top() {
        let mut tracker: FocusTracker<u32> = FocusTracker::new();
        tracker.begin_grab(9);
        assert_eq!(tracker.focus(&[1, 2]), Some(&2));
    }

    #[test]
    fn power_key_fires_only_on_release() {
        assert_eq!(
            power_key_action(KeyKind::PowerKey, true),
            Some(ShellAction::TogglePower)
        );
        assert_eq!(power_key_action(KeyKind::PowerKey, false), None);
        assert_eq!(power_key_action(KeyKind::Other, true), None);
        assert_eq!(power_key_action(KeyKind::Other, false), None);
    }
}
