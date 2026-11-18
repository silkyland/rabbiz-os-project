//! libinput event → seat dispatch (M1-T1/M1-T8) + shell gestures (M3-T6).
//!
//! Every touch event is also fed to the shell's `GestureTracker`: contacts
//! starting in a system zone (nav bar, top edge, side edges) are claimed by
//! the shell and never reach the client, so a home swipe doesn't scroll the
//! app underneath. Content-zone touches reach the client as usual; the
//! tracker sees them too so taps still resolve when the shell is in the
//! foreground.
//!
//! Touch carries libinput timestamps through to the seat so gesture code
//! can compute release velocity; the power key spawns `a21s-power-key
//! toggle` detached on release.

use smithay::reexports::wayland_server::Resource;
use std::process::Command;

use de_ui::gestures::{Gesture, GestureEvent, Zone};

use smithay::backend::input::{
    AbsolutePositionEvent, Axis, AxisSource, Event, InputBackend, InputEvent, KeyState,
    KeyboardKeyEvent, PointerAxisEvent, PointerButtonEvent, PointerMotionEvent, TouchEvent as _,
};
use smithay::desktop::WindowSurfaceType;
use smithay::input::keyboard::{FilterResult, Keysym, ModifiersState};
use smithay::input::pointer::{AxisFrame, ButtonEvent, MotionEvent, RelativeMotionEvent};
use smithay::input::touch::{DownEvent, MotionEvent as TouchMotion, UpEvent};
use smithay::utils::{Logical, Point, Size, SERIAL_COUNTER as SCOUNTER};

use crate::state::De;

impl De {
    /// Dispatch one libinput event into the seat.
    pub fn process_input_event<B: InputBackend>(&mut self, event: InputEvent<B>) {
        // M1 bring-up aid: DE_INPUT_DEBUG=1 logs every backend event.
        if std::env::var_os("DE_INPUT_DEBUG").is_some() {
            eprintln!("input: {}", variant_name::<B>(&event));
        }
        match event {
            InputEvent::Keyboard { event } => self.on_keyboard_key::<B>(event),
            InputEvent::TouchDown { event } => self.on_touch_down::<B>(event),
            InputEvent::TouchMotion { event } => self.on_touch_motion::<B>(event),
            InputEvent::TouchUp { event } => self.on_touch_up::<B>(event),
            InputEvent::PointerMotion { event } => {
                let pos = self.pointer.current_location() + event.delta();
                self.pointer_motion::<B>(event.time_msec(), pos, event.delta())
            }
            InputEvent::PointerMotionAbsolute { event } => {
                let pos = event.position_transformed(Size::from((
                    self.output.width as i32,
                    self.output.height as i32,
                )));
                let delta = pos - self.pointer.current_location();
                self.pointer_motion::<B>(event.time_msec(), pos, delta)
            }
            InputEvent::PointerButton { event } => {
                let serial = SCOUNTER.next_serial();
                let ptr = self.pointer.clone();
                ptr.button(
                    self,
                    &ButtonEvent {
                        button: event.button_code(),
                        state: event.state(),
                        serial,
                        time: event.time_msec(),
                    },
                );
            }
            InputEvent::PointerAxis { event } => {
                let mut frame = AxisFrame::new(event.time_msec()).source(event.source());
                let v = event.amount(Axis::Vertical).unwrap_or(0.0);
                let h = event.amount(Axis::Horizontal).unwrap_or(0.0);
                frame = frame
                    .relative_direction(Axis::Vertical, event.relative_direction(Axis::Vertical))
                    .relative_direction(
                        Axis::Horizontal,
                        event.relative_direction(Axis::Horizontal),
                    );
                if event.source() == AxisSource::Finger {
                    frame = frame.value(Axis::Vertical, v).value(Axis::Horizontal, h);
                } else {
                    frame = frame
                        .v120(
                            Axis::Vertical,
                            event.amount_v120(Axis::Vertical).unwrap_or(0.0) as i32,
                        )
                        .v120(
                            Axis::Horizontal,
                            event.amount_v120(Axis::Horizontal).unwrap_or(0.0) as i32,
                        );
                }
                if matches!(event.source(), AxisSource::Finger | AxisSource::Continuous)
                    && v == 0.0
                    && h == 0.0
                {
                    frame = frame.stop(Axis::Vertical).stop(Axis::Horizontal);
                }
                let ptr = self.pointer.clone();
                ptr.axis(self, frame);
            }
            _ => {}
        }
    }

