use smithay::desktop::{PopupKind, PopupManager, Window};
use smithay::reexports::wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode;
use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel::State;
use smithay::reexports::wayland_server::protocol::wl_seat::WlSeat;
use smithay::utils::{Rectangle, Serial, SERIAL_COUNTER};
use smithay::wayland::shell::xdg::decoration::XdgDecorationHandler;
use smithay::wayland::shell::xdg::{
    PopupSurface, PositionerState, ToplevelSurface, XdgShellHandler, XdgShellState,
};
use smithay::{delegate_xdg_decoration, delegate_xdg_shell};

use crate::state::ProtocolState;

impl XdgShellHandler for ProtocolState {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState {
        &mut self.xdg_shell_state
    }

    fn new_toplevel(&mut self, surface: ToplevelSurface) {
        let (w, h) = self.output_size;
        surface.with_pending_state(|state| {
            state.states.set(State::Fullscreen);
            state.states.set(State::Activated);
            state.size = Some((w, h).into());
            state.decoration_mode = Some(Mode::ServerSide);
        });
        surface.send_configure();

        // The window is NOT mapped yet: it enters `space`, rendering and the
        // focus stack only on its first buffer commit (CompositorHandler::commit).
        let window = Window::new_wayland_window(surface);
        self.uncommitted_windows.push(window);
    }

    fn new_popup(&mut self, surface: PopupSurface, positioner: PositionerState) {
        // Constrain the requested geometry into the output so popups cannot
        // land offscreen (menus/dropdowns get slide/flip adjustments applied
        // per xdg_positioner.constraint_adjustment).
        let (w, h) = self.output_size;
        let bounds: Rectangle<i32, smithay::utils::Logical> =
            Rectangle::from_size((w, h).into());
        let geometry = positioner.get_unconstrained_geometry(bounds);
        surface.with_pending_state(|state| {
            state.geometry = geometry;
            state.positioner = positioner;
        });
        if let Err(err) = surface.send_configure() {
            eprintln!("popup send_configure failed: {err:?}");
        }
        if let Err(err) = self.popup_manager.track_popup(PopupKind::Xdg(surface)) {
            eprintln!("track_popup failed: {err:?}");
        }
    }

    fn grab(&mut self, surface: PopupSurface, _seat: WlSeat, _serial: Serial) {
        // smithay's PopupManager::grab_popup requires SeatHandler focus types
        // that can represent popups (KeyboardFocus: From<PopupKind>); the
        // shared input lane forwards WlSurface focus only, so we track the
        // grab ourselves: while set, focus_surface() hands input to the
        // popup's wl_surface.
        self.popup_grab = Some(surface.wl_surface().clone());
        self.refocus(SERIAL_COUNTER.next_serial());
    }

    fn reposition_request(
        &mut self,
        surface: PopupSurface,
        positioner: PositionerState,
        token: u32,
    ) {
        let (w, h) = self.output_size;
        let bounds: Rectangle<i32, smithay::utils::Logical> =
            Rectangle::from_size((w, h).into());
        let geometry = positioner.get_unconstrained_geometry(bounds);
        surface.with_pending_state(|state| {
            state.geometry = geometry;
            state.positioner = positioner;
        });
        surface.send_repositioned(token);
    }

    fn toplevel_destroyed(&mut self, surface: ToplevelSurface) {
        let wl_surface = surface.wl_surface().clone();

        // Queued-but-never-committed window: just drop it.
        self.uncommitted_windows
            .retain(|w| w.toplevel().map(|t| t != &surface).unwrap_or(false));

        // Dismiss popups owned by the dying toplevel.
        for (popup, _) in PopupManager::popups_for_surface(&wl_surface) {
            let _ = PopupManager::dismiss_popup(&wl_surface, &popup);
            if self
                .popup_grab
                .as_ref()
                .map(|grab| grab == popup.wl_surface())
                .unwrap_or(false)
            {
                self.popup_grab = None;
            }
        }
        self.popup_manager.cleanup();

        let window = self
            .space
            .elements()
            .find(|w| w.toplevel().map(|t| t == &surface).unwrap_or(false))
            .cloned();
        if let Some(window) = window {
            self.space.unmap_elem(&window);
        }
        self.focus_stack_remove(&wl_surface);
        // Fall back to the previous window in the stack, not just "last space
        // element".
        self.refocus(SERIAL_COUNTER.next_serial());
    }

    fn popup_destroyed(&mut self, surface: PopupSurface) {
        if self
            .popup_grab
            .as_ref()
            .map(|grab| grab == surface.wl_surface())
            .unwrap_or(false)
        {
            self.popup_grab = None;
            self.refocus(SERIAL_COUNTER.next_serial());
        }
        self.popup_manager.cleanup();
    }
}

impl XdgDecorationHandler for ProtocolState {
    fn new_decoration(&mut self, toplevel: ToplevelSurface) {
        toplevel.with_pending_state(|state| {
            state.decoration_mode = Some(Mode::ServerSide);
        });
        toplevel.send_configure();
    }

    fn request_mode(&mut self, toplevel: ToplevelSurface, _mode: Mode) {
        toplevel.with_pending_state(|state| {
            state.decoration_mode = Some(Mode::ServerSide);
        });
        toplevel.send_configure();
    }

    fn unset_mode(&mut self, toplevel: ToplevelSurface) {
        toplevel.with_pending_state(|state| {
            state.decoration_mode = Some(Mode::ServerSide);
        });
        toplevel.send_configure();
    }
}

delegate_xdg_shell!(ProtocolState);
delegate_xdg_decoration!(ProtocolState);
