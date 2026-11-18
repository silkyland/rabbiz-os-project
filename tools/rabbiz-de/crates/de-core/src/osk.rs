//! Text input and the session-owned input method.

use std::io;
use std::os::fd::OwnedFd;
use std::os::unix::net::UnixStream;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;

use smithay::desktop::{PopupKind, PopupManager};
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::{Client, DisplayHandle};
use smithay::utils::{Logical, Rectangle};
use smithay::wayland::input_method::{InputMethodHandler, PopupSurface};

use crate::state::{De, DeClientState};

pub(crate) fn is_session_input_method(client: &Client) -> bool {
    client
        .get_data::<DeClientState>()
        .is_some_and(|state| state.session_input_method)
}

/// The private socket gives the session keyboard sole access to the two
/// privileged globals. Normal clients use text-input-v3 on the public socket.
pub(crate) fn spawn(display: &mut DisplayHandle, socket_name: &str) -> io::Result<OskProcess> {
    let (server, client) = UnixStream::pair()?;
    display.insert_client(
        server,
        Arc::new(DeClientState {
            session_input_method: true,
            ..DeClientState::default()
        }),
    )?;
    let launch = |name, socket| {
        Command::new(name)
            .stdin(Stdio::from(socket))
            .env("WAYLAND_SOCKET", "0")
            .env("WAYLAND_DISPLAY", socket_name)
            .spawn()
    };
    let child = match launch("rabbiz-osk", OwnedFd::from(client.try_clone()?)) {
        // Existing images may still have the pre-rename binary.
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            launch("a21s-osk", OwnedFd::from(client))?
        }
        result => result?,
    };
    eprintln!("rabbiz-de: started rabbiz-osk pid={}", child.id());
    Ok(OskProcess(child))
}

pub(crate) struct OskProcess(Child);

impl Drop for OskProcess {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

impl InputMethodHandler for De {
    fn new_popup(&mut self, surface: PopupSurface) {
        self.popup_repositioned(surface.clone());
        let _ = self.popups.track_popup(PopupKind::from(surface));
        self.needs_present = true;
    }

    fn dismiss_popup(&mut self, surface: PopupSurface) {
        if let Some(parent) = surface.get_parent().map(|p| p.surface.clone()) {
            let _ = PopupManager::dismiss_popup(&parent, &PopupKind::from(surface));
        }
        self.needs_present = true;
    }

    fn popup_repositioned(&mut self, surface: PopupSurface) {
        let cursor = surface.text_input_rectangle();
        surface.set_location((cursor.loc.x, cursor.loc.y.saturating_add(cursor.size.h)).into());
        self.needs_present = true;
    }

    fn parent_geometry(&self, parent: &WlSurface) -> Rectangle<i32, Logical> {
        self.window_for_surface(parent)
            .and_then(|window| self.space.element_geometry(&window))
            .unwrap_or_default()
    }
}

smithay::delegate_input_method_manager!(De);
smithay::delegate_text_input_manager!(De);
smithay::delegate_virtual_keyboard_manager!(De);
