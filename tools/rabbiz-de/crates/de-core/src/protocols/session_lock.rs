//! `ext-session-lock-v1` handler (Tier 2.8).
//!
//! Allows privileged session lock clients to lock/unlock the compositor cleanly
//! while coordinating with `rabbiz-de`'s built-in Keyguard (`power_lock` /
//! `lock.unlock_now`). Only one session lock may be active at a time; a second
//! concurrent lock request is refused by dropping the locker (which sends
//! `finished` per the Wayland protocol).

use smithay::reexports::wayland_server::protocol::wl_output::WlOutput;
use smithay::wayland::session_lock::{
    LockSurface, SessionLockHandler, SessionLockManagerState, SessionLocker,
};

use crate::state::De;

/// Ownership state of an `ext-session-lock-v1` session lock (`docs/DE-KEYGUARD-AUTH.md` §5).
///
/// Enforces the Wayland protocol's fail-closed contract: if an external locker
/// disconnects without calling `unlock_and_destroy`, the session MUST remain
/// locked (`RecoveryRequired`) until built-in recovery authentication succeeds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LockOwner {
    /// No external session lock is active.
    #[default]
    Unlocked,
    /// An external `ext-session-lock-v1` client currently holds the lock.
    ExternalActive,
    /// The external locker died without calling `unlock()`; fail-closed until
    /// built-in recovery auth succeeds.
    RecoveryRequired,
}

/// Pure state machine enforcing the `ext-session-lock-v1` single-lock and
/// dead-locker fail-closed contract.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SessionLockTracker {
    owner: LockOwner,
}

impl SessionLockTracker {
    /// Create a tracker in the [`LockOwner::Unlocked`] state.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            owner: LockOwner::Unlocked,
        }
    }

    /// Current lock ownership state.
    #[must_use]
    pub const fn owner(&self) -> LockOwner {
        self.owner
    }

    /// Attempt to acquire the session lock for an external client.
    ///
    /// Returns `true` if the lock was granted (`Unlocked -> ExternalActive`),
    /// or `false` if a lock is already active or awaiting recovery.
    pub fn request_lock(&mut self) -> bool {
        match self.owner {
            LockOwner::Unlocked => {
                self.owner = LockOwner::ExternalActive;
                true
            }
            LockOwner::ExternalActive | LockOwner::RecoveryRequired => false,
        }
    }

    /// Record a clean `unlock_and_destroy` from the active external locker.
    pub fn on_external_unlock(&mut self) {
        if self.owner == LockOwner::ExternalActive {
            self.owner = LockOwner::Unlocked;
        }
    }

    /// Record unexpected disconnection of the external locker client.
    ///
    /// Fail-closed: transitions `ExternalActive -> RecoveryRequired` and never
    /// unlocks the session.
    pub fn on_external_client_died(&mut self) {
        if self.owner == LockOwner::ExternalActive {
            self.owner = LockOwner::RecoveryRequired;
        }
    }

    /// Record successful built-in recovery authentication (PIN/PAM).
    ///
    /// Transitions `RecoveryRequired -> Unlocked`.
    pub fn on_recovery_authenticated(&mut self) {
        if self.owner == LockOwner::RecoveryRequired {
            self.owner = LockOwner::Unlocked;
        }
    }

    /// Whether the session is currently locked (either externally or in
    /// fail-closed recovery).
    #[must_use]
    pub const fn is_locked(&self) -> bool {
        !matches!(self.owner, LockOwner::Unlocked)
    }

    /// Whether the built-in recovery bouncer must be presented because the
    /// external locker died without unlocking.
    #[must_use]
    pub const fn needs_recovery_bouncer(&self) -> bool {
        matches!(self.owner, LockOwner::RecoveryRequired)
    }
}

impl De {
    /// Detect unexpected disconnection of an active `ext-session-lock-v1` locker
    /// and fail closed into the built-in recovery bouncer (`SEC-008`).
    pub(crate) fn check_session_lock_liveness(&mut self) {
        use smithay::reexports::wayland_server::Resource as _;

        if self.session_lock_tracker.owner() != LockOwner::ExternalActive {
            return;
        }
        let dead = self
            .active_session_lock
            .as_ref()
            .is_none_or(|lock| !lock.is_alive());
        if dead {
            self.active_session_lock = None;
            self.session_lock_tracker.on_external_client_died();
            if self.session_lock_tracker.needs_recovery_bouncer() {
                eprintln!(
                    "rabbiz-de: external session lock client died without unlocking; entering recovery bouncer"
                );
                self.lock.open();
                self.lock.open_bouncer();
                self.needs_present = true;
            }
        }
    }
}

