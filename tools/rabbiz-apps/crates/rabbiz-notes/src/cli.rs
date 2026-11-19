//! Headless front-ends for the notes store, sharing `store.rs` with the GUI
//! so atomic writes, corrupt-file tolerance and legacy migration behave
//! identically in every mode:
//!
//! - `rabbiz-notes --cli <command>` — text CLI on stdout/stderr.
//! - `rabbiz-notes --mcp` — MCP server over stdio via `rabbiz_app_kit::mcp`.
//!
//! Write ops (`new`/`set`/`append`/`delete`) go through the same
//! `save_note`/`delete_note` tmp+rename path as the editor's autosave.

use std::io::Read;
use std::path::Path;

use serde_json::Value;

use rabbiz_app_kit::mcp::Tool;

use crate::model::{matching_indices, normalize_body, now_unix, Note};
use crate::store;

const USAGE: &str = "\
rabbiz-notes — notes CLI

usage:
  rabbiz-notes                     launch the GUI
  rabbiz-notes --cli <command>     text CLI (same store as the GUI)
  rabbiz-notes --mcp               MCP server on stdio

commands:
  list                      id, updated, title — one per line (tab-separated)
  search <query>            notes matching query (title + body, case-insensitive)
  get <id>                  full note body
  new <text...>             create a note; no text reads stdin
  set <id> <body...>        replace a note's body; no body reads stdin
  append <id> <text...>     append text to a note's body
  delete <id>               delete a note
  help                      this text
";

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

#[derive(Debug, PartialEq, Eq)]
enum Cmd {
    Help,
    List,
    Search(String),
    Get(u64),
    New { text: String },
    Set { id: u64, body: String },
    Append { id: u64, text: String },
    Delete(u64),
}

impl Cmd {
    /// True when the command should slurp stdin because no inline text was
    /// given (`new`/`set` only — `append` with no text is an error instead of
    /// a silent hang).
    fn wants_stdin(&self) -> bool {
        matches!(
            self,
            Cmd::New { text } | Cmd::Set { body: text, .. } if text.is_empty()
        )
    }
}

fn parse_id(s: Option<&String>, what: &str) -> Result<u64, String> {
    s.and_then(|v| v.parse::<u64>().ok())
        .ok_or_else(|| format!("{what}: expected a numeric note id"))
}

fn parse(args: &[String]) -> Result<Cmd, String> {
    let Some((head, rest)) = args.split_first() else {
        return Ok(Cmd::Help);
    };
    match head.as_str() {
        "help" | "--help" | "-h" => Ok(Cmd::Help),
        "list" => {
            if rest.is_empty() {
                Ok(Cmd::List)
            } else {
                Err("list takes no arguments".into())
            }
        }
        "search" => {
            if rest.is_empty() {
                Err("search needs a query".into())
            } else {
                Ok(Cmd::Search(rest.join(" ")))
            }
        }
        "get" => {
            if rest.len() != 1 {
                return Err("get needs exactly one id".into());
            }
            Ok(Cmd::Get(parse_id(rest.first(), "get")?))
        }
        "new" | "create" => Ok(Cmd::New {
            text: rest.join(" "),
        }),
        "set" => {
            if rest.is_empty() {
                return Err("set needs an id".into());
            }
            let id = parse_id(rest.first(), "set")?;
            Ok(Cmd::Set {
                id,
                body: rest[1..].join(" "),
            })
        }
        "append" => {
            if rest.len() < 2 {
                return Err("append needs an id and text".into());
            }
            let id = parse_id(rest.first(), "append")?;
            Ok(Cmd::Append {
                id,
                text: rest[1..].join(" "),
            })
        }
        "delete" | "rm" => {
            if rest.len() != 1 {
                return Err("delete needs exactly one id".into());
            }
            Ok(Cmd::Delete(parse_id(rest.first(), "delete")?))
        }
        other => Err(format!("unknown command: {other}")),
    }
}

/// One line per note for `list`/`search`: `<id>\t<updated epoch>\t<title>`.
/// Titles are already single-line (derived via `derive_title`).
fn format_notes_line(n: &Note) -> String {
    format!("{}\t{}\t{}\n", n.id, n.updated, n.title)
}

