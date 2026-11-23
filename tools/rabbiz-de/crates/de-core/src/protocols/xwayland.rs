//! XWayland (`M5-T1` / Phase D3) integration for `De`.
//!
//! Spawns `Xwayland` on demand when the binary is present on `PATH`, starts
//! Smithay's `X11Wm`, and maps X11 windows into `De::space` using the active
//! Handset (fullscreen) or Desktop (floating) policy.

use std::process::Stdio;

use smithay::desktop::Window;
use smithay::reexports::calloop::LoopHandle;
use smithay::reexports::wayland_server::DisplayHandle;
use smithay::utils::{Logical, Rectangle};
use smithay::xwayland::{
    xwm::{ResizeEdge as XwmResizeEdge, X11Surface, X11Wm, XwmHandler, XwmId},
    XWayland, XWaylandClientData, XWaylandEvent,
};

use crate::frame::Run;
use crate::state::De;

/// Spawn `Xwayland` if the `Xwayland` binary exists on `PATH`, and wire its
/// readiness event into `handle`.
pub(crate) fn init(dh: &DisplayHandle, handle: &LoopHandle<'static, Run>) {
    let has_bin = std::env::var_os("PATH").is_some_and(|paths| {
        std::env::split_paths(&paths).any(|dir| dir.join("Xwayland").is_file())
    });
    if !has_bin {
        return;
    }

    let Ok((xwayland, client)) = XWayland::spawn(
        dh,
        None,
        std::iter::empty::<(String, String)>(),
        true,
        Stdio::null(),
        Stdio::null(),
        |_| {},
    ) else {
        return;
    };

    let handle_clone = handle.clone();
    let _ = handle.insert_source(xwayland, move |event, (), run: &mut Run| match event {
        XWaylandEvent::Ready {
            x11_socket,
            display_number,
        } => {
            if let Ok(wm) = X11Wm::start_wm(handle_clone.clone(), x11_socket, client.clone()) {
                run.de.xwm = Some(wm);
                run.de.xwayland_display = Some(display_number);
                std::env::set_var("DISPLAY", format!(":{display_number}"));
            }
        }
        XWaylandEvent::Error => {
            run.de.xwm = None;
            run.de.xwayland_display = None;
        }
    });
}

impl smithay::wayland::xwayland_shell::XWaylandShellHandler for De {
    fn xwayland_shell_state(&mut self) -> &mut smithay::wayland::xwayland_shell::XWaylandShellState {
        &mut self.xwayland_shell_state
    }
}
smithay::delegate_xwayland_shell!(De);

impl smithay::wayland::xwayland_shell::XWaylandShellHandler for Run {
    fn xwayland_shell_state(&mut self) -> &mut smithay::wayland::xwayland_shell::XWaylandShellState {
        &mut self.de.xwayland_shell_state
    }
}

impl XwmHandler for Run {
    fn xwm_state(&mut self, xwm: XwmId) -> &mut X11Wm {
        self.de.xwm_state(xwm)
    }
    fn new_window(&mut self, xwm: XwmId, window: X11Surface) {
        self.de.new_window(xwm, window);
    }
    fn new_override_redirect_window(&mut self, xwm: XwmId, window: X11Surface) {
        self.de.new_override_redirect_window(xwm, window);
    }
    fn map_window_request(&mut self, xwm: XwmId, window: X11Surface) {
        self.de.map_window_request(xwm, window);
    }
    fn mapped_override_redirect_window(&mut self, xwm: XwmId, window: X11Surface) {
        self.de.mapped_override_redirect_window(xwm, window);
    }
    fn unmapped_window(&mut self, xwm: XwmId, window: X11Surface) {
        self.de.unmapped_window(xwm, window);
    }
    fn destroyed_window(&mut self, xwm: XwmId, window: X11Surface) {
        self.de.destroyed_window(xwm, window);
    }
    fn configure_request(
        &mut self,
        xwm: XwmId,
        window: X11Surface,
        x: Option<i32>,
        y: Option<i32>,
        w: Option<u32>,
        h: Option<u32>,
        reorder: Option<smithay::xwayland::xwm::Reorder>,
    ) {
        self.de.configure_request(xwm, window, x, y, w, h, reorder);
    }
    fn configure_notify(
        &mut self,
        xwm: XwmId,
        window: X11Surface,
        geometry: Rectangle<i32, Logical>,
        above: Option<u32>,
    ) {
        self.de.configure_notify(xwm, window, geometry, above);
    }
    fn resize_request(
        &mut self,
        xwm: XwmId,
        window: X11Surface,
        button: u32,
        resize_edge: XwmResizeEdge,
    ) {
        self.de.resize_request(xwm, window, button, resize_edge);
    }
    fn move_request(&mut self, xwm: XwmId, window: X11Surface, button: u32) {
        self.de.move_request(xwm, window, button);
    }
}

