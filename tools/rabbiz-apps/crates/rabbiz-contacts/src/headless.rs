//! Headless entry points for the same binary: `--cli <cmd>` text CLI and
//! `--mcp` MCP server (stdio JSON-RPC via `rabbiz_app_kit::mcp`).
//!
//! Both surfaces go through the same `store.rs` load/save path as the GUI,
//! so atomic writes, demo seeding and corrupt-file quarantine behave
//! identically in every mode.

use std::path::{Path, PathBuf};

use rabbiz_app_kit::mcp::{serve, Tool};
use serde_json::Value;

use crate::model::{self, Contact};
use crate::store;

const HELP: &str = "\
rabbiz-contacts — headless commands

USAGE:
    rabbiz-contacts              launch the GUI (Wayland)
    rabbiz-contacts --cli <cmd>  run a text command and exit
    rabbiz-contacts --mcp        serve MCP tools on stdio

COMMANDS:
    list                     all contacts, display order (favorites first,
                            then name): name, favorite, phones, emails
    search <query>           contacts matching name/phone/email substring
    get <id-or-name>         full contact detail (phones/emails with labels,
                            note, created/updated)
    add <name> [phone] [email]
                            create a contact; trailing phone-like /
                            email-like tokens are taken as the optional
                            phone and email, the rest is the name
    set-favorite <id-or-name> <on|off>
                            mark a contact favorite or not
    delete <id-or-name>      delete a contact
    help                     show this text

EXIT CODES: 0 ok · 1 command failed · 2 usage error";

// ---------------------------------------------------------------------------
// CLI parsing
// ---------------------------------------------------------------------------

/// Parsed `--cli` command. `args` are the tokens after `--cli`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cli {
    Help,
    List,
    Search(String),
    Get(String),
    Add {
        name: String,
        phone: Option<String>,
        email: Option<String>,
    },
    SetFavorite {
        query: String,
        on: bool,
    },
    Delete(String),
    Unknown(String),
}

/// A token that can be a phone number: has digits and at least 3 dialable
/// characters after `digits_only` (so "User2" stays part of a name but
/// "081-234" and "+66 81" are taken as a phone).
fn looks_like_phone(tok: &str) -> bool {
    model::digits_only(tok).chars().count() >= 3
}

fn looks_like_email(tok: &str) -> bool {
    tok.contains('@')
}

/// Split `add` tokens into (name, phone, email): a trailing email-like token
/// is the email, a trailing phone-like token before it is the phone, and
/// everything left is the (possibly multi-word) name.
#[must_use]
pub fn split_add_args(tokens: &[String]) -> (String, Option<String>, Option<String>) {
    let mut rest: Vec<&str> = tokens.iter().map(String::as_str).collect();
    let email = rest
        .pop_if(|last| looks_like_email(last))
        .map(str::to_string);
    let phone = rest
        .pop_if(|last| looks_like_phone(last))
        .map(str::to_string);
    (rest.join(" "), phone, email)
}

/// Parse CLI tokens. Multi-word names and queries are rejoined with single
/// spaces; `set-favorite` takes its on/off flag as the last token.
#[must_use]
pub fn parse_cli(args: &[String]) -> Cli {
    let Some((cmd, rest)) = args.split_first() else {
        return Cli::Help;
    };
    let query = rest.join(" ");
    match cmd.as_str() {
        "help" | "--help" | "-h" => Cli::Help,
        "list" => Cli::List,
        "search" if !query.is_empty() => Cli::Search(query),
        "get" if !query.is_empty() => Cli::Get(query),
        "delete" if !query.is_empty() => Cli::Delete(query),
        "add" if !rest.is_empty() => {
            let (name, phone, email) = split_add_args(rest);
            if name.is_empty() && phone.is_none() && email.is_none() {
                Cli::Unknown("add: missing name".to_string())
            } else {
                Cli::Add { name, phone, email }
            }
        }
        "set-favorite" => {
            let Some((on_tok, query_toks)) = rest.split_last() else {
                return Cli::Unknown("set-favorite: missing arguments".to_string());
            };
            let on = match on_tok.to_lowercase().as_str() {
                "on" | "true" | "yes" | "1" => true,
                "off" | "false" | "no" | "0" => false,
                other => {
                    return Cli::Unknown(format!("set-favorite: bad flag '{other}' (on|off)"));
                }
            };
            let q = query_toks.join(" ");
            if q.is_empty() {
                Cli::Unknown("set-favorite: missing contact".to_string())
            } else {
                Cli::SetFavorite { query: q, on }
            }
        }
        "search" | "get" | "delete" | "add" => Cli::Unknown(format!("{cmd}: missing argument")),
        other => Cli::Unknown(other.to_string()),
    }
}

