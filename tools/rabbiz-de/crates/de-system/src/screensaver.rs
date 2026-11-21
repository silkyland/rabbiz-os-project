//! `org.freedesktop.ScreenSaver` session service (AUDIT-DE-STANDARDS-2026-11-21 §2).
//!
//! Media players, browsers and video-call apps call `Inhibit`/`SimulateUserActivity`
//! instead of — or alongside — `zwp_idle_inhibit_v1`; without this name on the
//! session bus those apps simply never inhibit and the panel sleeps mid-video.
//!
//! Same worker pattern as [`crate::notifications::Server`]: one thread owns the
//! zbus connection, the compositor reads a `Shared` snapshot each frame tick and
//! never blocks on D-Bus. `de-core` maps `inhibited` onto the same idle-blank
//! gate as `zwp_idle_inhibit`, applies `SimulateUserActivity` as real activity,
//! and reports session idle time + screen-blank state back through `Shared`.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

const IFACE: &str = "org.freedesktop.ScreenSaver";
const PATH: &str = "/org/freedesktop/ScreenSaver";

/// Compositor-visible view of the ScreenSaver state.
#[derive(Default)]
pub struct Shared {
    /// Active `Inhibit` cookies: cookie → (app_name, reason).
    pub inhibits: HashMap<u32, (String, String)>,
    /// Bumped by `SimulateUserActivity` and `SetActive(true)` — the compositor
    /// drains the counter each tick and treats it as hardware input.
    pub activity_pokes: u64,
    /// `SetActive(false)` requested a blank (screenshot lockers use this).
    pub sleep_requests: u64,
}

struct Service {
    shared: Arc<Mutex<Shared>>,
    /// Screen is currently blanked — the compositor owns this flag.
    blank_active: Arc<AtomicBool>,
    /// Session idle ms, refreshed by the compositor each tick.
    session_idle_ms: Arc<AtomicU64>,
    next_cookie: Mutex<u32>,
}

#[zbus::interface(name = "org.freedesktop.ScreenSaver")]
impl Service {
    fn inhibit(&self, app_name: &str, reason: &str) -> u32 {
        let mut next = self.next_cookie.lock().unwrap_or_else(|e| e.into_inner());
        *next = next.checked_add(1).unwrap_or(1);
        let cookie = *next;
        self.shared
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .inhibits
            .insert(cookie, (app_name.to_string(), reason.to_string()));
        cookie
    }

    fn un_inhibit(&self, cookie: u32) {
        self.shared
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .inhibits
            .remove(&cookie);
    }

    fn simulate_user_activity(&self) {
        let mut s = self.shared.lock().unwrap_or_else(|e| e.into_inner());
        s.activity_pokes = s.activity_pokes.saturating_add(1);
    }

    /// Spec: true while the saver is running (screen blanked/locked).
    fn get_active(&self) -> bool {
        self.blank_active.load(Ordering::Relaxed)
    }

    /// Seconds since the saver became active.
    fn get_active_time(&self) -> u32 {
        if self.blank_active.load(Ordering::Relaxed) {
            self.session_idle_ms.load(Ordering::Relaxed) as u32 / 1000
        } else {
            0
        }
    }

    /// Seconds since the last input (or simulated input).
    fn get_session_idle_time(&self) -> u32 {
        self.session_idle_ms.load(Ordering::Relaxed) as u32 / 1000
    }

    /// Spec: `false` blanks, `true` un-blanks. Returns whether it was honored.
    fn set_active(&self, active: bool) -> bool {
        let mut s = self.shared.lock().unwrap_or_else(|e| e.into_inner());
        if active {
            s.activity_pokes = s.activity_pokes.saturating_add(1);
        } else {
            s.sleep_requests = s.sleep_requests.saturating_add(1);
        }
        true
    }
}

/// Handle held by the compositor; the D-Bus worker lives as long as this does.
pub struct ScreenSaver {
    shared: Arc<Mutex<Shared>>,
    /// Last poke counters already applied by the compositor.
    seen_pokes: u64,
    seen_sleeps: u64,
    /// Written by the compositor: screen-blank state + session idle ms.
    blank_active: Arc<AtomicBool>,
    session_idle_ms: Arc<AtomicU64>,
    available: Arc<AtomicBool>,
}

