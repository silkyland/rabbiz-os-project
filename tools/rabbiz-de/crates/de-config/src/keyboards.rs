//! Keyboard catalog & 3rd-party keyboard discovery (`rabbiz-osk` + market OSKs).

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::prefs::Prefs;

pub const DEFAULT_KEYBOARD_ID: &str = "rabbiz-osk";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyboardInfo {
    pub id: String,
    pub name: String,
    pub exec: String,
    pub third_party: bool,
    pub installed: bool,
}

#[derive(Debug, Deserialize)]
struct CustomKeyboardFile {
    id: String,
    name: String,
    exec: String,
}

const KNOWN_THIRD_PARTY: &[(&str, &str, &str)] = &[
    ("squeekboard", "Squeekboard", "squeekboard"),
    ("stevia", "Stevia (Phosh OSK)", "stevia"),
    ("maliit-keyboard", "Maliit Keyboard", "maliit-keyboard"),
    ("fcitx5", "Fcitx5", "fcitx5"),
];

/// Discover built-in and (when enabled) 3rd-party keyboards available on the system.
#[must_use]
pub fn discover_keyboards(allow_third_party: bool) -> Vec<KeyboardInfo> {
    let mut out = vec![KeyboardInfo {
        id: DEFAULT_KEYBOARD_ID.into(),
        name: "Rabbiz Keyboard".into(),
        exec: "rabbiz-osk".into(),
        third_party: false,
        installed: is_executable_available("rabbiz-osk") || is_executable_available("a21s-osk"),
    }];
    if !allow_third_party {
        return out;
    }
    for (id, name, exec) in KNOWN_THIRD_PARTY {
        out.push(KeyboardInfo {
            id: (*id).into(),
            name: (*name).into(),
            exec: (*exec).into(),
            third_party: true,
            installed: is_executable_available(exec),
        });
    }
    for dir in custom_keyboard_dirs() {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("toml") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let Ok(parsed) = toml::from_str::<CustomKeyboardFile>(&text) else {
                continue;
            };
            if parsed.id.is_empty() || parsed.exec.is_empty() {
                continue;
            }
            if out.iter().any(|k| k.id == parsed.id) {
                continue;
            }
            let installed = is_executable_available(&parsed.exec);
            out.push(KeyboardInfo {
                id: parsed.id,
                name: parsed.name,
                exec: parsed.exec,
                third_party: true,
                installed,
            });
        }
    }
    out
}

/// Resolve which keyboard binary `de-core` should launch for `prefs`.
#[must_use]
pub fn resolve_active_keyboard(prefs: &Prefs) -> KeyboardInfo {
    let list = discover_keyboards(prefs.allow_third_party_keyboards);
    if prefs.allow_third_party_keyboards {
        if let Some(found) = list
            .iter()
            .find(|k| k.id == prefs.keyboard_id && k.installed)
        {
            return found.clone();
        }
    }
    list.into_iter()
        .next()
        .expect("rabbiz-osk is always first in discover_keyboards")
}

fn custom_keyboard_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![PathBuf::from("/usr/share/rabbiz-de/keyboards")];
    if let Some(cfg) = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
    {
        dirs.push(cfg.join("rabbiz-de").join("keyboards"));
    }
    dirs
}

/// Check whether `exec` exists on disk (absolute path) or in `$PATH` / standard bin dirs.
#[must_use]
pub fn is_executable_available(exec: &str) -> bool {
    let p = Path::new(exec);
    if p.is_absolute() {
        return p.is_file();
    }
    if let Some(path_env) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path_env) {
            if dir.join(exec).is_file() {
                return true;
            }
        }
    }
    ["/usr/local/bin", "/usr/bin"]
        .iter()
        .any(|d| Path::new(d).join(exec).is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabling_third_party_forces_builtin_keyboard() {
        let mut prefs = Prefs::default();
        prefs.keyboard_id = "squeekboard".into();
        prefs.allow_third_party_keyboards = false;
        let active = resolve_active_keyboard(&prefs);
        assert_eq!(active.id, DEFAULT_KEYBOARD_ID);
        assert!(!active.third_party);

        let list = discover_keyboards(false);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, DEFAULT_KEYBOARD_ID);
    }

    #[test]
    fn enabling_third_party_lists_market_keyboards() {
        let list = discover_keyboards(true);
        assert!(list.iter().any(|k| k.id == "rabbiz-osk" && !k.third_party));
        assert!(list.iter().any(|k| k.id == "squeekboard" && k.third_party));
        assert!(list.iter().any(|k| k.id == "stevia" && k.third_party));
        assert!(list
            .iter()
            .any(|k| k.id == "maliit-keyboard" && k.third_party));
        assert!(list.iter().any(|k| k.id == "fcitx5" && k.third_party));
    }
}
