//! XDG Autostart (AUDIT-DE-STANDARDS-2026-11-21 §2): user/session agents
//! installed under `autostart/` — `gnome-keyring-daemon`, `mako`, `kdeconnect`,
//! input remappers — launch at session start like on every other desktop.
//!
//! Spec rules honored: `Hidden`, `OnlyShowIn`/`NotShowIn` against
//! `XDG_CURRENT_DESKTOP` (Rabbiz), `X-GNOME-Autostart-enabled=false`, and
//! `TryExec` binaries must exist on `PATH`. `$XDG_CONFIG_HOME` entries shadow
//! same-named `$XDG_CONFIG_DIRS` ones (spec precedence).

use std::path::{Path, PathBuf};

use freedesktop_desktop_entry::DesktopEntry;

/// One autostart entry that passed the spec filters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutostartEntry {
    /// Desktop file stem — also the log-file name.
    pub id: String,
    /// Field-code-stripped `Exec=` line, ready for `launch_command`.
    pub exec: String,
}

/// Autostart dirs in spec order (`$XDG_CONFIG_HOME` first).
fn autostart_dirs() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(dir) = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from) {
        out.push(dir.join("autostart"));
    } else if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        out.push(home.join(".config/autostart"));
    }
    for dir in std::env::var("XDG_CONFIG_DIRS")
        .unwrap_or_else(|_| "/etc/xdg".into())
        .split(':')
        .filter(|s| !s.is_empty())
    {
        out.push(PathBuf::from(dir).join("autostart"));
    }
    out
}

/// `TryExec` binary resolvable on `PATH` (absolute path counts directly).
fn try_exec_ok(name: &str) -> bool {
    if name.starts_with('/') {
        return Path::new(name).exists();
    }
    std::env::var_os("PATH")
        .is_some_and(|paths| std::env::split_paths(&paths).any(|dir| dir.join(name).exists()))
}

/// Scan autostart dirs for entries eligible under the current desktop
/// (`desktop` = `XDG_CURRENT_DESKTOP`, e.g. "Rabbiz").
#[must_use]
pub fn entries(desktop: &str) -> Vec<AutostartEntry> {
    entries_in(&autostart_dirs(), desktop)
}

/// Testable variant over explicit dirs.
#[must_use]
pub fn entries_in(dirs: &[PathBuf], desktop: &str) -> Vec<AutostartEntry> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for dir in dirs {
        let Ok(read) = std::fs::read_dir(dir) else {
            continue;
        };
        for file in read.flatten() {
            let path = file.path();
            if path.extension().and_then(|e| e.to_str()) != Some("desktop") {
                continue;
            }
            let id = path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            if !seen.insert(id.clone()) {
                continue; // higher-precedence dir already claimed this name
            }
            let Some(entry) = eligible(&path, desktop) else {
                continue;
            };
            out.push(AutostartEntry { id, exec: entry });
        }
    }
    out
}

/// The stripped `Exec=` if the file may autostart under `desktop`.
fn eligible(path: &Path, desktop: &str) -> Option<String> {
    let de = DesktopEntry::from_path(path.to_path_buf(), None::<&[&str]>).ok()?;
    if de.hidden() {
        return None;
    }
    if de
        .desktop_entry("X-GNOME-Autostart-enabled")
        .is_some_and(|v| v.eq_ignore_ascii_case("false"))
    {
        return None;
    }
    let names: Vec<&str> = desktop.split(':').collect();
    if let Some(only) = de.only_show_in() {
        if !only.iter().any(|n| names.contains(n)) {
            return None;
        }
    }
    if de
        .desktop_entry("NotShowIn")
        .is_some_and(|v| v.split(';').any(|n| names.contains(&n.trim())))
    {
        return None;
    }
    if let Some(try_exec) = de.try_exec() {
        if !try_exec_ok(try_exec) {
            return None;
        }
    }
    let exec = de.exec()?;
    let stripped = strip_field_codes(exec);
    (!stripped.is_empty()).then_some(stripped)
}

/// Same field-code removal as `launch.rs` (kept private on both sides — the
/// autostart spec adds `@@` Flatpak markers already covered there).
fn strip_field_codes(exec: &str) -> String {
    exec.split_whitespace()
        .filter(|t| {
            !((t.starts_with('%') && t.len() == 2 && *t != "%%") || *t == "@@u" || *t == "@@")
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Launch every eligible autostart entry (detached, app env contract).
/// Returns per-entry outcomes for the session log.
pub fn launch_all(desktop: &str) -> Vec<(String, Result<i32, String>)> {
    entries(desktop)
        .into_iter()
        .map(|e| {
            let id = format!("autostart-{}", e.id);
            (e.id.clone(), crate::launch::launch_command(&e.exec, &id))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, name: &str, body: &str) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join(name), body).unwrap();
    }

    #[test]
    fn filters_hidden_showin_and_shadowing() {
        let tmp = std::env::temp_dir().join(format!("de-autostart-{}", std::process::id()));
        let home_dir = tmp.join("home");
        let sys_dir = tmp.join("sys");
        write(
            &home_dir,
            "agent.desktop",
            "[Desktop Entry]\nType=Application\nName=A\nExec=/bin/true\n",
        );
        write(
            &sys_dir,
            "agent.desktop",
            "[Desktop Entry]\nType=Application\nName=A\nExec=/bin/false\n",
        );
        write(
            &sys_dir,
            "gnome-only.desktop",
            "[Desktop Entry]\nType=Application\nName=G\nExec=/bin/true\nOnlyShowIn=GNOME\n",
        );
        write(
            &sys_dir,
            "hidden.desktop",
            "[Desktop Entry]\nType=Application\nName=H\nExec=/bin/true\nHidden=true\n",
        );
        let out = entries_in(&[home_dir.clone(), sys_dir.clone()], "Rabbiz");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].id, "agent");
        assert_eq!(out[0].exec, "/bin/true"); // $HOME shadows /etc/xdg
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn strips_field_codes_and_tryexec() {
        let tmp = std::env::temp_dir().join(format!("de-autostart2-{}", std::process::id()));
        write(
            &tmp,
            "tryexec.desktop",
            "[Desktop Entry]\nType=Application\nName=T\nExec=foo %U --bar\nTryExec=/nonexistent-bin-xyz\n",
        );
        assert!(entries_in(&[tmp.clone()], "Rabbiz").is_empty());
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
