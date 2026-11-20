//! Shell gesture state machine: navigation bar drags, edge-back, drawer/recents
//! interactions, lock screen gestures, and recognized gesture actions.

use std::time::Instant;

use de_ui::gestures::{Gesture, GestureEvent, Zone};

use crate::state::De;

impl De {
    pub(super) fn on_gesture_event(&mut self, ev: GestureEvent, time_ms: u64) {
        use crate::navigation::RecentsAxis;
        use crate::state::Foreground;
        use de_ui::nav::Screen;
        if !matches!(
            ev,
            GestureEvent::Progress { .. } | GestureEvent::HoldDrag { .. }
        ) && std::env::var_os("DE_GESTURE_DEBUG").is_some()
        {
            eprintln!("gesture: {ev:?}");
        }
        // The lock overlay owns every gesture while it covers the screen
        // (until an unlock has been committed).
        if self.lock.shown() && !self.lock.is_unlocking() {
            self.lock_gesture(ev);
            return;
        }
        let g = match ev {
            GestureEvent::Progress {
                zone: Zone::NavBar,
                dx,
                dy,
            } => {
                // Horizontal scrub along the bottom bar is QuickSwitch; don't
                // hijack it as a vertical home-drag.
                if dx.abs() > dy.abs() * 1.5 && dy > -24.0 {
                    return;
                }
                self.follow_home_drag(dx, dy, time_ms);
                return;
            }
            GestureEvent::Progress { zone, dx, dy } if zone != Zone::NavBar => {
                if matches!(zone, Zone::EdgeLeft | Zone::EdgeRight) {
                    self.back_progress(zone, dx, dy);
                    if self.navigation.back.is_some() {
                        return;
                    }
                }
                if self.panel_progress(zone == Zone::TopEdge, dx, dy) {
                    return;
                }
                if zone == Zone::Content {
                    if self.foreground == Foreground::Shell
                        && self.nav.current == Screen::Drawer
                        && !self.navigation.drawer_closing
                    {
                        if self.navigation.drawer_dragging
                            && self.navigation.drawer_pull_y == 0.0
                            && dy < 0.0
                        {
                            self.drag_drawer(dy);
                        } else if self.navigation.drawer_pull_y > 0.0
                            || (self.navigation.drawer_drag_origin == 0 && dy > 0.0)
                        {
                            let pull = dy.max(0.0);
                            self.navigation.drawer_dragging = true;
                            self.navigation.drawer_pull_y = pull;
                            self.navigation.drawer_slide.value = pull;
                            self.navigation.drawer_slide.target = pull;
                            self.needs_present = true;
                        } else {
                            self.navigation.drawer_dragging = false;
                            self.navigation.drawer_pull_y = 0.0;
                            let next =
                                (self.navigation.drawer_drag_origin - dy.round() as i32).max(0);
                            if next != self.navigation.drawer_scroll {
                                self.navigation.drawer_scroll = next;
                                self.shell_dirty = true;
                                self.needs_present = true;
                            }
                        }
                    } else if self.foreground == Foreground::Shell
                        && self.nav.current == Screen::Recents
                    {
                        let axis = *self.navigation.recents_axis.get_or_insert_with(|| {
                            if dx.abs() > dy.abs() {
                                RecentsAxis::Horizontal
                            } else {
                                RecentsAxis::Vertical
                            }
                        });
                        if axis == RecentsAxis::Vertical && self.morph.is_none() {
                            self.card_dismiss_drag(dy);
                        } else if axis == RecentsAxis::Horizontal {
                            let size = de_api::Size::new(
                                self.output.width as i32,
                                self.output.height as i32,
                            );
                            self.navigation
                                .recents_pager
                                .drag(dx, de_ui::recents::card_stride(size) as f32);
                            self.shell_dirty = true;
                            self.needs_present = true;
                        }
                    } else if self.foreground == Foreground::Shell
                        && self.nav.current == Screen::Home
                        && !self.home_ui.paging()
                        && !self.home_ui.dragging_icon()
                        && dy < -16.0
                        && -dy > dx.abs() * 1.25
                    {
                        self.home_drag_cancelled();
                        self.begin_drawer_drag();
                        self.drag_drawer(dy);
                        return;
                    } else if self.foreground == Foreground::Shell
                        && self.nav.current == Screen::Home
                        && !self.home_ui.paging()
                        && dy > 24.0
                        && dy > dx.abs() * 1.4
                        && self.panel_progress(true, dx, dy)
                    {
                        return;
                    } else {
                        self.home_drag_progress(dx, dy);
                    }
                }
                return;
            }
            GestureEvent::Started { pos, zone } => {
                self.clear_back_feedback();
                self.navigation.zone = zone;
                self.navigation.start = pos;
                self.navigation.drawer_drag_origin = self.navigation.drawer_scroll;
                if self.foreground == Foreground::Shell
                    && self.nav.current == Screen::Recents
                    && zone == Zone::Content
                    && self.morph.is_none()
                {
                    self.navigation.recents_axis = None;
                    self.navigation.recents_pager.begin();
                }
                self.panel_touch_started(pos);
                self.home_touch_started();
                return;
            }
            GestureEvent::NavPaused => {
                crate::haptics::pulse(crate::haptics::HapticKind::RecentsHold);
                self.nav_preview(true);
                return;
            }
            GestureEvent::NavResumed => {
                self.nav_preview(false);
                return;
            }
            GestureEvent::NavReleased { dy, vy, paused } => {
                use de_ui::gestures::{decide_navigation, NavDestination};
                match decide_navigation(dy, vy, self.output.height as f32, paused) {
                    NavDestination::Home if self.panels.shown() => self.panel_close(),
                    NavDestination::Home => {
                        crate::haptics::pulse(crate::haptics::HapticKind::NavCommit);
                        if self.foreground == Foreground::Shell && self.nav.current == Screen::Home
                        {
                            self.open_drawer_animated(None, vy);
                        } else if self.foreground == Foreground::App {
                            self.minimize_foreground();
                        } else {
                            self.show_shell(Screen::Home);
                        }
                    }
                    NavDestination::Recents => {
                        self.show_shell(Screen::Recents);
                    }
                    NavDestination::Cancel => {
                        self.release_home_drag(false);
                    }
                }
                return;
            }
            GestureEvent::Released { dx, dy, vx, vy } => {
                if self.panels.pull.dragging() {
                    self.panel_released(vy);
                } else if self.foreground == Foreground::Shell
                    && self.nav.current == Screen::Recents
                {
                    let axis = self.navigation.recents_axis;
                    match axis {
                        // Swipe up on a card closes that app (Android).
                        Some(RecentsAxis::Vertical) if self.card_dismiss_release(dy, vy) => {
                            self.release_recents_pager(0.0);
                        }
                        // Swipe up on the space around the cards: Home.
                        Some(RecentsAxis::Vertical) if -dy > self.output.height as f32 * 0.09 => {
                            self.release_recents_pager(0.0);
                            self.show_shell(Screen::Home);
                        }
                        Some(RecentsAxis::Horizontal) => self.release_recents_pager(vx),
                        _ => self.release_recents_pager(0.0),
                    }
                } else if self.foreground == Foreground::Shell
                    && self.nav.current == Screen::Drawer
                    && self.navigation.drawer_dragging
                    && self.navigation.drawer_pull_y == 0.0
                {
                    self.release_drawer_drag(vy);
                } else if self.foreground == Foreground::Shell
                    && self.nav.current == Screen::Drawer
                    && self.navigation.drawer_pull_y > 0.0
                {
                    let pulled = self.navigation.drawer_pull_y;
                    self.navigation.drawer_dragging = false;
                    self.navigation.drawer_pull_y = 0.0;
                    self.navigation.drawer_slide.value = pulled;
                    self.navigation.drawer_last_tick = Some(std::time::Instant::now());
                    if pulled > 140.0 || vy > 500.0 {
                        self.navigation.drawer_closing = true;
                        self.navigation
                            .drawer_slide
                            .retarget_with_velocity(self.output.height as f32, vy.max(0.0));
                    } else {
                        self.navigation.drawer_closing = false;
                        self.navigation.drawer_slide.retarget(0.0);
                    }
                    self.needs_present = true;
                } else if self.foreground == Foreground::Shell
                    && self.nav.current == Screen::Home
                    && (dy < -64.0 || (dy < -28.0 && vy <= -350.0))
                    && -dy > dx.abs() * 1.4
                {
                    self.home_drag_cancelled();
                    let from_y = (self.output.height as f32 + dy).max(0.0);
                    self.open_drawer_animated(Some(from_y), vy);
                } else {
                    self.home_drag_released(vx);
                }
                return;
            }
            GestureEvent::HoldDrag { pos } => {
                self.home_icon_dragged(pos);
                return;
            }
            GestureEvent::HoldDrop { pos } => {
                self.home_icon_dropped(pos);
                return;
            }
            GestureEvent::Cancelled => {
                self.clear_back_feedback();
                self.panel_cancelled();
                self.home_drag_cancelled();
                if self.foreground == Foreground::Shell
                    && self.nav.current == Screen::Drawer
                    && self.navigation.drawer_dragging
                    && self.navigation.drawer_pull_y == 0.0
                {
                    self.release_drawer_drag(0.0);
                }
                if self.nav.current == Screen::Recents {
                    self.card_dismiss_release(0.0, 0.0);
                    self.release_recents_pager(0.0);
                }
                self.release_home_drag(false);
                return;
            }
            GestureEvent::Recognized(gesture) => gesture,
            GestureEvent::Progress { .. } => return,
        };
        self.clear_back_feedback();
        match g {
            Gesture::Home | Gesture::EdgeBack { .. } if self.panels.shown() => self.panel_close(),
            Gesture::Home => {
                crate::haptics::pulse(crate::haptics::HapticKind::NavCommit);
                if self.foreground == Foreground::App {
                    self.minimize_foreground();
                } else {
                    self.show_shell(Screen::Home);
                }
            }
            Gesture::QuickSwitch { to_previous } => {
                crate::haptics::pulse(crate::haptics::HapticKind::QuickSwitch);
                if !self.panels.shown() {
                    let windows = self.recent_windows();
                    if !windows.is_empty() {
                        let target = if self.foreground == Foreground::App {
                            if windows.len() >= 2 {
                                self.capture_topmost();
                                if to_previous {
                                    windows[1].clone()
                                } else {
                                    windows[windows.len() - 1].clone()
                                }
                            } else {
                                windows[0].clone()
                            }
                        } else if to_previous {
                            windows[0].clone()
                        } else {
                            windows[windows.len() - 1].clone()
                        };
                        self.morph = None;
                        self.launch.cancel();
                        self.show_app_window(&target);
                    }
                }
            }
            Gesture::EdgeBack { .. } => {
                if matches!(self.foreground, Foreground::App) {
                    // Android: Back belongs to the app (owner decision
                    // 2026-11-19, docs/AUDIT-UIUX-2026-11-19.md Q1).
                    self.app_back(time_ms);
                } else if self.nav.current == Screen::Recents {
                    let windows = self.recent_windows();
                    if self.navigation.recents_from_app && self.morph.is_none() {
                        if let Some(window) = windows.into_iter().next() {
                            let size = de_api::Size::new(
                                self.output.width as i32,
                                self.output.height as i32,
                            );
                            let pos = self.navigation.recents_pager.pos();
                            self.release_recents_pager(0.0);
                            self.navigation.recents_fade = None;
                            self.restore_recent(window, de_ui::recents::card_at(size, 0, pos));
                        } else {
                            self.show_shell(Screen::Home);
                        }
                    } else {
                        self.show_shell(Screen::Home);
                    }
                } else if self.nav.current == Screen::Drawer {
                    self.shell_im.set_visible(false);
                    self.navigation.drawer_dragging = false;
                    self.navigation.drawer_closing = true;
                    self.navigation.drawer_last_tick = Some(std::time::Instant::now());
                    let h = self.output.height.max(1) as f32;
                    self.navigation
                        .drawer_slide
                        .retarget_with_velocity(h, 1200.0);
                    self.needs_present = true;
                } else if self.nav.current == Screen::Settings && self.navigation.settings_keyboard
                {
                    self.navigation.settings_keyboard = false;
                    self.shell_dirty = true;
                    self.needs_present = true;
                } else {
                    self.launch.cancel();
                    self.morph = None;
                    self.nav.back();
                    self.shell_dirty = true;
                    self.needs_present = true;
                }
            }
            Gesture::Tap { .. } if self.panels.shown() => {
                if let Gesture::Tap { pos } = g {
                    self.panel_tap(pos);
                }
            }
            Gesture::Tap { pos } => {
                let size = de_api::Size::new(self.output.width as i32, self.output.height as i32);
                if self.shot_fx.tap(pos, size, Instant::now()) {
                    self.needs_present = true;
                } else if !self.release_home_drag(false) {
                    self.on_shell_tap(pos);
                }
            }
            // Long-press is the icon-context-menu hook (hide/pin) — no-op
            // until the drawer action sheet lands.
            Gesture::LongPress { pos } => {
                crate::haptics::pulse(crate::haptics::HapticKind::LongPress);
                if !self.home_lift_icon(pos) {
                    self.open_home_menu(pos);
                }
            }
        }
    }

