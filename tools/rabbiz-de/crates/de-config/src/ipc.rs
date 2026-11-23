//! Runtime shell control IPC wire contract (Tier 1.6, see `docs/DE-IPC.md`).
//!
//! Line-delimited JSON messages (`\n`-terminated, capped at [`MAX_MESSAGE_BYTES`])
//! sent over `$XDG_RUNTIME_DIR/rabbiz-de-ctl.sock`.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
#[cfg(unix)]
use std::os::unix::net::UnixStream;
#[cfg(unix)]
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;

use de_api::{OrientationRequest, Size};
use serde::{Deserialize, Serialize};

/// Hard upper bound on a single IPC line (16 KiB) to prevent unbounded reads.
pub const MAX_MESSAGE_BYTES: usize = 16 * 1024;

/// Default socket filename inside `$XDG_RUNTIME_DIR`.
pub const SOCKET_NAME: &str = "rabbiz-de-ctl.sock";

/// Resolve the control socket path for the current user session.
///
/// Returns `None` when `XDG_RUNTIME_DIR` is unset or empty — callers must
/// refuse to fall back to `/tmp` and disable IPC instead.
#[must_use]
pub fn socket_path() -> Option<PathBuf> {
    let dir = std::env::var_os("XDG_RUNTIME_DIR").filter(|v| !v.is_empty())?;
    Some(PathBuf::from(dir).join(SOCKET_NAME))
}

/// Request envelope sent from a client to `rabbiz-de`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IpcRequest {
    pub id: u64,
    #[serde(flatten)]
    pub command: IpcCommand,
}

/// Supported control commands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum IpcCommand {
    /// Query current compositor/shell state.
    GetState,
    /// Engage the lock screen / Keyguard.
    Lock,
    /// Launch an app by `.desktop` id.
    Launch { desktop_id: String },
    /// Capture a screenshot (and optionally persist it to Pictures).
    Screenshot {
        #[serde(default = "default_true")]
        save: bool,
    },
    /// Show an on-screen display pill (volume or brightness).
    ShowOsd {
        kind: OsdKind,
        percent: u8,
        #[serde(default)]
        muted: bool,
    },
    /// Reload theme/palette and re-rasterize shell chrome.
    ReloadTheme,
    /// Rescan `.desktop` application catalog and invalidate the home grid.
    ReloadApps,
    /// Open or close a top-edge panel.
    SetPanel { panel: PanelKind, open: bool },
    /// Inject a synthetic touch event into the compositor input pipeline.
    InjectTouch {
        phase: TouchInjectPhase,
        #[serde(default)]
        slot: i32,
        x: i32,
        y: i32,
    },
    /// Inject a synthetic Linux evdev key event into the compositor input pipeline.
    InjectKey {
        keycode: u32,
        #[serde(default = "default_true")]
        pressed: bool,
    },
    /// Inject a synthetic hardware button press (power / volume).
    InjectHwButton { button: HwButtonKind },
    /// Update a synthetic system/hardware state entry in `de_system::mock`.
    SetMock { key: String, value: String },
    /// Screen rotation: `"auto"` follows the sensor, an orientation name
    /// (`portrait|landscape_ccw|portrait_flipped|landscape_cw`) locks to it.
    SetRotation { mode: String },
    /// Orientation the app with `app_id` asks for while it is foreground
    /// (runtime counterpart of `X-Rabbiz-Orientation=`).
    SetAppOrientation {
        app_id: String,
        request: OrientationRequest,
    },
    /// Enable or disable Desktop Mode (floating windows + auto-landscape on handset).
    SetDesktopMode { enabled: bool },
    /// Enable or disable AOSP-style Immersive Mode (hides system bars).
    SetImmersive { enabled: bool },
    /// Resize the virtual output (e.g., 1920x1080 for `rabbiz-mirror` DeX-on-PC).
    SetVirtualOutput { width: u32, height: u32 },
}

const fn default_true() -> bool {
    true
}

/// Touch injection phase for [`IpcCommand::InjectTouch`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TouchInjectPhase {
    Down,
    Move,
    Up,
    Cancel,
}

/// Hardware button kind for [`IpcCommand::InjectHwButton`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HwButtonKind {
    Power,
    VolumeUp,
    VolumeDown,
}

/// OSD pill type for [`IpcCommand::ShowOsd`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OsdKind {
    Volume,
    Brightness,
}

/// Top-edge panel identifier for [`IpcCommand::SetPanel`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PanelKind {
    Notifications,
    Quick,
}

