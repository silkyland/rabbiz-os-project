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

/// Validate a freedesktop `desktop_id` (or log stem) against a strict allowlist (`APP-001`).
///
/// Rejects empty strings, path separators (`/` and `\`), traversal (`..`), leading `.` or `-`,
/// NUL, whitespace, and control characters; permits only `[A-Za-z0-9._:-]+`.
#[must_use]
pub fn is_valid_desktop_id(id: &str) -> bool {
    if id.is_empty()
        || id.len() > 255
        || id.starts_with('.')
        || id.starts_with('-')
        || id.contains("..")
    {
        return false;
    }
    id.bytes().all(|b| {
        b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b':' | b'-')
    })
}

/// Launch `<desktop-id>` detached from the compositor.
///
/// Resolution order: `launch.toml` override → `<id>.desktop` Exec line.
/// Unknown `desktop_id`s are rejected rather than executed verbatim. Logs land in
/// `$XDG_STATE_HOME/rabbiz-de/apps/<desktop-id>.log` (append).
pub fn launch(desktop_id: &str) -> Result<i32, String> {
    launch_with_target(desktop_id, None)
}

/// Launch `<desktop-id>` detached, optionally passing a file path or URI
/// argument (`target`) to the resolved command.
pub fn launch_with_target(desktop_id: &str, target: Option<&str>) -> Result<i32, String> {
    if !is_valid_desktop_id(desktop_id) {
        return Err(format!("invalid desktop_id: {desktop_id:?}"));
    }
    let table: LaunchTable =
        toml::from_str(LAUNCH_TOML).map_err(|e| format!("embedded launch.toml is invalid: {e}"))?;

    let (cmdline, extra_env) = match table.overrides.get(desktop_id) {
        Some(o) => (o.exec.clone(), o.env.clone()),
        None => {
            let exec = desktop_exec(desktop_id)
                .ok_or_else(|| format!("unknown desktop_id: {desktop_id}"))?;
            (exec, BTreeMap::new())
        }
    };
    let mut argv = split_shellish(&cmdline);
    if argv.is_empty() {
        return Err(format!("empty exec for {desktop_id}"));
    }
    if let Some(arg) = target.filter(|s| !s.is_empty()) {
        argv.push(arg.to_string());
    }
    spawn(&argv, desktop_id, &extra_env)
}

/// Resolve the default `desktop_id` (without `.desktop`) for `mime` using
/// `mimeapps.list` defaults first, falling back to associated catalog entries.
#[must_use]
pub fn resolve_default_for_mime(
    mime: &str,
    entries: &[crate::catalog::AppEntry],
    mime_apps: &crate::mime::MimeApps,
) -> Option<String> {
    let valid = |desktop_file_id: &str| {
        let stem = desktop_file_id.strip_suffix(".desktop").unwrap_or(desktop_file_id);
        entries.iter().any(|e| e.id == stem)
    };
    if let Some(chosen) = mime_apps.default_app(mime, Some(&valid)) {
        return Some(chosen.strip_suffix(".desktop").unwrap_or(&chosen).to_string());
    }
    let catalog_iter = entries.iter().flat_map(|e| {
        e.mime_types
            .iter()
            .map(move |m| (e.id.as_str(), m.as_str()))
    });
    mime_apps
        .associated_apps(mime, catalog_iter)
        .into_iter()
        .map(|id| id.strip_suffix(".desktop").unwrap_or(&id).to_string())
        .find(|stem| entries.iter().any(|e| e.id == *stem))
}

/// Resolve and launch the default application for `mime` with optional `target`.
pub fn launch_for_mime(mime: &str, target: Option<&str>) -> Result<i32, String> {
    let catalog = crate::catalog::Catalog::scan(&[]);
    let mime_apps = crate::mime::MimeApps::load();
    let id = resolve_default_for_mime(mime, catalog.all(), &mime_apps)
        .ok_or_else(|| format!("no application registered for {mime}"))?;
    launch_with_target(&id, target)
}

/// Spawn an already-resolved command line detached — the same env/log/plumbing
/// as [`launch`] but without desktop-id or `launch.toml` resolution. Used by
/// `autostart` entries and the `rabbiz-de autostart` session step.
pub fn launch_command(cmdline: &str, log_name: &str) -> Result<i32, String> {
    let argv = split_shellish(cmdline);
    if argv.is_empty() {
        return Err(format!("empty exec: {cmdline}"));
    }
    spawn(&argv, log_name, &BTreeMap::new())
}

