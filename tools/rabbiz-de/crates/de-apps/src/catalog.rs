//! App catalog (M3-T3): scans XDG `applications/` dirs into `AppEntry`
//! values, honours `Hidden`/`NoDisplay`, dedupes by desktop-id (XDG dir
//! order wins), and offers ranked search for the drawer/spotlight UI.
//!
//! Hide/pin state is *not* part of the catalog — it lives in
//! [`de_config::Prefs`] (see [`crate::prefs`]); the UI filters with it.

use std::path::{Path, PathBuf};

use de_config::Prefs;
use freedesktop_desktop_entry::DesktopEntry;

/// A `[Desktop Action *]` shortcut (long-press menu, e.g. "New window").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppAction {
    pub id: String,
    pub name: String,
    pub exec: String,
}

/// One launchable desktop entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppEntry {
    /// Desktop-id (`<id>.desktop` stem) — the key used by prefs/launch.
    pub id: String,
    pub name: String,
    pub generic_name: String,
    /// `Icon=` value (name or absolute path).
    pub icon: String,
    pub exec: Option<String>,
    pub categories: Vec<String>,
    pub keywords: Vec<String>,
    /// `MimeType=` associations (for `mime.rs` default-app resolution).
    pub mime_types: Vec<String>,
    pub actions: Vec<AppAction>,
}

/// Scanned, sorted application set.
#[derive(Debug, Default)]
pub struct Catalog {
    entries: Vec<AppEntry>,
}

impl Catalog {
    /// Scan the real XDG application dirs for `locales` (e.g. `["th", "en"]`).
    #[must_use]
    pub fn scan(locales: &[String]) -> Self {
        Self::scan_dirs(&applications_dirs(), locales)
    }

    /// Scan specific dirs (testable). First dir wins on id collisions
    /// (XDG precedence: `$XDG_DATA_HOME` before `$XDG_DATA_DIRS`).
    #[must_use]
    pub fn scan_dirs(dirs: &[PathBuf], locales: &[String]) -> Self {
        let mut seen = std::collections::HashSet::new();
        let mut entries = Vec::new();
        for dir in dirs {
            let Ok(read) = std::fs::read_dir(dir) else {
                continue;
            };
            for file in read.flatten() {
                let path = file.path();
                if path.extension().and_then(|e| e.to_str()) != Some("desktop") {
                    continue;
                }
                let Ok(de) = DesktopEntry::from_path(path.clone(), Some(locales)) else {
                    continue;
                };
                if de.hidden() || de.no_display() {
                    continue;
                }
                let id = de.appid.clone();
                if !seen.insert(id.clone()) {
                    continue;
                }
                entries.push(entry_from(&de, id, locales));
            }
        }
        entries.sort_by_key(|a| a.name.to_lowercase());
        Self { entries }
    }

    /// All entries (including hidden) — for the "show hidden" affordance.
    #[must_use]
    pub fn all(&self) -> &[AppEntry] {
        &self.entries
    }

    /// Entries visible under `prefs` (hidden apps filtered out). A hidden
    /// flag stored under the pre-rename `a21s-*` id still hides the app.
    pub fn visible<'a>(&'a self, prefs: &'a Prefs) -> impl Iterator<Item = &'a AppEntry> {
        self.entries.iter().filter(|e| !hidden(prefs, &e.id))
    }

    /// Exact desktop-id lookup; falls back to the pre-rename `a21s-*` /
    /// `rabbiz-*` spelling so persisted prefs keep resolving.
    #[must_use]
    pub fn find(&self, id: &str) -> Option<&AppEntry> {
        self.entries
            .iter()
            .find(|e| e.id == id)
            .or_else(|| alt_id(id).and_then(|a| self.entries.iter().find(|e| e.id == a)))
    }

    /// Ranked search (drawer + P14 spotlight): name prefix > name word >
    /// substring in name/generic/keywords/id. Thai-safe — matching is on
    /// lowercased codepoints, which works for Thai (no case folding needed,
    /// but normalization of the query is the caller's job).
    #[must_use]
    pub fn search<'a>(&'a self, prefs: &'a Prefs, query: &str) -> Vec<SearchHit<'a>> {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return Vec::new();
        }
        let mut hits: Vec<SearchHit<'a>> = self
            .visible(prefs)
            .filter_map(|e| score(e, &q).map(|s| SearchHit { entry: e, score: s }))
            .collect();
        hits.sort_by(|a, b| b.score.cmp(&a.score).then(a.entry.name.cmp(&b.entry.name)));
        hits
    }

    /// Home-grid order: `prefs.home_order` first (visible only), then the
    /// remaining visible entries in catalog order. Apps pinned to the dock
    /// are not repeated on the grid (membership is exclusive).
    #[must_use]
    pub fn home_order<'a>(&'a self, prefs: &'a Prefs) -> Vec<&'a AppEntry> {
        let mut out: Vec<&AppEntry> = Vec::with_capacity(self.entries.len());
        let docked: Vec<&str> = self.dock(prefs).iter().map(|e| e.id.as_str()).collect();
        for id in &prefs.home_order {
            if let Some(e) = self.find(id) {
                if !hidden(prefs, id) && !out.contains(&e) && !docked.contains(&e.id.as_str()) {
                    out.push(e);
                }
            }
        }
        let tail: Vec<&AppEntry> = self
            .visible(prefs)
            .filter(|e| !out.iter().any(|o| o.id == e.id) && !docked.contains(&e.id.as_str()))
            .collect();
        out.extend(tail);
        out
    }

    /// Dock entries: pinned ids that resolve and are not hidden.
    #[must_use]
    pub fn dock<'a>(&'a self, prefs: &'a Prefs) -> Vec<&'a AppEntry> {
        prefs
            .dock
            .iter()
            .filter_map(|id| self.find(id))
            .filter(|e| !hidden(prefs, &e.id))
            .collect()
    }
}

