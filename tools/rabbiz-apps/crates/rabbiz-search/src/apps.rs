//! Local installed desktop-app discovery and launching for `rabbiz-search`.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Debug, Clone)]
pub struct AppItem {
    pub id: String,
    pub name: String,
    pub exec: String,
}

pub fn discover_apps() -> Vec<AppItem> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(home) = std::env::var_os("HOME").filter(|v| !v.is_empty()) {
        dirs.push(PathBuf::from(home).join(".local/share/applications"));
    }
    dirs.push(PathBuf::from("/usr/local/share/applications"));
    dirs.push(PathBuf::from("/usr/share/applications"));

    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for dir in dirs {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in rd.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("desktop") {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            if stem == "rabbiz-search" || !seen.insert(stem.to_string()) {
                continue;
            }
            if let Some(item) = parse_desktop(stem, &path) {
                out.push(item);
            }
        }
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out
}

fn parse_desktop(id: &str, path: &Path) -> Option<AppItem> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut in_entry = false;
    let mut name: Option<String> = None;
    let mut exec: Option<String> = None;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_entry = trimmed == "[Desktop Entry]";
            continue;
        }
        if !in_entry || trimmed.starts_with('#') {
            continue;
        }
        if let Some(v) = trimmed.strip_prefix("NoDisplay=") {
            if v.eq_ignore_ascii_case("true") {
                return None;
            }
        } else if let Some(v) = trimmed.strip_prefix("Hidden=") {
            if v.eq_ignore_ascii_case("true") {
                return None;
            }
        } else if let Some(v) = trimmed.strip_prefix("Type=") {
            if v != "Application" {
                return None;
            }
        } else if let Some(v) = trimmed.strip_prefix("Name=") {
            if name.is_none() {
                name = Some(v.to_string());
            }
        } else if let Some(v) = trimmed.strip_prefix("Exec=") {
            if exec.is_none() {
                exec = Some(v.to_string());
            }
        }
    }
    Some(AppItem {
        id: id.to_string(),
        name: name?,
        exec: exec?,
    })
}

/// Filter installed apps: empty query returns nothing (clean initial state),
/// prefix matches rank ahead of substring matches.
pub fn filter_apps<'a>(apps: &'a [AppItem], query: &str) -> Vec<&'a AppItem> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return Vec::new();
    }
    let mut matched: Vec<(bool, &'a AppItem)> = apps
        .iter()
        .filter_map(|a| {
            let name_l = a.name.to_lowercase();
            let id_l = a.id.to_lowercase();
            if !name_l.contains(&q) && !id_l.contains(&q) {
                return None;
            }
            let prefix = name_l.starts_with(&q) || id_l.starts_with(&q);
            Some((prefix, a))
        })
        .collect();
    matched.sort_by(|a, b| b.0.cmp(&a.0));
    matched.into_iter().take(5).map(|(_, a)| a).collect()
}

pub fn launch_app(item: &AppItem) {
    let parts: Vec<String> = item
        .exec
        .split_whitespace()
        .filter(|tok| !tok.starts_with('%'))
        .map(|s| s.trim_matches('"').to_string())
        .collect();
    let Some((prog, args)) = parts.split_first() else {
        return;
    };
    let mut cmd = Command::new("setsid");
    cmd.arg(prog).args(args);
    let _ = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}

#[must_use]
pub fn is_allowed_http_url(url: &str) -> bool {
    (url.starts_with("https://") || url.starts_with("http://"))
        && !url.chars().any(|c| c.is_control() || c.is_whitespace())
}

pub fn open_url_in_browser(url: &str) {
    if !is_allowed_http_url(url) {
        return;
    }
    let chrome = Path::new("/opt/google/chrome/google-chrome");
    let mut cmd = if chrome.exists() {
        let mut c = Command::new("setsid");
        c.arg(chrome)
            .args([
                "--disable-gpu",
                "--ozone-platform=wayland",
                "--no-first-run",
                "--no-default-browser-check",
                "--touch-events=enabled",
                "--enable-pinch",
                "--force-device-scale-factor=1.25",
                "--disable-component-update",
                "--enable-wayland-ime",
                "--",
            ])
            .arg(url);
        c
    } else {
        let mut c = Command::new("setsid");
        c.arg("xdg-open").arg("--").arg(url);
        c
    };
    let _ = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_query_returns_no_apps_and_prefix_ranks_first() {
        let apps = vec![
            AppItem {
                id: "my-settings".into(),
                name: "My Settings".into(),
                exec: "x".into(),
            },
            AppItem {
                id: "rabbiz-settings".into(),
                name: "Settings".into(),
                exec: "rabbiz-settings".into(),
            },
        ];
        assert!(filter_apps(&apps, "").is_empty());
        assert!(filter_apps(&apps, "   ").is_empty());
        let res = filter_apps(&apps, "set");
        assert_eq!(res.len(), 2);
        assert_eq!(res[0].name, "Settings");
    }
}
