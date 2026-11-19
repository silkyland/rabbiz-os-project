//! Runtime shell control IPC wire contract (Tier 1.6, see `docs/DE-IPC.md`).
//!
//! Line-delimited JSON messages (`\n`-terminated, capped at [`MAX_MESSAGE_BYTES`])
//! sent over `$XDG_RUNTIME_DIR/rabbiz-de-ctl.sock`.

use std::path::PathBuf;

use de_api::Size;
use serde::{Deserialize, Serialize};

/// Hard upper bound on a single IPC line (16 KiB) to prevent unbounded reads.
pub const MAX_MESSAGE_BYTES: usize = 16 * 1024;

/// Default socket filename inside `$XDG_RUNTIME_DIR`.
pub const SOCKET_NAME: &str = "rabbiz-de-ctl.sock";

/// Resolve the control socket path for the current user session.
#[must_use]
pub fn socket_path() -> PathBuf {
    if let Ok(dir) = std::env::var("XDG_RUNTIME_DIR") {
        if !dir.is_empty() {
            return PathBuf::from(dir).join(SOCKET_NAME);
        }
    }
    PathBuf::from("/tmp").join(SOCKET_NAME)
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
    /// Open or close a top-edge panel.
    SetPanel { panel: PanelKind, open: bool },
}

const fn default_true() -> bool {
    true
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

/// Encode a single IPC response as a newline-terminated JSON line.
pub fn encode_response(resp: &IpcResponse) -> Result<String, String> {
    let mut s = serde_json::to_string(resp).map_err(|e| format!("encode response: {e}"))?;
    s.push('\n');
    Ok(s)
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
}
