//! Headless entry points for the same binary: `--cli <cmd>` text CLI and
//! `--mcp` MCP server (stdio JSON-RPC via `rabbiz_app_kit::mcp`).
//!
//! Honesty rule: the CLI/MCP never claims a call happened unless the
//! detected [`Backend`] is ModemManager and `mmcli` accepted the call.
//! On the simulated backend both surfaces report the backend status line
//! and place nothing.

use rabbiz_app_kit::mcp::{serve, Tool};
use serde_json::Value;

use crate::app::{now_unix, DialerApp};
use crate::backend::{self, Backend, BackendKind};
use crate::contacts;
use crate::model::{self, CallDir, Contact, Recents};

const HELP: &str = "\
rabbiz-dialer — headless commands

USAGE:
    rabbiz-dialer                launch the GUI (Wayland)
    rabbiz-dialer --dial <num>   launch the GUI with the keypad prefilled
                                 (hand-off from rabbiz-contacts; no auto-call)
    rabbiz-dialer --cli <cmd>    run a text command and exit
    rabbiz-dialer --mcp          serve MCP tools on stdio

COMMANDS:
    dial <number>        place a call through the detected backend
                         (ModemManager/mmcli only — a simulated backend
                         reports its status and places nothing)
    recents              list the persisted call log
                         (~/.local/share/rabbiz-dialer/recents.json)
    contact <query>      look up a contact by name or number
    help                 show this text

EXIT CODES: 0 ok · 1 command failed · 2 usage error · 3 no real backend";

/// Parsed `--cli` command. `args` are the tokens after `--cli`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cli {
    Help,
    Recents,
    Contact(String),
    Dial(String),
    Unknown(String),
}

/// Parse CLI tokens. Multi-word queries (contact names, "+66 81 …") are
/// rejoined with single spaces; `digits_only` strips them for dialing.
#[must_use]
pub fn parse_cli(args: &[String]) -> Cli {
    let Some((cmd, rest)) = args.split_first() else {
        return Cli::Help;
    };
    let query = rest.join(" ");
    match cmd.as_str() {
        "help" | "--help" | "-h" => Cli::Help,
        "recents" => Cli::Recents,
        "contact" if !query.is_empty() => Cli::Contact(query),
        "dial" if !query.is_empty() => Cli::Dial(query),
        "contact" | "dial" => Cli::Unknown(format!("{cmd}: missing argument")),
        other => Cli::Unknown(other.to_string()),
    }
}

// ---------------------------------------------------------------------------
// Shared formatting (CLI + MCP render the same text)
// ---------------------------------------------------------------------------

/// One line per recent call, newest first.
#[must_use]
pub fn format_recents(recents: &Recents, now: i64) -> String {
    if recents.entries.is_empty() {
        return "No calls in recents.".to_string();
    }
    let mut out = String::new();
    for e in &recents.entries {
        let name = if e.name.is_empty() {
            "-".to_string()
        } else {
            e.name.clone()
        };
        out.push_str(&format!(
            "{}\t{}\t{}\t{}\n",
            model::when_label(now, e.unix),
            e.dir.label(),
            model::format_number(&e.number),
            name,
        ));
    }
    // Drop the trailing newline for MCP-friendliness; CLI adds its own.
    out.pop();
    out
}

/// Contacts matching a free-text query against the resolved list (shared
/// store when present, built-in demo otherwise): exact/normalized number
/// lookup first, otherwise case-insensitive name/number substring filter.
#[must_use]
pub fn contact_search(query: &str) -> Vec<Contact> {
    let all = contacts::resolved();
    let digits = model::digits_only(query);
    if !digits.is_empty() {
        if let Some(c) = model::lookup_in(&all, query) {
            return vec![c.clone()];
        }
    }
    all.iter()
        .filter(|c| model::contact_matches(c, query))
        .cloned()
        .collect()
}