    /// Every gesture while the lock covers the screen: drag-up unlocks,
    /// taps hit the bottom actions (or the power menu when it floats above
    /// the lock), back dismisses the menu overlay.
    fn lock_gesture(&mut self, ev: GestureEvent) {
        let h = self.output.height as f32;
        match ev {
            GestureEvent::Started { pos, .. } => {
                self.lock.begin_drag();
                if self.power.menu_over_lock {
                    self.power_menu_press(pos);
                }
            }
            GestureEvent::Progress { dy, .. } => {
                if !self.power.menu_over_lock {
                    self.lock.drag(dy, h);
                }
            }
            GestureEvent::Released { vy, .. } | GestureEvent::NavReleased { vy, .. } => {
                let method = self.app_prefs.prefs().lock_method.clone();
                self.lock.release(vy, h, &method);
                self.power_menu_release();
            }
            GestureEvent::Cancelled => {
                self.lock.cancel_drag();
                self.power_menu_release();
            }
            GestureEvent::Recognized(Gesture::Tap { pos }) => {
                self.lock.cancel_drag();
                if self.power.menu_over_lock {
                    self.power_menu_tap(pos, true);
                } else {
                    self.lock_tap(pos);
                }
            }
            GestureEvent::Recognized(Gesture::EdgeBack { .. }) if self.power.menu_over_lock => {
                self.lock.cancel_drag();
                self.power.menu_over_lock = false;
            }
            // The Recents preview means nothing on the lock screen.
            GestureEvent::NavPaused | GestureEvent::NavResumed => {}
            _ => self.lock.cancel_drag(),
        }
        self.needs_present = true;
    }
}
