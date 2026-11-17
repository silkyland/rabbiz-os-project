//! xdg-shell + xdg-decoration + xdg-activation handlers (M1-T2).
//!
//! Handset policy: every toplevel is configured fullscreen at the output
//! size and mapped at (0,0); move/resize/minimize requests are no-ops.

use smithay::desktop::{PopupKind, Window};
use smithay::reexports::wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1;
use smithay::reexports::wayland_server::protocol::{wl_seat, wl_surface::WlSurface};
use smithay::utils::{Logical, Serial, Size};
use smithay::wayland::shell::xdg::decoration::XdgDecorationHandler;
use smithay::wayland::shell::xdg::{
    PopupSurface, PositionerState, ToplevelSurface, XdgShellHandler, XdgShellState,
};
use smithay::wayland::xdg_activation::{
    XdgActivationHandler, XdgActivationState, XdgActivationToken, XdgActivationTokenData,
};

use crate::state::De;

impl XdgShellHandler for De {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState {
        &mut self.xdg_shell_state
    }

    fn new_toplevel(&mut self, surface: ToplevelSurface) {
        let size = Size::<i32, Logical>::from((
            self.output.width as i32,
            self.output.height as i32,
        ));
        de_protocols::configure_fullscreen(&surface, size);
        de_protocols::set_ssd(&surface);
        let window = Window::new_wayland_window(surface);
        self.space.map_element(window, (0, 0), true);
        self.needs_present = true;
    }

    fn new_popup(&mut self, surface: PopupSurface, _positioner: PositionerState) {
        if let Err(err) = self.popups.track_popup(PopupKind::from(surface)) {
            eprintln!("track_popup: {err}");
        }
    }

    fn reposition_request(
        &mut self,
        surface: PopupSurface,
        positioner: PositionerState,
        token: u32,
    ) {
        surface.with_pending_state(|state| {
            state.geometry = positioner.get_geometry();
            state.positioner = positioner;
        });
        surface.send_repositioned(token);
    }

    fn grab(&mut self, _surface: PopupSurface, _seat: wl_seat::WlSeat, _serial: Serial) {
        // Popups may grab; handset needs no interactive grab bookkeeping.
    }

    fn toplevel_destroyed(&mut self, surface: ToplevelSurface) {
        let window = self
            .space
            .elements()
            .find(|w| {
                w.toplevel()
                    .map(|t| t.wl_surface() == surface.wl_surface())
                    .unwrap_or(false)
            })
            .cloned();
        if let Some(window) = window {
            self.space.unmap_elem(&window);
        }
        self.needs_present = true;
    }

    // The following requests are meaningless on a fullscreen handset shell —
    // acknowledge politely but do nothing.
    fn move_request(&mut self, _surface: ToplevelSurface, _seat: wl_seat::WlSeat, _s: Serial) {}
    fn maximize_request(&mut self, surface: ToplevelSurface) {
        surface.send_configure();
    }
    fn fullscreen_request(
        &mut self,
        surface: ToplevelSurface,
        _output: Option<smithay::reexports::wayland_server::protocol::wl_output::WlOutput>,
    ) {
        de_protocols::configure_fullscreen(
            &surface,
            Size::from((self.output.width as i32, self.output.height as i32)),
        );
        surface.send_configure();
    }
    fn unfullscreen_request(&mut self, surface: ToplevelSurface) {
        de_protocols::configure_fullscreen(
            &surface,
            Size::from((self.output.width as i32, self.output.height as i32)),
        );
        surface.send_configure();
    }
    fn minimize_request(&mut self, _surface: ToplevelSurface) {}
}

smithay::delegate_xdg_shell!(De);

impl XdgDecorationHandler for De {
    fn new_decoration(&mut self, toplevel: ToplevelSurface) {
        de_protocols::set_ssd(&toplevel);
    }

    fn request_mode(
        &mut self,
        toplevel: ToplevelSurface,
        _mode: zxdg_toplevel_decoration_v1::Mode,
    ) {
        de_protocols::set_ssd(&toplevel);
        if toplevel.is_initial_configure_sent() {
            toplevel.send_pending_configure();
        }
    }

    fn unset_mode(&mut self, toplevel: ToplevelSurface) {
        de_protocols::set_ssd(&toplevel);
        if toplevel.is_initial_configure_sent() {
            toplevel.send_pending_configure();
        }
    }
}

smithay::delegate_xdg_decoration!(De);

impl XdgActivationHandler for De {
    fn activation_state(&mut self) -> &mut XdgActivationState {
        &mut self.xdg_activation_state
    }

    fn token_created(&mut self, _token: XdgActivationToken, _data: XdgActivationTokenData) -> bool {
        // Single fullscreen window: grant activation to whoever asks.
        true
    }

    fn request_activation(
        &mut self,
        _token: XdgActivationToken,
        _data: XdgActivationTokenData,
        surface: WlSurface,
    ) {
        if let Some(window) = self.window_for_surface(&surface) {
            self.space.raise_element(&window, true);
            self.needs_present = true;
        }
    }
}

smithay::delegate_xdg_activation!(De);
