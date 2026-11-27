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
        let t = self.theme;
        let (status, date) = self.shell_meta();
        let base = match self.nav.current {
            Screen::Recents => {
                let windows = self.recent_windows();
                let m = self.recents_model(&windows);
                let count = m.entries().len().max(1);
                self.navigation.recents_pager.set_pages(count);
                let moving = self.morph.as_ref().and_then(|m| m.overview_window());
                let hidden = moving.and_then(|w| windows.iter().position(|entry| entry == w));
                let lift = self.card_lift(&windows);
                de_ui::recents::render_at_pos(
                    size,
                    &s,
                    &t,
                    &status,
                    &m,
                    self.navigation.recents_backdrop.as_ref(),
                    self.navigation.recents_pager.pos(),
                    hidden,
                    lift,
                )
            }
            Screen::Drawer => {
                let recent = self.recent_drawer_icons();
                let ids = self.visible_ids();
                let apps = self.app_icons(&ids);
                let blink = !self.navigation.search_query.is_empty()
                    && (self.started.elapsed().as_millis() / 530) % 2 == 0;
                de_ui::drawer::render_with_query(
                    size,
                    &s,
                    &t,
                    &status,
                    &recent,
                    &apps,
                    &self.navigation.search_query,
                    blink,
                    self.navigation.drawer_scroll,
                )
            }
            Screen::Settings => {
                let prefs = self.app_prefs.prefs();
                if self.navigation.settings_keyboard {
                    let active = de_config::keyboards::resolve_active_keyboard(prefs);
                    let options =
                        de_config::keyboards::discover_keyboards(prefs.allow_third_party_keyboards)
                            .into_iter()
                            .map(|k| de_ui::settings::keyboard::KeyboardOption {
                                active: k.id == active.id,
                                id: k.id,
                                name: k.name,
                                third_party: k.third_party,
                                installed: k.installed,
                            })
                            .collect::<Vec<_>>();
                    de_ui::settings::keyboard::render(
                        size,
                        &s,
                        &t,
                        &status.clock,
                        prefs.allow_third_party_keyboards,
                        &options,
                    )
                } else {
                    let active = de_config::keyboards::resolve_active_keyboard(prefs);
                    de_ui::settings::index::render_index_with_keyboard(
                        size,
                        &s,
                        &t,
                        &status.clock,
                        &active.name,
                    )
                }
            }
            Screen::Search => {
                let rows = self.search_rows();
                let blink = (self.started.elapsed().as_millis() / 530) % 2 == 0;
                de_ui::search::render_with_caret(
                    size,
                    &s,
                    &t,
                    &self.navigation.search_query,
                    &rows,
                    blink,
                )
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
        let (status, date) = (self.status_info(), clock_date());
        self.meta_cache = Some((now, self.shell_minute, status.clone(), date.clone()));
        (status, date)
    }

    /// Visible app ids under current prefs.
    /// Up to 4 recent/frequent apps for the top row of the Drawer.
    pub(crate) fn recent_drawer_icons(&mut self) -> Vec<IconEntry> {
        let mut ids: Vec<String> = self
            .recent_windows()
            .into_iter()
            .filter_map(|w| w.toplevel().map(crate::snapshots::window_app_id))
            .collect();
        if ids.len() < 4 {
            let prefs = self.app_prefs.prefs().clone();
            for e in self
                .catalog
                .dock(&prefs)
                .into_iter()
                .chain(self.catalog.home_order(&prefs))
            {
                if ids.len() >= 4 {
                    break;
                }
                if !ids.iter().any(|x| x == &e.id) {
                    ids.push(e.id.clone());
                }
            }
        }
        ids.truncate(4);
        self.app_icons(&ids)
    }

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

    pub(crate) fn recents_model(&mut self, windows: &[smithay::desktop::Window]) -> RecentsModel {
        let mut entries: Vec<RecentEntry> = windows
            .iter()
            .map(|window| {
                let id = window
                    .toplevel()
                    .map(crate::snapshots::window_app_id)
                    .unwrap_or_default();
                let mut entry = self.recent_entry(&id);
                entry.snapshot = crate::snapshots::snapshot(window);
                entry
            })
            .collect();
        // A card being flung off keeps its slot after the app exits: the
        // frozen entry stands in for the dead window until the fly-off
        // spring completes, so the dismiss animation plays to the end.
        if let Some(d) = self.navigation.card_dismiss.as_ref() {
            if let Some(frozen) = d.entry.as_ref() {
                if !entries.iter().any(|e| e.id == d.id) {
                    entries.insert(d.index.min(entries.len()), frozen.clone());
                }
            }
        }
        RecentsModel::new(entries)
    }

    pub(crate) fn recent_entry(&mut self, id: &str) -> RecentEntry {
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

    /// Search rows: empty until the user types; once typing, matching apps
    /// appear first (prefix matches ahead of substring matches), followed by a
    /// web-search action for the configured default search engine.
    pub(crate) fn search_rows(&mut self) -> Vec<SearchRow> {
        let raw = self.navigation.search_query.trim().to_string();
        if raw.is_empty() {
            return Vec::new();
        }
        let q = raw.to_lowercase();
        let ids = self.visible_ids();
        let mut apps: Vec<(bool, SearchRow)> = self
            .app_icons(&ids)
            .into_iter()
            .filter_map(|i| {
                let label_l = i.label.to_lowercase();
                let id_l = i.id.to_lowercase();
                if !label_l.contains(&q) && !id_l.contains(&q) {
                    return None;
                }
                let prefix = label_l.starts_with(&q) || id_l.starts_with(&q);
                Some((
                    prefix,
                    SearchRow {
                        kind: SearchKind::App,
                        id: i.id,
                        title: i.label,
                        subtitle: String::new(),
                        icon: i.icon,
                    },
                ))
            })
            .collect();
        apps.sort_by(|a, b| b.0.cmp(&a.0));
        let mut rows: Vec<SearchRow> = apps.into_iter().map(|(_, r)| r).collect();
        let engine = de_config::prefs::search_engine_label(&self.app_prefs.prefs().search_engine);
        rows.push(SearchRow {
            kind: SearchKind::Action,
            id: "web-search".into(),
            title: format!("Search {engine} for \"{raw}\""),
            subtitle: format!("Open in {engine}"),
            icon: None,
        });
        rows
    }

    /// Status bar data from cached off-thread pollers + `panels.control_state`
    /// (Wi-Fi / Bluetooth / Airplane / DND). Never blocks on sysfs/procfs.
    fn status_info(&self) -> StatusInfo {
        let b = &self.cached_battery;
        let p = &self.cached_privacy;
        let cs = &self.panels.control_state;
        StatusInfo {
            clock: clock_hhmm(),
            battery_pct: b.percent,
            charging: b.charging,
            battery_full: b.full,
            wifi: cs.wifi,
            bluetooth: cs.bluetooth.unwrap_or(false),
            airplane: cs.airplane.unwrap_or(false),
            dnd: self.app_prefs.prefs().do_not_disturb,
            privacy_cam: p.camera,
            privacy_mic: p.mic,
            ..StatusInfo::default()
        }
    }
}

fn ensure_locale_initialized() {
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(|| {
        #[allow(unsafe_code)]
        unsafe {
            libc::setlocale(libc::LC_TIME, c"".as_ptr());
        }
    });
}

fn format_local_time(fmt: &std::ffi::CStr) -> Option<String> {
    ensure_locale_initialized();
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs();
    let t = secs as libc::time_t;
    #[allow(unsafe_code)]
    unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&t, &mut tm).is_null() {
            return None;
        }
        let mut buf = [0_u8; 64];
        let n = libc::strftime(
            buf.as_mut_ptr().cast::<libc::c_char>(),
            buf.len(),
            fmt.as_ptr(),
            &tm,
        );
        if n == 0 {
            return None;
        }
        std::str::from_utf8(&buf[..n])
            .ok()
            .map(|s| s.trim().to_string())
    }
}

/// "HH:MM" local time (in-process `localtime_r`/`strftime`, no `date` fork).
#[must_use]
pub fn clock_hhmm() -> String {
    format_local_time(c"%H:%M").unwrap_or_else(|| "--:--".to_string())
}

/// "Mon, 9 Jun" — locale comes from the session environment.
#[must_use]
pub fn clock_date() -> String {
    format_local_time(c"%a, %-d %b").unwrap_or_default()
}