// ---------------------------------------------------------------------------
// Shared store ops — used by both the CLI commands and the MCP tools.
// ---------------------------------------------------------------------------

fn data_dir() -> PathBuf {
    store::data_dir()
}

fn load(dir: &Path) -> Vec<Contact> {
    store::load(dir)
}

fn save(dir: &Path, contacts: &mut [Contact]) {
    model::sort_contacts(contacts);
    store::save(dir, contacts);
}

/// Find a contact by exact id first, then case-insensitive name: exact name
/// match wins, otherwise a unique name-substring match. Multiple substring
/// hits are an ambiguity error, never a silent pick.
fn find_contact<'a>(contacts: &'a [Contact], query: &str) -> Result<&'a Contact, String> {
    let q = query.trim();
    if q.is_empty() {
        return Err("empty contact query".to_string());
    }
    if let Some(c) = contacts.iter().find(|c| c.id == q) {
        return Ok(c);
    }
    let ql = q.to_lowercase();
    let exact: Vec<&Contact> = contacts
        .iter()
        .filter(|c| c.name.to_lowercase() == ql)
        .collect();
    if exact.len() == 1 {
        return Ok(exact[0]);
    }
    if exact.len() > 1 {
        return Err(format!("ambiguous name '{q}' ({})", exact.len()));
    }
    let partial: Vec<&Contact> = contacts
        .iter()
        .filter(|c| c.name.to_lowercase().contains(&ql))
        .collect();
    match partial.len() {
        0 => Err(format!("no contact '{q}'")),
        1 => Ok(partial[0]),
        n => Err(format!("ambiguous name '{q}' ({n} contacts match)")),
    }
}

/// A contact to create; shared shape for CLI `add` and MCP `contact_add`.
pub struct NewContact {
    pub name: String,
    /// (label, number) pairs; numbers are normalized via `digits_only`.
    pub phones: Vec<(String, String)>,
    /// (label, address) pairs.
    pub emails: Vec<(String, String)>,
    pub note: String,
}

/// Create and persist a contact. Fails when the result would be meaningless
/// (no name and no phone — same rule the GUI form enforces).
fn create_contact(dir: &Path, nc: &NewContact) -> Result<Contact, String> {
    let mut contacts = load(dir);
    let now = model::now_unix();
    let mut c = Contact::new(model::fresh_id(), &nc.name, now);
    for (label, number) in &nc.phones {
        let n = model::digits_only(number);
        if n.is_empty() {
            continue;
        }
        c.phones.push(model::Phone {
            label: label.trim().to_string(),
            number: n,
        });
    }
    for (label, address) in &nc.emails {
        let a = address.trim();
        if a.is_empty() {
            continue;
        }
        c.emails.push(model::Email {
            label: label.trim().to_string(),
            address: a.to_string(),
        });
    }
    c.note = nc.note.trim().to_string();
    c.prune_empty();
    if !c.is_meaningful() {
        return Err("contact needs a name or a phone number".to_string());
    }
    contacts.push(c.clone());
    save(dir, &mut contacts);
    Ok(c)
}

fn get_contact(dir: &Path, query: &str) -> Result<Contact, String> {
    find_contact(&load(dir), query).cloned()
}

