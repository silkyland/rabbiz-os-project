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
        self.pending = Some(Pending {
            desktop_id: desktop_id.to_string(),
            pid,
            started: Instant::now(),
        });
        Ok(pid)
    }

    /// Called from `new_toplevel`. A pending launch resolves on the *first*
    /// toplevel that maps after spawn — app_id correlation is fuzzy (the
    /// Chrome wrapper's app_id is `google-chrome` while the launched id is
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
            .is_some_and(|p| p.started.elapsed() > self.timeout)
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
