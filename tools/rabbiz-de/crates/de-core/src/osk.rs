//! Text input and the session-owned input method.

use std::io;
use std::os::fd::OwnedFd;
use std::os::unix::net::UnixStream;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

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
    let (name, child) = match launch("rabbiz-osk", OwnedFd::from(client.try_clone()?)) {
        // Existing images may still have the pre-rename binary.
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            ("a21s-osk", launch("a21s-osk", OwnedFd::from(client))?)
        }
        result => ("rabbiz-osk", result?),
    };
    eprintln!("rabbiz-de: started {name} pid={}", child.id());
    Ok(OskProcess(child))
}

pub(crate) struct OskProcess(Child);

impl OskProcess {
    /// `true` once the child has exited (or its status can no longer be read).
    /// Reaps the zombie via `try_wait`.
    pub(crate) fn exited(&mut self) -> bool {
        !matches!(self.0.try_wait(), Ok(None))
    }
}

impl Drop for OskProcess {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

impl De {
    /// Re-spawn the session keyboard when it exits (or was never started).
    /// The binary is re-resolved through PATH on every spawn, so replacing
    /// `rabbiz-osk` on disk takes effect without a session restart. Called
    /// from the periodic tick.
    pub(crate) fn check_osk(&mut self) {
        let now = Instant::now();
        if self.osk.as_mut().is_none_or(OskProcess::exited) {
            self.osk = None;
            if self.osk_respawn_at.is_none() {
                // A child that died fast (missing binary, crash loop) backs
                // off; a long-lived one respawns promptly.
                self.osk_respawn_at = Some(now + respawn_delay(self.osk_spawned_at));
            }
        }
        if self.osk.is_some() || self.osk_respawn_at.is_none_or(|t| now < t) {
            return;
        }
        self.osk_respawn_at = None;
        let Some(socket) = self.socket_name.clone() else {
            return;
        };
        match spawn(&mut self.display_handle, &socket) {
            Ok(child) => {
                self.osk_spawned_at = Some(now);
                self.osk = Some(child);
            }
            Err(error) => {
                eprintln!("rabbiz-de: could not start the session OSK: {error}");
                self.osk_respawn_at = Some(now + RESPAWN_FAIL_DELAY);
            }
        }
    }
}

/// A child that died fast (missing binary, crash loop) backs off; a
/// long-lived one respawns promptly.
const QUICK_DEATH: Duration = Duration::from_secs(5);
const RESPAWN_FAST: Duration = Duration::from_secs(1);
const RESPAWN_SLOW: Duration = Duration::from_secs(10);
/// Spawn itself failed (e.g. PATH resolution) — retry far less often.
const RESPAWN_FAIL_DELAY: Duration = Duration::from_secs(30);

/// Delay before re-spawning an exited session keyboard. `spawned_at` is
/// when the dead child was started (`None` = it was never started).
fn respawn_delay(spawned_at: Option<Instant>) -> Duration {
    match spawned_at {
        Some(t) if t.elapsed() < QUICK_DEATH => RESPAWN_SLOW,
        _ => RESPAWN_FAST,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn respawn_backoff_is_1s_10s_or_30s() {
        // Never started / long-lived child → prompt 1 s respawn.
        assert_eq!(respawn_delay(None), Duration::from_secs(1));
        assert_eq!(
            respawn_delay(Some(
                Instant::now().checked_sub(Duration::from_secs(60)).unwrap()
            )),
            Duration::from_secs(1)
        );
        // Child died inside 5 s (crash loop / missing binary) → 10 s.
        assert_eq!(respawn_delay(Some(Instant::now())), Duration::from_secs(10));
        // Spawn failure → 30 s.
        assert_eq!(RESPAWN_FAIL_DELAY, Duration::from_secs(30));
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
