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

impl De {
    /// Rasterize the current shell screen into a fresh `Pixels` frame.
    pub fn shell_pixels(&mut self) -> Pixels {
        let size = Size::new(self.output.width as i32, self.output.height as i32);
        let s = self.strings.clone();
        let t = tokens::DEFAULT;
        let (status, date) = self.shell_meta();
        let base = match self.nav.current {
            Screen::Recents => {
                let windows = self.recent_windows();
                let m = self.recents_model(&windows);
                let count = m.entries().len().max(1);
                self.navigation.recents_pager.set_pages(count);
                let moving = self.morph.as_ref().and_then(|m| m.overview_window());
                let hidden = moving.and_then(|w| windows.iter().position(|entry| entry == w));
                de_ui::recents::render_at_pos(
                    size,
                    &s,
                    &t,
                    &status,
                    &m,
                    self.navigation.recents_backdrop.as_ref(),
                    self.navigation.recents_pager.pos(),
                    hidden,
                )
            }
            Screen::Drawer => {
                let ids = self.visible_ids();
                let apps = self.app_icons(&ids);
                de_ui::drawer::render(size, &s, &t, &status, &apps, self.navigation.drawer_scroll)
            }
            Screen::Settings => {
                de_ui::settings::render_index(size, &s, &t, &status.clock)
            }
            Screen::Search => {
                let rows = self.search_rows();
                let blink = (self.started.elapsed().as_millis() / 530) % 2 == 0;
                de_ui::search::render_with_caret(size, &s, &t, &self.navigation.search_query, &rows, blink)
            }
            Screen::Shade => de_ui::shade::render(size, &s, &t, &status.clock),
            // The interactive lock lives in `lock_ui.rs`; the nav screen
            // variant exists only as a placeholder between boot and unlock.
            Screen::Lock => {
                let wp = self.wallpaper_for(&self.app_prefs.prefs().wallpaper_id.clone(), size);
                let backdrop = de_paint::glass::blurred_backdrop(&wp);
                let model = de_ui::lock::LockModel {
                    status: &status,
                    clock: &status.clock,
                    date: &date,
                    charging: "",
                    notifications: &[],
                    hidden_notifications: 0,
                    fingerprint: false,
                    flashlight: None,
                    quick: None,
                    backdrop: &backdrop,
                };
                de_ui::lock::render(size, &s, &t, &model)
            }
            Screen::Power => {
                let m = self.power_menu_model();
                de_ui::power::render(size, &s, &t, &m)
            }
            Screen::Error => {
                let app = self.launch_error.clone().unwrap_or_else(|| "app".into());
                de_ui::error::render(size, &s, &t, &app)
            }
            _ => self.home_pixels(size, &s, &t, status, &date),
        };
        self.overlay_shell_menu(base)
    }