fn format_contact(c: &Contact) -> String {
    let fav = if c.favorite { " ★" } else { "" };
    format!(
        "{}{}\t{}\t{}",
        c.name,
        fav,
        model::format_number(&c.number),
        c.label
    )
}

/// Render contact search results.
#[must_use]
pub fn format_contacts(found: &[Contact]) -> String {
    if found.is_empty() {
        return "No contacts match.".to_string();
    }
    found
        .iter()
        .map(format_contact)
        .collect::<Vec<_>>()
        .join("\n")
}

// ---------------------------------------------------------------------------
// Dialing — real path only; simulated backend is reported, never faked
// ---------------------------------------------------------------------------

/// Attempt a call. `Ok` text is the honest success report; `Err` carries
/// either a usage problem or the backend's status line when no real call
/// path exists. A placed call is recorded in recents (Outgoing) exactly
/// like the GUI records it when the call UI opens.
fn attempt_dial(number: &str) -> Result<String, String> {
    let n = model::digits_only(number);
    if n.is_empty() {
        return Err("empty number after filtering".to_string());
    }
    let be = Backend::detect();
    match be.kind {
        BackendKind::Mmcli { modem } => {
            let path = backend::mmcli_create_call(modem, &n)?;
            let mut recents = DialerApp::load_recents();
            let name = contacts::lookup(&n).map_or(String::new(), |c| c.name);
            recents.add(&n, &name, CallDir::Out, now_unix());
            DialerApp::persist_recents(&recents);
            let obj = if path.is_empty() {
                "call object path not reported".to_string()
            } else {
                format!("call object {path}")
            };
            Ok(format!(
                "call placed via ModemManager (modem {modem}); {obj}; backend: {}",
                be.status_line()
            ))
        }
        BackendKind::Simulated => Err(format!(
            "no call placed — backend is {} ({}); simulated calls are GUI-only",
            be.chip(),
            be.status_line()
        )),
    }
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
            eprintln!("rabbiz-dialer: unknown command '{u}'\n\n{HELP}");
            2
        }
        Cli::Recents => {
            let recents = DialerApp::load_recents();
            println!("{}", format_recents(&recents, now_unix()));
            0
        }
        Cli::Contact(q) => {
            let found = contact_search(&q);
            if found.is_empty() {
                println!("No contacts match \"{q}\".");
                1
            } else {
                println!("{}", format_contacts(&found));
                0
            }
        }
        Cli::Dial(n) => match attempt_dial(&n) {
            Ok(msg) => {
                println!("dial {}: {}", model::digits_only(&n), msg);
                0
            }
            Err(msg) => {
                eprintln!("dial {}: {}", model::digits_only(&n), msg);
                // 3 = detected backend cannot place real calls; 1 = mmcli
                // accepted nothing / other failure.
                if Backend::detect().kind == BackendKind::Simulated {
                    3
                } else {
                    1
                }
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

fn mcp_dial(args: &Value) -> Result<String, String> {
    attempt_dial(arg_str(args, "number")?)
}

fn mcp_recents(_args: &Value) -> Result<String, String> {
    let recents = DialerApp::load_recents();
    Ok(format_recents(&recents, now_unix()))
}

fn mcp_contact(args: &Value) -> Result<String, String> {
    let q = arg_str(args, "query")?;
    Ok(format_contacts(&contact_search(q)))
}

static TOOLS: &[Tool] = &[
    Tool {
        name: "dial",
        description: "Place a phone call through the detected telephony backend \
                      (ModemManager/mmcli). Fails honestly when only the \
                      simulated backend exists — no call is faked.",
        input_schema: r#"{
            "type": "object",
            "properties": {
                "number": {"type": "string", "description": "Phone number or contact name to dial"}
            },
            "required": ["number"]
        }"#,
        handler: mcp_dial,
    },
    Tool {
        name: "recents_list",
        description: "List the persisted call log (recents), newest first.",
        input_schema: r#"{"type": "object", "properties": {}}"#,
        handler: mcp_recents,
    },
    Tool {
        name: "contact_lookup",
        description: "Look up a contact by name or phone number. Returns \
                      matching demo contacts (name, formatted number, label).",
        input_schema: r#"{
            "type": "object",
            "properties": {
                "query": {"type": "string", "description": "Contact name or number"}
            },
            "required": ["query"]
        }"#,
        handler: mcp_contact,
    },
];

