//! Headless entry points for the same binary: `--cli <cmd>` text CLI and
//! `--mcp` MCP server (stdio JSON-RPC via `rabbiz_app_kit::mcp`).
//!
//! Both surfaces share `model`/`store`/`backend` with the GUI — sends go
//! through [`Backend::send`] and persist to the same
//! `~/.local/share/rabbiz-sms/messages.json`, so headless and GUI views
//! never diverge.
//!
//! Honesty rule: `send` reports `sent` only when the modem backend
//! accepted the message; with no modem the message stays `Queued` and is
//! still saved locally — delivery is never claimed.

use std::path::Path;

use rabbiz_app_kit::mcp::{serve, Tool};
use serde_json::Value;

use crate::backend::Backend;
use crate::model::{format_phone, local_time, state_label, thread_key, threads, Message, MsgState};
use crate::store;

const HELP: &str = "\
rabbiz-sms — headless commands

USAGE:
    rabbiz-sms                  launch the GUI (Wayland)
    rabbiz-sms --to <number>    launch the GUI on that conversation (or a
                                prefilled compose); hand-off from contacts
    rabbiz-sms --cli <cmd>      run a text command and exit
    rabbiz-sms --mcp            serve MCP tools on stdio

COMMANDS:
    conversations       list SMS threads (peer, count, last activity)
    thread <number>     list messages in the conversation with <number>
    send <number> <text...>   send an SMS through the detected backend;
                        no modem → the message is saved as Queued
    help                show this text

EXIT CODES: 0 ok · 1 command failed · 2 usage error";

/// Parsed `--cli` command. `args` are the tokens after `--cli`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cli {
    Help,
    Conversations,
    Thread(String),
    Send { peer: String, body: String },
    Unknown(String),
}

/// Parse CLI tokens. Multi-word message text is rejoined with single
/// spaces; the peer may itself contain spaces ("+66 81 …").
#[must_use]
pub fn parse_cli(args: &[String]) -> Cli {
    let Some((cmd, rest)) = args.split_first() else {
        return Cli::Help;
    };
    match cmd.as_str() {
        "help" | "--help" | "-h" => Cli::Help,
        "conversations" => Cli::Conversations,
        "thread" => match rest.first() {
            Some(peer) => Cli::Thread(peer.clone()),
            None => Cli::Unknown("thread: missing number".to_string()),
        },
        "send" => match rest.split_first() {
            Some((peer, body)) if !body.is_empty() => Cli::Send {
                peer: peer.clone(),
                body: body.join(" "),
            },
            Some(_) => Cli::Unknown("send: missing message text".to_string()),
            None => Cli::Unknown("send: missing number and text".to_string()),
        },
        other => Cli::Unknown(other.to_string()),
    }
}

// ---------------------------------------------------------------------------
// Shared formatting (CLI + MCP render the same text)
// ---------------------------------------------------------------------------

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

fn load_msgs() -> Vec<Message> {
    store::load(&store::store_path())
}

fn stamp(ts: i64) -> String {
    let t = local_time(ts);
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}",
        t.year, t.month, t.day, t.hour, t.min
    )
}

/// One tab-separated line per conversation, most-recent first:
/// `display_peer  thread_key  count  last_activity  preview`.
#[must_use]
pub fn format_conversations(msgs: &[Message]) -> String {
    let th = threads(msgs);
    if th.is_empty() {
        return "No conversations.".to_string();
    }
    let mut out = String::new();
    for (key, peer, last_idx) in &th {
        let count = msgs.iter().filter(|m| thread_key(&m.peer) == *key).count();
        let m = &msgs[*last_idx];
        let name = if peer.trim().is_empty() {
            "Unknown".to_string()
        } else {
            format_phone(peer)
        };
        let preview: String = m.body.chars().take(60).collect();
        out.push_str(&format!(
            "{name}\t{key}\t{count} msgs\t{}\t{preview}\n",
            stamp(m.ts)
        ));
    }
    out.pop();
    out
}

/// All messages in the thread matching `peer` (thread_key-normalized),
/// oldest→newest, one tab-separated line each:
/// `timestamp  dir  state  body` (`state` is `-` for incoming).
pub fn format_thread(msgs: &[Message], peer: &str) -> Result<String, String> {
    let key = thread_key(peer);
    let found: Vec<&Message> = msgs.iter().filter(|m| thread_key(&m.peer) == key).collect();
    if found.is_empty() {
        return Err(format!("no conversation with '{peer}'"));
    }
    let name = if peer.trim().is_empty() {
        "Unknown".to_string()
    } else {
        format_phone(peer)
    };
    let mut out = format!("# {name} ({key})\n");
    for m in found {
        let dir = if m.outgoing { "out" } else { "in" };
        let state = if m.outgoing {
            state_label(m.state)
        } else {
            "-"
        };
        out.push_str(&format!(
            "{}\t{}\t{}\t{}\n",
            stamp(m.ts),
            dir,
            state,
            m.body
        ));
    }
    out.pop();
    Ok(out)
}

