//! Headless modes for the same binary: `--cli <cmd>` text commands and the
//! `--mcp` stdio server (tool table + handlers live here; `main.rs` only
//! dispatches on argv). Nothing in this module touches Wayland.
//!
//! Every operation reuses `model.rs` — `load_dir`/`sort_entries` for
//! listings, `dir_stats`/`WALK_CAP` for recursive sizes, `make_dir`,
//! `move_entry`, `remove_entry`, `free_space`, `mode_string`,
//! `format_size`, `format_time` — so CLI/MCP results always match what the
//! GUI shows. Deletes are permanent (the app has no trash either) and are
//! guarded: `--yes`/`confirm:true` is required for everything, and a
//! non-empty directory additionally needs `--recursive`/`recursive:true`.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use rabbiz_app_kit::mcp::{serve, Tool};
use serde_json::{json, Value};

use crate::model::{self, Entry, SortKey};

const HELP: &str = "\
rabbiz-files — RabbizOS Files

USAGE:
    rabbiz-files                 launch the GUI
    rabbiz-files <path>          open the GUI at <path>; a file opens its
                                parent directory (the app is a browser,
                                not a viewer)
    rabbiz-files --cli <cmd>     text CLI (commands below)
    rabbiz-files --mcp           MCP server on stdio
    rabbiz-files --help          this text

COMMANDS:
    pwd                     print the current directory
    list [dir]              entries as TSV: name<TAB>type<TAB>size<TAB>mtime
                            (type = file|dir|link; dir size is recursive,
                            capped like the app; hidden files included)
    info <path>             type, size, permissions, mtime, link target
    mkdir <path>            create a directory (parents must exist)
    rename <from> <to>      rename/move an entry; refuses if 'to' exists
    delete <path> [--recursive] [--yes]
                            permanent delete. --yes is REQUIRED for every
                            delete; a non-empty directory also needs
                            --recursive. Empty dirs delete without it.
    storage                 free/total bytes of the filesystem at cwd
    help                    this text

Paths accept ~ and are resolved relative to the current directory.
Exit codes: 0 ok, 1 command failed, 2 usage error.
";

/// Usage text for `--help` / bare `help` invocations.
pub fn help_text() -> &'static str {
    HELP
}

/// A parsed `--cli` invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Pwd,
    List {
        dir: Option<PathBuf>,
    },
    Info {
        path: PathBuf,
    },
    Mkdir {
        path: PathBuf,
    },
    Rename {
        from: PathBuf,
        to: PathBuf,
    },
    Delete {
        path: PathBuf,
        recursive: bool,
        yes: bool,
    },
    Storage,
    Help,
}

fn need_path(args: &[String], i: usize, cmd: &str) -> Result<PathBuf, String> {
    let Some(a) = args.get(i) else {
        return Err(format!("'{cmd}' needs a path"));
    };
    Ok(PathBuf::from(a))
}

/// Parse `--cli` arguments (everything after the literal `--cli`).
pub fn parse(args: &[String]) -> Result<Command, String> {
    let Some(cmd) = args.first() else {
        return Ok(Command::Help);
    };
    match cmd.as_str() {
        "pwd" => {
            if args.len() > 1 {
                return Err(format!("unexpected argument '{}'", args[1]));
            }
            Ok(Command::Pwd)
        }
        "list" => {
            if args.len() > 2 {
                return Err(format!("unexpected argument '{}'", args[2]));
            }
            let dir = args.get(1).map(PathBuf::from);
            Ok(Command::List { dir })
        }
        "info" => {
            if args.len() > 2 {
                return Err(format!("unexpected argument '{}'", args[2]));
            }
            Ok(Command::Info {
                path: need_path(args, 1, "info")?,
            })
        }
        "mkdir" => {
            if args.len() > 2 {
                return Err(format!("unexpected argument '{}'", args[2]));
            }
            Ok(Command::Mkdir {
                path: need_path(args, 1, "mkdir")?,
            })
        }
        "rename" => {
            let from = need_path(args, 1, "rename")?;
            let to = need_path(args, 2, "rename")?;
            if args.len() > 3 {
                return Err(format!("unexpected argument '{}'", args[3]));
            }
            Ok(Command::Rename { from, to })
        }
        "delete" => {
            let path = need_path(args, 1, "delete")?;
            let mut recursive = false;
            let mut yes = false;
            for a in &args[2..] {
                match a.as_str() {
                    "--recursive" | "-r" => recursive = true,
                    "--yes" | "-y" => yes = true,
                    other => return Err(format!("unexpected argument '{other}'")),
                }
            }
            Ok(Command::Delete {
                path,
                recursive,
                yes,
            })
        }
        "storage" => {
            if args.len() > 1 {
                return Err(format!("unexpected argument '{}'", args[1]));
            }
            Ok(Command::Storage)
        }
        "help" | "--help" | "-h" => Ok(Command::Help),
        other => Err(format!("unknown command '{other}'")),
    }
}