    fn pointer_motion<B: InputBackend>(
        &mut self,
        time: u32,
        pos: Point<f64, Logical>,
        delta: Point<f64, Logical>,
    ) {
        let serial = SCOUNTER.next_serial();
        let under = self.surface_under(pos).map(|(s, l)| (s, l.to_f64()));
        let ptr = self.pointer.clone();
        ptr.motion(
            self,
            under.clone(),
            &MotionEvent {
                location: pos,
                serial,
                time,
            },
        );
        ptr.relative_motion(
            self,
            under,
            &RelativeMotionEvent {
                delta,
                delta_unaccel: delta,
                utime: time as u64 * 1000,
            },
        );
    }

    fn on_keyboard_key<B: InputBackend>(&mut self, evt: B::KeyboardKeyEvent) {
        let serial = SCOUNTER.next_serial();
        let keycode = evt.key_code();
        let state = evt.state();
        let time = evt.time_msec();
        let Some(keyboard) = self.seat.get_keyboard() else {
            return;
        };
        keyboard.input(
            self,
            keycode,
            state,
            serial,
            time,
            |_de, _mods: &ModifiersState, keysym| {
                let raw = keysym.raw_syms().first().copied();
                // Power button: act on release only, detached so the 300 ms
                // combo guard in a21s-power-key never stalls the loop.
                if raw == Some(Keysym::XF86_PowerOff) && state == KeyState::Released {
                    let _ = Command::new("setsid")
                        .args(["a21s-power-key", "toggle"])
                        .spawn();
                    return FilterResult::Intercept(());
                }
                FilterResult::Forward
            },
        );
    }

    fn on_touch_down<B: InputBackend>(&mut self, evt: B::TouchDownEvent) {
        let pos = evt.position_transformed(Size::from((
            self.output.width as i32,
            self.output.height as i32,
        )));
        self.last_touch_pos = pos;
        // Shell gestures first: a contact starting in a system zone — or any
        // contact while a shell screen is foreground — is claimed by the
        // shell and never reaches the client.
        let zone = self.gestures.zone(api_point(pos));
        self.feed_gesture(
            i32::from(evt.slot()),
            de_api::input::TouchPhase::Down,
            pos,
            evt.time_msec(),
        );
        self.touch_claimed =
            zone != Zone::Content || matches!(self.foreground, crate::state::Foreground::Shell);
        if self.touch_claimed {
            return;
        }
        let serial = SCOUNTER.next_serial();
        let under = self.surface_under(pos).map(|(s, l)| (s, l.to_f64()));
        if std::env::var_os("DE_INPUT_DEBUG").is_some() {
            let info = under.as_ref().map(|(s, l)| {
                let alive = self
                    .display_handle
                    .get_client(s.id())
                    .map(|_| "alive")
                    .unwrap_or("DEAD");
                format!("{:?}@{l:?} client={alive}", s.id())
            });
            eprintln!(
                "touch_down pos={pos:?} under={info:?} windows={} kbd={}",
                self.space.elements().count(),
                self.seat.get_keyboard().is_some()
            );
        }
        if let Some((surface, _)) = &under {
            if let Some(kbd) = self.seat.get_keyboard() {
                kbd.set_focus(self, Some(surface.clone()), serial);
            }
        }
        let touch = self.touch.clone();
        touch.down(
            self,
            under,
            &DownEvent {
                slot: evt.slot(),
                location: pos,
                serial,
                time: evt.time_msec(),
            },
        );
    }

    fn on_touch_motion<B: InputBackend>(&mut self, evt: B::TouchMotionEvent) {
        let pos = evt.position_transformed(Size::from((
            self.output.width as i32,
            self.output.height as i32,
        )));
        self.last_touch_pos = pos;
        self.feed_gesture(
            i32::from(evt.slot()),
            de_api::input::TouchPhase::Motion,
            pos,
            evt.time_msec(),
        );
        if self.touch_claimed {
            return;
        }
        let under = self.surface_under(pos).map(|(s, l)| (s, l.to_f64()));
        let touch = self.touch.clone();
        touch.motion(
            self,
            under,
            &TouchMotion {
                slot: evt.slot(),
                location: pos,
                time: evt.time_msec(),
            },
        );
    }

