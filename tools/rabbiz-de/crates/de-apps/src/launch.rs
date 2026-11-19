//! App launch helper (M1-T7): spawn a desktop entry under `setsid` with the
//! single Rust-owned environment contract and per-id `launch.toml` overrides.

use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// Embedded override table — exact desktop-id matches only, no globs.
const LAUNCH_TOML: &str = include_str!("../data/launch.toml");

#[derive(Debug, serde::Deserialize)]
struct LaunchTable {
    #[serde(default)]
    overrides: BTreeMap<String, Override>,
}

#[derive(Debug, serde::Deserialize)]
struct Override {
    exec: String,
    #[serde(default)]
    env: BTreeMap<String, String>,
}

/// Launch `<desktop-id>` detached from the compositor.
///
/// Resolution order: `launch.toml` override → `<id>.desktop` Exec line →
/// treat the id itself as a command. Logs land in
/// `$XDG_STATE_HOME/rabbiz-de/apps/<desktop-id>.log` (append).
pub fn launch(desktop_id: &str) -> Result<i32, String> {
    let table: LaunchTable =
        toml::from_str(LAUNCH_TOML).map_err(|e| format!("embedded launch.toml is invalid: {e}"))?;

    let (cmdline, extra_env) = match table.overrides.get(desktop_id) {
        Some(o) => (o.exec.clone(), o.env.clone()),
        None => (desktop_exec(desktop_id), BTreeMap::new()),
    };
    let argv = split_shellish(&cmdline);
    if argv.is_empty() {
        return Err(format!("empty exec for {desktop_id}"));
    }

    let state = std::env::var("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|_| std::env::var("HOME").map(|h| PathBuf::from(h).join(".local/state")))
        .map_err(|_| "no XDG_STATE_HOME/HOME for app logs".to_string())?;
    let logdir = state.join("rabbiz-de/apps");
    std::fs::create_dir_all(&logdir).map_err(|e| format!("logdir: {e}"))?;
    let log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(logdir.join(format!("{desktop_id}.log")))
        .map_err(|e| format!("log open: {e}"))?;
    let log_err = log.try_clone().map_err(|e| format!("log clone: {e}"))?;

    let mut cmd = Command::new("setsid");
    cmd.arg(&argv[0]).args(&argv[1..]).stdin(Stdio::null());
    cmd.stdout(Stdio::from(log)).stderr(Stdio::from(log_err));
    for (k, v) in de_system::env::app_env() {
        match v {
            Some(val) => cmd.env(k, val),
            None => cmd.env_remove(k),
        };
    }
    for name in de_system::env::unset_names() {
        cmd.env_remove(name);
    }
    for (k, v) in &extra_env {
        cmd.env(k, v);
    }

    match cmd.spawn() {
        Ok(mut child) => {
            let pid = child.id() as i32;
            // A dropped `Child` is never reaped — without this every exited
            // app would linger as a zombie under the compositor.
            std::thread::spawn(move || {
                let _ = child.wait();
            });
            Ok(pid)
        }
        Err(e) => Err(format!("spawn {}: {e}", argv[0])),
    }
}

/// Find `<id>.desktop` in the application dirs and extract its `Exec`,
/// falling back to the bare id as a command name.
fn desktop_exec(desktop_id: &str) -> String {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Ok(home) = std::env::var("XDG_DATA_HOME") {
        dirs.push(PathBuf::from(home).join("applications"));
    } else if let Ok(home) = std::env::var("HOME") {
        dirs.push(PathBuf::from(home).join(".local/share/applications"));
    }
    let data_dirs =
        std::env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".into());
    for d in data_dirs.split(':') {
        dirs.push(PathBuf::from(d).join("applications"));
    }
    for dir in dirs {
        let f = dir.join(format!("{desktop_id}.desktop"));
        if let Ok(text) = std::fs::read_to_string(&f) {
            if let Some(exec) = parse_desktop_exec(&text) {
                return exec;
            }
        }
    }
    desktop_id.to_string()
}

/// Pull `Exec=` out of the `[Desktop Entry]` group and strip field codes.
fn parse_desktop_exec(text: &str) -> Option<String> {
    let mut in_entry = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if in_entry && line.starts_with("Exec=") {
            let raw = &line[5..];
            return Some(strip_field_codes(raw));
        }
    }
    None
}

/// Remove freedesktop field codes (%f %F %u %U %i %c %k ...) per spec.
fn strip_field_codes(exec: &str) -> String {
    let mut out = String::with_capacity(exec.len());
    for tok in exec.split_whitespace() {
        if tok.starts_with('%') && tok.len() == 2 && tok != "%%" {
            continue;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(tok);
    }
    out
}

/// Minimal shell-ish split honoring single/double quotes.
fn split_shellish(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let chars = s.chars().peekable();
    let mut quote: Option<char> = None;
    for c in chars {
        match quote {
            Some(q) if c == q => quote = None,
            None if c == '\'' || c == '"' => quote = Some(c),
            None if c.is_whitespace() => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            _ => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_field_codes() {
        assert_eq!(
            strip_field_codes("chrome %U --new-window"),
            "chrome --new-window"
        );
        assert_eq!(strip_field_codes("foo"), "foo");
    }

    #[test]
    fn splits_quotes() {
        assert_eq!(
            split_shellish("rabbiz-browser --flag 'two words'"),
            vec!["rabbiz-browser", "--flag", "two words"]
        );
    }

    #[test]
    fn embedded_toml_parses() {
        let t: LaunchTable = toml::from_str(LAUNCH_TOML).unwrap();
        assert_eq!(t.overrides["google-chrome"].exec, "rabbiz-browser");
    }
}