// ---------------------------------------------------------------------------
// Path handling
// ---------------------------------------------------------------------------

/// Expand a leading `~`/`~/` against `$HOME`. Other `~user` forms are left
/// untouched (they'll fail resolve with a clear error).
fn expand_home(p: &Path) -> PathBuf {
    let s = p.as_os_str().to_string_lossy();
    if s == "~" {
        if let Some(h) = std::env::var_os("HOME") {
            return PathBuf::from(h);
        }
    } else if let Some(rest) = s.strip_prefix("~/") {
        if let Some(h) = std::env::var_os("HOME") {
            return PathBuf::from(h).join(rest);
        }
    }
    p.to_path_buf()
}

/// Absolute path for `p`: `~` expansion + relative→cwd. Deliberately NOT
/// canonicalized — canonicalize would resolve the final symlink, and
/// `info`/`delete`/`rename` must operate on the link itself, never its
/// target (deleting `link` must not delete the file it points at).
fn resolve(p: &Path) -> Result<PathBuf, String> {
    let p = expand_home(p);
    if p.is_absolute() {
        Ok(p)
    } else {
        std::env::current_dir()
            .map_err(|e| format!("cwd: {e}"))
            .map(|c| c.join(p))
    }
}

fn cwd() -> Result<PathBuf, String> {
    std::env::current_dir().map_err(|e| format!("cwd: {e}"))
}

/// Resolve the `rabbiz-files <path>` positional arg (the `.desktop` `%f`)
/// to the directory the GUI opens: the directory itself, or the parent of
/// a regular file — the app is a browser, not a viewer, so a file arg
/// honestly lands on the folder containing it. `~` expands via
/// [`expand_home`]; relative paths anchor at the process cwd via
/// [`resolve`]. The path must exist and be readable.
pub fn browse_dir_arg(arg: &str) -> Result<PathBuf, String> {
    let p = resolve(Path::new(arg))?;
    let meta = fs::metadata(&p).map_err(|e| format!("cannot access '{}': {e}", p.display()))?;
    if meta.is_dir() {
        // Confirm the directory can actually be listed.
        fs::read_dir(&p).map_err(|e| format!("cannot read '{}': {e}", p.display()))?;
        Ok(p)
    } else if meta.is_file() {
        fs::File::open(&p).map_err(|e| format!("cannot read '{}': {e}", p.display()))?;
        p.parent()
            .map(Path::to_path_buf)
            .ok_or_else(|| format!("'{}' has no parent directory", p.display()))
    } else {
        Err(format!(
            "'{}' is not a regular file or directory",
            p.display()
        ))
    }
}

// ---------------------------------------------------------------------------
// Shared operations (CLI text + MCP JSON both go through these)
// ---------------------------------------------------------------------------

/// `file|dir|link` for TSV/JSON output — link wins over the followed type.
fn type_of(e: &Entry) -> &'static str {
    if e.is_link {
        "link"
    } else if e.is_dir {
        "dir"
    } else {
        "file"
    }
}

/// Recursive byte size for directories via `model::dir_stats` (WALK_CAP-capped,
/// symlinks not followed — same numbers the details sheet shows); plain file
/// size otherwise.
fn entry_bytes(e: &Entry) -> u64 {
    if e.is_dir && !e.is_link {
        model::dir_stats(&e.path).map(|(_, b)| b).unwrap_or(0)
    } else {
        e.size
    }
}

/// One row of `list` output.
#[derive(Debug, Clone)]
pub struct Row {
    pub name: String,
    pub path: PathBuf,
    pub kind: &'static str,
    pub size: u64,
    pub mtime: i64,
}