/// Response envelope returned by `rabbiz-de`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IpcResponse {
    pub id: u64,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl IpcResponse {
    /// Build a successful response with optional JSON payload.
    #[must_use]
    pub const fn ok(id: u64, data: Option<serde_json::Value>) -> Self {
        Self {
            id,
            ok: true,
            data,
            error: None,
        }
    }

    /// Build a failed response with a machine-readable error code/reason.
    #[must_use]
    pub fn err(id: u64, reason: impl Into<String>) -> Self {
        Self {
            id,
            ok: false,
            data: None,
            error: Some(reason.into()),
        }
    }
}

/// Summary of one mapped toplevel window returned in [`ShellStateSnapshot`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowSummary {
    pub app_id: String,
    pub focused: bool,
}

/// Compact inset summary included in [`ShellStateSnapshot`].
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct InsetSummary {
    pub ime_bottom: i32,
}

/// Response payload for [`IpcCommand::GetState`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellStateSnapshot {
    pub foreground: String,
    pub screen: String,
    pub locked: bool,
    pub output: Size,
    pub insets: InsetSummary,
    pub windows: Vec<WindowSummary>,
    /// Applied screen orientation name (`portrait`, `landscape_cw`, ...).
    #[serde(default)]
    pub orientation: String,
    /// User rotation policy: follow the sensor.
    #[serde(default)]
    pub auto_rotate: bool,
    /// Whether Desktop Mode (floating windows) is currently active.
    #[serde(default)]
    pub desktop_mode: bool,
    /// Active form factor (`handset`, `tablet`, `desktop`).
    #[serde(default)]
    pub form_factor: String,
    /// Whether Immersive Mode (hidden system bars) is active.
    #[serde(default)]
    pub immersive: bool,
}

/// Decode a single IPC request line, enforcing [`MAX_MESSAGE_BYTES`].
pub fn decode_request(line: &str) -> Result<IpcRequest, String> {
    if line.len() > MAX_MESSAGE_BYTES {
        return Err(format!(
            "message exceeds limit ({} > {MAX_MESSAGE_BYTES} bytes)",
            line.len()
        ));
    }
    serde_json::from_str(line.trim()).map_err(|e| format!("invalid request: {e}"))
}

/// Encode a single IPC request as a newline-terminated JSON line.
pub fn encode_request(req: &IpcRequest) -> Result<String, String> {
    let mut s = serde_json::to_string(req).map_err(|e| format!("encode request: {e}"))?;
    if s.len() >= MAX_MESSAGE_BYTES {
        return Err(format!(
            "request exceeds limit ({} >= {MAX_MESSAGE_BYTES} bytes)",
            s.len()
        ));
    }
    s.push('\n');
    Ok(s)
}

/// Decode a single IPC response line, enforcing [`MAX_MESSAGE_BYTES`].
pub fn decode_response(line: &str) -> Result<IpcResponse, String> {
    if line.len() > MAX_MESSAGE_BYTES {
        return Err(format!(
            "response exceeds limit ({} > {MAX_MESSAGE_BYTES} bytes)",
            line.len()
        ));
    }
    serde_json::from_str(line.trim()).map_err(|e| format!("invalid response: {e}"))
}

/// Encode a single IPC response as a newline-terminated JSON line.
pub fn encode_response(resp: &IpcResponse) -> Result<String, String> {
    let mut s = serde_json::to_string(resp).map_err(|e| format!("encode response: {e}"))?;
    s.push('\n');
    Ok(s)
}

/// Send one request to a running `rabbiz-de` TCP control listener and read its reply.
pub fn send_request_tcp(addr: &str, req: &IpcRequest) -> Result<IpcResponse, String> {
    let payload = encode_request(req)?;
    let timeout = Duration::from_secs(2);
    let sock_addr = addr
        .to_socket_addrs()
        .map_err(|e| format!("resolve {addr}: {e}"))?
        .next()
        .ok_or_else(|| format!("no address for {addr}"))?;
    let mut stream = TcpStream::connect_timeout(&sock_addr, timeout)
        .map_err(|e| format!("connect {addr}: {e}"))?;
    let _ = stream.set_read_timeout(Some(timeout));
    let _ = stream.set_write_timeout(Some(timeout));
    stream
        .write_all(payload.as_bytes())
        .map_err(|e| format!("write request: {e}"))?;
    let mut reader = BufReader::new(stream.take((MAX_MESSAGE_BYTES + 1) as u64));
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .map_err(|e| format!("read response: {e}"))?;
    if line.is_empty() {
        return Err("empty response from compositor".into());
    }
    decode_response(&line)
}