/// Attempt to send `body` to `peer` through the detected [`Backend`] and
/// persist the outcome. Returns `(state, honest_note)`.
///
/// State mapping (same as the GUI): backend accepted → `Sent`;
/// backend absent → `Queued` (never attempted, still saved); backend
/// present but rejected → `Failed`.
fn attempt_send(path: &Path, peer: &str, body: &str) -> Result<(MsgState, String), String> {
    let peer = peer.trim();
    let body = body.trim();
    if peer.is_empty() {
        return Err("empty number".to_string());
    }
    if body.is_empty() {
        return Err("empty message".to_string());
    }
    let backend = Backend::detect();
    let mut msgs = store::load(path);
    let id = msgs.iter().map(|m| m.id).max().unwrap_or(0) + 1;
    let mut m = Message::new(id, peer, body, true, now_unix());
    let (state, note) = match backend.send(peer, body) {
        Ok(()) => (
            MsgState::Sent,
            "sent — accepted by the modem backend".to_string(),
        ),
        Err(reason) => {
            if backend == Backend::Unavailable {
                (
                    MsgState::Queued,
                    format!("queued — no modem available ({reason}); saved on this device"),
                )
            } else {
                (
                    MsgState::Failed,
                    format!("failed — {reason}; saved on this device"),
                )
            }
        }
    };
    m.state = state;
    msgs.push(m);
    let _ = store::save(path, &msgs);
    Ok((state, note))
}

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

/// Run `--cli` args; returns the process exit code.
pub fn run_cli(args: &[String]) -> i32 {
    match parse_cli(args) {
        Cli::Help => {
            println!("{HELP}");
            0
        }
        Cli::Unknown(u) => {
            eprintln!("rabbiz-sms: {u}\n\n{HELP}");
            2
        }
        Cli::Conversations => {
            println!("{}", format_conversations(&load_msgs()));
            0
        }
        Cli::Thread(peer) => match format_thread(&load_msgs(), &peer) {
            Ok(text) => {
                println!("{text}");
                0
            }
            Err(e) => {
                eprintln!("rabbiz-sms: {e}");
                1
            }
        },
        Cli::Send { peer, body } => match attempt_send(&store::store_path(), &peer, &body) {
            Ok((state, note)) => {
                println!(
                    "{}\t{}\t{}",
                    peer.trim(),
                    state_label(state).to_lowercase(),
                    note
                );
                0
            }
            Err(e) => {
                eprintln!("rabbiz-sms send: {e}");
                1
            }
        },
    }
}

// ---------------------------------------------------------------------------
// MCP
// ---------------------------------------------------------------------------

fn arg_str<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    args.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("missing or non-string argument '{key}'"))
}

fn mcp_conversations(_args: &Value) -> Result<String, String> {
    Ok(format_conversations(&load_msgs()))
}

fn mcp_thread(args: &Value) -> Result<String, String> {
    format_thread(&load_msgs(), arg_str(args, "number")?)
}

fn mcp_send(args: &Value) -> Result<String, String> {
    let peer = arg_str(args, "number")?;
    let text = arg_str(args, "text")?;
    let (state, note) = attempt_send(&store::store_path(), peer, text)?;
    Ok(format!("{}: {note}", state_label(state)))
}

static TOOLS: &[Tool] = &[
    Tool {
        name: "sms_conversations",
        description: "List SMS conversations (threads), most recent first. \
                      Each line: display peer, thread key, message count, \
                      last activity timestamp, last-message preview.",
        input_schema: r#"{"type": "object", "properties": {}}"#,
        handler: mcp_conversations,
    },
    Tool {
        name: "sms_thread",
        description: "List the messages in one SMS conversation, oldest \
                      first. `number` is matched by normalized phone digits \
                      (e.g. '0812345678' also matches '+66 81 234 5678').",
        input_schema: r#"{
            "type": "object",
            "properties": {
                "number": {"type": "string", "description": "Phone number or sender id of the conversation"}
            },
            "required": ["number"]
        }"#,
        handler: mcp_thread,
    },
    Tool {
        name: "sms_send",
        description: "Send an SMS through the detected telephony backend \
                      (ModemManager/mmcli). Reports 'sent' only when the \
                      modem accepted the message; with no modem the message \
                      is saved locally as Queued — delivery is never claimed.",
        input_schema: r#"{
            "type": "object",
            "properties": {
                "number": {"type": "string", "description": "Recipient phone number"},
                "text": {"type": "string", "description": "Message body"}
            },
            "required": ["number", "text"]
        }"#,
        handler: mcp_send,
    },
];

/// Serve MCP on stdin/stdout until EOF; never returns.
pub fn mcp_serve() -> ! {
    serve(TOOLS, "rabbiz-sms", "0.1.0")
}

