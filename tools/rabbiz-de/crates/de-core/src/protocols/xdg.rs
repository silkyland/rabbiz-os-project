//! xdg-shell + xdg-decoration + xdg-activation handlers (M1-T2).
//!
//! Handset policy: every toplevel is configured fullscreen at the output
//! size and mapped at (0,0); minimizing the foreground window returns Home.

use smithay::desktop::{PopupKind, Window};
use smithay::reexports::wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1;
use smithay::reexports::wayland_server::protocol::{wl_seat, wl_surface::WlSurface};
use smithay::reexports::wayland_server::Resource;
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
        let size =
            Size::<i32, Logical>::from((self.output.width as i32, self.output.height as i32));
        de_protocols::configure_fullscreen(&surface, size);
        de_protocols::set_ssd(&surface);
        // Register for configure/commit bookkeeping. Only the first root
        // buffer can hand over the launcher to this window.
        let window = Window::new_wayland_window(surface);
        self.space.map_element(window, (0, 0), false);
        self.needs_present = true;
    }

    fn new_popup(&mut self, surface: PopupSurface, _positioner: PositionerState) {
        self.unconstrain_popup(&surface);
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
            state.positioner = positioner;
        });
        self.unconstrain_popup(&surface);
        surface.send_repositioned(token);
    }

    fn grab(&mut self, surface: PopupSurface, seat: wl_seat::WlSeat, serial: Serial) {
        self.grab_popup(surface, &seat, serial);
    }

    fn toplevel_destroyed(&mut self, surface: ToplevelSurface) {
        let n = self.space.elements().count();
        let window = self
            .space
            .elements()
            .find(|w| {
                w.toplevel()
                    .is_some_and(|t| t.wl_surface() == surface.wl_surface())
            })
            .cloned();
        eprintln!(
            "rabbiz-de: toplevel_destroyed {:?} matched={} space={n}",
            surface.wl_surface().id(),
            window.is_some(),
        );
        if let Some(window) = window {
            self.space.unmap_elem(&window);
            self.app_disappeared(&window);
        }
        // Last window gone → the shell reappears.
        if self.space.elements().next().is_none() {
            self.foreground = crate::state::Foreground::Shell;
        }
        self.shell_dirty = true;
        self.focus_foreground();
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
    fn minimize_request(&mut self, surface: ToplevelSurface) {
        if self.foreground == crate::state::Foreground::App
            && self.recent_windows().first().and_then(Window::toplevel) == Some(&surface)
        {
            self.minimize_foreground();
        }
    }
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
        if self.foreground == crate::state::Foreground::App {
            let Some(window) = self.window_for_surface(&surface) else {
                return;
            };
            self.space.raise_element(&window, true);
            self.focus_foreground();
            self.needs_present = true;
        }
    }
}

smithay::delegate_xdg_activation!(De);
