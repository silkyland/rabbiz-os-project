//! `zwp_relative_pointer_manager_v1` and `zwp_pointer_constraints_v1` handlers
//! (Tier 3.13 — required for Steam/games, remote desktop, and 3D apps).

use smithay::input::pointer::PointerHandle;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::wayland::pointer_constraints::{with_pointer_constraint, PointerConstraintsHandler};

use crate::state::De;

impl PointerConstraintsHandler for De {
    fn new_constraint(&mut self, surface: &WlSurface, pointer: &PointerHandle<Self>) {
        // Activate immediately when the requesting surface already holds pointer focus.
        if pointer
            .current_focus()
            .is_some_and(|focused| &focused == surface)
        {
            with_pointer_constraint(surface, pointer, |constraint| {
                if let Some(c) = constraint {
                    c.activate();
                }
            });
        }
    }

    fn cursor_position_hint(
        &mut self,
        surface: &WlSurface,
        pointer: &PointerHandle<Self>,
        location: smithay::utils::Point<f64, smithay::utils::Logical>,
    ) {
        let active = with_pointer_constraint(surface, pointer, |constraint| {
            constraint.is_some_and(|c| c.is_active())
        });
        if active {
            let _ = location;
        }
    }
}

smithay::delegate_relative_pointer!(De);
smithay::delegate_pointer_constraints!(De);
