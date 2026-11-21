//! XDG MIME association resolution (Associations spec / `mimeapps.list`,
//! AUDIT-DE-STANDARDS-2026-11-21 §2): which desktop entry opens a MIME type —
//! defaults from `mimeapps.list`, associations merged with every entry's
//! `MimeType=`, removals subtracted. Powers "open with" and the default-app
//! checks standard apps (Chromium, Files) make.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

/// Parsed `mimeapps.list` files, spec precedence order.
#[derive(Debug, Default)]
pub struct MimeApps {
    /// mime → desktop ids, in the order spec precedence produced them.
    defaults: HashMap<String, Vec<String>>,
    added: HashMap<String, Vec<String>>,
    removed: HashMap<String, Vec<String>>,
}

/// `mimeapps.list` locations in spec lookup order (first hit wins):
/// `$XDG_CONFIG_HOME`, each `$XDG_CONFIG_DIRS`, `$XDG_DATA_HOME/applications`,
/// each `$XDG_DATA_DIRS/applications`.
fn mimeapps_paths() -> Vec<PathBuf> {
    let mut out = Vec::new();
    let home = std::env::var_os("HOME").map(PathBuf::from);
    if let Some(dir) = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from) {
        out.push(dir.join("mimeapps.list"));
    } else if let Some(h) = &home {
        out.push(h.join(".config/mimeapps.list"));
    }
    for dir in std::env::var("XDG_CONFIG_DIRS")
        .unwrap_or_else(|_| "/etc/xdg".into())
        .split(':')
        .filter(|s| !s.is_empty())
    {
        out.push(PathBuf::from(dir).join("mimeapps.list"));
    }
    if let Some(dir) = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from) {
        out.push(dir.join("applications/mimeapps.list"));
    } else if let Some(h) = &home {
        out.push(h.join(".local/share/applications/mimeapps.list"));
    }
    for dir in std::env::var("XDG_DATA_DIRS")
        .unwrap_or_else(|_| "/usr/local/share:/usr/share".into())
        .split(':')
        .filter(|s| !s.is_empty())
    {
        out.push(PathBuf::from(dir).join("applications/mimeapps.list"));
    }
    out
}

fn group(name: &str) -> &'static str {
    match name {
        "Default Applications" => "default",
        "Added Associations" => "added",
        "Removed Associations" => "removed",
        _ => "",
    }
}

impl MimeApps {
    /// Load the real XDG locations.
    #[must_use]
    pub fn load() -> Self {
        let mut out = Self::default();
        for path in mimeapps_paths() {
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            out.parse(&text);
        }
        out
    }

    /// Merge one file's contents; earlier files win per mime (spec precedence).
    pub fn parse(&mut self, text: &str) {
        let mut kind = "";
        for line in text.lines() {
            let line = line.trim();
            if line.starts_with('[') {
                kind = group(line.trim_matches(['[', ']']));
                continue;
            }
            let Some((mime, ids)) = line.split_once('=') else {
                continue;
            };
            let ids: Vec<String> = ids
                .split(';')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect();
            if ids.is_empty() {
                continue;
            }
            let target = match kind {
                "default" => &mut self.defaults,
                "added" => &mut self.added,
                "removed" => &mut self.removed,
                _ => continue,
            };
            // First listing wins per spec; later files may only *add* ids.
            target
                .entry(mime.trim().to_string())
                .and_modify(|v| {
                    for id in &ids {
                        if !v.contains(id) {
                            v.push(id.clone());
                        }
                    }
                })
                .or_insert(ids);
        }
    }

    /// The desktop-id chosen to open `mime` — first default that still
    /// exists in `valid` (pass `None` to skip the existence check).
    #[must_use]
    pub fn default_app(&self, mime: &str, valid: Option<&dyn Fn(&str) -> bool>) -> Option<String> {
        self.defaults
            .get(mime)?
            .iter()
            .find(|id| {
                let not_removed = !self.removed.get(mime).is_some_and(|r| r.contains(*id));
                not_removed && valid.is_none_or(|ok| ok(id))
            })
            .cloned()
    }

    /// All desktop-ids associated with `mime`: entries' `MimeType=` plus
    /// `Added Associations`, minus `Removed Associations`, spec order.
    #[must_use]
    pub fn associated_apps<'a>(
        &self,
        mime: &str,
        catalog: impl Iterator<Item = (&'a str, &'a str)>,
    ) -> Vec<String> {
        let mut out: Vec<String> = self.added.get(mime).cloned().unwrap_or_default();
        for (id, m) in catalog {
            if m == mime && !out.iter().any(|s| s == id) {
                out.push(id.to_string());
            }
        }
        if let Some(removed) = self.removed.get(mime) {
            let rm: HashSet<&String> = removed.iter().collect();
            out.retain(|id| !rm.contains(id));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_file_default_wins_and_removed_drops_candidates() {
        let mut m = MimeApps::default();
        m.parse("[Default Applications]\ntext/html=chrome.desktop\n");
        m.parse("[Default Applications]\ntext/html=firefox.desktop\n[Removed Associations]\ntext/html=chrome.desktop\n");
        // First file's default wins — but it's also removed → next in ITS list,
        // then falls through to later files' candidates.
        assert_eq!(
            m.default_app("text/html", None).as_deref(),
            Some("firefox.desktop")
        );
        m.parse("[Default Applications]\ntext/plain=a.desktop;b.desktop\n");
        assert_eq!(
            m.default_app("text/plain", None).as_deref(),
            Some("a.desktop")
        );
    }

    #[test]
    fn default_skips_ids_not_in_catalog() {
        let mut m = MimeApps::default();
        m.parse("[Default Applications]\ntext/html=gone.desktop;real.desktop\n");
        let ok = |id: &str| id == "real.desktop";
        assert_eq!(
            m.default_app("text/html", Some(&ok)).as_deref(),
            Some("real.desktop")
        );
    }

    #[test]
    fn associated_merges_added_catalog_and_subtracts_removed() {
        let mut m = MimeApps::default();
        m.parse("[Added Associations]\nimage/png=pinsel.desktop\n[Removed Associations]\nimage/png=gimp.desktop\n");
        let catalog = [
            ("gimp.desktop", "image/png"),
            ("gallery.desktop", "image/png"),
        ];
        let out = m.associated_apps("image/png", catalog.into_iter());
        assert_eq!(
            out,
            vec!["pinsel.desktop".to_string(), "gallery.desktop".to_string()]
        );
    }
}