/// Send one request to a running `rabbiz-de` control socket and read its reply.
///
/// When `RABBIZ_DE_CTL_TCP` is set to a non-empty `host:port`, routes over TCP
/// instead of the local Unix domain socket.
#[cfg(unix)]
pub fn send_request(path: &Path, req: &IpcRequest) -> Result<IpcResponse, String> {
    if let Ok(addr) = std::env::var("RABBIZ_DE_CTL_TCP") {
        let trimmed = addr.trim();
        if !trimmed.is_empty() {
            return send_request_tcp(trimmed, req);
        }
    }
    let payload = encode_request(req)?;
    let mut stream =
        UnixStream::connect(path).map_err(|e| format!("connect {}: {e}", path.display()))?;
    let timeout = Some(Duration::from_secs(2));
    let _ = stream.set_read_timeout(timeout);
    let _ = stream.set_write_timeout(timeout);
    stream
        .write_all(payload.as_bytes())
        .map_err(|e| format!("write request: {e}"))?;
    let mut reader = BufReader::new(stream.take((MAX_MESSAGE_BYTES + 1) as u64));
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .map_err(|e| format!("read response: {e}"))?;
    if line.is_empty() {
        return Err("empty response from compositor".into());
    }
    decode_response(&line)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_every_command_variant() {
        let cases = [
            IpcRequest {
                id: 1,
                command: IpcCommand::GetState,
            },
            IpcRequest {
                id: 2,
                command: IpcCommand::Lock,
            },
            IpcRequest {
                id: 3,
                command: IpcCommand::Launch {
                    desktop_id: "org.kde.kalk".into(),
                },
            },
            IpcRequest {
                id: 4,
                command: IpcCommand::Screenshot { save: true },
            },
            IpcRequest {
                id: 5,
                command: IpcCommand::ShowOsd {
                    kind: OsdKind::Volume,
                    percent: 65,
                    muted: false,
                },
            },
            IpcRequest {
                id: 6,
                command: IpcCommand::ReloadTheme,
            },
            IpcRequest {
                id: 7,
                command: IpcCommand::SetPanel {
                    panel: PanelKind::Quick,
                    open: true,
                },
            },
            IpcRequest {
                id: 8,
                command: IpcCommand::ReloadApps,
            },
            IpcRequest {
                id: 9,
                command: IpcCommand::InjectTouch {
                    phase: TouchInjectPhase::Down,
                    slot: 0,
                    x: 360,
                    y: 800,
                },
            },
            IpcRequest {
                id: 10,
                command: IpcCommand::InjectKey {
                    keycode: 28,
                    pressed: true,
                },
            },
            IpcRequest {
                id: 11,
                command: IpcCommand::InjectHwButton {
                    button: HwButtonKind::Power,
                },
            },
            IpcRequest {
                id: 12,
                command: IpcCommand::SetMock {
                    key: "battery".into(),
                    value: "42".into(),
                },
            },
        ];

        for req in cases {
            let wire = serde_json::to_string(&req).expect("serialize");
            let parsed = decode_request(&wire).expect("decode");
            assert_eq!(parsed, req);
        }
    }

    #[test]
    fn screenshot_defaults_save_to_true() {
        let req = decode_request(r#"{"id": 9, "cmd": "screenshot"}"#).expect("decode");
        assert_eq!(req.command, IpcCommand::Screenshot { save: true });
        let req_no_save =
            decode_request(r#"{"id": 10, "cmd": "screenshot", "save": false}"#).expect("decode");
        assert_eq!(req_no_save.command, IpcCommand::Screenshot { save: false });
    }

    #[test]
    fn show_osd_defaults_muted_and_supports_brightness() {
        let vol =
            decode_request(r#"{"id": 11, "cmd": "show_osd", "kind": "volume", "percent": 50}"#)
                .expect("decode vol");
        assert_eq!(
            vol.command,
            IpcCommand::ShowOsd {
                kind: OsdKind::Volume,
                percent: 50,
                muted: false,
            }
        );
        let br = decode_request(
            r#"{"id": 12, "cmd": "show_osd", "kind": "brightness", "percent": 80, "muted": false}"#,
        )
        .expect("decode brightness");
        assert_eq!(
            br.command,
            IpcCommand::ShowOsd {
                kind: OsdKind::Brightness,
                percent: 80,
                muted: false,
            }
        );
    }

    #[test]
    fn set_panel_covers_both_panels_and_directions() {
        for (wire, panel, open) in [
            (
                r#"{"id": 1, "cmd": "set_panel", "panel": "notifications", "open": true}"#,
                PanelKind::Notifications,
                true,
            ),
            (
                r#"{"id": 2, "cmd": "set_panel", "panel": "notifications", "open": false}"#,
                PanelKind::Notifications,
                false,
            ),
            (
                r#"{"id": 3, "cmd": "set_panel", "panel": "quick", "open": true}"#,
                PanelKind::Quick,
                true,
            ),
            (
                r#"{"id": 4, "cmd": "set_panel", "panel": "quick", "open": false}"#,
                PanelKind::Quick,
                false,
            ),
        ] {
            let req = decode_request(wire).expect("decode set_panel");
            assert_eq!(req.command, IpcCommand::SetPanel { panel, open });
        }
    }

    #[test]
    fn screenshot_and_panel_error_responses_roundtrip() {
        let ok = IpcResponse::ok(
            4,
            Some(serde_json::json!({
                "path": "/tmp/rabbiz-de-shot.png",
                "width": 720,
                "height": 1600,
                "saved": false,
            })),
        );
        let line = encode_response(&ok).expect("encode ok");
        assert_eq!(decode_response(&line).expect("decode ok"), ok);

        for code in ["panel_not_allowed", "no_frame_available"] {
            let err = IpcResponse::err(7, code);
            let line = encode_response(&err).expect("encode err");
            assert_eq!(decode_response(&line).expect("decode err"), err);
        }
    }

    #[test]
    fn oversized_message_is_rejected() {
        let huge = "a".repeat(MAX_MESSAGE_BYTES + 1);
        let err = decode_request(&huge).expect_err("must reject oversized message");
        assert!(err.contains("exceeds limit"));
    }

    #[test]
    fn response_encoding_appends_newline_and_omits_none_fields() {
        let snap = ShellStateSnapshot {
            foreground: "shell".into(),
            screen: "home".into(),
            locked: false,
            output: Size::new(720, 1600),
            insets: InsetSummary { ime_bottom: 0 },
            orientation: "portrait".into(),
            auto_rotate: true,
            desktop_mode: false,
            form_factor: "handset".into(),
            immersive: false,
            windows: vec![WindowSummary {
                app_id: "org.kde.kalk".into(),
                focused: false,
            }],
        };
        let resp = IpcResponse::ok(1, Some(serde_json::to_value(&snap).expect("value")));
        let line = encode_response(&resp).expect("encode");
        assert!(line.ends_with('\n'));
        assert!(!line.contains("\"error\""));

        let err = IpcResponse::err(2, "permission_denied");
        let err_line = encode_response(&err).expect("encode err");
        assert!(err_line.contains("\"permission_denied\""));
        assert!(!err_line.contains("\"data\""));
    }

    #[cfg(unix)]
    #[test]
    fn unix_socket_roundtrip() {
        use std::os::unix::net::UnixListener;

        let dir = tempfile::tempdir().expect("tempdir");
        let sock = dir.path().join(SOCKET_NAME);
        let listener = UnixListener::bind(&sock).expect("bind");

        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut reader = BufReader::new(stream.try_clone().expect("clone"));
            let mut line = String::new();
            reader.read_line(&mut line).expect("read");
            let req = decode_request(&line).expect("decode req");
            assert_eq!(req.command, IpcCommand::Lock);
            let reply = encode_response(&IpcResponse::ok(req.id, None)).expect("encode");
            stream.write_all(reply.as_bytes()).expect("write");
        });

        let resp = send_request(
            &sock,
            &IpcRequest {
                id: 42,
                command: IpcCommand::Lock,
            },
        )
        .expect("send_request");
        assert_eq!(resp, IpcResponse::ok(42, None));
        server.join().expect("server thread");
    }

    #[test]
    fn tcp_socket_roundtrip() {
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let addr = listener.local_addr().expect("local_addr").to_string();

        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut reader = BufReader::new(stream.try_clone().expect("clone"));
            let mut line = String::new();
            reader.read_line(&mut line).expect("read");
            let req = decode_request(&line).expect("decode req");
            assert_eq!(req.command, IpcCommand::ReloadApps);
            let reply = encode_response(&IpcResponse::ok(
                req.id,
                Some(serde_json::json!({ "apps": 9 })),
            ))
            .expect("encode");
            stream.write_all(reply.as_bytes()).expect("write");
        });

        let resp = send_request_tcp(
            &addr,
            &IpcRequest {
                id: 77,
                command: IpcCommand::ReloadApps,
            },
        )
        .expect("send_request_tcp");
        assert_eq!(
            resp,
            IpcResponse::ok(77, Some(serde_json::json!({ "apps": 9 })))
        );
        server.join().expect("server thread");
    }

    #[test]
    fn socket_path_returns_none_without_xdg_runtime_dir() {
        let prev = std::env::var_os("XDG_RUNTIME_DIR");
        std::env::remove_var("XDG_RUNTIME_DIR");
        assert_eq!(socket_path(), None);

        std::env::set_var("XDG_RUNTIME_DIR", "");
        assert_eq!(socket_path(), None);

        std::env::set_var("XDG_RUNTIME_DIR", "/run/user/1000");
        assert_eq!(
            socket_path(),
            Some(PathBuf::from("/run/user/1000/rabbiz-de-ctl.sock"))
        );

        match prev {
            Some(v) => std::env::set_var("XDG_RUNTIME_DIR", v),
            None => std::env::remove_var("XDG_RUNTIME_DIR"),
        }
    }
}
