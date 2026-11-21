//! Shell screen transitions, shell-surface tap dispatch, and app launching.

use crate::state::De;

impl De {
    /// Switch the screen to a shell surface (app keeps running).
    pub(crate) fn show_shell(&mut self, screen: de_ui::nav::Screen) {
        use crate::state::Foreground;
        use de_ui::nav::Screen;
        let from_overview = self.nav.current == Screen::Recents
            && self.foreground == Foreground::Shell
            && self.morph.is_none();
        let animate_overview = !self.motion_clock.motion_restricted();
        if screen == Screen::Home && from_overview && animate_overview {
            self.fade_recents(false);
            return;
        }
        if screen == Screen::Recents && !from_overview {
            self.navigation.recents_from_app = self.foreground == Foreground::App
                || self.morph.as_ref().is_some_and(|m| m.window.is_some());
        }
        if screen == Screen::Recents {
            if self
                .morph
                .as_ref()
                .is_some_and(|m| m.phase == crate::morph::Phase::Recents)
                || self.release_home_to_recents()
            {
                return;
            }
            if from_overview {
                if self.navigation.recents_fade.is_some() {
                    self.fade_recents(true);
                }
                return;
            }
            self.reset_recents_pager();
            self.prepare_recents_backdrop();
        }
        let fade_in =
            screen == Screen::Recents && self.foreground == Foreground::Shell && animate_overview;
        self.navigation.recents_fade = None;
        if matches!(self.foreground, Foreground::App) {
            // The window is still presented — grab its last frame for the
            // recents card before the shell takes over.
            self.capture_topmost();
        }
        self.morph = None;
        self.launch.cancel();
        self.foreground = Foreground::Shell;
        self.focus_foreground();
        self.nav.current = screen;
        if fade_in {
            self.fade_recents(true);
        }
        self.shell_dirty = true;
        self.needs_present = true;
    }

    /// Tap on a shell screen: icon → launch, search pill → search,
    /// recents card → back to the app.
    pub(super) fn on_shell_tap(&mut self, pos: de_api::Point) {
        use crate::state::Foreground;
        use de_ui::nav::Screen;
        if !matches!(self.foreground, Foreground::Shell) {
            return;
        }
        // The media island owns taps in its band (expand / transport / collapse).
        if self.media_tap(pos) {
            self.needs_present = true;
            return;
        }
        let size = de_api::Size::new(self.output.width as i32, self.output.height as i32);
        match self.nav.current {
            Screen::Home => {
                let prefs = self.app_prefs.prefs().clone();
                let ordered: Vec<String> = self
                    .catalog
                    .home_order(&prefs)
                    .iter()
                    .map(|e| e.id.clone())
                    .collect();
                let on_page = ordered
                    .len()
                    .saturating_sub(self.home_page * de_ui::home::PAGE_SIZE)
                    .min(de_ui::home::PAGE_SIZE);
                let dock: Vec<String> = self
                    .catalog
                    .dock(&prefs)
                    .iter()
                    .map(|e| e.id.clone())
                    .collect();
                match de_ui::home::hit_test(size, pos, on_page, dock.len()) {
                    Some(de_ui::home::HomeHit::Grid(i)) => {
                        let id = ordered[self.home_page * de_ui::home::PAGE_SIZE + i].clone();
                        self.try_launch(
                            &id,
                            de_ui::home::icon_rect(size, de_ui::home::HomeHit::Grid(i), dock.len()),
                        );
                    }
                    Some(de_ui::home::HomeHit::Dock(i)) => {
                        if let Some(id) = dock.get(i).cloned() {
                            self.try_launch(
                                &id,
                                de_ui::home::icon_rect(
                                    size,
                                    de_ui::home::HomeHit::Dock(i),
                                    dock.len(),
                                ),
                            );
                        }
                    }
                    Some(de_ui::home::HomeHit::Drawer) => {
                        self.open_drawer_animated(None, 0.0);
                    }
                    Some(de_ui::home::HomeHit::Search) => {
                        self.nav.go(Screen::Search);
                        self.shell_im.set_visible(true);
                        self.shell_dirty = true;
                        self.needs_present = true;
                    }
                    None => {}
                }
            }
            Screen::Search => {
                let ids = self
                    .catalog
                    .visible(self.app_prefs.prefs())
                    .map(|e| e.id.clone())
                    .collect::<Vec<_>>();
                let rows = self.search_row_ids(&ids);
                if let Some(i) = de_ui::search::hit_test(size, &rows, pos) {
                    self.try_launch(&rows[i].id, de_ui::search::icon_rect(size, &rows, i));
                }
            }
            Screen::Drawer => {
                let recent = self.recent_drawer_icons();
                let ids = self.visible_ids();
                let apps = self.app_icons(&ids);
                let query = self.navigation.search_query.clone();
                match de_ui::drawer::hit_test_with_query(
                    size,
                    &recent,
                    &apps,
                    &query,
                    self.navigation.drawer_scroll,
                    pos,
                ) {
                    Some(de_ui::drawer::DrawerHit::App(id)) => {
                        let cx = de_api::Rect::new(size.w / 2 - 56, size.h / 2 - 56, 112, 112);
                        self.try_launch(&id, Some(cx));
                    }
                    Some(de_ui::drawer::DrawerHit::Search) => {
                        self.shell_im.set_visible(true);
                        self.shell_dirty = true;
                        self.needs_present = true;
                    }
                    Some(de_ui::drawer::DrawerHit::Letter(letter)) => {
                        if let Some(y) = de_ui::drawer::scroll_for_letter(&apps, &letter) {
                            self.navigation.drawer_scroll = y;
                            self.shell_dirty = true;
                            self.needs_present = true;
                        }
                    }
                    None => {}
                }
            }
            Screen::Error => match de_ui::error::hit_test(size, pos) {
                Some(de_ui::error::ErrorHit::Wait) if self.space.elements().next().is_some() => {
                    self.launch_error = None;
                    self.foreground = Foreground::App;
                    self.focus_foreground();
                    self.needs_present = true;
                }
                Some(de_ui::error::ErrorHit::Wait | de_ui::error::ErrorHit::Close) => {
                    self.launch_error = None;
                    self.show_shell(Screen::Home);
                }
                None => {}
            },
            Screen::Settings => {
                if self.navigation.settings_keyboard {
                    let prefs = self.app_prefs.prefs();
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
                    match de_ui::settings::keyboard::hit_test(size, &options, pos) {
                        Some(de_ui::settings::keyboard::KeyboardHit::Back) => {
                            self.navigation.settings_keyboard = false;
                            self.shell_dirty = true;
                            self.needs_present = true;
                        }
                        Some(de_ui::settings::keyboard::KeyboardHit::ToggleThirdParty) => {
                            let next = !self.app_prefs.prefs().allow_third_party_keyboards;
                            if self.app_prefs.set_allow_third_party_keyboards(next) {
                                self.restart_osk();
                                self.shell_dirty = true;
                                self.needs_present = true;
                            }
                        }
                        Some(de_ui::settings::keyboard::KeyboardHit::Select(i)) => {
                            if let Some(opt) = options.get(i) {
                                if self.app_prefs.set_keyboard(&opt.id) {
                                    self.restart_osk();
                                    self.shell_dirty = true;
                                    self.needs_present = true;
                                }
                            }
                        }
                        None => {}
                    }
                } else if let Some(idx) = de_ui::settings::index::hit_test(size, pos) {
                    match de_ui::settings::index::section_key(idx) {
                        Some("settings.display") => {
                            let next =
                                de_paint::wallpaper::next_id(&self.app_prefs.prefs().wallpaper_id);
                            if self.app_prefs.set_wallpaper(next) {
                                self.wallpaper = None;
                                self.shell_dirty = true;
                                self.needs_present = true;
                            }
                        }
                        Some("settings.keyboard") => {
                            self.navigation.settings_keyboard = true;
                            self.shell_dirty = true;
                            self.needs_present = true;
                        }
                        _ => {}
                    }
                }
            }
            Screen::Power => self.power_menu_tap(pos, false),
            Screen::Recents => {
                if self.morph.is_some() {
                    return;
                }
                let windows = self.recent_windows();
                let index = self.navigation.recents_pager.pos();
                self.release_recents_pager(0.0);
                match de_ui::recents::hit_test(size, windows.len(), index, pos) {
                    Some(de_ui::recents::RecentsHit::ClearAll) => {
                        for w in windows {
                            if let Some(t) = w.toplevel() {
                                t.send_close();
                            }
                        }
                        self.show_shell(Screen::Home);
                    }
                    Some(de_ui::recents::RecentsHit::Card(idx)) => {
                        if let Some(window) = windows.into_iter().nth(idx) {
                            self.navigation.recents_fade = None;
                            self.restore_recent(window, de_ui::recents::card_at(size, idx, index));
                        }
                    }
                    None => self.show_shell(Screen::Home),
                }
            }
            _ => {}
        }
    }