    fn on_touch_up<B: InputBackend>(&mut self, evt: B::TouchUpEvent) {
        let pos = self.last_touch_pos;
        self.feed_gesture(
            i32::from(evt.slot()),
            de_api::input::TouchPhase::Up,
            pos,
            evt.time_msec(),
        );
        if std::mem::take(&mut self.touch_claimed) {
            self.needs_present = true;
            return;
        }
        let serial = SCOUNTER.next_serial();
        let touch = self.touch.clone();
        touch.up(
            self,
            &UpEvent {
                slot: evt.slot(),
                serial,
                time: evt.time_msec(),
            },
        );
    }

    /// Feed the gesture tracker and act on what it emits.
    fn feed_gesture(
        &mut self,
        slot: i32,
        phase: de_api::input::TouchPhase,
        pos: Point<f64, Logical>,
        time_ms: u32,
    ) {
        let ev = de_api::input::TouchEvent {
            slot,
            phase,
            pos: api_point(pos),
            time_ms: u64::from(time_ms),
        };
        if let Some(g) = self.gestures.feed(&ev) {
            self.on_gesture_event(g);
        }
    }

    fn on_gesture_event(&mut self, ev: GestureEvent) {
        use crate::state::Foreground;
        use de_ui::nav::Screen;
        let GestureEvent::Recognized(g) = ev else {
            return;
        };
        match g {
            Gesture::Home => self.show_shell(Screen::Home),
            Gesture::Recents => self.show_shell(Screen::Recents),
            Gesture::ShadePull => self.show_shell(Screen::Shade),
            Gesture::EdgeBack { .. } => {
                if matches!(self.foreground, Foreground::App) {
                    // Phone v1: back dismisses the topmost fullscreen app —
                    // the shell appears when the last window unmaps.
                    self.close_topmost();
                } else {
                    self.nav.back();
                    self.shell_dirty = true;
                    self.needs_present = true;
                }
            }
            Gesture::PageSwitch { dir } => {
                if matches!(self.foreground, Foreground::Shell) && self.nav.current == Screen::Home
                {
                    let pages = de_ui::home::pages_for(
                        self.catalog.home_order(self.app_prefs.prefs()).len(),
                    );
                    let next = (self.home_page as i32 - dir).clamp(0, pages as i32 - 1);
                    if next as usize != self.home_page {
                        self.home_page = next as usize;
                        self.shell_dirty = true;
                        self.needs_present = true;
                    }
                }
            }
            Gesture::Tap { pos } => self.on_shell_tap(pos),
            // Long-press is the icon-context-menu hook (hide/pin) — no-op
            // until the drawer action sheet lands.
            Gesture::LongPress { .. } => {}
        }
    }

    /// Switch the screen to a shell surface (app keeps running).
    fn show_shell(&mut self, screen: de_ui::nav::Screen) {
        use crate::state::Foreground;
        if matches!(self.foreground, Foreground::App) {
            // The window is still presented — grab its last frame for the
            // recents card before the shell takes over.
            self.capture_topmost();
        }
        self.foreground = Foreground::Shell;
        if self.nav.current != screen {
            self.nav.current = screen;
        }
        self.shell_dirty = true;
        self.needs_present = true;
    }