fn set_favorite(dir: &Path, query: &str, on: bool) -> Result<Contact, String> {
    let mut contacts = load(dir);
    // find_contact enforces id/exact-name/unique-substring semantics.
    let id = find_contact(&contacts, query)?.id.clone();
    let c = contacts
        .iter_mut()
        .find(|c| c.id == id)
        .expect("contact found above must still exist");
    c.favorite = on;
    // Favorite is a list-ordering flag, not content — don't bump `updated`.
    let snap = c.clone();
    save(dir, &mut contacts);
    Ok(snap)
}

fn delete_contact(dir: &Path, query: &str) -> Result<Contact, String> {
    let mut contacts = load(dir);
    let victim = find_contact(&contacts, query)?.clone();
    contacts.retain(|c| c.id != victim.id);
    save(dir, &mut contacts);
    Ok(victim)
}

fn search_contacts(dir: &Path, query: &str) -> Vec<Contact> {
    load(dir)
        .into_iter()
        .filter(|c| model::contact_matches(c, query))
        .collect()
}

// ---------------------------------------------------------------------------
// Shared formatting (CLI + MCP render the same text)
// ---------------------------------------------------------------------------

fn dash(s: &str) -> String {
    if s.trim().is_empty() {
        "-".to_string()
    } else {
        s.to_string()
    }
}

fn phones_join(c: &Contact) -> String {
    let v: Vec<String> = c
        .phones
        .iter()
        .map(|p| p.number.trim())
        .filter(|n| !n.is_empty())
        .map(model::format_number)
        .collect();
    if v.is_empty() {
        "-".to_string()
    } else {
        v.join(",")
    }
}

fn emails_join(c: &Contact) -> String {
    let v: Vec<&str> = c
        .emails
        .iter()
        .map(|e| e.address.trim())
        .filter(|a| !a.is_empty())
        .collect();
    if v.is_empty() {
        "-".to_string()
    } else {
        v.join(",")
    }
}

/// One TSV line per contact: name, favorite (★/·), phones, emails.
#[must_use]
pub fn format_row(c: &Contact) -> String {
    let fav = if c.favorite { "★" } else { "·" };
    format!(
        "{}\t{}\t{}\t{}",
        c.name,
        fav,
        phones_join(c),
        emails_join(c)
    )
}