/// Load + sort a directory exactly like the app's default view (dirs first,
/// name order, hidden included — the CLI is explicit, not a casual glance).
fn list_rows(dir: &Path) -> Result<Vec<Row>, String> {
    let abs = resolve(dir)?;
    let raw = model::load_dir(&abs).map_err(|e| format!("{}: {e}", abs.display()))?;
    let vis = model::visible_entries(&raw, SortKey::Name, true, "");
    Ok(vis
        .iter()
        .map(|e| Row {
            name: e.name.clone(),
            path: e.path.clone(),
            kind: type_of(e),
            size: entry_bytes(e),
            mtime: e.mtime,
        })
        .collect())
}

fn list_text(dir: &Path) -> Result<String, String> {
    let abs = resolve(dir)?;
    let rows = list_rows(&abs)?;
    if rows.is_empty() {
        return Ok("(empty)\n".to_string());
    }
    let mut s = String::new();
    for r in &rows {
        s.push_str(&format!(
            "{}\t{}\t{}\t{}\n",
            r.name, r.kind, r.size, r.mtime
        ));
    }
    Ok(s)
}

/// Everything `info` reports about one path.
#[derive(Debug)]
pub struct Info {
    pub path: PathBuf,
    /// `dir`, `file` or `link` (type follows symlinks, like the app).
    pub kind: &'static str,
    /// Present when the path itself is a symlink.
    pub link_target: Option<PathBuf>,
    /// File length, or recursive bytes for a directory (WALK_CAP-capped).
    pub bytes: u64,
    /// `(entries_seen, bytes)` for directories; `None` for files.
    pub dir_stats: Option<(usize, u64)>,
    pub perms: String,
    pub mode_octal: u32,
    pub mtime: i64,
}

fn inspect(path: &Path) -> Result<Info, String> {
    let abs = resolve(path)?;
    // symlink_metadata tells us "is a link"; metadata follows to the target
    // for type/size — same split model::load_dir uses.
    let lmeta = fs::symlink_metadata(&abs).map_err(|e| format!("{}: {e}", abs.display()))?;
    let is_link = lmeta.file_type().is_symlink();
    // Follow links for type/size; a DANGLING link still gets reported (as a
    // link, with the link's own metadata) instead of a bare error.
    let meta = fs::metadata(&abs)
        .or_else(|_| fs::symlink_metadata(&abs))
        .map_err(|e| format!("{}: {e}", abs.display()))?;
    let kind = if is_link {
        "link"
    } else if meta.is_dir() {
        "dir"
    } else {
        "file"
    };
    let link_target = if is_link {
        fs::read_link(&abs).ok()
    } else {
        None
    };
    let dir_stats = if meta.is_dir() && !is_link {
        model::dir_stats(&abs).ok()
    } else {
        None
    };
    let bytes = dir_stats.map(|(_, b)| b).unwrap_or_else(|| meta.len());
    let mode = {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            meta.mode()
        }
        #[cfg(not(unix))]
        {
            0o644
        }
    };
    Ok(Info {
        path: abs,
        kind,
        link_target,
        bytes,
        dir_stats,
        perms: model::mode_string(mode & 0o777),
        mode_octal: mode & 0o7777,
        mtime: meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0),
    })
}

fn info_text(path: &Path) -> Result<String, String> {
    let i = inspect(path)?;
    let mut s = String::new();
    s.push_str(&format!("path: {}\n", i.path.display()));
    s.push_str(&format!("type: {}\n", i.kind));
    if let Some(t) = &i.link_target {
        s.push_str(&format!("link: -> {}\n", t.display()));
    }
    match i.dir_stats {
        Some((n, _)) => {
            let cnt = if n >= model::WALK_CAP {
                format!("{}+", model::WALK_CAP)
            } else {
                n.to_string()
            };
            s.push_str(&format!(
                "size: {} ({} items, recursive)\n",
                model::format_size(i.bytes),
                cnt
            ));
        }
        None => s.push_str(&format!(
            "size: {} ({} bytes)\n",
            model::format_size(i.bytes),
            i.bytes
        )),
    }
    s.push_str(&format!(
        "modified: {} (mtime {})\n",
        model::format_time(i.mtime),
        i.mtime
    ));
    s.push_str(&format!("permissions: {} ({:o})\n", i.perms, i.mode_octal));
    Ok(s)
}

fn mkdir(path: &Path) -> Result<PathBuf, String> {
    let abs = resolve(path)?;
    model::make_dir(&abs).map_err(|e| format!("{}: {e}", abs.display()))?;
    Ok(abs)
}