fn format_notes<'a>(notes: impl Iterator<Item = &'a Note>) -> String {
    let mut out = String::new();
    for n in notes {
        out.push_str(&format_notes_line(n));
    }
    out
}

fn find_note(notes: &[Note], id: u64) -> Result<&Note, String> {
    notes
        .iter()
        .find(|n| n.id == id)
        .ok_or_else(|| format!("no note {id}"))
}

/// Load + migrate, then run `cmd`. `stdin` is only read by the caller when
/// `cmd.wants_stdin()`; kept as a param so tests never touch real stdin.
fn execute(cmd: Cmd, dir: &Path, stdin: &str) -> Result<String, String> {
    store::migrate_legacy(dir);
    match cmd {
        Cmd::Help => Ok(USAGE.to_string()),
        Cmd::List => Ok(format_notes(store::load_all(dir).iter())),
        Cmd::Search(q) => {
            let notes = store::load_all(dir);
            let ids = matching_indices(&notes, &q);
            Ok(format_notes(ids.iter().map(|&i| &notes[i])))
        }
        Cmd::Get(id) => {
            let notes = store::load_all(dir);
            let n = find_note(&notes, id)?;
            let mut body = n.body.clone();
            if !body.ends_with('\n') {
                body.push('\n');
            }
            Ok(body)
        }
        Cmd::New { text } => {
            let body = if text.is_empty() {
                stdin
            } else {
                text.as_str()
            };
            let n = create_note(dir, body);
            Ok(format_notes(std::iter::once(&n)))
        }
        Cmd::Set { id, body } => {
            let body = if body.is_empty() {
                stdin
            } else {
                body.as_str()
            };
            let n = set_body(dir, id, body)?;
            Ok(format_notes(std::iter::once(&n)))
        }
        Cmd::Append { id, text } => {
            let n = append_body(dir, id, &text)?;
            Ok(format_notes(std::iter::once(&n)))
        }
        Cmd::Delete(id) => {
            delete_note(dir, id)?;
            Ok(format!("deleted {id}\n"))
        }
    }
}

fn read_stdin() -> String {
    let mut s = String::new();
    let _ = std::io::stdin().read_to_string(&mut s);
    s
}

/// Entry point from `main`. Resolves the real data dir (honouring
/// `XDG_DATA_HOME`), runs the command, returns the process exit code.
pub fn run_cli(args: &[String]) -> i32 {
    let dir = store::data_dir();
    let cmd = match parse(args) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("rabbiz-notes: {e}\n{USAGE}");
            return 2;
        }
    };
    let stdin = if cmd.wants_stdin() {
        read_stdin()
    } else {
        String::new()
    };
    match execute(cmd, &dir, &stdin) {
        Ok(out) => {
            print!("{out}");
            0
        }
        Err(e) => {
            eprintln!("rabbiz-notes: {e}");
            1
        }
    }
}

// ---------------------------------------------------------------------------
// Shared write ops — used by both the CLI commands and the MCP tools.
// ---------------------------------------------------------------------------

fn create_note(dir: &Path, body: &str) -> Note {
    store::migrate_legacy(dir);
    let existing = store::load_all(dir);
    let id = store::fresh_id(dir, &existing);
    let n = Note::new(id, body, now_unix());
    store::save_note(dir, &n);
    n
}

fn set_body(dir: &Path, id: u64, body: &str) -> Result<Note, String> {
    store::migrate_legacy(dir);
    let mut notes = store::load_all(dir);
    let n = notes
        .iter_mut()
        .find(|n| n.id == id)
        .ok_or_else(|| format!("no note {id}"))?;
    n.body = normalize_body(body);
    n.touch(now_unix());
    let snap = n.clone();
    store::save_note(dir, &snap);
    Ok(snap)
}

fn append_body(dir: &Path, id: u64, text: &str) -> Result<Note, String> {
    store::migrate_legacy(dir);
    let mut notes = store::load_all(dir);
    let n = notes
        .iter_mut()
        .find(|n| n.id == id)
        .ok_or_else(|| format!("no note {id}"))?;
    if !n.body.is_empty() && !n.body.ends_with('\n') && !text.is_empty() {
        n.body.push('\n');
    }
    n.body.push_str(&normalize_body(text));
    n.touch(now_unix());
    let snap = n.clone();
    store::save_note(dir, &snap);
    Ok(snap)
}