/// A search result with its rank (higher = better match).
#[derive(Debug)]
pub struct SearchHit<'a> {
    pub entry: &'a AppEntry,
    pub score: u32,
}

/// The pre-rename spelling of a desktop-id (`a21s-x` ↔ `rabbiz-x`, 2026-11
/// cut-over). Only first-party prefixes swap — third-party ids are untouched.
fn alt_id(id: &str) -> Option<String> {
    if let Some(rest) = id.strip_prefix("a21s-") {
        Some(format!("rabbiz-{rest}"))
    } else {
        id.strip_prefix("rabbiz-")
            .map(|rest| format!("a21s-{rest}"))
    }
}

/// Hidden check across both spellings of a first-party id.
fn hidden(prefs: &Prefs, id: &str) -> bool {
    prefs.is_hidden(id) || alt_id(id).is_some_and(|a| prefs.is_hidden(&a))
}

fn score(e: &AppEntry, q: &str) -> Option<u32> {
    let name = e.name.to_lowercase();
    if name == q {
        return Some(1000);
    }
    if name.starts_with(q) {
        return Some(800);
    }
    // Word-boundary match ("files" hits "A21s Files" on the second word).
    if name.split_whitespace().any(|w| w.starts_with(q)) {
        return Some(600);
    }
    if name.contains(q) {
        return Some(400);
    }
    if e.generic_name.to_lowercase().contains(q)
        || e.keywords.iter().any(|k| k.to_lowercase().contains(q))
    {
        return Some(200);
    }
    if e.id.to_lowercase().contains(q) {
        return Some(100);
    }
    None
}

fn entry_from(de: &DesktopEntry, id: String, locales: &[String]) -> AppEntry {
    let actions = de
        .actions()
        .unwrap_or_default()
        .into_iter()
        .filter(|a| !a.is_empty())
        .map(|a| AppAction {
            id: a.to_string(),
            name: de
                .action_entry_localized(a, "Name", locales)
                .map_or_else(|| a.to_string(), std::borrow::Cow::into_owned),
            exec: de.action_entry(a, "Exec").unwrap_or_default().to_string(),
        })
        .collect();
    AppEntry {
        id,
        name: de
            .name(locales)
            .map(std::borrow::Cow::into_owned)
            .unwrap_or_default(),
        generic_name: de
            .generic_name(locales)
            .map(std::borrow::Cow::into_owned)
            .unwrap_or_default(),
        icon: de.icon().unwrap_or_default().to_string(),
        exec: de.exec().map(str::to_string),
        categories: de
            .categories()
            .unwrap_or_default()
            .iter()
            .map(|s| (*s).to_string())
            .collect(),
        mime_types: de
            .mime_type()
            .unwrap_or_default()
            .iter()
            .map(|s| (*s).to_string())
            .collect(),
        keywords: de
            .keywords(locales)
            .unwrap_or_default()
            .iter()
            .map(std::string::ToString::to_string)
            .collect(),
        actions,
    }
}

