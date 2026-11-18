//! Seat / focus / selection-capability handler impls (M1-T1/M1-T8).

use smithay::input::keyboard::LedState;
use smithay::input::pointer::CursorImageStatus;
use smithay::input::{Seat, SeatHandler, SeatState};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::Resource;
use smithay::utils::{Serial, SERIAL_COUNTER};
use smithay::wayland::seat::WaylandFocus;
use smithay::wayland::selection::data_device::set_data_device_focus;
use smithay::wayland::selection::primary_selection::set_primary_focus;

use crate::state::De;

impl SeatHandler for De {
    type KeyboardFocus = WlSurface;
    type PointerFocus = WlSurface;
    type TouchFocus = WlSurface;

    fn seat_state(&mut self) -> &mut SeatState<De> {
        &mut self.seat_state
    }

    fn focus_changed(&mut self, seat: &Seat<Self>, target: Option<&Self::KeyboardFocus>) {
        let dh = &self.display_handle;
        let focus = target
            .and_then(WaylandFocus::wl_surface)
            .and_then(|s| dh.get_client(s.as_ref().id()).ok());
        set_data_device_focus(dh, seat, focus.clone());
        set_primary_focus(dh, seat, focus);
    }

    fn cursor_image(&mut self, _seat: &Seat<Self>, _image: CursorImageStatus) {}

    fn led_state_changed(&mut self, _seat: &Seat<Self>, led_state: LedState) {
        self.update_led_state(led_state);
    }
}

smithay::delegate_seat!(De);

impl De {
    pub(crate) fn focus_surface(&mut self, target: Option<WlSurface>, serial: Serial) {
        if let Some(keyboard) = self.seat.get_keyboard() {
            keyboard.set_focus(
                self,
                effective_focus(self.exclusive_layer_focus.clone(), target),
                serial,
            );
        }
    }

    pub(crate) fn focus_foreground(&mut self) {
        let target = match self.foreground {
            crate::state::Foreground::Shell => None,
            crate::state::Foreground::App => self
                .space
                .elements()
                .rev()
                .filter_map(|window| window.toplevel())
                .map(smithay::wayland::shell::xdg::ToplevelSurface::wl_surface)
                .find(|surface| crate::comp::has_buffer(surface))
                .cloned(),
        };
        self.focus_surface(target, SERIAL_COUNTER.next_serial());
    }

    pub(crate) fn sync_exclusive_layer_focus(&mut self) {
        let layer = self.exclusive_layer();
        if self.exclusive_layer_focus != layer {
            self.exclusive_layer_focus = layer;
            self.focus_foreground();
        }
    }
}

/// Keyboard focus precedence: an exclusive-interactivity layer surface
/// (the OSK grabs none today, but lock/screenshot overlays may) wins over
/// any window or client target.
fn effective_focus<T>(exclusive: Option<T>, target: Option<T>) -> Option<T> {
    exclusive.or(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exclusive_layer_wins_over_client_focus() {
        assert_eq!(effective_focus(Some(1), Some(2)), Some(1));
        assert_eq!(effective_focus(None, Some(2)), Some(2));
        assert_eq!(effective_focus::<i32>(Some(1), None), Some(1));
        assert_eq!(effective_focus::<i32>(None, None), None);
    }
}