fn delete_note(dir: &Path, id: u64) -> Result<(), String> {
    store::migrate_legacy(dir);
    let notes = store::load_all(dir);
    find_note(&notes, id)?;
    store::delete_note(dir, id);
    Ok(())
}

fn get_note(dir: &Path, id: u64) -> Result<Note, String> {
    store::migrate_legacy(dir);
    let notes = store::load_all(dir);
    find_note(&notes, id).cloned()
}

// ---------------------------------------------------------------------------
// MCP tools
// ---------------------------------------------------------------------------

/// The live store dir with legacy migration applied — resolved per call so
/// `XDG_DATA_HOME` changes between invocations are honoured.
fn data_dir() -> std::path::PathBuf {
    let d = store::data_dir();
    store::migrate_legacy(&d);
    d
}

fn arg_str<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(Value::as_str)
}

fn arg_u64(args: &Value, key: &str) -> Result<u64, String> {
    let v = args.get(key).ok_or_else(|| format!("missing \"{key}\""))?;
    v.as_u64()
        .or_else(|| v.as_str().and_then(|s| s.parse::<u64>().ok()))
        .ok_or_else(|| format!("\"{key}\" must be a positive integer"))
}

fn notes_search_in(dir: &Path, query: &str) -> String {
    let notes = store::load_all(dir);
    let ids = matching_indices(&notes, query);
    format_notes(ids.iter().map(|&i| &notes[i]))
}

fn tool_notes_list(_: &Value) -> Result<String, String> {
    Ok(format_notes(store::load_all(&data_dir()).iter()))
}

fn tool_notes_search(args: &Value) -> Result<String, String> {
    let q = arg_str(args, "query").unwrap_or("");
    Ok(notes_search_in(&data_dir(), q))
}

fn tool_note_get(args: &Value) -> Result<String, String> {
    let n = get_note(&data_dir(), arg_u64(args, "id")?)?;
    Ok(n.body)
}

fn tool_note_create(args: &Value) -> Result<String, String> {
    let body = arg_str(args, "body").ok_or("missing \"body\"")?;
    let n = create_note(&data_dir(), body);
    Ok(format_notes(std::iter::once(&n)))
}

fn tool_note_update(args: &Value) -> Result<String, String> {
    let dir = data_dir();
    let id = arg_u64(args, "id")?;
    if let Some(body) = arg_str(args, "body") {
        let n = set_body(&dir, id, body)?;
        return Ok(format_notes(std::iter::once(&n)));
    }
    if let Some(text) = arg_str(args, "append") {
        let n = append_body(&dir, id, text)?;
        return Ok(format_notes(std::iter::once(&n)));
    }
    Err("note_update needs \"body\" (replace) or \"append\" (append)".into())
}

fn tool_note_delete(args: &Value) -> Result<String, String> {
    let id = arg_u64(args, "id")?;
    delete_note(&data_dir(), id)?;
    Ok(format!("deleted {id}"))
}

pub static TOOLS: &[Tool] = &[
    Tool {
        name: "notes_list",
        description: "List all notes, most recently edited first. Each line: id, updated (unix seconds), title (tab-separated).",
        input_schema: r#"{"type":"object","properties":{},"additionalProperties":false}"#,
        handler: tool_notes_list,
    },
    Tool {
        name: "notes_search",
        description: "Search notes by case-insensitive substring over title and body. Returns the same id/updated/title lines as notes_list.",
        input_schema: r#"{"type":"object","required":["query"],"properties":{"query":{"type":"string","description":"search text"}},"additionalProperties":false}"#,
        handler: tool_notes_search,
    },
    Tool {
        name: "note_get",
        description: "Get the full body of a note by id.",
        input_schema: r#"{"type":"object","required":["id"],"properties":{"id":{"type":"integer","minimum":1,"description":"note id from notes_list"}},"additionalProperties":false}"#,
        handler: tool_note_get,
    },
    Tool {
        name: "note_create",
        description: "Create a note. The title is derived from the first non-empty line of body.",
        input_schema: r#"{"type":"object","required":["body"],"properties":{"body":{"type":"string","description":"note body (may contain newlines)"}},"additionalProperties":false}"#,
        handler: tool_note_create,
    },
    Tool {
        name: "note_update",
        description: "Update a note by id: pass \"body\" to replace the whole body, or \"append\" to append text (newline-separated).",
        input_schema: r#"{"type":"object","required":["id"],"properties":{"id":{"type":"integer","minimum":1},"body":{"type":"string","description":"replacement body"},"append":{"type":"string","description":"text to append"}},"additionalProperties":false}"#,
        handler: tool_note_update,
    },
    Tool {
        name: "note_delete",
        description: "Delete a note by id.",
        input_schema: r#"{"type":"object","required":["id"],"properties":{"id":{"type":"integer","minimum":1}},"additionalProperties":false}"#,
        handler: tool_note_delete,
    },
];