/// Render a contact list (list or search results).
#[must_use]
pub fn format_list(contacts: &[Contact]) -> String {
    if contacts.is_empty() {
        return "No contacts.".to_string();
    }
    contacts
        .iter()
        .map(format_row)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Full detail for `get` / `contact_get`.
#[must_use]
pub fn format_detail(c: &Contact) -> String {
    let mut out = String::new();
    out.push_str(&format!("name: {}\n", dash(&c.name)));
    out.push_str(&format!("id: {}\n", c.id));
    out.push_str(&format!(
        "favorite: {}\n",
        if c.favorite { "yes" } else { "no" }
    ));
    out.push_str("phones:\n");
    if c.phones.is_empty() {
        out.push_str("  -\n");
    } else {
        for p in &c.phones {
            let label = if p.label.trim().is_empty() {
                "phone"
            } else {
                p.label.trim()
            };
            out.push_str(&format!(
                "  {}: {}\n",
                label,
                dash(&model::format_number(p.number.trim()))
            ));
        }
    }
    out.push_str("emails:\n");
    if c.emails.is_empty() {
        out.push_str("  -\n");
    } else {
        for e in &c.emails {
            let label = if e.label.trim().is_empty() {
                "email"
            } else {
                e.label.trim()
            };
            out.push_str(&format!("  {}: {}\n", label, dash(e.address.trim())));
        }
    }
    out.push_str(&format!("note: {}\n", dash(&c.note)));
    out.push_str(&format!("created: {}\n", c.created));
    out.push_str(&format!("updated: {}", c.updated));
    out
}

// ---------------------------------------------------------------------------
// Command execution — pure over `dir`, returns the text to print.
// ---------------------------------------------------------------------------

fn execute(cmd: Cli, dir: &Path) -> Result<String, String> {
    match cmd {
        Cli::Help => Ok(HELP.to_string()),
        Cli::List => Ok(format_list(&load(dir))),
        Cli::Search(q) => {
            let found = search_contacts(dir, &q);
            if found.is_empty() {
                Err(format!("no contacts match \"{q}\""))
            } else {
                Ok(format_list(&found))
            }
        }
        Cli::Get(q) => Ok(format_detail(&get_contact(dir, &q)?)),
        Cli::Add { name, phone, email } => {
            let nc = NewContact {
                name,
                phones: phone.map(|p| (String::new(), p)).into_iter().collect(),
                emails: email.map(|e| (String::new(), e)).into_iter().collect(),
                note: String::new(),
            };
            let c = create_contact(dir, &nc)?;
            Ok(format!("added {} ({})", c.name, c.id))
        }
        Cli::SetFavorite { query, on } => {
            let c = set_favorite(dir, &query, on)?;
            Ok(format!(
                "{} {}",
                c.name,
                if on { "favorite" } else { "not favorite" }
            ))
        }
        Cli::Delete(q) => {
            let c = delete_contact(dir, &q)?;
            Ok(format!("deleted {} ({})", c.name, c.id))
        }
        Cli::Unknown(u) => Err(format!("unknown command '{u}'")),
    }
}

/// Run `--cli` args; returns the process exit code.
pub fn run_cli(args: &[String]) -> i32 {
    let dir = data_dir();
    match parse_cli(args) {
        Cli::Unknown(u) => {
            eprintln!("rabbiz-contacts: unknown command '{u}'\n\n{HELP}");
            2
        }
        cmd => match execute(cmd, &dir) {
            Ok(out) => {
                println!("{out}");
                0
            }
            Err(e) => {
                eprintln!("rabbiz-contacts: {e}");
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

fn arg_opt_str<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(Value::as_str)
}

/// Parse an optional array of labelled entries. Each element is either a
/// plain string (empty label) or an object `{"label": "…", "number"|
/// "address": "…"}`.
fn arg_pairs(args: &Value, key: &str, value_key: &str) -> Result<Vec<(String, String)>, String> {
    let Some(v) = args.get(key) else {
        return Ok(Vec::new());
    };
    if v.is_null() {
        return Ok(Vec::new());
    }
    let arr = v
        .as_array()
        .ok_or_else(|| format!("'{key}' must be an array"))?;
    let mut out = Vec::new();
    for item in arr {
        match item {
            Value::String(s) => out.push((String::new(), s.clone())),
            Value::Object(_) => {
                let label = item
                    .get("label")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let value = item
                    .get(value_key)
                    .and_then(Value::as_str)
                    .ok_or_else(|| format!("'{key}' entries need a \"{value_key}\" string"))?;
                out.push((label, value.to_string()));
            }
            _ => return Err(format!("'{key}' entries must be strings or objects")),
        }
    }
    Ok(out)
}

fn arg_bool(args: &Value, key: &str) -> Result<bool, String> {
    let v = args
        .get(key)
        .ok_or_else(|| format!("missing argument '{key}'"))?;
    v.as_bool()
        .or_else(|| {
            v.as_str().and_then(|s| match s.to_lowercase().as_str() {
                "on" | "true" | "yes" | "1" => Some(true),
                "off" | "false" | "no" | "0" => Some(false),
                _ => None,
            })
        })
        .ok_or_else(|| format!("'{key}' must be a boolean"))
}

fn mcp_list(_args: &Value) -> Result<String, String> {
    Ok(format_list(&load(&data_dir())))
}

fn mcp_search(args: &Value) -> Result<String, String> {
    let q = arg_str(args, "query")?;
    let found = search_contacts(&data_dir(), q);
    if found.is_empty() {
        return Err(format!("no contacts match \"{q}\""));
    }
    Ok(format_list(&found))
}

fn mcp_get(args: &Value) -> Result<String, String> {
    Ok(format_detail(&get_contact(
        &data_dir(),
        arg_str(args, "id")?,
    )?))
}

fn mcp_add(args: &Value) -> Result<String, String> {
    let nc = NewContact {
        name: arg_str(args, "name")?.to_string(),
        phones: arg_pairs(args, "phones", "number")?,
        emails: arg_pairs(args, "emails", "address")?,
        note: arg_opt_str(args, "note").unwrap_or("").to_string(),
    };
    let c = create_contact(&data_dir(), &nc)?;
    Ok(format!("added {} ({})", c.name, c.id))
}

fn mcp_set_favorite(args: &Value) -> Result<String, String> {
    let dir = data_dir();
    let c = set_favorite(&dir, arg_str(args, "id")?, arg_bool(args, "favorite")?)?;
    Ok(format!("{} favorite={}", c.name, c.favorite))
}

fn mcp_delete(args: &Value) -> Result<String, String> {
    let c = delete_contact(&data_dir(), arg_str(args, "id")?)?;
    Ok(format!("deleted {} ({})", c.name, c.id))
}

static TOOLS: &[Tool] = &[
    Tool {
        name: "contacts_list",
        description: "List all contacts in display order (favorites first, \
                      then name). Each line: name, favorite (star or dot), \
                      phones, emails (tab-separated).",
        input_schema: r#"{
            "type": "object",
            "properties": {},
            "additionalProperties": false
        }"#,
        handler: mcp_list,
    },
    Tool {
        name: "contacts_search",
        description: "Search contacts by case-insensitive substring over name, \
                      phone number and email address. Returns the same \
                      tab-separated lines as contacts_list.",
        input_schema: r#"{
            "type": "object",
            "required": ["query"],
            "properties": {
                "query": {"type": "string", "description": "search text"}
            },
            "additionalProperties": false
        }"#,
        handler: mcp_search,
    },
    Tool {
        name: "contact_get",
        description: "Get full detail for one contact by id or name: phones \
                      and emails with labels, note, created/updated.",
        input_schema: r#"{
            "type": "object",
            "required": ["id"],
            "properties": {
                "id": {"type": "string", "description": "contact id (c-…) or name"}
            },
            "additionalProperties": false
        }"#,
        handler: mcp_get,
    },
    Tool {
        name: "contact_add",
        description: "Create a contact. phones/emails are arrays of strings \
                      or {\"label\",\"number\"|\"address\"} objects.",
        input_schema: r#"{
            "type": "object",
            "required": ["name"],
            "properties": {
                "name": {"type": "string", "description": "contact name"},
                "phones": {
                    "type": "array",
                    "items": {
                        "oneOf": [
                            {"type": "string"},
                            {"type": "object",
                             "properties": {
                                 "label": {"type": "string"},
                                 "number": {"type": "string"}
                             },
                             "required": ["number"],
                             "additionalProperties": false}
                        ]
                    },
                    "description": "phone numbers"
                },
                "emails": {
                    "type": "array",
                    "items": {
                        "oneOf": [
                            {"type": "string"},
                            {"type": "object",
                             "properties": {
                                 "label": {"type": "string"},
                                 "address": {"type": "string"}
                             },
                             "required": ["address"],
                             "additionalProperties": false}
                        ]
                    },
                    "description": "email addresses"
                },
                "note": {"type": "string", "description": "free-form note"}
            },
            "additionalProperties": false
        }"#,
        handler: mcp_add,
    },
    Tool {
        name: "contact_set_favorite",
        description: "Mark a contact favorite or not (id or name).",
        input_schema: r#"{
            "type": "object",
            "required": ["id", "favorite"],
            "properties": {
                "id": {"type": "string", "description": "contact id (c-…) or name"},
                "favorite": {"type": "boolean", "description": "true = favorite"}
            },
            "additionalProperties": false
        }"#,
        handler: mcp_set_favorite,
    },
    Tool {
        name: "contact_delete",
        description: "Delete a contact by id or name.",
        input_schema: r#"{
            "type": "object",
            "required": ["id"],
            "properties": {
                "id": {"type": "string", "description": "contact id (c-…) or name"}
            },
            "additionalProperties": false
        }"#,
        handler: mcp_delete,
    },
];

