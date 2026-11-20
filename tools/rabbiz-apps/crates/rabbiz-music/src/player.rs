//! mpv backend: one long-lived `mpv --idle=yes --input-ipc-server=<sock>`
//! process per app run, controlled over mpv's JSON-lines IPC on a unix
//! socket. The socket I/O is isolated in [`Mpv`]; every protocol detail
//! (message shape, property names, command encoding) lives in the pure
//! functions [`encode_command`], [`encode_observe`] and [`parse_line`],
//! which the unit tests exercise without a running mpv.
//!
//! Device-proven flags mirror `rootfs-overlay/usr/local/bin/rabbiz-spotify-ctl`
//! (`--ao=pulse,pipewire`, `--no-terminal`, `--force-window=no`): audio goes
//! to the PipeWire/PulseAudio default sink, so volume follows the system
//! sink and the side keys — the app never touches volume itself.

use std::collections::VecDeque;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use serde_json::{json, Value};

/// Socket file name inside `$XDG_RUNTIME_DIR` (spec: `<runtime>/rabbiz-music.sock`).
pub const SOCKET_NAME: &str = "rabbiz-music.sock";

/// Properties observed once per connection; changes arrive as
/// `property-change` events. `time-pos` is additionally polled with
/// `get_property` because mpv throttles its change notifications.
pub const OBSERVED: [&str; 7] = [
    "pause",
    "time-pos",
    "duration",
    "path",
    "filename",
    "media-title",
    "eof-reached",
];

/// Resolve the IPC socket path: `$XDG_RUNTIME_DIR/rabbiz-music.sock`,
/// falling back to `/run/user/<uid>/` then `/tmp`.
#[must_use]
pub fn socket_path() -> PathBuf {
    if let Some(rt) = std::env::var_os("XDG_RUNTIME_DIR") {
        let p = PathBuf::from(rt).join(SOCKET_NAME);
        if !p.as_os_str().is_empty() {
            return p;
        }
    }
    #[cfg(unix)]
    #[allow(unsafe_code)]
    {
        // SAFETY: `libc::getuid` is a thread-safe POSIX syscall with no
        // preconditions and cannot fail.
        let uid = unsafe { libc::getuid() };
        return PathBuf::from(format!("/run/user/{uid}/{SOCKET_NAME}"));
    }
    #[allow(unreachable_code)]
    PathBuf::from(format!("/tmp/{SOCKET_NAME}"))
}