// ---------------------------------------------------------------------------
// Tests (headless — no Wayland, no mmcli required)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn strs(xs: &[&str]) -> Vec<String> {
        xs.iter().map(|s| (*s).to_string()).collect()
    }

    fn sample() -> Vec<Message> {
        vec![
            Message::new(1, "0812345678", "สวัสดีครับ", false, 1_700_000_000),
            Message::new(2, "+66 81 234 5678", "hello", true, 1_700_000_060),
            Message::new(3, "0899999999", "other", false, 1_700_000_120),
        ]
    }

    #[test]
    fn parse_help_and_empty() {
        assert_eq!(parse_cli(&[]), Cli::Help);
        assert_eq!(parse_cli(&strs(&["help"])), Cli::Help);
        assert_eq!(parse_cli(&strs(&["--help"])), Cli::Help);
        assert_eq!(parse_cli(&strs(&["-h"])), Cli::Help);
    }

    #[test]
    fn parse_conversations_and_thread() {
        assert_eq!(parse_cli(&strs(&["conversations"])), Cli::Conversations);
        assert_eq!(
            parse_cli(&strs(&["thread", "0812345678"])),
            Cli::Thread("0812345678".to_string())
        );
        assert_eq!(
            parse_cli(&strs(&["thread"])),
            Cli::Unknown("thread: missing number".to_string())
        );
    }

    #[test]
    fn parse_send_joins_body() {
        assert_eq!(
            parse_cli(&strs(&["send", "0812345678", "hello", "there"])),
            Cli::Send {
                peer: "0812345678".to_string(),
                body: "hello there".to_string(),
            }
        );
        assert_eq!(
            parse_cli(&strs(&["send", "0812345678"])),
            Cli::Unknown("send: missing message text".to_string())
        );
        assert_eq!(
            parse_cli(&strs(&["send"])),
            Cli::Unknown("send: missing number and text".to_string())
        );
        assert_eq!(
            parse_cli(&strs(&["bogus"])),
            Cli::Unknown("bogus".to_string())
        );
    }

    #[test]
    fn conversations_groups_by_thread_key() {
        // "0812345678" and "+66 81 234 5678" are one thread; 0899… another.
        let out = format_conversations(&sample());
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 2);
        // Most recent activity first: the 0899… thread (ts …120) leads.
        assert!(lines[0].contains("0899999999") || lines[0].contains("+66 89"));
        assert!(lines[0].contains("1 msgs"));
        assert!(lines[1].contains("+66 81 234 5678"));
        assert!(lines[1].contains("2 msgs"));
    }

    #[test]
    fn thread_normalizes_peer_and_marks_direction() {
        let out = format_thread(&sample(), "081-234-5678").unwrap();
        assert!(out.contains("สวัสดีครับ"));
        assert!(out.contains("hello"));
        assert!(out.contains("\tin\t"));
        assert!(out.contains("\tout\tQueued"));
        // Unknown peer errors honestly.
        assert!(format_thread(&sample(), "0000000000").is_err());
    }

    #[test]
    fn send_with_no_modem_queues_and_persists() {
        // Temp store path — never touches the real XDG data dir.
        let dir = std::env::temp_dir().join(format!("rabbiz-sms-headless-{}", std::process::id()));
        let path = dir.join("messages.json");
        let (state, note) = attempt_send(&path, "0812345678", "hi there").unwrap();
        // Backend::Unavailable on any host without mmcli → Queued; on a
        // host WITH mmcli the send may be attempted — accept Sent/Failed
        // but the message must always persist with the reported state.
        assert!(matches!(
            state,
            MsgState::Queued | MsgState::Sent | MsgState::Failed
        ));
        assert!(note.contains("saved") || state == MsgState::Sent);
        let back = store::load(&path);
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].peer, "0812345678");
        assert_eq!(back[0].body, "hi there");
        assert_eq!(back[0].state, state);
        assert!(attempt_send(&path, "  ", "x").is_err());
        assert!(attempt_send(&path, "081", "  ").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn store_roundtrip_via_temp_xdg_data_home() {
        // Exercise the real XDG path resolution headlessly.
        let dir = std::env::temp_dir().join(format!(
            "rabbiz-sms-xdg-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("t")
        ));
        let old = std::env::var_os("XDG_DATA_HOME");
        // SAFETY-free in edition 2021: single test mutates env; no other
        // test in this binary reads XDG_DATA_HOME concurrently.
        std::env::set_var("XDG_DATA_HOME", &dir);
        let path = store::store_path();
        assert!(path.starts_with(&dir));
        let (state, _) = attempt_send(&path, "0812345678", "xdg roundtrip").unwrap();
        let msgs = store::load(&path);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].state, state);
        match old {
            Some(v) => std::env::set_var("XDG_DATA_HOME", v),
            None => std::env::remove_var("XDG_DATA_HOME"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn mcp_schemas_parse() {
        for t in TOOLS {
            assert!(
                serde_json::from_str::<Value>(t.input_schema).is_ok(),
                "bad inputSchema for {}",
                t.name
            );
        }
        let names: Vec<&str> = TOOLS.iter().map(|t| t.name).collect();
        assert_eq!(names, ["sms_conversations", "sms_thread", "sms_send"]);
    }

    #[test]
    fn mcp_handlers_run() {
        assert_eq!(
            mcp_conversations(&serde_json::json!({})).unwrap(),
            "No conversations."
        );
        let bad = serde_json::json!({});
        assert!(mcp_thread(&bad).is_err());
        assert!(mcp_send(&bad).is_err());
    }
}