/// XDG `applications/` dirs in precedence order (including Flatpak user & system exports).
#[must_use]
pub fn applications_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(h) = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from) {
        dirs.push(h.join("applications"));
    } else if let Some(h) = std::env::var_os("HOME").map(PathBuf::from) {
        dirs.push(h.join(".local/share/applications"));
    }
    if let Some(h) = std::env::var_os("HOME").map(PathBuf::from) {
        dirs.push(h.join(".local/share/flatpak/exports/share/applications"));
    }
    let data_dirs =
        std::env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".into());
    for d in data_dirs.split(':').filter(|d| !d.is_empty()) {
        dirs.push(Path::new(d).join("applications"));
    }
    let sys_flatpak = PathBuf::from("/var/lib/flatpak/exports/share/applications");
    if !dirs.contains(&sys_flatpak) {
        dirs.push(sys_flatpak);
    }
    dirs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_desktop(dir: &Path, id: &str, body: &str) {
        std::fs::write(dir.join(format!("{id}.desktop")), body).unwrap();
    }

    fn fixture() -> (tempfile::TempDir, [PathBuf; 2]) {
        let base = tempfile::tempdir().unwrap();
        let (d1, d2) = (base.path().join("a"), base.path().join("b"));
        std::fs::create_dir_all(&d1).unwrap();
        std::fs::create_dir_all(&d2).unwrap();
        write_desktop(
            &d1,
            "org.kde.dolphin",
            "[Desktop Entry]\nName=Files\nGenericName=File Manager\nExec=dolphin\nIcon=system-file-manager\nCategories=Utility;\nActions=new-window;\n\n[Desktop Action new-window]\nName=Open a New Window\nExec=dolphin --new-window\n",
        );
        write_desktop(
            &d1,
            "hidden-one",
            "[Desktop Entry]\nName=Hidden App\nExec=secret\nNoDisplay=true\n",
        );
        write_desktop(
            &d2,
            "rabbiz-calculator",
            "[Desktop Entry]\nName=Calculator\nName[th]=เครื่องคิดเลข\nExec=rabbiz-calculator\nKeywords=math;คำนวณ;\n",
        );
        // Same id in the lower-precedence dir — d1 must win.
        write_desktop(&d1, "dupe", "[Desktop Entry]\nName=First\nExec=first\n");
        write_desktop(&d2, "dupe", "[Desktop Entry]\nName=Second\nExec=second\n");
        (base, [d1, d2])
    }

    #[test]
    fn scans_dedupes_and_filters() {
        let (_fixture, dirs) = fixture();
        let c = Catalog::scan_dirs(&dirs, &["en".to_string()]);
        assert!(c.find("org.kde.dolphin").is_some());
        assert!(c.find("hidden-one").is_none());
        assert_eq!(c.find("dupe").unwrap().name, "First");
        // Sorted by name.
        let names: Vec<_> = c.all().iter().map(|e| e.name.as_str()).collect();
        let mut sorted = names.clone();
        sorted.sort_by_key(|a| a.to_lowercase());
        assert_eq!(names, sorted);
        // Actions parsed.
        let d = c.find("org.kde.dolphin").unwrap();
        assert_eq!(d.actions.len(), 1);
        assert_eq!(d.actions[0].id, "new-window");
    }

    #[test]
    fn localized_name_and_keyword_search() {
        let (_fixture, dirs) = fixture();
        let c = Catalog::scan_dirs(&dirs, &["th".to_string(), "en".to_string()]);
        let p = Prefs::default();
        // Thai name picked up when th locale leads.
        assert_eq!(c.find("rabbiz-calculator").unwrap().name, "เครื่องคิดเลข");
        // Thai keyword search hits.
        let hits = c.search(&p, "คำนวณ");
        assert!(hits.iter().any(|h| h.entry.id == "rabbiz-calculator"));
        // Prefix outranks substring/keyword.
        let hits = c.search(&p, "fi");
        assert_eq!(hits[0].entry.id, "org.kde.dolphin");
    }

    #[test]
    fn docked_apps_are_not_repeated_on_the_grid() {
        let (_fixture, dirs) = fixture();
        let c = Catalog::scan_dirs(&dirs, &["en".to_string()]);
        let mut p = Prefs::default();
        p.pin_dock("rabbiz-calculator");
        let grid: Vec<_> = c.home_order(&p).iter().map(|e| e.id.as_str()).collect();
        assert!(!grid.contains(&"rabbiz-calculator"));
        assert!(grid.contains(&"org.kde.dolphin"));
        p.unpin_dock("rabbiz-calculator");
        assert!(c.home_order(&p).iter().any(|e| e.id == "rabbiz-calculator"));
    }

    #[test]
    fn hidden_and_dock_respect_prefs() {
        let (_fixture, dirs) = fixture();
        let c = Catalog::scan_dirs(&dirs, &["en".to_string()]);
        let mut p = Prefs::default();
        p.set_hidden("dupe", true);
        p.pin_dock("rabbiz-calculator");
        p.pin_dock("dupe"); // hidden pin is skipped by dock()
        p.pin_dock("nonexistent");
        let visible: Vec<_> = c.visible(&p).map(|e| e.id.as_str()).collect();
        assert!(!visible.contains(&"dupe"));
        let dock: Vec<_> = c.dock(&p).iter().map(|e| e.id.as_str()).collect();
        assert_eq!(dock, vec!["rabbiz-calculator"]);
        // Search skips hidden too.
        assert!(c.search(&p, "first").is_empty());
    }

    #[test]
    fn legacy_a21s_ids_resolve() {
        let (_fixture, dirs) = fixture();
        let c = Catalog::scan_dirs(&dirs, &["en".to_string()]);
        let mut p = Prefs::default();
        // Prefs persisted under the pre-rename id still apply.
        p.set_hidden("a21s-calculator", true);
        p.pin_dock("a21s-calculator");
        assert!(c.find("a21s-calculator").is_some());
        assert!(!c.visible(&p).any(|e| e.id == "rabbiz-calculator"));
        // Dock lookup resolves the legacy id, but is_hidden drops it again.
        assert_eq!(c.dock(&p), Vec::<&AppEntry>::new());
    }
}