fn rename(from: &Path, to: &Path) -> Result<(PathBuf, PathBuf), String> {
    let f = resolve(from)?;
    let t = resolve(to)?;
    // symlink_metadata (not exists()) so dangling symlinks count too.
    if fs::symlink_metadata(&f).is_err() {
        return Err(format!("{}: no such file or directory", f.display()));
    }
    if fs::symlink_metadata(&t).is_ok() {
        return Err(format!("{}: already exists", t.display()));
    }
    model::move_entry(&f, &t).map_err(|e| format!("{} -> {}: {e}", f.display(), t.display()))?;
    Ok((f, t))
}

/// What a delete would remove — computed BEFORE any `--yes` check so the
/// refusal message can say exactly what is at stake.
struct DeletePlan {
    path: PathBuf,
    /// True when the path is a real directory (symlinks to dirs are files
    /// for delete purposes — `remove_entry` removes the link, not the tree).
    is_dir: bool,
    /// Entries a recursive delete would remove (`count_tree`, capped).
    items: usize,
}

fn delete_plan(path: &Path) -> Result<DeletePlan, String> {
    let abs = resolve(path)?;
    // Guard on the CANONICAL path too: "/", "/..", "/." all reach the root,
    // and `resolve` no longer canonicalizes (see its doc comment).
    let canon = abs.canonicalize().unwrap_or_else(|_| abs.clone());
    if canon == Path::new("/") || abs == Path::new("/") {
        return Err("refusing to delete '/'".to_string());
    }
    let lmeta = fs::symlink_metadata(&abs).map_err(|e| format!("{}: {e}", abs.display()))?;
    let is_dir = lmeta.is_dir() && !lmeta.file_type().is_symlink();
    // Only real directories get a tree count — a symlink reports 1 (the
    // link itself is what gets removed, never the target's contents).
    let items = if is_dir { model::count_tree(&abs) } else { 1 };
    Ok(DeletePlan {
        path: abs,
        is_dir,
        items,
    })
}

/// Outcome of a completed delete.
#[derive(Debug)]
pub struct Deleted {
    pub path: PathBuf,
    pub items: usize,
    pub was_dir: bool,
}

/// Guarded permanent delete shared by CLI (`--yes`/`--recursive`) and MCP
/// (`confirm:true`/`recursive:true`). Rules, in order:
///   1. `/` is never deleted.
///   2. `confirm` is required for EVERYTHING — files included. The error
///      message states what would be removed.
///   3. A non-empty directory additionally requires `recursive`.
fn delete(path: &Path, recursive: bool, confirm: bool) -> Result<Deleted, String> {
    let plan = delete_plan(path)?;
    if !confirm {
        return Err(format!(
            "refusing to delete {} ({} item{}) — re-run with --yes/confirm:true",
            plan.path.display(),
            plan.items,
            if plan.items == 1 { "" } else { "s" }
        ));
    }
    if plan.is_dir && plan.items > 0 && !recursive {
        return Err(format!(
            "{} is a non-empty directory ({} items) — pass --recursive/recursive:true",
            plan.path.display(),
            plan.items
        ));
    }
    model::remove_entry(&plan.path).map_err(|e| format!("{}: {e}", plan.path.display()))?;
    Ok(Deleted {
        path: plan.path,
        items: plan.items,
        was_dir: plan.is_dir,
    })
}

fn delete_text(path: &Path, recursive: bool, yes: bool) -> Result<String, String> {
    let d = delete(path, recursive, yes)?;
    Ok(if d.was_dir {
        format!(
            "Deleted {} ({} item{})\n",
            d.path.display(),
            d.items,
            if d.items == 1 { "" } else { "s" }
        )
    } else {
        format!("Deleted {}\n", d.path.display())
    })
}

fn storage_text(dir: &Path) -> Result<String, String> {
    let abs = resolve(dir)?;
    let (free, total) =
        model::free_space(&abs).ok_or_else(|| format!("{}: statvfs failed", abs.display()))?;
    let used_pct = if total > 0 {
        ((total - free) as f64 / total as f64 * 100.0) as i32
    } else {
        0
    };
    Ok(format!(
        "path: {}\nfree: {} ({} bytes)\ntotal: {} ({} bytes)\nused: {}%\n",
        abs.display(),
        model::format_size(free),
        free,
        model::format_size(total),
        total,
        used_pct
    ))
}