    /// `SearchRow` ids only (icon pixels are a render concern).
    fn search_row_ids(&self, ids: &[String]) -> Vec<de_ui::search::SearchRow> {
        ids.iter()
            .map(|id| de_ui::search::SearchRow {
                kind: de_ui::search::SearchKind::App,
                id: id.clone(),
                title: self
                    .catalog
                    .find(id)
                    .map_or_else(|| id.clone(), |e| e.name.clone()),
                subtitle: String::new(),
                icon: None,
            })
            .collect()
    }

    /// Spawn an app through the launch contract; failures go to the error
    /// screen, the pending launch resolves on `new_toplevel`.
    pub fn try_launch(&mut self, desktop_id: &str, source: Option<de_api::Rect>) {
        self.launch.cancel();
        self.morph = None;
        if let Some(window) = self.recent_windows().into_iter().find(|w| {
            w.toplevel().is_some_and(|t| {
                let id = crate::snapshots::window_app_id(t);
                id.trim_end_matches(".desktop") == desktop_id.trim_end_matches(".desktop")
            })
        }) {
            self.launch_error = None;
            if let Some(rect) = source.filter(|_| !self.motion_clock.motion_restricted()) {
                self.begin_launch_morph(desktop_id, rect);
                if let Some(morph) = self.morph.as_mut() {
                    morph.reuse_window(window);
                }
                self.needs_present = true;
            } else {
                self.show_app_window(&window);
            }
            return;
        }
        match self.launch.begin(desktop_id) {
            Ok(pid) => {
                eprintln!("launch: {desktop_id} pid={pid}");
                self.launch_error = None;
                if let Some(rect) = source {
                    self.begin_launch_morph(desktop_id, rect);
                }
                self.needs_present = true;
            }
            Err(e) => {
                self.launch_error = Some(format!("{desktop_id}: {e}"));
                self.foreground = crate::state::Foreground::Shell;
                self.focus_foreground();
                self.nav.go(de_ui::nav::Screen::Error);
                self.shell_dirty = true;
                self.needs_present = true;
            }
        }
    }
}
