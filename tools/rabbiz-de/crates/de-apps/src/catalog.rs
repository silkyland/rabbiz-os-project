use std::collections::{HashMap, HashSet};

use crate::desktop::DesktopEntry;

/// One launcher entry per base-app role. Alternatives stay installed and remain
/// available as a fallback when a preferred desktop entry is absent or hidden.
///
/// Ported verbatim from `shell/A21s/Desktop/AppCatalog.js`.
pub const PREFERRED_GROUPS: &[&[&str]] = &[
    &[
        "google-chrome",
        "com.google.Chrome",
        "chromium",
        "org.kde.angelfish",
        "a21s-browser",
    ],
    &["a21s-dialer", "org.gnome.Calls"],
    &["a21s-sms", "sm.puri.Chatty"],
    &["a21s-settings", "org.kde.mobile.plasmasettings"],
    &["org.kde.index", "org.kde.dolphin", "a21s-files"],
    &[
        "org.kde.merkuro.calendar",
        "org.kde.merkuro",
        "org.kde.calindori",
        "a21s-calendar",
    ],
    &["a21s-calculator", "org.kde.kalk"],
    &["a21s-clock", "org.kde.kclock"],
    &["a21s-notes", "org.kde.marknote", "org.kde.buho"],
    &["a21s-photos", "a21s-gallery", "org.kde.pix", "org.kde.koko"],
    &["a21s-music", "org.kde.elisa", "org.kde.vvave"],
    &[
        "a21s-terminal",
        "org.kde.qmlkonsole",
        "org.kde.konsole",
        "org.kde.station",
        "xterm",
        "uxterm",
    ],
    &[
        "org.gnome.Contacts",
        "org.kde.merkuro.contact",
        "org.kde.merkuro.contacts",
        "a21s-contacts",
    ],
];

/// Filters hidden/NoDisplay entries, deduplicates identical IDs (first wins),
/// and suppresses lower-priority entries within each [`PREFERRED_GROUPS`] role.
pub fn visible_entries(entries: &[DesktopEntry]) -> Vec<DesktopEntry> {
    let mut available: HashMap<&str, usize> = HashMap::new();
    for (idx, entry) in entries.iter().enumerate() {
        if !entry.no_display && !entry.hidden && !entry.id.is_empty() {
            available.entry(entry.id.as_str()).or_insert(idx);
        }
    }

    let mut suppressed: HashSet<&str> = HashSet::new();
    for group in PREFERRED_GROUPS {
        let mut chosen: Option<&str> = None;
        for &id in *group {
            if !available.contains_key(id) {
                continue;
            }
            if chosen.is_none() {
                chosen = Some(id);
            } else {
                suppressed.insert(id);
            }
        }
    }

    let mut seen: HashSet<&str> = HashSet::new();
    let mut result = Vec::new();
    for (idx, candidate) in entries.iter().enumerate() {
        let id = candidate.id.as_str();
        if available.get(id).copied() != Some(idx)
            || suppressed.contains(id)
            || !seen.insert(id)
        {
            continue;
        }
        result.push(candidate.clone());
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str) -> DesktopEntry {
        DesktopEntry {
            id: id.to_string(),
            name: id.to_string(),
            exec: id.to_string(),
            icon: id.to_string(),
            categories: Vec::new(),
            no_display: false,
            hidden: false,
        }
    }

    fn no_display(id: &str) -> DesktopEntry {
        DesktopEntry {
            no_display: true,
            ..entry(id)
        }
    }

    fn hidden(id: &str) -> DesktopEntry {
        DesktopEntry {
            hidden: true,
            ..entry(id)
        }
    }

    fn ids(entries: &[DesktopEntry]) -> Vec<String> {
        visible_entries(entries)
            .into_iter()
            .map(|e| e.id)
            .collect()
    }

    #[test]
    fn third_party_identity_wins_and_fallbacks_work() {
        // Ported from shell/tests/appcatalog.test.mjs
        assert_eq!(
            ids(&[
                entry("google-chrome"),
                entry("a21s-browser"),
                entry("com.google.Chrome"),
            ]),
            vec!["google-chrome"]
        );
        assert_eq!(ids(&[entry("a21s-browser")]), vec!["a21s-browser"]);
        assert_eq!(
            ids(&[entry("org.kde.index"), entry("a21s-files")]),
            vec!["org.kde.index"]
        );
        assert_eq!(ids(&[entry("a21s-files")]), vec!["a21s-files"]);
        assert_eq!(
            ids(&[entry("org.kde.calindori"), entry("org.kde.merkuro.calendar")]),
            vec!["org.kde.merkuro.calendar"]
        );
    }

    #[test]
    fn hidden_and_nodisplay_entries_yield_to_fallbacks() {
        assert_eq!(
            ids(&[
                entry("a21s-photos"),
                no_display("a21s-gallery"),
                entry("org.kde.pix"),
            ]),
            vec!["a21s-photos"]
        );
        assert_eq!(
            ids(&[
                no_display("a21s-terminal"),
                entry("xterm"),
                entry("org.kde.station"),
            ]),
            vec!["org.kde.station"]
        );
        assert_eq!(
            ids(&[hidden("a21s-notes"), entry("org.kde.buho")]),
            vec!["org.kde.buho"]
        );
    }

    #[test]
    fn duplicate_and_empty_ids_are_filtered_preserving_order() {
        let mut other = entry("other");
        other.name = "ai.opencode".to_string();
        assert_eq!(
            ids(&[
                entry(""),
                entry("ai.opencode"),
                entry("ai.opencode"),
                other,
            ]),
            vec!["ai.opencode", "other"]
        );
        assert!(ids(&[]).is_empty());
    }

    #[test]
    fn native_apps_suppress_kde_alternatives() {
        let native = [
            "a21s-clock",
            "a21s-calculator",
            "a21s-notes",
            "a21s-music",
            "a21s-terminal",
            "a21s-photos",
        ];
        let kde = [
            "org.kde.kclock",
            "org.kde.kalk",
            "org.kde.marknote",
            "org.kde.elisa",
            "org.kde.qmlkonsole",
            "org.kde.koko",
        ];
        let all: Vec<DesktopEntry> = native.iter().chain(kde.iter()).map(|id| entry(id)).collect();
        assert_eq!(ids(&all), native);
    }
}