// ---------------------------------------------------------------------------
// CLI entry point
// ---------------------------------------------------------------------------

fn execute(cmd: &Command) -> Result<String, String> {
    match cmd {
        Command::Pwd => Ok(format!("{}\n", cwd()?.display())),
        Command::List { dir } => list_text(dir.as_deref().unwrap_or_else(|| Path::new("."))),
        Command::Info { path } => info_text(path),
        Command::Mkdir { path } => {
            let abs = mkdir(path)?;
            Ok(format!("Created {}\n", abs.display()))
        }
        Command::Rename { from, to } => {
            let (f, t) = rename(from, to)?;
            Ok(format!("Renamed {} -> {}\n", f.display(), t.display()))
        }
        Command::Delete {
            path,
            recursive,
            yes,
        } => delete_text(path, *recursive, *yes),
        Command::Storage => storage_text(Path::new(".")),
        Command::Help => Ok(HELP.to_string()),
    }
}

/// Run `--cli` args; returns the process exit code (0 ok / 1 failed / 2 usage).
pub fn run_cli(args: &[String]) -> i32 {
    let cmd = match parse(args) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("rabbiz-files: {e}\n{HELP}");
            return 2;
        }
    };
    match execute(&cmd) {
        Ok(text) => {
            print!("{text}");
            if !text.ends_with('\n') {
                println!();
            }
            0
        }
        Err(e) => {
            eprintln!("rabbiz-files: {e}");
            1
        }
    }
}

// ---------------------------------------------------------------------------
// MCP server
// ---------------------------------------------------------------------------

fn arg_str<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    args.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("missing required argument '{key}'"))
}

fn mcp_list(args: &Value) -> Result<String, String> {
    let dir = args
        .get("dir")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let rows = list_rows(&dir)?;
    let arr: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "name": r.name,
                "path": r.path,
                "type": r.kind,
                "size": r.size,
                "mtime": r.mtime,
            })
        })
        .collect();
    Ok(json!(arr).to_string())
}

fn mcp_info(args: &Value) -> Result<String, String> {
    let i = inspect(Path::new(arg_str(args, "path")?))?;
    Ok(json!({
        "path": i.path,
        "type": i.kind,
        "link_target": i.link_target,
        "bytes": i.bytes,
        "items": i.dir_stats.map(|(n, _)| n),
        "mtime": i.mtime,
        "permissions": i.perms,
        "mode": format!("{:o}", i.mode_octal),
    })
    .to_string())
}

fn mcp_mkdir(args: &Value) -> Result<String, String> {
    let abs = mkdir(Path::new(arg_str(args, "path")?))?;
    Ok(json!({ "path": abs, "created": true }).to_string())
}

fn mcp_rename(args: &Value) -> Result<String, String> {
    let (f, t) = rename(
        Path::new(arg_str(args, "from")?),
        Path::new(arg_str(args, "to")?),
    )?;
    Ok(json!({ "from": f, "to": t }).to_string())
}

fn mcp_delete(args: &Value) -> Result<String, String> {
    let path = arg_str(args, "path")?;
    let recursive = args
        .get("recursive")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    // Honest safety: `confirm: true` is required for every delete — files
    // included — and non-empty directories additionally need `recursive: true`.
    let confirm = args
        .get("confirm")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let d = delete(Path::new(path), recursive, confirm)?;
    Ok(json!({
        "deleted": d.path,
        "items": d.items,
        "was_dir": d.was_dir,
    })
    .to_string())
}

fn mcp_storage(_args: &Value) -> Result<String, String> {
    let abs = cwd()?;
    let (free, total) = model::free_space(&abs).ok_or_else(|| "statvfs failed".to_string())?;
    let used_pct = if total > 0 {
        ((total - free) as f64 / total as f64 * 100.0) as i32
    } else {
        0
    };
    Ok(json!({
        "path": abs,
        "free_bytes": free,
        "total_bytes": total,
        "free": model::format_size(free),
        "total": model::format_size(total),
        "used_pct": used_pct,
    })
    .to_string())
}