fn spawn(
    argv: &[String],
    log_name: &str,
    extra_env: &BTreeMap<String, String>,
) -> Result<i32, String> {
    if !is_valid_desktop_id(log_name) {
        return Err(format!("invalid log_name: {log_name:?}"));
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
        .open(logdir.join(format!("{log_name}.log")))
        .map_err(|e| format!("log open: {e}"))?;
    let log_err = log.try_clone().map_err(|e| format!("log clone: {e}"))?;

    let mut cmd = Command::new("setsid");
    cmd.arg("--")
        .arg(&argv[0])
        .args(&argv[1..])
        .stdin(Stdio::null());
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
    for (k, v) in extra_env {
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

/// Find `<id>.desktop` in the application dirs and extract its `Exec`.
/// Returns `None` if no matching `.desktop` file exists (never falls back to
/// executing an arbitrary uncatalogued command string).
fn desktop_exec(desktop_id: &str) -> Option<String> {
    for dir in crate::catalog::applications_dirs() {
        let f = dir.join(format!("{desktop_id}.desktop"));
        if let Ok(text) = std::fs::read_to_string(&f) {
            if let Some(exec) = parse_desktop_exec(&text) {
                return Some(exec);
            }
        }
    }
    None
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

/// Remove freedesktop field codes (%f %F %u %U %i %c %k ...) and Flatpak @@u / @@ markers per spec.
fn strip_field_codes(exec: &str) -> String {
    let mut out = String::with_capacity(exec.len());
    for tok in exec.split_whitespace() {
        if (tok.starts_with('%') && tok.len() == 2 && tok != "%%")
            || tok == "@@u"
            || tok == "@@"
            || tok == "--file-forwarding"
        {
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
        assert!(t.overrides["google-chrome"]
            .exec
            .starts_with("/opt/google/chrome/google-chrome"));
    }

    #[test]
    fn resolves_mime_default_then_catalog_fallback() {
        let entries = vec![
            crate::catalog::AppEntry {
                id: "rabbiz-pdf".to_string(),
                name: "PDF".to_string(),
                generic_name: String::new(),
                icon: String::new(),
                exec: Some("rabbiz-pdf".to_string()),
                categories: vec![],
                keywords: vec![],
                mime_types: vec!["application/pdf".to_string()],
                actions: vec![],
                orientation: de_api::OrientationRequest::Unspecified,
            },
            crate::catalog::AppEntry {
                id: "google-chrome".to_string(),
                name: "Chrome".to_string(),
                generic_name: String::new(),
                icon: String::new(),
                exec: Some("google-chrome".to_string()),
                categories: vec![],
                keywords: vec![],
                mime_types: vec!["text/html".to_string()],
                actions: vec![],
                orientation: de_api::OrientationRequest::Unspecified,
            },
        ];
        let mut mime_apps = crate::mime::MimeApps::default();
        mime_apps.parse("[Default Applications]\ntext/html=google-chrome.desktop\n");
        assert_eq!(
            resolve_default_for_mime("text/html", &entries, &mime_apps).as_deref(),
            Some("google-chrome")
        );
        assert_eq!(
            resolve_default_for_mime("application/pdf", &entries, &mime_apps).as_deref(),
            Some("rabbiz-pdf")
        );
        assert_eq!(
            resolve_default_for_mime("application/x-unknown", &entries, &mime_apps),
            None
        );
    }

    #[test]
    fn desktop_id_validation_rejects_traversal_and_shell_strings() {
        for good in [
            "rabbiz-files",
            "org.kde.dolphin",
            "google-chrome",
            "app_1:main",
        ] {
            assert!(is_valid_desktop_id(good), "expected valid: {good}");
        }
        for bad in [
            "",
            ".",
            "..",
            "../etc/passwd",
            "foo/bar",
            "foo\\bar",
            "foo..bar",
            ".hidden",
            "-flag",
            "--help",
            "sh -c id",
            "foo\0bar",
            "foo\nbar",
        ] {
            assert!(!is_valid_desktop_id(bad), "expected invalid: {bad:?}");
        }
    }

    #[test]
    fn launch_rejects_unknown_and_malicious_desktop_ids() {
        assert!(launch("sh -c id").is_err());
        assert!(launch("../../bin/sh").is_err());
        assert!(launch("nonexistent-app-id-xyz-999").is_err());
    }
}