/// Serve MCP on stdin/stdout until EOF; never returns.
pub fn mcp_serve() -> ! {
    serve(TOOLS, "rabbiz-contacts", env!("CARGO_PKG_VERSION"))
}

// ---------------------------------------------------------------------------
// Tests (headless — temp dirs only, no Wayland, no env mutation)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn strs(xs: &[&str]) -> Vec<String> {
        xs.iter().map(|s| (*s).to_string()).collect()
    }

    fn tmpdir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "rabbiz-contacts-headless-test-{}-{}-{}",
            tag,
            std::process::id(),
            model::now_unix()
        ));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn seed_dir(tag: &str) -> PathBuf {
        let dir = tmpdir(tag);
        // Write an explicit empty store so `load` does not reseed the demo.
        fs::write(dir.join("contacts.json"), "{\"version\":1,\"contacts\":[]}").unwrap();
        dir
    }

    fn add_via_cli(dir: &Path, args: &[&str]) -> String {
        let cli_args: Vec<String> = args.iter().map(|s| (*s).to_string()).collect();
        match execute(parse_cli(&cli_args), dir) {
            Ok(s) => s,
            Err(e) => panic!("cli failed: {e}"),
        }
    }

    // ---- parse ----

    #[test]
    fn parse_help_and_empty() {
        assert_eq!(parse_cli(&[]), Cli::Help);
        assert_eq!(parse_cli(&strs(&["help"])), Cli::Help);
        assert_eq!(parse_cli(&strs(&["--help"])), Cli::Help);
        assert_eq!(parse_cli(&strs(&["-h"])), Cli::Help);
    }

    #[test]
    fn parse_list_search_get_delete() {
        assert_eq!(parse_cli(&strs(&["list"])), Cli::List);
        assert_eq!(
            parse_cli(&strs(&["search", "sarah", "chen"])),
            Cli::Search("sarah chen".to_string())
        );
        assert_eq!(
            parse_cli(&strs(&["get", "c-123"])),
            Cli::Get("c-123".to_string())
        );
        assert_eq!(
            parse_cli(&strs(&["delete", "mom"])),
            Cli::Delete("mom".to_string())
        );
    }

    #[test]
    fn parse_add_splits_name_phone_email() {
        assert_eq!(
            parse_cli(&strs(&["add", "Test", "User", "0812345678"])),
            Cli::Add {
                name: "Test User".to_string(),
                phone: Some("0812345678".to_string()),
                email: None,
            }
        );
        assert_eq!(
            parse_cli(&strs(&["add", "Test", "User", "0812345678", "t@x.com"])),
            Cli::Add {
                name: "Test User".to_string(),
                phone: Some("0812345678".to_string()),
                email: Some("t@x.com".to_string()),
            }
        );
        // Email alone, no phone.
        assert_eq!(
            parse_cli(&strs(&["add", "Solo", "s@x.com"])),
            Cli::Add {
                name: "Solo".to_string(),
                phone: None,
                email: Some("s@x.com".to_string()),
            }
        );
        // A name that is not phone/email-like stays whole.
        assert_eq!(
            parse_cli(&strs(&["add", "Grandma", "Two"])),
            Cli::Add {
                name: "Grandma Two".to_string(),
                phone: None,
                email: None,
            }
        );
    }

    #[test]
    fn parse_set_favorite() {
        assert_eq!(
            parse_cli(&strs(&["set-favorite", "c-1", "on"])),
            Cli::SetFavorite {
                query: "c-1".to_string(),
                on: true
            }
        );
        assert_eq!(
            parse_cli(&strs(&["set-favorite", "mom", "OFF"])),
            Cli::SetFavorite {
                query: "mom".to_string(),
                on: false
            }
        );
        assert!(matches!(
            parse_cli(&strs(&["set-favorite", "c-1", "maybe"])),
            Cli::Unknown(_)
        ));
        assert!(matches!(
            parse_cli(&strs(&["set-favorite"])),
            Cli::Unknown(_)
        ));
    }

    #[test]
    fn parse_missing_and_unknown() {
        assert!(matches!(parse_cli(&strs(&["search"])), Cli::Unknown(_)));
        assert!(matches!(parse_cli(&strs(&["get"])), Cli::Unknown(_)));
        assert!(matches!(
            parse_cli(&strs(&["add"])),
            Cli::Help | Cli::Unknown(_)
        ));
        assert!(matches!(parse_cli(&strs(&["bogus"])), Cli::Unknown(_)));
    }

    // ---- formatting ----

    #[test]
    fn format_row_shape() {
        let mut c = Contact::new("c-1".into(), "Mom", 1);
        c.favorite = true;
        c.phones.push(model::Phone {
            label: "mobile".into(),
            number: "0865551234".into(),
        });
        c.emails.push(model::Email {
            label: "home".into(),
            address: "mom@x.com".into(),
        });
        let row = format_row(&c);
        let cols: Vec<&str> = row.split('\t').collect();
        assert_eq!(cols.len(), 4);
        assert_eq!(cols[0], "Mom");
        assert_eq!(cols[1], "★");
        assert_eq!(cols[2], "086-555-1234");
        assert_eq!(cols[3], "mom@x.com");

        let plain = Contact::new("c-2".into(), "Bob", 1);
        assert_eq!(format_row(&plain), "Bob\t·\t-\t-");
    }

    #[test]
    fn format_list_empty_and_sorted() {
        assert_eq!(format_list(&[]), "No contacts.");
        let mut v = vec![
            Contact::new("c-2".into(), "zed", 0),
            Contact::new("c-1".into(), "Amy", 0),
        ];
        v[1].favorite = true;
        model::sort_contacts(&mut v);
        let out = format_list(&v);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].starts_with("Amy\t★"));
        assert!(lines[1].starts_with("zed\t·"));
    }

    #[test]
    fn format_detail_has_all_fields() {
        let mut c = Contact::new("c-9".into(), "Ann", 10);
        c.updated = 20;
        c.phones.push(model::Phone {
            label: "work".into(),
            number: "021234567".into(),
        });
        c.emails.push(model::Email {
            label: "home".into(),
            address: "a@b.c".into(),
        });
        c.note = "met at office".into();
        let d = format_detail(&c);
        assert!(d.contains("name: Ann"));
        assert!(d.contains("id: c-9"));
        assert!(d.contains("favorite: no"));
        assert!(d.contains("work: 021-234-567"));
        assert!(d.contains("home: a@b.c"));
        assert!(d.contains("note: met at office"));
        assert!(d.contains("created: 10"));
        assert!(d.contains("updated: 20"));
    }

    // ---- store round-trips through the shared ops ----

    #[test]
    fn add_list_search_get_delete_roundtrip() {
        let dir = seed_dir("roundtrip");
        assert_eq!(format_list(&load(&dir)), "No contacts.");

        add_via_cli(&dir, &["add", "Test", "User", "0812345678", "t@x.com"]);
        let list = execute(parse_cli(&strs(&["list"])), &dir).unwrap();
        assert!(list.contains("Test User"));
        assert!(list.contains("081-234-5678"));
        assert!(list.contains("t@x.com"));

        let found = execute(parse_cli(&strs(&["search", "test"])), &dir).unwrap();
        assert!(found.contains("Test User"));
        let by_phone = execute(parse_cli(&strs(&["search", "081234"])), &dir).unwrap();
        assert!(by_phone.contains("Test User"));

        let detail = execute(parse_cli(&strs(&["get", "test", "user"])), &dir).unwrap();
        assert!(detail.contains("name: Test User"));
        assert!(detail.contains("id: c-"));

        // set-favorite by name substring.
        execute(parse_cli(&strs(&["set-favorite", "test", "on"])), &dir).unwrap();
        let list = execute(parse_cli(&strs(&["list"])), &dir).unwrap();
        assert!(list.contains("Test User\t★"));

        execute(parse_cli(&strs(&["delete", "test", "user"])), &dir).unwrap();
        assert_eq!(format_list(&load(&dir)), "No contacts.");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn add_requires_meaningful_contact() {
        let dir = seed_dir("meaningful");
        // Phone-like token only → nameless contact with a number is allowed.
        let out = execute(parse_cli(&strs(&["add", "0812345678"])), &dir).unwrap();
        assert!(out.starts_with("added "));
        // Nothing meaningful → error.
        assert!(execute(parse_cli(&strs(&["add", "   "])), &dir).is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn find_contact_ambiguous_and_missing() {
        let dir = seed_dir("find");
        let nc = NewContact {
            name: "Samantha".into(),
            phones: vec![(String::new(), "111".into())],
            emails: vec![],
            note: String::new(),
        };
        create_contact(&dir, &nc).unwrap();
        let nc2 = NewContact {
            name: "Samuel".into(),
            phones: vec![(String::new(), "222".into())],
            emails: vec![],
            note: String::new(),
        };
        create_contact(&dir, &nc2).unwrap();
        // "sam" substring-matches both, no exact hit → ambiguous.
        assert!(get_contact(&dir, "sam").is_err());
        // Exact name (case-insensitive) wins over partials.
        assert_eq!(get_contact(&dir, "samantha").unwrap().name, "Samantha");
        assert!(get_contact(&dir, "nobody").is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn delete_and_set_favorite_missing_is_error() {
        let dir = seed_dir("missing");
        assert!(execute(parse_cli(&strs(&["delete", "ghost"])), &dir).is_err());
        assert!(execute(parse_cli(&strs(&["set-favorite", "ghost", "on"])), &dir).is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    // ---- MCP ----

    #[test]
    fn mcp_schemas_parse_and_have_no_extra_props() {
        for t in TOOLS {
            let v: Value = serde_json::from_str(t.input_schema).unwrap();
            assert_eq!(v["additionalProperties"], false, "tool {}", t.name);
            assert_eq!(v["type"], "object", "tool {}", t.name);
        }
        let names: Vec<&str> = TOOLS.iter().map(|t| t.name).collect();
        assert_eq!(
            names,
            [
                "contacts_list",
                "contacts_search",
                "contact_get",
                "contact_add",
                "contact_set_favorite",
                "contact_delete"
            ]
        );
    }

    #[test]
    fn mcp_handlers_validate_args() {
        // Missing required args must error, never panic.
        assert!(mcp_get(&serde_json::json!({})).is_err());
        assert!(mcp_search(&serde_json::json!({})).is_err());
        assert!(mcp_add(&serde_json::json!({})).is_err());
        assert!(mcp_set_favorite(&serde_json::json!({"id": "x"})).is_err());
        assert!(mcp_delete(&serde_json::json!({})).is_err());
        // contacts_list has no required args and always renders.
        assert!(mcp_list(&serde_json::json!({})).is_ok());
    }

    #[test]
    fn arg_pairs_accepts_strings_and_objects() {
        let args = serde_json::json!({
            "phones": ["0812345678", {"label": "work", "number": "021234567"}],
            "emails": [{"label": "home", "address": "a@b.c"}, "b@c.d"]
        });
        let phones = arg_pairs(&args, "phones", "number").unwrap();
        assert_eq!(
            phones,
            vec![
                (String::new(), "0812345678".to_string()),
                ("work".to_string(), "021234567".to_string())
            ]
        );
        let emails = arg_pairs(&args, "emails", "address").unwrap();
        assert_eq!(emails.len(), 2);
        assert_eq!(emails[0].0, "home");
        // Missing key → empty.
        assert!(arg_pairs(&args, "nope", "number").unwrap().is_empty());
        // Wrong shape → error.
        assert!(arg_pairs(&serde_json::json!({"phones": 5}), "phones", "number").is_err());
    }
}
