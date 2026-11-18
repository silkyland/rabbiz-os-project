//! Shell surface raster (M3): paints `nav.current` from live compositor
//! state — catalog order, prefs, wallpaper, polled battery/privacy, recents
//! snapshots — into the `Pixels` frame the texture path uploads.
//!
//! Re-raster happens only when `shell_dirty` (nav/status/page/prefs change)
//! or the wall-clock minute rolls over; between rasters the cached texture
//! presents with zero CPU.

use de_api::{Pixels, Size};
use de_apps::catalog::AppEntry;
use de_design::tokens;
use de_ui::chrome::{IconEntry, StatusInfo};
use de_ui::nav::Screen;
use de_ui::recents::{RecentEntry, RecentsModel};
use de_ui::search::{SearchKind, SearchRow};

use crate::state::De;

/// Recents card thumbnail target (`recents.rs` cover-scales into the card).
pub const RECENT_THUMB: Size = Size { w: 480, h: 1040 };

impl De {
    /// Rasterize the current shell screen into a fresh `Pixels` frame.
    pub fn shell_pixels(&mut self) -> Pixels {
        let size = Size::new(self.output.width as i32, self.output.height as i32);
        let s = self.strings.clone();
        let t = tokens::DEFAULT;
        let status = self.status_info();
        match self.nav.current {
            Screen::Recents => {
                let m = self.recents_model();
                de_ui::recents::render(size, &s, &t, &status, &m)
            }
            Screen::Drawer => {
                let ids = self.visible_ids();
                let apps = self.app_icons(&ids);
                de_ui::drawer::render(size, &s, &t, &status, &apps, 0)
            }
            Screen::Search => {
                let rows = self.search_rows();
                de_ui::search::render(size, &s, &t, "", &rows)
            }
            Screen::Shade => de_ui::shade::render(size, &s, &t, &status.clock),
            Screen::Lock => de_ui::lock::render(size, &s, &t, &status.clock, &clock_date()),
            Screen::Power => de_ui::power::render(size, &s, &t),
            Screen::Error => {
                let app = self.launch_error.clone().unwrap_or_else(|| "app".into());
                de_ui::error::render(size, &s, &t, &app)
            }
            _ => self.home_pixels(size, &s, &t, status),
        }
    }

    /// Home screen frame: wallpaper (asset or procedural), catalog icons in
    /// `home_order`, pinned dock, current page.
    fn home_pixels(
        &mut self,
        size: Size,
        s: &de_ui::i18n::Strings,
        t: &tokens::Theme,
        status: StatusInfo,
    ) -> Pixels {
        let prefs = self.app_prefs.prefs().clone();
        let wp_id = prefs.wallpaper_id.clone();
        let wallpaper = match &self.wallpaper {
            Some((id, px)) if *id == wp_id => px.clone(),
            _ => {
                let px = de_paint::wallpaper::bundled(&wp_id, size);
                self.wallpaper = Some((wp_id, px.clone()));
                px
            }
        };
        let ordered: Vec<AppEntry> = self
            .catalog
            .home_order(&prefs)
            .into_iter()
            .cloned()
            .collect();
        let page = self
            .home_page
            .min(de_ui::home::pages_for(ordered.len()) - 1);
        let page_ids: Vec<String> = ordered
            .iter()
            .skip(page * de_ui::home::PAGE_SIZE)
            .take(de_ui::home::PAGE_SIZE)
            .map(|e| e.id.clone())
            .collect();
        let dock_ids: Vec<String> = self
            .catalog
            .dock(&prefs)
            .into_iter()
            .map(|e| e.id.clone())
            .collect();
        let apps = self.app_icons(&page_ids);
        let dock = self.app_icons(&dock_ids);
        let clock = status.clock.clone();
        let date = clock_date();
        let m = de_ui::home::HomeModel {
            status,
            clock: &clock,
            date: &date,
            charging: "",
            wallpaper: Some(&wallpaper),
            apps: &apps,
            dock: &dock,
            page,
            page_count: de_ui::home::pages_for(ordered.len()),
        };
        de_ui::home::render(size, s, t, &m)
    }