impl SessionLockHandler for De {
    fn lock_state(&mut self) -> &mut SessionLockManagerState {
        &mut self.session_lock_state
    }

    fn lock(&mut self, confirmation: SessionLocker) {
        if !self.session_lock_tracker.request_lock() {
            // Already locked by an external client (or awaiting fail-closed
            // recovery) — dropping `confirmation` sends `ext_session_lock_v1.finished`
            // as required by the spec.
            return;
        }
        self.active_session_lock = Some(confirmation.ext_session_lock().clone());
        self.power_lock();
        confirmation.lock();
        self.needs_present = true;
    }

    fn unlock(&mut self) {
        self.active_session_lock = None;
        self.session_lock_tracker.on_external_unlock();
        if !self.session_lock_tracker.is_locked() {
            self.lock.unlock_now();
        }
        self.needs_present = true;
    }

    fn new_surface(&mut self, surface: LockSurface, output: WlOutput) {
        if !self.output_obj.owns(&output) {
            return;
        }
        let size = smithay::utils::Size::from((self.output.width, self.output.height));
        surface.with_pending_state(|states| {
            states.size = Some(size);
        });
        surface.send_configure();
        self.needs_present = true;
    }
}

smithay::delegate_session_lock!(De);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_active_lock_rejects_second_concurrent_request() {
        let mut tracker = SessionLockTracker::new();
        assert!(!tracker.is_locked());
        assert!(!tracker.needs_recovery_bouncer());
        assert_eq!(tracker.owner(), LockOwner::Unlocked);

        assert!(tracker.request_lock());
        assert!(tracker.is_locked());
        assert!(!tracker.needs_recovery_bouncer());
        assert_eq!(tracker.owner(), LockOwner::ExternalActive);

        // Second concurrent lock request must be rejected.
        assert!(!tracker.request_lock());
        assert_eq!(tracker.owner(), LockOwner::ExternalActive);
    }

    #[test]
    fn normal_lock_and_unlock_cycle() {
        let mut tracker = SessionLockTracker::new();
        assert!(tracker.request_lock());
        assert!(tracker.is_locked());

        tracker.on_external_unlock();
        assert!(!tracker.is_locked());
        assert!(!tracker.needs_recovery_bouncer());
        assert_eq!(tracker.owner(), LockOwner::Unlocked);

        // A new lock can be acquired cleanly after unlock.
        assert!(tracker.request_lock());
        assert_eq!(tracker.owner(), LockOwner::ExternalActive);
    }

    #[test]
    fn dead_client_fails_closed_into_recovery_bouncer() {
        let mut tracker = SessionLockTracker::new();
        assert!(tracker.request_lock());

        tracker.on_external_client_died();
        assert!(tracker.is_locked());
        assert!(tracker.needs_recovery_bouncer());
        assert_eq!(tracker.owner(), LockOwner::RecoveryRequired);

        // External unlock and concurrent lock requests cannot bypass recovery.
        tracker.on_external_unlock();
        assert!(tracker.is_locked());
        assert!(tracker.needs_recovery_bouncer());
        assert!(!tracker.request_lock());
        assert_eq!(tracker.owner(), LockOwner::RecoveryRequired);
    }

    #[test]
    fn recovery_authentication_exits_recovery_cleanly() {
        let mut tracker = SessionLockTracker::new();
        assert!(tracker.request_lock());
        // Recovery auth cannot prematurely unlock an active external locker.
        tracker.on_recovery_authenticated();
        assert_eq!(tracker.owner(), LockOwner::ExternalActive);

        tracker.on_external_client_died();
        assert!(tracker.needs_recovery_bouncer());

        tracker.on_recovery_authenticated();
        assert!(!tracker.is_locked());
        assert!(!tracker.needs_recovery_bouncer());
        assert_eq!(tracker.owner(), LockOwner::Unlocked);

        // Subsequent external lock works after recovery completes.
        assert!(tracker.request_lock());
        assert_eq!(tracker.owner(), LockOwner::ExternalActive);
    }
}