/// `true` when an `mpv` binary is runnable (cheap `--version` probe).
#[must_use]
pub fn mpv_available() -> bool {
    Command::new("mpv")
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// argv for the player process (pure — unit-tested).
#[must_use]
pub fn mpv_args(sock: &Path) -> Vec<String> {
    vec![
        "--idle=yes".to_string(),
        "--no-video".to_string(),
        "--no-terminal".to_string(),
        "--force-window=no".to_string(),
        "--ao=pulse,pipewire".to_string(),
        "--audio-client-name=Rabbiz Music".to_string(),
        "--cache=yes".to_string(),
        "--hr-seek=yes".to_string(),
        "--loop-file=no".to_string(),
        format!("--input-ipc-server={}", sock.display()),
    ]
}

/// One decoded line from the IPC stream.
#[derive(Debug, Clone, PartialEq)]
pub enum Line {
    /// `property-change` event (from `observe_property`).
    Property { name: String, value: Value },
    /// `end-file` event (`reason`: "eof", "stop", "quit", "error", …).
    EndFile { reason: String },
    /// Command/property response (`request_id` matched by the caller).
    Response {
        request_id: u64,
        ok: bool,
        data: Value,
    },
}

/// JSON for `{"command":[...],"request_id":N}`.
#[must_use]
pub fn encode_command(request_id: u64, command: &[Value]) -> String {
    let mut s = String::with_capacity(64);
    s.push_str("{\"command\":");
    s.push_str(&serde_json::to_string(command).unwrap_or_else(|_| "[]".into()));
    s.push_str(",\"request_id\":");
    s.push_str(&request_id.to_string());
    s.push('}');
    s
}

/// JSON for `{"command":["observe_property",id,"name"]}`.
#[must_use]
pub fn encode_observe(observe_id: u64, name: &str) -> String {
    encode_command(
        observe_id,
        &[json!("observe_property"), json!(observe_id), json!(name)],
    )
}

/// Parse one newline-delimited IPC message. Unknown shapes → `None`.
#[must_use]
pub fn parse_line(text: &str) -> Option<Line> {
    let v: Value = serde_json::from_str(text.trim()).ok()?;
    let obj = v.as_object()?;
    match obj.get("event").and_then(Value::as_str) {
        Some("property-change") => Some(Line::Property {
            name: obj.get("name")?.as_str()?.to_string(),
            value: obj.get("data").cloned().unwrap_or(Value::Null),
        }),
        Some("end-file") => Some(Line::EndFile {
            reason: obj
                .get("reason")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        }),
        _ => {
            // Command/get_property response: {"data":…,"error":"success","request_id":N}
            let request_id = obj.get("request_id")?.as_u64()?;
            Some(Line::Response {
                request_id,
                ok: obj.get("error").and_then(Value::as_str) == Some("success"),
                data: obj.get("data").cloned().unwrap_or(Value::Null),
            })
        }
    }
}

/// The player process + its IPC connection. All socket I/O is here; the app
/// only sees [`Mpv::poll`] lines and high-level commands.
pub struct Mpv {
    /// `Some` only when this handle spawned the process itself; client-only
    /// handles ([`Mpv::connect_existing`]) leave it `None` so dropping them
    /// never kills a player owned by someone else (GUI or a detached CLI).
    child: Option<Child>,
    sock: PathBuf,
    stream: Option<UnixStream>,
    /// Partial line carried between non-blocking reads.
    buf: Vec<u8>,
    /// `get_property` request ids still awaiting their response.
    pending_gets: VecDeque<(u64, String)>,
    next_id: u64,
}

impl Mpv {
    /// Spawn `mpv` with the IPC socket at `sock`. The socket file is removed
    /// first so a stale socket from a crashed run can't be connected to.
    pub fn spawn(sock: &Path) -> std::io::Result<Self> {
        let _ = std::fs::remove_file(sock);
        let args = mpv_args(sock);
        let child = Command::new("mpv")
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        Ok(Self {
            child: Some(child),
            sock: sock.to_path_buf(),
            stream: None,
            buf: Vec::with_capacity(4096),
            pending_gets: VecDeque::new(),
            next_id: 1,
        })
    }

    /// Client-only handle: attach to the IPC socket of an mpv that is
    /// already running (spawned by the GUI or by an earlier headless
    /// `play`). Fails fast when nothing listens — callers fall back to
    /// [`Mpv::spawn`] or report "no session" honestly.
    pub fn connect_existing(sock: &Path) -> std::io::Result<Self> {
        let s = UnixStream::connect(sock)?;
        s.set_nonblocking(true)?;
        Ok(Self {
            child: None,
            sock: sock.to_path_buf(),
            stream: Some(s),
            buf: Vec::with_capacity(4096),
            pending_gets: VecDeque::new(),
            next_id: 1,
        })
    }

    /// `true` while the mpv process is running. For client-only handles
    /// (no child) this means the socket still accepts a connection.
    pub fn alive(&mut self) -> bool {
        if let Some(c) = self.child.as_mut() {
            return matches!(c.try_wait(), Ok(None));
        }
        if self.stream.is_none() {
            let _ = self.try_connect();
        }
        self.stream.is_some()
    }

    /// `true` once the IPC socket accepts a connection (mpv creates it early,
    /// but callers retry until then).
    pub fn connected(&self) -> bool {
        self.stream.is_some()
    }

    pub fn try_connect(&mut self) -> bool {
        if self.stream.is_some() {
            return true;
        }
        match UnixStream::connect(&self.sock) {
            Ok(s) => {
                let _ = s.set_nonblocking(true);
                self.stream = Some(s);
                true
            }
            Err(_) => false,
        }
    }

    /// Subscribe to `OBSERVED` properties. Idempotent per connection via the
    /// app's `observed` flag — calling twice just re-subscribes harmlessly.
    pub fn observe_defaults(&mut self) -> bool {
        OBSERVED.iter().all(|n| self.observe(n))
    }

    /// Fire-and-forget command (`loadfile`, `cycle pause`, `seek`, `stop`, …).
    /// `args` are the mpv command words; strings are JSON-escaped.
    pub fn command(&mut self, args: &[&str]) -> bool {
        let id = self.next_id;
        self.next_id += 1;
        let cmd: Vec<Value> = args.iter().map(|a| json!(a)).collect();
        self.send(&encode_command(id, &cmd))
    }

    /// `get_property`, tracked so the response can be matched to `name`.
    pub fn get_property(&mut self, name: &str) -> bool {
        let id = self.next_id;
        self.next_id += 1;
        if !self.send(&encode_command(id, &[json!("get_property"), json!(name)])) {
            return false;
        }
        self.pending_gets.push_back((id, name.to_string()));
        true
    }

    /// `observe_property <id> <name>`; the integer id is mpv's observe slot.
    pub fn observe(&mut self, name: &str) -> bool {
        let id = self.next_id;
        self.next_id += 1;
        self.send(&encode_observe(id, name))
    }

    /// Drain the socket and return decoded messages. A closed/errored stream
    /// drops the connection (mpv may still be alive; `try_connect` retries).
    pub fn poll(&mut self) -> Vec<Line> {
        let mut out = Vec::new();
        // Read until WouldBlock.
        if let Some(s) = self.stream.as_mut() {
            let mut tmp = [0u8; 8192];
            loop {
                match s.read(&mut tmp) {
                    Ok(0) => {
                        self.stream = None;
                        break;
                    }
                    Ok(n) => self.buf.extend_from_slice(&tmp[..n]),
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                    Err(_) => {
                        self.stream = None;
                        break;
                    }
                }
                // Bound the partial-line buffer against a peer that never
                // sends '\n'.
                if self.buf.len() > (1 << 20) {
                    self.buf.clear();
                }
            }
        }
        while let Some(pos) = self.buf.iter().position(|&b| b == b'\n') {
            let raw: Vec<u8> = self.buf.drain(..=pos).collect();
            let Ok(text) = std::str::from_utf8(&raw) else {
                continue;
            };
            let Some(line) = parse_line(text) else {
                continue;
            };
            match line {
                // A get_property response is just a property value: rewrite it
                // as a Property line so the app handles one shape. The pending
                // entry is consumed either way so failures can't leak ids.
                Line::Response {
                    request_id,
                    ok,
                    data,
                } => {
                    if let Some(idx) = self
                        .pending_gets
                        .iter()
                        .position(|(id, _)| *id == request_id)
                    {
                        let (_, name) = self.pending_gets.remove(idx).unwrap_or_default();
                        if ok {
                            out.push(Line::Property { name, value: data });
                        }
                    }
                    // Unmatched responses are fire-and-forget commands —
                    // nothing to do with them.
                }
                other => out.push(other),
            }
        }
        out
    }

    fn send(&mut self, line: &str) -> bool {
        let Some(s) = self.stream.as_mut() else {
            return false;
        };
        let mut bytes = line.as_bytes().to_vec();
        bytes.push(b'\n');
        s.write_all(&bytes).is_ok()
    }

    /// Release the handle without killing mpv. Headless `play` uses this so
    /// playback survives the one-shot CLI process; the leaked socket fd
    /// closes with process exit and mpv simply sees a client disconnect.
    pub fn detach(self) {
        std::mem::forget(self);
    }
}

impl Drop for Mpv {
    fn drop(&mut self) {
        if let Some(mut c) = self.child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

/// Throttle for `get_property time-pos` polls (mpv's own property-change
/// cadence for `time-pos` is not contractual).
pub const POLL_INTERVAL_MS: u128 = 400;

/// How long the app waits for mpv's socket after spawn before declaring the
/// backend dead. Only used for diagnostics; connect retries continue anyway.
#[allow(dead_code)]
pub const CONNECT_BUDGET_MS: u128 = 3000;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_command_shape() {
        let s = encode_command(
            7,
            &[json!("loadfile"), json!("/tmp/a b.mp3"), json!("replace")],
        );
        let v: Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v["request_id"], 7);
        assert_eq!(v["command"][0], "loadfile");
        assert_eq!(v["command"][1], "/tmp/a b.mp3");
        assert_eq!(v["command"][2], "replace");
    }

    #[test]
    fn encode_observe_shape() {
        let s = encode_observe(3, "pause");
        let v: Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v["command"], json!(["observe_property", 3, "pause"]));
    }

    #[test]
    fn parse_property_change() {
        let line = r#"{"event":"property-change","id":2,"name":"pause","data":true}"#;
        assert_eq!(
            parse_line(line),
            Some(Line::Property {
                name: "pause".into(),
                value: json!(true)
            })
        );
    }

    #[test]
    fn parse_end_file_and_response() {
        assert_eq!(
            parse_line(r#"{"event":"end-file","reason":"eof","file_error":""}"#),
            Some(Line::EndFile {
                reason: "eof".into()
            })
        );
        assert_eq!(
            parse_line(r#"{"data":42.5,"error":"success","request_id":9}"#),
            Some(Line::Response {
                request_id: 9,
                ok: true,
                data: json!(42.5)
            })
        );
        assert_eq!(
            parse_line(r#"{"data":"msg","error":"success","request_id":1}"#)
                .map(|l| matches!(l, Line::Response { ok: true, .. })),
            Some(true)
        );
        assert!(parse_line("not json").is_none());
        assert!(parse_line(r#"{"event":"unknown"}"#).is_none());
    }

    #[test]
    fn mpv_args_carry_ipc_socket_and_pipewire() {
        let args = mpv_args(Path::new("/run/user/1001/rabbiz-music.sock"));
        let joined = args.join(" ");
        assert!(joined.contains("--input-ipc-server=/run/user/1001/rabbiz-music.sock"));
        assert!(joined.contains("--ao=pulse,pipewire"));
        assert!(joined.contains("--no-video"));
        assert!(joined.contains("--idle=yes"));
    }
}
