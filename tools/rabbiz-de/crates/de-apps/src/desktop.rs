use std::fs;
use std::io;
use std::path::Path;

/// Parsed XDG `.desktop` application entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopEntry {
    pub id: String,
    pub name: String,
    pub exec: String,
    pub icon: String,
    pub categories: Vec<String>,
    pub no_display: bool,
    pub hidden: bool,
}

/// Parses the `[Desktop Entry]` section of a `.desktop` file.
///
/// Returns `None` if `[Desktop Entry]` is missing, `Type` is present and not
/// `Application`, or `Name` is empty.
pub fn parse_desktop_entry(id: &str, content: &str) -> Option<DesktopEntry> {
    let mut in_desktop_entry = false;
    let mut entry_type: Option<String> = None;
    let mut name = String::new();
    let mut exec = String::new();
    let mut icon = String::new();
    let mut categories = Vec::new();
    let mut no_display = false;
    let mut hidden = false;

    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            let section = &line[1..line.len() - 1];
            if section == "Desktop Entry" {
                in_desktop_entry = true;
                continue;
            } else if in_desktop_entry {
                // Stop once we leave [Desktop Entry] (e.g. [Desktop Action ...]).
                break;
            } else {
                continue;
            }
        }
        if !in_desktop_entry {
            continue;
        }

        let Some((raw_key, raw_val)) = line.split_once('=') else {
            continue;
        };
        let key = raw_key.trim();
        let val = raw_val.trim();

        match key {
            "Type" => entry_type = Some(val.to_string()),
            "Name" => name = val.to_string(),
            "Exec" => exec = val.to_string(),
            "Icon" => icon = val.to_string(),
            "Categories" => {
                categories = val
                    .split(';')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect();
            }
            "NoDisplay" => no_display = parse_bool(val),
            "Hidden" => hidden = parse_bool(val),
            _ => {}
        }
    }

    if !in_desktop_entry || name.is_empty() {
        return None;
    }
    if let Some(t) = entry_type {
        if t != "Application" {
            return None;
        }
    }

    Some(DesktopEntry {
        id: id.to_string(),
        name,
        exec,
        icon,
        categories,
        no_display,
        hidden,
    })
}

/// Reads and parses a `.desktop` file, deriving `id` from the filename stem.
pub fn parse_desktop_file(path: &Path) -> io::Result<Option<DesktopEntry>> {
    let id = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    let content = fs::read_to_string(path)?;
    Ok(parse_desktop_entry(id, &content))
}

/// Loads all valid `.desktop` entries from `dir` (non-recursive), sorted by `id`.
pub fn load_desktop_dir(dir: &Path) -> Vec<DesktopEntry> {
    let Ok(read_dir) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut entries = Vec::new();
    for item in read_dir.flatten() {
        let path = item.path();
        if path.extension().and_then(|e| e.to_str()) != Some("desktop") {
            continue;
        }
        if let Ok(Some(entry)) = parse_desktop_file(&path) {
            entries.push(entry);
        }
    }
    entries.sort_by(|a, b| a.id.cmp(&b.id));
    entries
}

fn parse_bool(val: &str) -> bool {
    matches!(val.trim().to_ascii_lowercase().as_str(), "true" | "1" | "yes")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_standard_desktop_entry_and_ignores_actions() {
        let raw = r#"# Comment
[Desktop Entry]
Type=Application
Name=Google Chrome
Exec=/usr/bin/google-chrome-stable %U
Icon=google-chrome
Categories=Network;WebBrowser;
NoDisplay=false
Hidden=false

[Desktop Action new-window]
Name=New Window
Exec=/usr/bin/google-chrome-stable --new-window
"#;
        let entry = parse_desktop_entry("google-chrome", raw).expect("parsed");
        assert_eq!(entry.id, "google-chrome");
        assert_eq!(entry.name, "Google Chrome");
        assert_eq!(entry.exec, "/usr/bin/google-chrome-stable %U");
        assert_eq!(entry.icon, "google-chrome");
        assert_eq!(entry.categories, vec!["Network", "WebBrowser"]);
        assert!(!entry.no_display);
        assert!(!entry.hidden);
    }

    #[test]
    fn rejects_non_application_or_missing_name() {
        let link = "[Desktop Entry]\nType=Link\nName=Docs\nURL=https://example.com\n";
        assert!(parse_desktop_entry("docs", link).is_none());

        let nameless = "[Desktop Entry]\nType=Application\nExec=foo\n";
        assert!(parse_desktop_entry("foo", nameless).is_none());
    }

    #[test]
    fn parses_nodisplay_and_hidden_flags() {
        let raw = "[Desktop Entry]\nType=Application\nName=Helper\nNoDisplay=true\nHidden=true\n";
        let entry = parse_desktop_entry("helper", raw).expect("parsed");
        assert!(entry.no_display);
        assert!(entry.hidden);
    }
}