/// Serve MCP on stdin/stdout until EOF; never returns.
pub fn mcp_serve() -> ! {
    serve(TOOLS, "rabbiz-dialer", "0.1.0")
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

    #[test]
    fn parse_help_and_empty() {
        assert_eq!(parse_cli(&[]), Cli::Help);
        assert_eq!(parse_cli(&strs(&["help"])), Cli::Help);
        assert_eq!(parse_cli(&strs(&["--help"])), Cli::Help);
    }

    #[test]
    fn parse_recents() {
        assert_eq!(parse_cli(&strs(&["recents"])), Cli::Recents);
        // Extra args are ignored for flag-less commands.
        assert_eq!(parse_cli(&strs(&["recents", "x"])), Cli::Recents);
    }

    #[test]
    fn parse_contact_and_dial_join_args() {
        assert_eq!(
            parse_cli(&strs(&["contact", "sarah", "chen"])),
            Cli::Contact("sarah chen".to_string())
        );
        assert_eq!(
            parse_cli(&strs(&["dial", "+66", "81", "234", "5678"])),
            Cli::Dial("+66 81 234 5678".to_string())
        );
    }

    #[test]
    fn parse_missing_and_unknown() {
        assert_eq!(
            parse_cli(&strs(&["dial"])),
            Cli::Unknown("dial: missing argument".to_string())
        );
        assert_eq!(
            parse_cli(&strs(&["contact"])),
            Cli::Unknown("contact: missing argument".to_string())
        );
        assert_eq!(
            parse_cli(&strs(&["bogus"])),
            Cli::Unknown("bogus".to_string())
        );
    }

    #[test]
    fn contact_search_by_name_and_number() {
        let by_name = contact_search("sarah");
        assert_eq!(by_name.len(), 1);
        assert_eq!(by_name[0].name, "Sarah Chen");

        let by_number = contact_search("0812345678");
        assert_eq!(by_number.len(), 1);
        assert_eq!(by_number[0].name, "Alex");

        // International form resolves through lookup_contact.
        let intl = contact_search("+66 81 234 5678");
        assert_eq!(intl.len(), 1);
        assert_eq!(intl[0].name, "Alex");

        assert!(contact_search("zzz-no-such-person").is_empty());
    }

    #[test]
    fn format_recents_empty_and_seeded() {
        assert_eq!(
            format_recents(&Recents::default(), 0),
            "No calls in recents."
        );

        let mut r = Recents::default();
        r.add("0812345678", "Alex", CallDir::Out, 1_000);
        let text = format_recents(&r, 86_500);
        assert!(text.contains("Outgoing"));
        assert!(text.contains("081-234-5678"));
        assert!(text.contains("Alex"));
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
        assert_eq!(names, ["dial", "recents_list", "contact_lookup"]);
    }

    #[test]
    fn mcp_contact_lookup_handler() {
        let args = serde_json::json!({"query": "mom"});
        let out = mcp_contact(&args).unwrap();
        assert!(out.contains("Mom"));
        assert!(out.contains("086-555-1234"));

        let bad = serde_json::json!({});
        assert!(mcp_contact(&bad).is_err());

        let none = serde_json::json!({"query": "zzz"});
        assert_eq!(mcp_contact(&none).unwrap(), "No contacts match.");
    }

    #[test]
    fn mcp_recents_handler_runs() {
        // Reads whatever XDG/HOME resolves to (demo seed when absent) —
        // must always produce text, never panic.
        let out = mcp_recents(&serde_json::json!({})).unwrap();
        assert!(!out.is_empty());
    }
}