// ---------------------------------------------------------------------------
// Tests — headless, temp dirs only (no env mutation, no real store).
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tmpdir(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!(
            "rabbiz-notes-cli-test-{}-{}-{}",
            tag,
            std::process::id(),
            now_unix()
        ));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parse_commands() {
        assert_eq!(parse(&args(&["list"])).unwrap(), Cmd::List);
        assert_eq!(
            parse(&args(&["search", "foo", "bar"])).unwrap(),
            Cmd::Search("foo bar".into())
        );
        assert_eq!(parse(&args(&["get", "42"])).unwrap(), Cmd::Get(42));
        assert_eq!(
            parse(&args(&["new", "hello", "world"])).unwrap(),
            Cmd::New {
                text: "hello world".into()
            }
        );
        assert_eq!(
            parse(&args(&["create"])).unwrap(),
            Cmd::New {
                text: String::new()
            }
        );
        assert_eq!(
            parse(&args(&["set", "7", "a", "b"])).unwrap(),
            Cmd::Set {
                id: 7,
                body: "a b".into()
            }
        );
        assert_eq!(
            parse(&args(&["append", "7", "x"])).unwrap(),
            Cmd::Append {
                id: 7,
                text: "x".into()
            }
        );
        assert_eq!(parse(&args(&["delete", "9"])).unwrap(), Cmd::Delete(9));
        assert_eq!(parse(&args(&["help"])).unwrap(), Cmd::Help);
        assert!(parse(&args(&[])).unwrap() == Cmd::Help);
    }

    #[test]
    fn parse_rejects_bad_input() {
        assert!(parse(&args(&["get", "abc"])).is_err());
        assert!(parse(&args(&["get"])).is_err());
        assert!(parse(&args(&["search"])).is_err());
        assert!(parse(&args(&["set"])).is_err());
        assert!(parse(&args(&["append", "1"])).is_err());
        assert!(parse(&args(&["bogus"])).is_err());
        assert!(parse(&args(&["list", "extra"])).is_err());
    }

    #[test]
    fn wants_stdin_only_for_empty_new_and_set() {
        assert!(Cmd::New {
            text: String::new()
        }
        .wants_stdin());
        assert!(!Cmd::New { text: "x".into() }.wants_stdin());
        assert!(Cmd::Set {
            id: 1,
            body: String::new()
        }
        .wants_stdin());
        assert!(!Cmd::Append {
            id: 1,
            text: String::new()
        }
        .wants_stdin());
        assert!(!Cmd::List.wants_stdin());
    }

    #[test]
    fn cli_roundtrip_through_store() {
        let dir = tmpdir("roundtrip");

        // new → line output with derived title
        let out = execute(parse(&args(&["new", "first", "note"])).unwrap(), &dir, "").unwrap();
        let id: u64 = out.split('\t').next().unwrap().parse().unwrap();
        assert!(out.contains("first note"));
        assert_eq!(store::load_all(&dir).len(), 1);

        // get → full body
        let body = execute(parse(&args(&["get", &id.to_string()])).unwrap(), &dir, "").unwrap();
        assert_eq!(body, "first note\n");

        // list → id + title line
        let list = execute(parse(&args(&["list"])).unwrap(), &dir, "").unwrap();
        assert!(list.contains(&format!("{id}\t")));
        assert!(list.contains("first note"));

        // append → newline-separated
        execute(
            parse(&args(&["append", &id.to_string(), "line2"])).unwrap(),
            &dir,
            "",
        )
        .unwrap();
        let body = execute(parse(&args(&["get", &id.to_string()])).unwrap(), &dir, "").unwrap();
        assert_eq!(body, "first note\nline2\n");

        // set → full replace
        execute(
            parse(&args(&["set", &id.to_string(), "replaced"])).unwrap(),
            &dir,
            "",
        )
        .unwrap();
        let body = execute(parse(&args(&["get", &id.to_string()])).unwrap(), &dir, "").unwrap();
        assert_eq!(body, "replaced\n");

        // search hits body, misses otherwise
        let hits = execute(parse(&args(&["search", "replac"])).unwrap(), &dir, "").unwrap();
        assert!(hits.contains(&id.to_string()));
        let miss = execute(parse(&args(&["search", "zzz"])).unwrap(), &dir, "").unwrap();
        assert!(!miss.contains(&id.to_string()));

        // delete → gone
        execute(
            parse(&args(&["delete", &id.to_string()])).unwrap(),
            &dir,
            "",
        )
        .unwrap();
        assert!(store::load_all(&dir).is_empty());
        assert!(execute(parse(&args(&["get", &id.to_string()])).unwrap(), &dir, "").is_err());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn new_and_set_read_stdin_when_empty() {
        let dir = tmpdir("stdin");
        let out = execute(
            Cmd::New {
                text: String::new(),
            },
            &dir,
            "from stdin\n",
        )
        .unwrap();
        let id: u64 = out.split('\t').next().unwrap().parse().unwrap();
        let body = execute(parse(&args(&["get", &id.to_string()])).unwrap(), &dir, "").unwrap();
        assert_eq!(body, "from stdin\n");

        execute(
            Cmd::Set {
                id,
                body: String::new(),
            },
            &dir,
            "stdin body",
        )
        .unwrap();
        let body = execute(parse(&args(&["get", &id.to_string()])).unwrap(), &dir, "").unwrap();
        assert_eq!(body, "stdin body\n");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn writes_on_missing_id_error() {
        let dir = tmpdir("missing");
        assert!(set_body(&dir, 1, "x").is_err());
        assert!(append_body(&dir, 1, "x").is_err());
        assert!(delete_note(&dir, 1).is_err());
        assert!(get_note(&dir, 1).is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn mcp_dir_backed_ops_roundtrip() {
        let dir = tmpdir("mcp");
        let created = create_note(&dir, "mcp note body");
        assert_eq!(created.title, "mcp note body");

        let list = notes_search_in(&dir, "");
        assert!(list.contains(&created.id.to_string()));
        let hits = notes_search_in(&dir, "MCP"); // case-insensitive
        assert!(hits.contains(&created.id.to_string()));
        assert!(notes_search_in(&dir, "nope").is_empty());

        let n = set_body(&dir, created.id, "updated body").unwrap();
        assert_eq!(n.body, "updated body");
        let n = append_body(&dir, created.id, "tail").unwrap();
        assert_eq!(n.body, "updated body\ntail");

        delete_note(&dir, created.id).unwrap();
        assert!(store::load_all(&dir).is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn mcp_schemas_parse() {
        for t in TOOLS {
            assert!(
                serde_json::from_str::<Value>(t.input_schema).is_ok(),
                "bad schema for {}",
                t.name
            );
        }
        let names: Vec<&str> = TOOLS.iter().map(|t| t.name).collect();
        assert_eq!(
            names,
            [
                "notes_list",
                "notes_search",
                "note_get",
                "note_create",
                "note_update",
                "note_delete"
            ]
        );
    }

    #[test]
    fn arg_parsing_helpers() {
        let v = serde_json::json!({"id": 42});
        assert_eq!(arg_u64(&v, "id").unwrap(), 42);
        assert!(arg_u64(&v, "missing").is_err());
        assert!(arg_u64(&serde_json::json!({"id": "abc"}), "id").is_err());
        // String ids tolerated for hand-rolled clients.
        assert_eq!(arg_u64(&serde_json::json!({"id": "7"}), "id").unwrap(), 7);
    }
}