    /// Home screen frame: wallpaper (asset or procedural), catalog icons in
    /// `home_order`, pinned dock, current page.
    fn home_pixels(
        &mut self,
        size: Size,
        s: &de_ui::i18n::Strings,
        t: &tokens::Theme,
        status: StatusInfo,
        date: &str,
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
        // Rearrange preview: the lifted icon leaves its slot immediately
        // and a `GAP_ID` pocket opens where it would drop — the same
        // insert index `drop_on_grid`/`drop_on_dock` applies on release.
        let drag = self.home_ui.drag.as_ref().map(|d| (d.id.clone(), d.target));
        let mut ids: Vec<String> = ordered.iter().map(|e| e.id.clone()).collect();
        let mut dock_ids: Vec<String> = self
            .catalog
            .dock(&prefs)
            .into_iter()
            .map(|e| e.id.clone())
            .collect();
        if let Some((drag_id, target)) = &drag {
            ids.retain(|i| i != drag_id);
            dock_ids.retain(|i| i != drag_id);
            match target {
                Some(de_ui::home::DropTarget::Grid(i)) => {
                    let at = (page * de_ui::home::PAGE_SIZE + i).min(ids.len());
                    ids.insert(at, de_ui::home::GAP_ID.into());
                }
                Some(de_ui::home::DropTarget::Dock(slot)) => {
                    let at = (*slot).min(dock_ids.len());
                    dock_ids.insert(at, de_ui::home::GAP_ID.into());
                }
                None => {}
            }
        }
        let page_ids: Vec<String> = ids
            .into_iter()
            .skip(page * de_ui::home::PAGE_SIZE)
            .take(de_ui::home::PAGE_SIZE)
            .collect();
        let apps = self.app_icons(&page_ids);
        let dock = self.app_icons(&dock_ids);
        let clock = status.clock.clone();
        let date = date.to_owned();
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

    /// Status bar data and the date line. Polling sysfs and forking `date`
    /// costs tens of milliseconds on the A21s, so while frames are being
    /// drawn back to back (paging, dragging) the last reading is reused for
    /// up to two seconds instead of being fetched per frame.
    pub(crate) fn shell_meta(&mut self) -> (StatusInfo, String) {
        let now = std::time::Instant::now();
        if let Some((at, minute, status, date)) = &self.meta_cache {
            if *minute == self.shell_minute
                && now.duration_since(*at) < std::time::Duration::from_secs(2)
            {
                return (status.clone(), date.clone());
            }
        }
        let (status, date) = (Self::status_info(), clock_date());
        self.meta_cache = Some((now, self.shell_minute, status.clone(), date.clone()));
        (status, date)
    }

    /// Visible app ids under current prefs.
    pub(crate) fn visible_ids(&self) -> Vec<String> {
        let prefs = self.app_prefs.prefs();
        self.catalog.visible(prefs).map(|e| e.id.clone()).collect()
    }

    /// `IconEntry`s for `desktop_ids`, resolving real icons through the
    /// cache (first-party `rabbiz-*`/`a21s-*` get the sand-tile glyph).
    pub(crate) fn app_icons(&mut self, desktop_ids: &[String]) -> Vec<IconEntry> {
        desktop_ids.iter().map(|id| self.app_icon(id)).collect()
    }

    pub(crate) fn app_icon(&mut self, id: &str) -> IconEntry {
        // Drag-preview pocket — not a real app, never hits the icon cache.
        if id == de_ui::home::GAP_ID {
            return IconEntry {
                id: id.to_owned(),
                label: String::new(),
                icon: None,
            };
        }
        let (label, icon_name) = self.catalog.find(id).map_or_else(
            || (id.to_owned(), String::new()),
            |entry| (entry.name.clone(), entry.icon.clone()),
        );
        let icon = self
            .icon_cache
            .entry(id.to_owned())
            .or_insert_with(|| de_apps::icons::rasterize_icon(id, &icon_name, &label, 128));
        IconEntry {
            id: id.to_owned(),
            label,
            icon: Some(icon.clone()),
        }
    }

    fn recents_model(&mut self, windows: &[smithay::desktop::Window]) -> RecentsModel {
        RecentsModel::new(windows.iter().map(|window| {
            let id = window.toplevel().map(crate::snapshots::window_app_id).unwrap_or_default();
            let mut entry = self.recent_entry(&id);
            entry.snapshot = crate::snapshots::snapshot(window);
            entry
        }).collect())
    }

    fn recent_entry(&mut self, id: &str) -> RecentEntry {
        let title = self
            .catalog
            .find(id)
            .map_or_else(|| id.to_string(), |e| e.name.clone());
        let icon = self
            .app_icons(std::slice::from_ref(&id.to_string()))
            .pop()
            .and_then(|i| i.icon);
        let mut r = RecentEntry::new(id, &title);
        r.icon = icon;
        r
    }

    /// Search rows: every visible app (the query field is static until the
    /// on-screen keyboard/search input lands in M4 — tapping a row launches).
    fn search_rows(&mut self) -> Vec<SearchRow> {
        let q = self.navigation.search_query.trim().to_lowercase();
        let ids = self.visible_ids();
        self.app_icons(&ids)
            .into_iter()
            .filter(|i| {
                q.is_empty()
                    || i.label.to_lowercase().contains(&q)
                    || i.id.to_lowercase().contains(&q)
            })
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
    fn status_info() -> StatusInfo {
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
#[must_use]
pub fn clock_hhmm() -> String {
    std::process::Command::new("date")
        .arg("+%H:%M")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map_or_else(|| "--:--".to_string(), |s| s.trim().to_string())
}

/// "Mon, 9 Jun" — locale comes from the session environment.
#[must_use]
pub fn clock_date() -> String {
    std::process::Command::new("date")
        .arg("+%a, %-d %b")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}
