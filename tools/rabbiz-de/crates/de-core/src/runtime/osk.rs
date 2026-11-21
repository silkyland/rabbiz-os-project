//! Text input and the session-owned input method.

use std::io::{self, Read, Write};
use std::os::fd::OwnedFd;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
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

/// Events received from `rabbiz-osk` when the shell search bar owns the OSK.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ShellImEvent {
    Commit(String),
    Backspace,
    Enter,
}

/// Non-blocking Unix socket bridge between `de-core` (for `Screen::Search` / `Screen::Drawer`)
/// and `rabbiz-osk`, since shell-rendered screens have no client `zwp_text_input_v3` surface.
pub(crate) struct ShellImBridge {
    sock_path: Option<PathBuf>,
    listener: Option<UnixListener>,
    conn: Option<UnixStream>,
    buf: String,
    shown: bool,
}

impl ShellImBridge {
    pub(crate) fn bind() -> Self {
        let dir = std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/tmp"));
        let path = dir.join(format!("rabbiz-osk-im-{}.sock", std::process::id()));
        let _ = std::fs::remove_file(&path);
        match UnixListener::bind(&path) {
            Ok(listener) => {
                let _ = listener.set_nonblocking(true);
                Self {
                    sock_path: Some(path),
                    listener: Some(listener),
                    conn: None,
                    buf: String::new(),
                    shown: false,
                }
            }
            Err(err) => {
                eprintln!("rabbiz-de: shell IM bridge bind failed: {err}");
                Self {
                    sock_path: None,
                    listener: None,
                    conn: None,
                    buf: String::new(),
                    shown: false,
                }
            }
        }
    }

    pub(crate) fn sock_path(&self) -> Option<&PathBuf> {
        self.sock_path.as_ref()
    }

    pub(crate) fn is_shown(&self) -> bool {
        self.shown
    }

    pub(crate) fn set_visible(&mut self, show: bool) {
        // Always forward `SHOW` even when `self.shown` is already true so a
        // tap on the shell search field re-opens an OSK the user dismissed.
        if !show && !self.shown {
            return;
        }
        self.shown = show;
        self.accept_if_pending();
        let msg = if show { "SHOW\n" } else { "HIDE\n" };
        if let Some(stream) = &mut self.conn {
            if stream.write_all(msg.as_bytes()).is_err() || stream.flush().is_err() {
                self.conn = None;
            }
        }
    }

    fn accept_if_pending(&mut self) {
        let Some(listener) = &self.listener else {
            return;
        };
        while let Ok((stream, _)) = listener.accept() {
            let _ = stream.set_nonblocking(true);
            let mut s = stream;
            if self.shown {
                let _ = s.write_all(b"SHOW\n");
                let _ = s.flush();
            }
            self.conn = Some(s);
            self.buf.clear();
        }
    }

    pub(crate) fn poll(&mut self) -> Vec<ShellImEvent> {
        self.accept_if_pending();
        let Some(stream) = &mut self.conn else {
            return Vec::new();
        };
        let mut tmp = [0u8; 512];
        let mut closed = false;
        loop {
            match stream.read(&mut tmp) {
                Ok(0) => {
                    closed = true;
                    break;
                }
                Ok(n) => self.buf.push_str(&String::from_utf8_lossy(&tmp[..n])),
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(_) => {
                    closed = true;
                    break;
                }
            }
        }
        if closed {
            self.conn = None;
        }
        let mut out = Vec::new();
        while let Some(nl) = self.buf.find('\n') {
            let line = self.buf[..nl].trim_end_matches('\r').to_string();
            self.buf.drain(..=nl);
            if let Some(rest) = line.strip_prefix("C ") {
                if !rest.is_empty() {
                    out.push(ShellImEvent::Commit(rest.to_string()));
                }
            } else if line.starts_with("B ") || line == "B" {
                out.push(ShellImEvent::Backspace);
            } else if line == "E" {
                out.push(ShellImEvent::Enter);
            }
        }
        out
    }
}

impl Drop for ShellImBridge {
    fn drop(&mut self) {
        if let Some(path) = &self.sock_path {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// The private socket gives the session keyboard sole access to the two
/// privileged globals. Normal clients use text-input-v3 on the public socket.
pub(crate) fn spawn(
    display: &mut DisplayHandle,
    socket_name: &str,
    keyboard_exec: &str,
    shell_im_sock: Option<&std::path::Path>,
    hw_keyboard: bool,
) -> io::Result<OskProcess> {
    let (server, client) = UnixStream::pair()?;
    display.insert_client(
        server,
        Arc::new(DeClientState {
            session_input_method: true,
            ..DeClientState::default()
        }),
    )?;
    let launch = |name: &str, socket| {
        let mut cmd = Command::new(name);
        cmd.stdin(Stdio::from(socket))
            .env("WAYLAND_SOCKET", "0")
            .env("WAYLAND_DISPLAY", socket_name);
        if let Some(sock) = shell_im_sock {
            cmd.env("RABBIZ_OSK_IM_SOCK", sock);
        }
        // Hardware keyboard attached: the OSK should stay hidden (it still
        // provides the input-method role for text-input-v3 clients).
        if hw_keyboard {
            cmd.env("RABBIZ_HW_KEYBOARD", "1");
        }
        cmd.spawn()
    };
    let (name, child) = match launch(keyboard_exec, OwnedFd::from(client.try_clone()?)) {
        // Existing images may still have the pre-rename binary.
        Err(error) if error.kind() == io::ErrorKind::NotFound && keyboard_exec == "rabbiz-osk" => {
            ("a21s-osk", launch("a21s-osk", OwnedFd::from(client))?)
        }
        result => (keyboard_exec, result?),
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
        let sock = self.shell_im.sock_path().cloned();
        let kb = de_config::keyboards::resolve_active_keyboard(self.app_prefs.prefs());
        match spawn(
            &mut self.display_handle,
            &socket,
            &kb.exec,
            sock.as_deref(),
            self.hw_keyboard,
        ) {
            Ok(child) => {
                self.osk_spawned_at = Some(now);
                self.osk = Some(child);
            }
            Err(error) => {
                eprintln!(
                    "rabbiz-de: could not start the session OSK ({}): {error}",
                    kb.exec
                );
                self.osk_respawn_at = Some(now + RESPAWN_FAIL_DELAY);
            }
        }
    }

    /// Switch keyboards at runtime (e.g. from Settings -> Keyboard).
    pub(crate) fn restart_osk(&mut self) {
        self.osk = None;
        self.osk_respawn_at = None;
        self.osk_spawned_at = None;
        self.check_osk();
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
