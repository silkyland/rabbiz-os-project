//! Launch flow (M3-T5): desktop-id → `setsid` spawn → first mapped toplevel,
//! with a timeout so a broken app can't wedge the launcher.
//!
//! The tracker is pure bookkeeping; `de-apps::launch` owns the spawn/env
//! contract. One launch in flight at a time (handset policy) — a second tap
//! replaces the pending record rather than queueing.

use std::time::{Duration, Instant};

/// How long the launcher waits for a new toplevel after spawn before it
/// reports a launch failure to the UI.
pub const LAUNCH_TIMEOUT: Duration = Duration::from_millis(4_000);

/// Tracks the pending launch between spawn and `new_toplevel`.
#[derive(Debug)]
pub struct LaunchTracker {
    pending: Option<Pending>,
    timeout: Duration,
}

#[derive(Debug)]
struct Pending {
    desktop_id: String,
    /// Spawned pid (useful for the "killed on timeout" path later).
    #[allow(dead_code)]
    pid: i32,
    started: Instant,
}

impl LaunchTracker {
    #[must_use]
    pub fn new() -> Self {
        Self {
            pending: None,
            timeout: LAUNCH_TIMEOUT,
        }
    }

    /// Spawn `desktop_id` and arm the map-wait. Errors from
    /// `de_apps::launch` propagate unchanged — the UI shows the error
    /// screen instead of a stuck morph.
    pub fn begin(&mut self, desktop_id: &str) -> Result<i32, String> {
        let pid = de_apps::launch::launch(desktop_id)?;
        self.record(desktop_id, pid);
        Ok(pid)
    }

    /// Record a spawned pid awaiting its first toplevel (kept separate from
    /// the spawn so the state machine is testable without running apps).
    fn record(&mut self, desktop_id: &str, pid: i32) {
        self.pending = Some(Pending {
            desktop_id: desktop_id.to_string(),
            pid,
            started: Instant::now(),
        });
    }

    /// Called after the first buffer commit. A pending launch resolves on
    /// the first toplevel with content after spawn. App-id correlation is fuzzy (the
    /// Chrome wrapper's `app_id` is `google-chrome` while the launched id is
    /// `rabbiz-browser`), so an exact match isn't required. Returns the
    /// resolved desktop-id.
    pub fn on_toplevel_mapped(&mut self, app_id: Option<&str>) -> Option<String> {
        let p = self.pending.take()?;
        if let Some(id) = app_id {
            if id != p.desktop_id && !id.starts_with(&p.desktop_id) {
                eprintln!(
                    "launch: toplevel app_id {id:?} does not match pending {:?}",
                    p.desktop_id
                );
            }
        }
        Some(p.desktop_id)
    }

    /// Called each frame pump. Returns the desktop-id whose launch timed
    /// out (window never mapped).
    pub fn check_timeout(&mut self) -> Option<String> {
        if self
            .pending
            .as_ref()
            .is_some_and(|p| p.started.elapsed() >= self.timeout)
        {
            self.pending.take().map(|p| p.desktop_id)
        } else {
            None
        }
    }

    /// The desktop-id currently awaiting its first toplevel.
    #[must_use]
    pub fn pending(&self) -> Option<&str> {
        self.pending.as_ref().map(|p| p.desktop_id.as_str())
    }

    pub(crate) fn deadline(&self) -> Option<Instant> {
        self.pending.as_ref().map(|p| p.started + self.timeout)
    }

    /// Forget the pending launch (user navigated away / app crashed early).
    pub fn cancel(&mut self) {
        self.pending = None;
    }
}

impl Default for LaunchTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    impl LaunchTracker {
        fn with_timeout(timeout: Duration) -> Self {
            Self {
                pending: None,
                timeout,
            }
        }
    }

    #[test]
    fn pending_launch_resolves_on_first_toplevel() {
        let mut t = LaunchTracker::new();
        assert!(t.pending().is_none());
        t.record("rabbiz-files", 42);
        assert_eq!(t.pending(), Some("rabbiz-files"));
        assert!(t.deadline().is_some());
        // Fuzzy app-id: a different app_id still resolves (logged only).
        assert_eq!(
            t.on_toplevel_mapped(Some("org.kde.dolphin")),
            Some("rabbiz-files".into())
        );
        assert!(t.pending().is_none());
        assert_eq!(t.on_toplevel_mapped(Some("x")), None);
    }

    #[test]
    fn a_second_launch_replaces_the_pending_one() {
        let mut t = LaunchTracker::new();
        t.record("a", 1);
        t.record("b", 2);
        assert_eq!(t.pending(), Some("b"));
        assert_eq!(t.on_toplevel_mapped(None), Some("b".into()));
    }

    #[test]
    fn timeout_reports_and_clears_the_pending_launch() {
        let mut t = LaunchTracker::with_timeout(Duration::ZERO);
        t.record("stuck-app", 7);
        assert_eq!(t.check_timeout(), Some("stuck-app".into()));
        assert!(t.pending().is_none());
        assert_eq!(t.check_timeout(), None);
    }

    #[test]
    fn unexpired_launch_does_not_timeout() {
        let mut t = LaunchTracker::with_timeout(Duration::from_secs(3600));
        t.record("ok-app", 7);
        assert_eq!(t.check_timeout(), None);
        assert_eq!(t.pending(), Some("ok-app"));
    }

    #[test]
    fn cancel_forgets_the_pending_launch() {
        let mut t = LaunchTracker::new();
        t.record("a", 1);
        t.cancel();
        assert!(t.pending().is_none());
        assert_eq!(t.on_toplevel_mapped(Some("a")), None);
    }
}