    /// Tap on a shell screen: icon → launch, search pill → search,
    /// recents card → back to the app.
    fn on_shell_tap(&mut self, pos: de_api::Point) {
        use crate::state::Foreground;
        use de_ui::nav::Screen;
        if !matches!(self.foreground, Foreground::Shell) {
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
                        self.try_launch(&id);
                    }
                    Some(de_ui::home::HomeHit::Dock(i)) => {
                        if let Some(id) = dock.get(i).cloned() {
                            self.try_launch(&id);
                        }
                    }
                    Some(de_ui::home::HomeHit::Search) => {
                        self.nav.go(Screen::Search);
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
                    self.try_launch(&rows[i].id.clone());
                }
            }
            Screen::Recents => {
                // Tap anywhere → back to the last app.
                if self.space.elements().next().is_some() {
                    self.foreground = Foreground::App;
                    self.needs_present = true;
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
                    .map(|e| e.name.clone())
                    .unwrap_or_else(|| id.clone()),
                subtitle: String::new(),
                icon: None,
            })
            .collect()
    }

    /// Spawn an app through the launch contract; failures go to the error
    /// screen, the pending launch resolves on `new_toplevel`.
    pub fn try_launch(&mut self, desktop_id: &str) {
        match self.launch.begin(desktop_id) {
            Ok(pid) => {
                eprintln!("launch: {desktop_id} pid={pid}");
                self.launch_error = None;
            }
            Err(e) => {
                self.launch_error = Some(format!("{desktop_id}: {e}"));
                self.foreground = crate::state::Foreground::Shell;
                self.nav.go(de_ui::nav::Screen::Error);
                self.shell_dirty = true;
                self.needs_present = true;
            }
        }
    }

    /// Close the topmost window (edge-back / close-all).
    fn close_topmost(&mut self) {
        if let Some(top) = self.space.elements().last().cloned() {
            if let Some(toplevel) = top.toplevel() {
                toplevel.send_close();
            }
        }
    }

    /// Grab the topmost window's committed shm frame into the recents
    /// snapshot cache (no-op for dmabuf/EGL clients — they keep the icon
    /// placeholder until a capture seam exists for them).
    fn capture_topmost(&mut self) {
        let Some(top) = self.space.elements().last().cloned() else {
            return;
        };
        let app_id = top
            .toplevel()
            .map(|t| crate::snapshots::window_app_id(&t))
            .unwrap_or_default();
        if app_id.is_empty() {
            return;
        }
        if let Some(px) = crate::snapshots::capture_window(&top) {
            let thumb = de_paint::wallpaper::cover_scale(&px, crate::shell::RECENT_THUMB);
            self.snapshots.record(&app_id, thumb);
        } else {
            self.snapshots.touch(&app_id);
        }
    }

    /// App ids of all mapped windows (topmost last).
    pub fn window_app_ids(&self) -> Vec<String> {
        self.space
            .elements()
            .filter_map(|w| w.toplevel().map(|t| crate::snapshots::window_app_id(&t)))
            .filter(|id| !id.is_empty())
            .collect()
    }

    /// Top-most surface under `pos` (toplevel incl. popups/subsurfaces).
    pub fn surface_under(
        &self,
        pos: Point<f64, Logical>,
    ) -> Option<(
        smithay::reexports::wayland_server::protocol::wl_surface::WlSurface,
        Point<i32, Logical>,
    )> {
        let (window, loc) = self.space.element_under(pos)?;
        let rel = pos - loc.to_f64();
        window
            .surface_under(rel, WindowSurfaceType::ALL)
            .map(|(s, l)| (s, l + loc))
    }
}

/// smithay logical point → `de-api` point (physical px, i32).
fn api_point(p: Point<f64, Logical>) -> de_api::Point {
    de_api::Point::new(p.x as i32, p.y as i32)
}

/// Variant name for the `DE_INPUT_DEBUG` bring-up log.
fn variant_name<B: InputBackend>(e: &InputEvent<B>) -> &'static str {
    match e {
        InputEvent::DeviceAdded { .. } => "device_added",
        InputEvent::DeviceRemoved { .. } => "device_removed",
        InputEvent::Keyboard { .. } => "keyboard",
        InputEvent::PointerMotion { .. } => "pointer_motion",
        InputEvent::PointerMotionAbsolute { .. } => "pointer_motion_abs",
        InputEvent::PointerButton { .. } => "pointer_button",
        InputEvent::PointerAxis { .. } => "pointer_axis",
        InputEvent::TouchDown { .. } => "touch_down",
        InputEvent::TouchMotion { .. } => "touch_motion",
        InputEvent::TouchUp { .. } => "touch_up",
        InputEvent::TouchCancel { .. } => "touch_cancel",
        InputEvent::TouchFrame { .. } => "touch_frame",
        InputEvent::TabletToolAxis { .. } => "tablet_axis",
        InputEvent::TabletToolProximity { .. } => "tablet_prox",
        InputEvent::TabletToolTip { .. } => "tablet_tip",
        InputEvent::TabletToolButton { .. } => "tablet_button",
        InputEvent::SwitchToggle { .. } => "switch",
        _ => "other",
    }
}
