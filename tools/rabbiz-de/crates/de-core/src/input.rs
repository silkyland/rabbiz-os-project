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

use smithay::reexports::wayland_server::{protocol::wl_surface::WlSurface, Resource};
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

pub(crate) struct SurfaceHit {
    pub surface: WlSurface,
    pub location: Point<f64, Logical>,
    pub keyboard_target: Option<WlSurface>,
    pub above_windows: bool,
}

impl SurfaceHit {
    fn into_focus(self) -> (WlSurface, Point<f64, Logical>) {
        (self.surface, self.location)
    }
}

impl De {
    /// Dispatch one libinput event into the seat.
    pub fn process_input_event<B: InputBackend>(&mut self, event: InputEvent<B>) {
        // M1 bring-up aid: DE_INPUT_DEBUG=1 logs every backend event.
        if std::env::var_os("DE_INPUT_DEBUG").is_some() {
            eprintln!("input: {}", variant_name::<B>(&event));
        }
        match event {
            InputEvent::Keyboard { event } => self.on_keyboard_key::<B>(&event),
            InputEvent::TouchDown { event } => self.on_touch_down::<B>(&event),
            InputEvent::TouchMotion { event } => self.on_touch_motion::<B>(&event),
            InputEvent::TouchUp { event } => self.on_touch_up::<B>(&event),
            InputEvent::TouchCancel { event } => {
                self.feed_gesture(
                    i32::from(event.slot()),
                    de_api::input::TouchPhase::Cancel,
                    self.last_touch_pos,
                    event.time_msec(),
                );
                if !std::mem::take(&mut self.touch_claimed) {
                    self.touch.clone().cancel(self);
                }
            }
            InputEvent::PointerMotion { event } => {
                let pos = self.pointer.current_location() + event.delta();
                self.pointer_motion(event.time_msec(), pos, event.delta());
            }
            InputEvent::PointerMotionAbsolute { event } => {
                let pos = event.position_transformed(Size::from((
                    self.output.width as i32,
                    self.output.height as i32,
                )));
                let delta = pos - self.pointer.current_location();
                self.pointer_motion(event.time_msec(), pos, delta);
            }
            InputEvent::PointerButton { event } => self.pointer_button(
                event.time_msec(),
                event.button_code(),
                event.state(),
            ),
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
                self.pointer_axis_frame(frame);
            }
            _ => {}
        }
    }

    /// One pointer button event — shared by libinput and the virtual
    /// pointer protocol (a21s-wayvnc / Rabbiz Mirror).
    pub(crate) fn pointer_button(
        &mut self,
        time: u32,
        button: u32,
        state: smithay::backend::input::ButtonState,
    ) {
        let pressed = state == smithay::backend::input::ButtonState::Pressed;
        if self.menu_pointer_button(button, pressed, self.pointer.current_location()) {
            return;
        }
        let serial = SCOUNTER.next_serial();
        let ptr = self.pointer.clone();
        if state == smithay::backend::input::ButtonState::Pressed {
            if let Some(target) = self
                .surface_under(ptr.current_location())
                .and_then(|hit| hit.keyboard_target)
            {
                self.focus_surface(Some(target), serial);
            }
        }
        ptr.button(
            self,
            &ButtonEvent {
                button,
                state,
                serial,
                time,
            },
        );
    }

    /// One axis frame — shared by libinput and the virtual pointer protocol.
    pub(crate) fn pointer_axis_frame(&mut self, frame: AxisFrame) {
        let ptr = self.pointer.clone();
        ptr.axis(self, frame);
    }

    /// Pointer motion + hit-test — shared by libinput and the virtual
    /// pointer protocol.
    pub(crate) fn pointer_motion(&mut self, time: u32, pos: Point<f64, Logical>, delta: Point<f64, Logical>) {
        self.menu_pointer_moved(pos);
        let serial = SCOUNTER.next_serial();
        let under = self.surface_under(pos).map(SurfaceHit::into_focus);
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
                utime: u64::from(time) * 1000,
            },
        );
    }

    fn on_keyboard_key<B: InputBackend>(&mut self, evt: &B::KeyboardKeyEvent) {
        let serial = SCOUNTER.next_serial();
        let keycode = evt.key_code();
        let state = evt.state();
        let time = evt.time_msec();
        if state == KeyState::Pressed && self.menu_key(keycode.raw()) {
            return;
        }
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

    fn on_touch_down<B: InputBackend>(&mut self, evt: &B::TouchDownEvent) {
        let pos = evt.position_transformed(Size::from((
            self.output.width as i32,
            self.output.height as i32,
        )));
        self.last_touch_pos = pos;
        // Shell gestures first: a contact starting in a system zone — or any
        // contact while a shell screen is foreground — is claimed by the
        // shell and never reaches the client.
        let hit = self.surface_under(pos);
        // An open context menu is dismissed by a touch outside it, and that touch is consumed.
        if self.dismiss_popup_for_touch(hit.as_ref()) || self.menu_touch_down(pos) {
            self.touch_claimed = true;
            return;
        }
        let zone = self.gestures.zone(api_point(pos));
        // Top layers (including the OSK) own their interactive region.
        // Retain the nav strip as a way to leave the foreground client.
        let layer_contact = zone != Zone::NavBar && hit.as_ref().is_some_and(|h| h.above_windows);
        if !layer_contact {
            self.feed_gesture(
                i32::from(evt.slot()),
                de_api::input::TouchPhase::Down,
                pos,
                evt.time_msec(),
            );
        }
        self.touch_claimed = claim_touch(
            zone,
            self.foreground == crate::state::Foreground::Shell,
            layer_contact,
        ) || self.panel_wants_touch();
        if self.touch_claimed {
            return;
        }
        let serial = SCOUNTER.next_serial();
        if let Some(target) = hit.as_ref().and_then(|hit| hit.keyboard_target.clone()) {
            self.focus_surface(Some(target), serial);
        }
        let under = hit.map(SurfaceHit::into_focus);
        if std::env::var_os("DE_INPUT_DEBUG").is_some() {
            let info = under.as_ref().map(|(s, l)| {
                let alive = self
                    .display_handle
                    .get_client(s.id())
                    .map_or("DEAD", |_| "alive");
                format!("{:?}@{l:?} client={alive}", s.id())
            });
            eprintln!(
                "touch_down pos={pos:?} under={info:?} windows={} kbd={}",
                self.space.elements().count(),
                self.seat.get_keyboard().is_some()
            );
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

    fn on_touch_motion<B: InputBackend>(&mut self, evt: &B::TouchMotionEvent) {
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
        let under = self.surface_under(pos).map(SurfaceHit::into_focus);
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

    fn on_touch_up<B: InputBackend>(&mut self, evt: &B::TouchUpEvent) {
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
            self.on_gesture_event(g, ev.time_ms);
        }
    }

    fn on_gesture_event(&mut self, ev: GestureEvent, time_ms: u64) {
        use crate::state::Foreground;
        use de_ui::nav::Screen;
        if !matches!(ev, GestureEvent::Progress { .. } | GestureEvent::HoldDrag { .. })
            && std::env::var_os("DE_GESTURE_DEBUG").is_some()
        {
            eprintln!("gesture: {ev:?}");
        }
        let g = match ev {
            GestureEvent::Progress {
                zone: Zone::NavBar,
                dy,
                ..
            } => {
                self.follow_home_drag(dy, time_ms);
                return;
            }
            GestureEvent::Progress { zone, dy, .. } if zone != Zone::NavBar => {
                if self.panel_progress(zone == Zone::TopEdge, dy) {
                    return;
                }
                if zone == Zone::Content {
                    if let GestureEvent::Progress { dx, dy, .. } = ev {
                        self.home_drag_progress(dx, dy);
                    }
                }
                return;
            }
            GestureEvent::Started { pos, .. } => {
                self.panel_touch_started(pos);
                self.home_touch_started();
                return;
            }
            GestureEvent::Released { vx, vy, .. } => {
                if self.panels.pull.dragging() {
                    self.panel_released(vy);
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
                self.panel_cancelled();
                self.home_drag_cancelled();
                self.release_home_drag(false);
                return;
            }
            GestureEvent::Recognized(gesture) => gesture,
            GestureEvent::Progress { .. } => return,
        };
        match g {
            Gesture::Home | Gesture::EdgeBack { .. } if self.panels.shown() => self.panel_close(),
            Gesture::Home => self.show_shell(Screen::Home),
            Gesture::Recents => self.show_shell(Screen::Recents),
            Gesture::EdgeBack { .. } => {
                if matches!(self.foreground, Foreground::App) {
                    // Phone v1: back dismisses the topmost fullscreen app —
                    // the shell appears when the last window unmaps.
                    self.close_topmost();
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
                if !self.release_home_drag(false) {
                    self.on_shell_tap(pos);
                }
            }
            // Long-press is the icon-context-menu hook (hide/pin) — no-op
            // until the drawer action sheet lands.
            Gesture::LongPress { pos } => {
                if !self.home_lift_icon(pos) {
                    self.open_home_menu(pos);
                }
            }
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
        if screen != de_ui::nav::Screen::Home || !self.release_home_drag(true) {
            self.morph = None;
        }
        self.launch.cancel();
        self.foreground = Foreground::Shell;
        self.focus_foreground();
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
                        self.nav.go(Screen::Drawer);
                        self.shell_dirty = true;
                        self.needs_present = true;
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
                    self.try_launch(&rows[i].id, de_ui::search::icon_rect(size, &rows, i));
                }
            }
            Screen::Recents
                // Tap anywhere → back to the last app.
                if self.space.elements().next().is_some() => {
                    self.foreground = Foreground::App;
                    self.focus_foreground();
                    self.needs_present = true;
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
    pub(crate) fn capture_topmost(&mut self) {
        let Some(top) = self.space.elements().last().cloned() else {
            return;
        };
        let app_id = top
            .toplevel()
            .map(crate::snapshots::window_app_id)
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
    #[must_use]
    pub fn window_app_ids(&self) -> Vec<String> {
        self.space
            .elements()
            .filter_map(|w| w.toplevel().map(crate::snapshots::window_app_id))
            .filter(|id| !id.is_empty())
            .collect()
    }

    /// Hit-test in the same order used to compose the output.
    fn surface_under(&self, pos: Point<f64, Logical>) -> Option<SurfaceHit> {
        if let Some(hit) = self.layer_under(pos, true) {
            return Some(hit);
        }
        if self.foreground == crate::state::Foreground::Shell {
            return None;
        }
        if self.morph.is_none() {
            if let Some((window, loc)) = self.space.element_under(pos) {
                if let Some((surface, offset)) =
                    window.surface_under(pos - loc.to_f64(), WindowSurfaceType::ALL)
                {
                    return Some(SurfaceHit {
                        surface,
                        location: (loc + offset).to_f64(),
                        keyboard_target: window.toplevel().map(|t| t.wl_surface().clone()),
                        above_windows: false,
                    });
                }
            }
            return self.layer_under(pos, false);
        }
        None
    }
}

/// Should a new contact be claimed by the shell instead of the client?
///
/// A contact is claimed when it lands outside the client content area —
/// any system zone counts — or anywhere while a shell screen is in the
/// foreground, *unless* it landed on an interactive top/overlay layer
/// (the OSK owns its region even when the shell is up).
fn claim_touch(zone: Zone, shell_foreground: bool, layer_contact: bool) -> bool {
    !layer_contact && (zone != Zone::Content || shell_foreground)
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The claim decision decides who sees a contact. Layer surfaces with
    /// their own interactive region (OSK, overlays) always win; otherwise
    /// system zones are shell-only, and the content zone is shell-only only
    /// while a shell screen is foreground.
    #[test]
    fn claim_touch_decision_table() {
        for zone in [
            Zone::NavBar,
            Zone::TopEdge,
            Zone::EdgeLeft,
            Zone::EdgeRight,
            Zone::Content,
        ] {
            assert!(!claim_touch(zone, false, true), "layer owns {zone:?}");
            assert!(
                !claim_touch(zone, true, true),
                "layer owns {zone:?} (shell fg)"
            );
        }
        for zone in [Zone::NavBar, Zone::TopEdge, Zone::EdgeLeft, Zone::EdgeRight] {
            assert!(claim_touch(zone, false, false), "{zone:?} is shell-only");
            assert!(claim_touch(zone, true, false), "{zone:?} is shell-only");
        }
        // Content zone: reaches the client only while an app is foreground.
        assert!(!claim_touch(Zone::Content, false, false));
        assert!(claim_touch(Zone::Content, true, false));
    }
}