/// MCP tool table served by `--mcp`.
pub static TOOLS: &[Tool] = &[
    Tool {
        name: "files_list",
        description: "List a directory's entries (hidden files included, \
                      dirs first then name — same order as the app). \
                      Returns JSON array of {name,path,type,size,mtime}; \
                      type is file|dir|link, dir size is recursive \
                      (capped like the app). Defaults to the cwd.",
        input_schema: r#"{"type":"object","properties":{"dir":{"type":"string","description":"Directory to list (default: current directory)"}},"additionalProperties":false}"#,
        handler: mcp_list,
    },
    Tool {
        name: "file_info",
        description: "Metadata for one path: type (file|dir|link), size \
                      (recursive and capped for directories), mtime, \
                      permission string + octal mode, and the symlink \
                      target when the path is a link.",
        input_schema: r#"{"type":"object","properties":{"path":{"type":"string","description":"File or directory path (~ and relative paths allowed)"}},"required":["path"],"additionalProperties":false}"#,
        handler: mcp_info,
    },
    Tool {
        name: "files_mkdir",
        description: "Create a directory. Parent directories must already \
                      exist (like `mkdir`, not `mkdir -p`). Fails if the \
                      path already exists.",
        input_schema: r#"{"type":"object","properties":{"path":{"type":"string","description":"Directory path to create"}},"required":["path"],"additionalProperties":false}"#,
        handler: mcp_mkdir,
    },
    Tool {
        name: "file_rename",
        description: "Rename or move an entry. Fails if the destination \
                      already exists. Cross-device moves fall back to \
                      copy+delete (same as the app's Move).",
        input_schema: r#"{"type":"object","properties":{"from":{"type":"string","description":"Existing path"},"to":{"type":"string","description":"New path"}},"required":["from","to"],"additionalProperties":false}"#,
        handler: mcp_rename,
    },
    Tool {
        name: "file_delete",
        description: "Permanently delete a file, symlink or directory — \
                      there is NO trash, this cannot be undone. SAFETY: \
                      'confirm' must be true for EVERY delete; deleting a \
                      non-empty directory additionally requires \
                      'recursive': true (empty directories do not). \
                      Deleting '/' is always refused.",
        input_schema: r#"{"type":"object","properties":{"path":{"type":"string","description":"Path to delete"},"recursive":{"type":"boolean","description":"Required true to delete a non-empty directory"},"confirm":{"type":"boolean","description":"Must be true to confirm the permanent delete"}},"required":["path","confirm"],"additionalProperties":false}"#,
        handler: mcp_delete,
    },
    Tool {
        name: "storage_status",
        description: "Filesystem usage of the current working directory's \
                      mount (same statvfs numbers as the app footer): \
                      free/total bytes and used percentage.",
        input_schema: r#"{"type":"object","properties":{},"additionalProperties":false}"#,
        handler: mcp_storage,
    },
];

