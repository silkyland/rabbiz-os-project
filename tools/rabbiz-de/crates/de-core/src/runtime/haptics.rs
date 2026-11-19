//! Pure-Android (AOSP) gesture haptic feedback for ERM/LRA vibrators.
//!
//! Writes short, state-transition pulses to `/sys/kernel/a21s_vib/duration_ms`
//! with a 40 ms cooldown so an ERM motor produces crisp ticks instead of
//! blurring during continuous finger movement.

use std::sync::Mutex;
use std::time::{Duration, Instant};

const VIB_DURATION_PATH: &str = "/sys/kernel/a21s_vib/duration_ms";
const COOLDOWN: Duration = Duration::from_millis(60);

static LAST_PULSE: Mutex<Option<Instant>> = Mutex::new(None);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HapticKind {
    /// Edge-back crossed `BACK_COMMIT_PX` (`GESTURE_THRESHOLD_ACTIVATE`).
    GestureArm,
    /// Edge-back pulled back below `BACK_COMMIT_PX` (`GESTURE_THRESHOLD_DEACTIVATE`).
    GestureDisarm,
    /// Bottom-bar swipe-up paused to enter Recents preview.
    RecentsHold,
    /// Bottom-bar swipe committed Home / Drawer.
    NavCommit,
    /// Horizontal quick-switch along the bottom bar.
    QuickSwitch,
    /// Long-press on home icon / surface (`LONG_PRESS`).
    LongPress,
}

impl HapticKind {
    /// ERM coin-motor durations (ms): COINDC ERM needs ~25–35 ms to overcome
    /// static rotor inertia and produce a crisp tactile kick.
    const fn duration_ms(self) -> u32 {
        match self {
            Self::GestureArm => 38,
            Self::GestureDisarm => 28,
            Self::RecentsHold => 48,
            Self::NavCommit => 35,
            Self::QuickSwitch => 35,
            Self::LongPress => 55,
        }
    }
}

pub(crate) fn pulse(kind: HapticKind) {
    let now = Instant::now();
    if let Ok(mut guard) = LAST_PULSE.lock() {
        if let Some(prev) = *guard {
            if now.saturating_duration_since(prev) < COOLDOWN {
                return;
            }
        }
        *guard = Some(now);
    }
    let _ = std::fs::write(VIB_DURATION_PATH, format!("{}\n", kind.duration_ms()));
}
