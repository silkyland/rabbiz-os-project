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

impl SessionLockHandler for De {
    fn lock_state(&mut self) -> &mut SessionLockManagerState {
        &mut self.session_lock_state
    }

    fn lock(&mut self, confirmation: SessionLocker) {
        if self.session_locked {
            // Already locked by an external client — dropping `confirmation`
            // sends `ext_session_lock_v1.finished` as required by the spec.
            return;
        }
        self.session_locked = true;
        self.power_lock();
        confirmation.lock();
        self.needs_present = true;
    }

    fn unlock(&mut self) {
        self.session_locked = false;
        self.lock.unlock_now();
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