/// Serve the MCP tool table on stdin/stdout; never returns.
pub fn serve_mcp() -> ! {
    serve(TOOLS, "rabbiz-files", env!("CARGO_PKG_VERSION"))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static N: AtomicUsize = AtomicUsize::new(0);

    fn tmpdir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "rabbiz-files-cli-{tag}-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn s(args: &[&str]) -> Vec<String> {
        args.iter().map(|a| a.to_string()).collect()
    }

    #[test]
    fn parse_commands() {
        assert_eq!(parse(&[]).unwrap(), Command::Help);
        assert_eq!(parse(&s(&["help"])).unwrap(), Command::Help);
        assert_eq!(parse(&s(&["--help"])).unwrap(), Command::Help);
        assert_eq!(parse(&s(&["pwd"])).unwrap(), Command::Pwd);
        assert_eq!(
            parse(&s(&["list", "/tmp"])).unwrap(),
            Command::List {
                dir: Some(PathBuf::from("/tmp"))
            }
        );
        assert_eq!(
            parse(&s(&["info", "a.txt"])).unwrap(),
            Command::Info {
                path: PathBuf::from("a.txt")
            }
        );
        assert_eq!(
            parse(&s(&["rename", "a", "b"])).unwrap(),
            Command::Rename {
                from: PathBuf::from("a"),
                to: PathBuf::from("b")
            }
        );
        assert_eq!(
            parse(&s(&["delete", "x", "--recursive", "--yes"])).unwrap(),
            Command::Delete {
                path: PathBuf::from("x"),
                recursive: true,
                yes: true
            }
        );
        assert!(parse(&s(&["bogus"])).is_err());
        assert!(parse(&s(&["info"])).is_err());
        assert!(parse(&s(&["rename", "a"])).is_err());
        assert!(parse(&s(&["delete", "x", "--bogus"])).is_err());
        assert!(parse(&s(&["list", "a", "b"])).is_err());
    }

    #[test]
    fn list_tsv_format() {
        let d = tmpdir("list");
        fs::write(d.join("b.txt"), b"12345").unwrap();
        fs::create_dir(d.join("adir")).unwrap();
        fs::write(d.join(".hid"), b"x").unwrap();
        let text = execute(&Command::List {
            dir: Some(d.clone()),
        })
        .unwrap();
        // dirs first, then files by name; hidden included
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 3, "{text}");
        assert!(lines[0].starts_with("adir\tdir\t"), "{text}");
        assert!(lines[1].starts_with(".hid\tfile\t1\t"), "{text}");
        assert!(lines[2].starts_with("b.txt\tfile\t5\t"), "{text}");
        for l in &lines {
            assert_eq!(l.split('\t').count(), 4, "{l}");
        }
        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn info_fields() {
        let d = tmpdir("info");
        let f = d.join("f.txt");
        fs::write(&f, b"hello").unwrap();
        let text = execute(&Command::Info { path: f.clone() }).unwrap();
        assert!(text.contains("type: file"), "{text}");
        assert!(text.contains("size: 5 B (5 bytes)"), "{text}");
        assert!(text.contains("permissions: "), "{text}");
        assert!(text.contains("modified: "), "{text}");
        assert!(text.contains("mtime "), "{text}");
        // dir info reports recursive stats
        fs::create_dir(d.join("sub")).unwrap();
        fs::write(d.join("sub/x"), b"123").unwrap();
        let text = execute(&Command::Info {
            path: d.join("sub"),
        })
        .unwrap();
        assert!(text.contains("type: dir"), "{text}");
        assert!(text.contains("recursive"), "{text}");
        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn mkdir_rename_delete_roundtrip() {
        let d = tmpdir("ops");
        let sub = d.join("sub");
        let out = execute(&Command::Mkdir { path: sub.clone() }).unwrap();
        assert!(out.contains("Created"), "{out}");
        assert!(sub.is_dir());
        // mkdir refuses existing
        assert!(execute(&Command::Mkdir { path: sub.clone() }).is_err());

        let f = sub.join("f.txt");
        fs::write(&f, b"data").unwrap();
        let renamed = sub.join("g.txt");
        let out = execute(&Command::Rename {
            from: f.clone(),
            to: renamed.clone(),
        })
        .unwrap();
        assert!(out.contains("Renamed"), "{out}");
        assert!(renamed.exists() && !f.exists());
        // rename refuses existing destination
        fs::write(sub.join("h.txt"), b"x").unwrap();
        assert!(execute(&Command::Rename {
            from: renamed.clone(),
            to: sub.join("h.txt"),
        })
        .is_err());

        // delete requires --yes
        assert!(execute(&Command::Delete {
            path: renamed.clone(),
            recursive: false,
            yes: false,
        })
        .is_err());
        let out = execute(&Command::Delete {
            path: renamed.clone(),
            recursive: false,
            yes: true,
        })
        .unwrap();
        assert!(out.contains("Deleted"), "{out}");
        assert!(!renamed.exists());
        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn delete_recursive_guard() {
        let d = tmpdir("delguard");
        let tree = d.join("tree");
        fs::create_dir_all(tree.join("a/b")).unwrap();
        fs::write(tree.join("a/b/c.txt"), b"x").unwrap();

        // non-empty dir: --yes alone is not enough
        let e = execute(&Command::Delete {
            path: tree.clone(),
            recursive: false,
            yes: true,
        })
        .unwrap_err();
        assert!(e.contains("non-empty"), "{e}");
        assert!(tree.exists());

        // --recursive without --yes still refused, message says what it would do
        let e = execute(&Command::Delete {
            path: tree.clone(),
            recursive: true,
            yes: false,
        })
        .unwrap_err();
        assert!(e.contains("--yes"), "{e}");
        assert!(tree.exists());

        // both flags: gone
        execute(&Command::Delete {
            path: tree.clone(),
            recursive: true,
            yes: true,
        })
        .unwrap();
        assert!(!tree.exists());

        // '/' is never deleted
        assert!(execute(&Command::Delete {
            path: PathBuf::from("/"),
            recursive: true,
            yes: true,
        })
        .is_err());
        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn storage_reports() {
        let text = execute(&Command::Storage).unwrap();
        assert!(text.contains("free:"), "{text}");
        assert!(text.contains("total:"), "{text}");
        assert!(text.contains("used:"), "{text}");
    }

    #[test]
    fn expand_home_and_resolve() {
        // "~" expands to $HOME; relative paths anchor at cwd.
        let home = expand_home(Path::new("~"));
        assert!(home.is_absolute());
        let abs = resolve(Path::new(".")).unwrap();
        assert!(abs.is_absolute());
        let t = resolve(Path::new("~/definitely-not-here-xyz")).unwrap();
        assert!(t.starts_with(&home), "{t:?} vs {home:?}");
    }

    #[test]
    fn browse_dir_arg_dir_and_file() {
        let d = std::env::temp_dir().join(format!("rabbiz-files-arg-{}", std::process::id()));
        fs::create_dir_all(&d).unwrap();
        // A directory opens itself.
        assert_eq!(browse_dir_arg(d.to_str().unwrap()).unwrap(), d);
        // A regular file opens its parent directory (browser, not viewer).
        let f = d.join("note.txt");
        fs::write(&f, b"x").unwrap();
        assert_eq!(browse_dir_arg(f.to_str().unwrap()).unwrap(), d);
        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn browse_dir_arg_rejects_bad_paths() {
        assert!(browse_dir_arg("/definitely/not/here-rabbiz-xyz").is_err());
        // "~" alone expands to $HOME which exists and is readable.
        if std::env::var_os("HOME").is_some() {
            assert!(browse_dir_arg("~").is_ok());
        }
    }

    // ---- MCP ----

    #[test]
    fn mcp_schemas_parse() {
        assert!(!TOOLS.is_empty());
        for t in TOOLS {
            let v: Value = serde_json::from_str(t.input_schema).unwrap();
            assert_eq!(v["additionalProperties"], json!(false), "{}", t.name);
        }
        let names: Vec<&str> = TOOLS.iter().map(|t| t.name).collect();
        for want in [
            "files_list",
            "file_info",
            "files_mkdir",
            "file_rename",
            "file_delete",
            "storage_status",
        ] {
            assert!(names.contains(&want), "missing {want}");
        }
    }

    fn call(name: &str, args: Value) -> Result<String, String> {
        let t = TOOLS.iter().find(|t| t.name == name).unwrap();
        (t.handler)(&args)
    }

    #[test]
    fn mcp_handlers_smoke() {
        let d = tmpdir("mcp");
        fs::write(d.join("a.txt"), b"123").unwrap();
        fs::create_dir(d.join("sub")).unwrap();
        let dir = d.to_string_lossy().into_owned();

        // list
        let out = call("files_list", json!({"dir": dir})).unwrap();
        let arr: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(arr.as_array().unwrap().len(), 2);
        assert_eq!(arr[0]["type"], json!("dir")); // dirs first
        assert_eq!(arr[1]["name"], json!("a.txt"));

        // info
        let out = call("file_info", json!({"path": format!("{dir}/a.txt")})).unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["type"], json!("file"));
        assert_eq!(v["bytes"], json!(3));
        assert!(v["permissions"].as_str().unwrap().len() == 9);

        // mkdir + rename
        call("files_mkdir", json!({"path": format!("{dir}/newdir")})).unwrap();
        call(
            "file_rename",
            json!({"from": format!("{dir}/a.txt"), "to": format!("{dir}/b.txt")}),
        )
        .unwrap();
        assert!(d.join("b.txt").exists());

        // delete guards: confirm required, recursive required for non-empty
        fs::write(d.join("newdir/x"), b"y").unwrap();
        assert!(call(
            "file_delete",
            json!({"path": format!("{dir}/b.txt"), "confirm": false})
        )
        .is_err());
        assert!(call(
            "file_delete",
            json!({"path": format!("{dir}/newdir"), "confirm": true})
        )
        .is_err());
        call(
            "file_delete",
            json!({"path": format!("{dir}/newdir"), "confirm": true, "recursive": true}),
        )
        .unwrap();
        assert!(!d.join("newdir").exists());
        call(
            "file_delete",
            json!({"path": format!("{dir}/b.txt"), "confirm": true}),
        )
        .unwrap();

        // storage
        let out = call("storage_status", json!({})).unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert!(v["total_bytes"].as_u64().unwrap() > 0);
        fs::remove_dir_all(&d).ok();
    }
}
