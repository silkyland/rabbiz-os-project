//! `zwp_idle_inhibit_manager_v1` handler (Tier 2.8).
//!
//! Lets media players, video calls, and games inhibit screen dimming/sleep
//! while their surface is alive and mapped. Dead or unmapped surfaces are
//! ignored and pruned so a crashed client never leaks a permanent wake lock.

use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::Resource;
use smithay::wayland::idle_inhibit::IdleInhibitHandler;
use smithay::wayland::seat::WaylandFocus;

use crate::state::De;

impl IdleInhibitHandler for De {
    fn inhibit(&mut self, surface: WlSurface) {
        if !self.idle_inhibitors.contains(&surface) {
            self.idle_inhibitors.push(surface);
        }
    }

    fn uninhibit(&mut self, surface: WlSurface) {
        self.idle_inhibitors.retain(|s| s != &surface);
    }
}

smithay::delegate_idle_inhibit!(De);

impl De {
    /// Whether any live, currently mapped toplevel holds an active idle inhibitor.
    #[must_use]
    pub(crate) fn idle_inhibited(&self) -> bool {
        self.idle_inhibitors.iter().any(|surface| {
            surface.is_alive()
                && self
                    .space
                    .elements()
                    .any(|w| w.wl_surface().is_some_and(|ws| &*ws == surface))
        })
    }

    /// Drop any inhibitor whose Wayland surface has been destroyed.
    pub(crate) fn prune_idle_inhibitors(&mut self) {
        self.idle_inhibitors.retain(Resource::is_alive);
    }
}