impl XwmHandler for De {
    fn xwm_state(&mut self, _xwm: XwmId) -> &mut X11Wm {
        self.xwm.as_mut().expect("xwm_state called without X11Wm")
    }

    fn new_window(&mut self, _xwm: XwmId, _window: X11Surface) {}

    fn new_override_redirect_window(&mut self, _xwm: XwmId, _window: X11Surface) {}

    fn map_window_request(&mut self, _xwm: XwmId, window: X11Surface) {
        let out_size = smithay::utils::Size::<i32, Logical>::from((
            self.output.width as i32,
            self.output.height as i32,
        ));
        let geo = if self.desktop_mode {
            let idx = self.space.elements().count();
            let (x, y, s) = de_protocols::desktop_window_geometry(out_size, idx);
            Rectangle::new((x, y).into(), s)
        } else {
            Rectangle::new((0, 0).into(), out_size)
        };
        let _ = window.configure(geo);
        let _ = window.set_mapped(true);
        let win = Window::new_x11_window(window);
        self.space.map_element(win, geo.loc, false);
        self.needs_present = true;
    }

    fn mapped_override_redirect_window(&mut self, _xwm: XwmId, window: X11Surface) {
        let loc = window.geometry().loc;
        let win = Window::new_x11_window(window);
        self.space.map_element(win, loc, false);
        self.needs_present = true;
    }

    fn unmapped_window(&mut self, _xwm: XwmId, window: X11Surface) {
        let elem = self
            .space
            .elements()
            .find(|w| w.x11_surface() == Some(&window))
            .cloned();
        if let Some(w) = elem {
            self.space.unmap_elem(&w);
        }
        self.needs_present = true;
    }

    fn destroyed_window(&mut self, _xwm: XwmId, window: X11Surface) {
        let elem = self
            .space
            .elements()
            .find(|w| w.x11_surface() == Some(&window))
            .cloned();
        if let Some(w) = elem {
            self.space.unmap_elem(&w);
        }
        self.needs_present = true;
    }

    fn configure_request(
        &mut self,
        _xwm: XwmId,
        window: X11Surface,
        x: Option<i32>,
        y: Option<i32>,
        w: Option<u32>,
        h: Option<u32>,
        _reorder: Option<smithay::xwayland::xwm::Reorder>,
    ) {
        let mut geo = window.geometry();
        if self.desktop_mode {
            if let Some(nx) = x {
                geo.loc.x = nx;
            }
            if let Some(ny) = y {
                geo.loc.y = ny;
            }
            if let Some(nw) = w {
                geo.size.w = (nw as i32).max(240);
            }
            if let Some(nh) = h {
                geo.size.h = (nh as i32).max(160);
            }
        }
        let _ = window.configure(geo);
    }

    fn configure_notify(
        &mut self,
        _xwm: XwmId,
        _window: X11Surface,
        _geometry: Rectangle<i32, Logical>,
        _above: Option<u32>,
    ) {
    }

    fn resize_request(
        &mut self,
        _xwm: XwmId,
        _window: X11Surface,
        _button: u32,
        _resize_edge: XwmResizeEdge,
    ) {
    }

    fn move_request(&mut self, _xwm: XwmId, _window: X11Surface, _button: u32) {}
}

/// Tag type required by `XWayland::spawn` client data.
#[allow(dead_code)]
pub(crate) fn _client_data_marker(_: &XWaylandClientData) {}