impl ScreenSaver {
    /// Claim `org.freedesktop.ScreenSaver` on the session bus (worker thread).
    /// `None` when no bus exists (e.g. headless sim) — the handle still works
    /// with a no-op backend so callers never branch on availability.
    #[must_use]
    pub fn start() -> Self {
        let shared = Arc::new(Mutex::new(Shared::default()));
        let blank_active = Arc::new(AtomicBool::new(false));
        let session_idle_ms = Arc::new(AtomicU64::new(0));
        let available = Arc::new(AtomicBool::new(false));
        {
            let shared = shared.clone();
            let blank_active = blank_active.clone();
            let session_idle_ms = session_idle_ms.clone();
            let ready = available.clone();
            std::thread::spawn(move || {
                let connection = zbus::blocking::connection::Builder::session()
                    .and_then(|b| b.name(IFACE))
                    .and_then(|b| {
                        b.serve_at(
                            PATH,
                            Service {
                                shared,
                                blank_active,
                                session_idle_ms,
                                next_cookie: Mutex::new(0),
                            },
                        )
                    })
                    .and_then(|b| b.build());
                if connection.is_ok() {
                    ready.store(true, Ordering::Relaxed);
                    // Serve forever: zbus `blocking` dispatches on its own
                    // thread pool; parking here keeps the connection alive.
                    std::thread::park();
                }
            });
        }
        Self {
            shared,
            seen_pokes: 0,
            seen_sleeps: 0,
            blank_active,
            session_idle_ms,
            available,
        }
    }

    /// Any live `Inhibit` cookies (cookie list is intentionally not exposed to
    /// the compositor beyond the count — apps' reasons stay in the service).
    pub fn inhibited(&self) -> bool {
        !self
            .shared
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .inhibits
            .is_empty()
    }

    /// `SimulateUserActivity`/`SetActive(true)` calls since the last drain.
    pub fn take_activity_pokes(&mut self) -> u64 {
        let pokes = self
            .shared
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .activity_pokes;
        let fresh = pokes.saturating_sub(self.seen_pokes);
        self.seen_pokes = pokes;
        fresh
    }

    /// `SetActive(false)` calls since the last drain.
    pub fn take_sleep_requests(&mut self) -> u64 {
        let sleeps = self
            .shared
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .sleep_requests;
        let fresh = sleeps.saturating_sub(self.seen_sleeps);
        self.seen_sleeps = sleeps;
        fresh
    }

    /// Compositor → service: screen-blank state + session idle for `Get*Time`.
    pub fn report(&self, blanked: bool, session_idle_ms: u64) {
        self.blank_active.store(blanked, Ordering::Relaxed);
        self.session_idle_ms
            .store(session_idle_ms, Ordering::Relaxed);
    }

    /// The bus name was claimed (diagnostics only — the handle works either way).
    pub fn available(&self) -> bool {
        self.available.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counters_and_inhibit_roundtrip_without_bus() {
        // No session bus in tests: start() still yields a working handle.
        let mut s = ScreenSaver::start();
        assert!(!s.inhibited());
        s.shared
            .lock()
            .unwrap()
            .inhibits
            .insert(7, ("mpv".into(), "video".into()));
        assert!(s.inhibited());
        s.shared.lock().unwrap().activity_pokes = 3;
        assert_eq!(s.take_activity_pokes(), 3);
        assert_eq!(s.take_activity_pokes(), 0);
        s.shared.lock().unwrap().sleep_requests = 1;
        assert_eq!(s.take_sleep_requests(), 1);
        assert_eq!(s.take_sleep_requests(), 0);
        s.report(true, 90_000);
        assert!(s.blank_active.load(Ordering::Relaxed));
        assert_eq!(s.session_idle_ms.load(Ordering::Relaxed), 90_000);
    }
}
