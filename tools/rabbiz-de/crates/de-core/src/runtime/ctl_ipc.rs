//! Non-blocking Unix-socket runtime control server (`docs/DE-IPC.md`, Tier 1.6).
//!
//! Binds `$XDG_RUNTIME_DIR/rabbiz-de-ctl.sock` (`0600`) and polls accepted
//! connections synchronously inside `de-core`'s event loop.

use std::io::{BufRead, BufReader, ErrorKind, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::time::Duration;

use de_api::Size;
use de_config::ipc::{
    decode_request, encode_response, socket_path, InsetSummary, IpcCommand, IpcResponse,
    ShellStateSnapshot, WindowSummary, MAX_MESSAGE_BYTES,
};

use crate::state::{De, Foreground};

/// Bound non-blocking Unix listener for runtime `rabbiz-de ctl` requests.
pub(crate) struct CtlIpc {
    listener: UnixListener,
    path: PathBuf,
}

impl CtlIpc {
    /// Bind the user-scoped control socket (`0600`), replacing any stale socket file.
    pub(crate) fn bind() -> Option<Self> {
        let path = socket_path();
        let _ = std::fs::remove_file(&path);
        let listener = match UnixListener::bind(&path) {
            Ok(l) => l,
            Err(e) => {
                eprintln!("rabbiz-de: ctl ipc bind {} failed: {e}", path.display());
                return None;
            }
        };
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
        if let Err(e) = listener.set_nonblocking(true) {
            eprintln!("rabbiz-de: ctl ipc nonblocking failed: {e}");
            let _ = std::fs::remove_file(&path);
            return None;
        }
        Some(Self { listener, path })
    }

    fn try_accept(&self) -> Option<UnixStream> {
        match self.listener.accept() {
            Ok((stream, _)) => Some(stream),
            Err(e) if e.kind() == ErrorKind::WouldBlock => None,
            Err(e) => {
                eprintln!("rabbiz-de: ctl ipc accept error: {e}");
                None
            }
        }
    }
}

impl Drop for CtlIpc {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

impl De {
    /// Drain pending non-blocking control requests (at most 8 per tick to bound latency).
    pub(crate) fn poll_ctl_ipc(&mut self) {
        let mut accepted = Vec::new();
        if let Some(ipc) = self.ctl_ipc.as_ref() {
            for _ in 0..8 {
                let Some(stream) = ipc.try_accept() else {
                    break;
                };
                accepted.push(stream);
            }
        }
        for stream in accepted {
            self.handle_ctl_stream(stream);
        }
    }

    fn handle_ctl_stream(&mut self, mut stream: UnixStream) {
        let _ = stream.set_nonblocking(false);
        let timeout = Some(Duration::from_millis(250));
        let _ = stream.set_read_timeout(timeout);
        let _ = stream.set_write_timeout(timeout);

        let Ok(reader_stream) = stream.try_clone() else {
            return;
        };
        let mut reader = BufReader::new(reader_stream.take((MAX_MESSAGE_BYTES + 1) as u64));
        let mut line = String::new();
        if reader.read_line(&mut line).is_err() || line.is_empty() {
            return;
        }

        let resp = match decode_request(&line) {
            Ok(req) => self.dispatch_ctl(req.id, req.command),
            Err(err) => IpcResponse::err(0, err),
        };
        if let Ok(encoded) = encode_response(&resp) {
            let _ = stream.write_all(encoded.as_bytes());
        }
    }

    fn dispatch_ctl(&mut self, id: u64, cmd: IpcCommand) -> IpcResponse {
        match cmd {
            IpcCommand::GetState => {
                let snap = self.ctl_state_snapshot();
                match serde_json::to_value(snap) {
                    Ok(v) => IpcResponse::ok(id, Some(v)),
                    Err(e) => IpcResponse::err(id, format!("serialize state: {e}")),
                }
            }
            IpcCommand::Lock => {
                self.power_lock();
                self.needs_present = true;
                IpcResponse::ok(id, None)
            }
            IpcCommand::Launch { desktop_id } => match self.launch.begin(&desktop_id) {
                Ok(pid) => {
                    self.needs_present = true;
                    IpcResponse::ok(id, Some(serde_json::json!({ "pid": pid })))
                }
                Err(e) => IpcResponse::err(id, e),
            },
            IpcCommand::ReloadTheme => {
                self.app_prefs = de_apps::prefs::AppPrefs::load();
                self.theme = if self.app_prefs.prefs().theme_mode.trim() == "dark" {
                    de_design::tokens::DARK
                } else {
                    de_design::theme::persisted_mode().theme()
                };
                self.shell_dirty = true;
                self.needs_present = true;
                IpcResponse::ok(id, None)
            }
            IpcCommand::SetPanel { open: false, .. } => {
                self.panel_close();
                self.needs_present = true;
                IpcResponse::ok(id, None)
            }
            IpcCommand::SetPanel { open: true, .. }
            | IpcCommand::Screenshot { .. }
            | IpcCommand::ShowOsd { .. } => IpcResponse::err(id, "not_implemented_yet"),
        }
    }

    fn ctl_state_snapshot(&self) -> ShellStateSnapshot {
        let foreground = match self.foreground {
            Foreground::Shell => "shell",
            Foreground::App => "app",
        }
        .to_string();
        let screen = format!("{:?}", self.nav.current).to_ascii_lowercase();
        let focused_surface = self.seat.get_keyboard().and_then(|kb| kb.current_focus());
        let windows = self
            .space
            .elements()
            .map(|w| {
                let app_id = w
                    .toplevel()
                    .and_then(|t| {
                        smithay::wayland::compositor::with_states(t.wl_surface(), |states| {
                            states
                                .data_map
                                .get::<smithay::wayland::shell::xdg::XdgToplevelSurfaceData>()
                                .and_then(|d| d.lock().ok()?.app_id.clone())
                        })
                    })
                    .unwrap_or_default();
                let focused = focused_surface
                    .as_ref()
                    .is_some_and(|fs| w.toplevel().is_some_and(|t| t.wl_surface() == fs));
                WindowSummary { app_id, focused }
            })
            .collect();

        ShellStateSnapshot {
            foreground,
            screen,
            locked: self.locked(),
            output: Size::new(self.output.width as i32, self.output.height as i32),
            insets: InsetSummary {
                ime_bottom: self.insets.ime.bottom,
            },
            windows,
        }
    }
}