    /// Visible app ids under current prefs.
    fn visible_ids(&self) -> Vec<String> {
        let prefs = self.app_prefs.prefs();
        self.catalog.visible(prefs).map(|e| e.id.clone()).collect()
    }

    /// `IconEntry`s for `desktop_ids`, resolving real icons through the
    /// cache (first-party `rabbiz-*`/`a21s-*` get the sand-tile glyph).
    fn app_icons(&mut self, desktop_ids: &[String]) -> Vec<IconEntry> {
        desktop_ids.iter().map(|id| self.app_icon(id)).collect()
    }

    pub(crate) fn app_icon(&mut self, id: &str) -> IconEntry {
        let (label, icon_name) = self
            .catalog
            .find(id)
            .map(|entry| (entry.name.clone(), entry.icon.clone()))
            .unwrap_or_else(|| (id.to_owned(), String::new()));
        let icon = self
            .icon_cache
            .entry(id.to_owned())
            .or_insert_with(|| de_apps::icons::rasterize_icon(id, &icon_name, &label, 96));
        IconEntry {
            id: id.to_owned(),
            label,
            icon: Some(icon.clone()),
        }
    }

    /// Recents rail: snapshot-cache entries (recency order) plus running
    /// windows that have no captured frame yet.
    fn recents_model(&mut self) -> RecentsModel {
        let mut entries = Vec::new();
        let mut seen: Vec<String> = self.snapshots.recent_ids().to_vec();
        for id in seen.clone() {
            entries.push(self.recent_entry(&id));
        }
        for id in self.window_app_ids() {
            if seen.contains(&id) {
                continue;
            }
            seen.push(id.clone());
            entries.push(self.recent_entry(&id));
        }
        RecentsModel::new(entries)
    }

    fn recent_entry(&mut self, id: &str) -> RecentEntry {
        let title = self
            .catalog
            .find(id)
            .map(|e| e.name.clone())
            .unwrap_or_else(|| id.to_string());
        let icon = self
            .app_icons(std::slice::from_ref(&id.to_string()))
            .pop()
            .and_then(|i| i.icon);
        let mut r = RecentEntry::new(id, &title);
        r.icon = icon;
        r.snapshot = self.snapshots.get(id).cloned();
        r
    }

    /// Search rows: every visible app (the query field is static until the
    /// on-screen keyboard/search input lands in M4 — tapping a row launches).
    fn search_rows(&mut self) -> Vec<SearchRow> {
        let ids = self.visible_ids();
        self.app_icons(&ids)
            .into_iter()
            .map(|i| SearchRow {
                kind: SearchKind::App,
                id: i.id,
                title: i.label,
                subtitle: String::new(),
                icon: i.icon,
            })
            .collect()
    }

    /// Status bar data from the live sysfs/procfs pollers (Wi-Fi/BT arrive
    /// with the M4 D-Bus services — `Unknown`/`false` paints no icon).
    fn status_info(&self) -> StatusInfo {
        let b = de_system::upower::poll_battery();
        let p = de_system::privacy::poll_privacy();
        StatusInfo {
            clock: clock_hhmm(),
            battery_pct: b.percent,
            charging: b.charging,
            battery_full: b.full,
            privacy_cam: p.camera,
            privacy_mic: p.mic,
            ..StatusInfo::default()
        }
    }
}

/// "HH:MM" local time (same `date` contract as the M1 stub).
pub fn clock_hhmm() -> String {
    std::process::Command::new("date")
        .arg("+%H:%M")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "--:--".to_string())
}

/// "Mon, 9 Jun" — locale comes from the session environment.
pub fn clock_date() -> String {
    std::process::Command::new("date")
        .arg("+%a, %-d %b")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}
