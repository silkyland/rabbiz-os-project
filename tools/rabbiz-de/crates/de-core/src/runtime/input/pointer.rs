//! Pointer event dispatch (motion, buttons, wheel/axis) shared by libinput
//! and the virtual pointer protocol.

use std::time::Instant;

use de_ui::gestures::Zone;
use smithay::input::pointer::{AxisFrame, ButtonEvent, MotionEvent, RelativeMotionEvent};
use smithay::utils::{Logical, Point, SERIAL_COUNTER as SCOUNTER};

use super::{api_point, SurfaceHit};
use crate::state::De;

impl De {
    /// One pointer button event — shared by libinput and the virtual
    /// pointer protocol (a21s-wayvnc / Rabbiz Mirror).
    ///
    /// Click-to-focus: a press that lands on a client surface moves
    /// keyboard/pointer focus to it (the touch model has no equivalent —
    /// touch stays "last touched surface" via the down handler). A press on
    /// a shell-owned surface is consumed by the shell: a nearby release of
    /// the same button synthesises the tap the touch path would have made,
    /// and no button event leaks to a client underneath.
    pub(crate) fn pointer_button(
        &mut self,
        time: u32,
        button: u32,
        state: smithay::backend::input::ButtonState,
    ) {
        self.pointer_active = true;
        // A click between Super-down and Super-up means Super is a real
        // modifier, not the launcher key.
        self.launcher_pending = false;
        let pressed = state == smithay::backend::input::ButtonState::Pressed;
        if self.menu_pointer_button(button, pressed, self.pointer.current_location()) {
            return;
        }
        if self.sleeping() {
            // The lock/power screens are touch-only; swallow pointer input
            // so nothing reaches a client under the lock.
            return;
        }
        let pos = self.pointer.current_location();
        let serial = SCOUNTER.next_serial();
        let ptr = self.pointer.clone();
        if pressed {
            if self.pointer_shell_owned(pos) {
                self.pointer_press = Some((pos, button));
                return;
            }
            if let Some(target) = self.surface_under(pos).and_then(|hit| hit.keyboard_target) {
                self.focus_surface(Some(target), serial);
            }
        } else if let Some((down_pos, down_button)) = self.pointer_press.take() {
            let moved = ((pos.x - down_pos.x).powi(2) + (pos.y - down_pos.y).powi(2)).sqrt();
            if down_button == button && moved <= 12.0 {
                self.pointer_tap(down_pos, time);
            }
            return;
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
        self.pointer_active = true;
        let pos = self.pointer.current_location();
        if self.shell_axis_scroll(&frame, pos) {
            // The shell consumed the scroll (drawer/recents/home pager or a
            // consume-only surface) — the client must not see it too.
            return;
        }
        let ptr = self.pointer.clone();
        ptr.axis(self, frame);
    }

    /// Is the point owned by the shell rather than a Wayland client?
    ///
    /// Mirrors `claim_touch`: while the shell is foreground every point is
    /// shell-owned except interactive layer surfaces (the OSK); while an
    /// app is foreground only the system zones (nav bar, top edge, side
    /// edges) are shell-owned — a click in the content area is the app's.
    fn pointer_shell_owned(&self, pos: Point<f64, Logical>) -> bool {
        if self.panels.shown() || self.locked() || self.sleeping() || self.menu_is_open() {
            return true;
        }
        match self.surface_under(pos) {
            // Interactive layer (OSK) sits above everything, shell included.
            Some(hit) if hit.above_windows => false,
            Some(_) => {
                // A window surface: in an app only system zones belong to
                // the shell; on shell screens surface_under already
                // returned None except for layers.
                self.gestures.zone(api_point(pos)) != Zone::Content
            }
            None => true,
        }
    }

    /// Synthesize the tap a touch would have produced (`Gesture::Tap` in
    /// `handle_gesture`): panels and the lock screen first, then the
    /// shell-screen dispatcher.
    fn pointer_tap(&mut self, pos: Point<f64, Logical>, _time: u32) {
        use crate::state::Foreground;
        let api = api_point(pos);
        if self.panels.shown() {
            self.panel_tap(api);
            return;
        }
        if self.locked() {
            if self.power.menu_over_lock {
                self.power_menu_tap(api, true);
            } else {
                self.lock_tap(api);
            }
            return;
        }
        if self.menu_is_open() || self.foreground != Foreground::Shell {
            return;
        }
        let size = de_api::Size::new(self.output.width as i32, self.output.height as i32);
        if self.shot_fx.tap(api, size, std::time::Instant::now()) {
            self.needs_present = true;
        } else if !self.release_home_drag(false) {
            self.on_shell_tap(api);
        }
    }

    /// Route a scroll frame to the shell when the pointer is over a
    /// shell-owned surface. Returns `true` when consumed.
    ///
    /// Mapping (the pointer twin of the touch drags in `handle_gesture`):
    /// - Drawer: vertical scroll moves `drawer_scroll` (same clamp the
    ///   renderer applies — stored unclamped like the touch path).
    /// - Recents: horizontal (or vertical) scroll pages the card pager.
    /// - Home: horizontal scroll pages the icon grid; vertical scroll is
    ///   consumed (no vertical list on Home).
    /// - Panels/lock/search/etc.: consumed so nothing leaks to a client
    ///   underneath.
    fn shell_axis_scroll(&mut self, frame: &AxisFrame, pos: Point<f64, Logical>) -> bool {
        if !self.pointer_shell_owned(pos) {
            return false;
        }
        let (dx, dy) = wheel_delta(frame);
        if dx == 0.0 && dy == 0.0 {
            // axis-stop / zero frames still count as "consumed" on shell
            // surfaces so clients never see phantom scroll.
            return true;
        }
        use crate::state::Foreground;
        use de_ui::nav::Screen;
        match self.nav.current {
            Screen::Drawer if self.foreground == Foreground::Shell => {
                let step = if dy.abs() >= dx.abs() { dy } else { dx };
                let next = (self.navigation.drawer_scroll + step.round() as i32).max(0);
                if next != self.navigation.drawer_scroll {
                    self.navigation.drawer_scroll = next;
                    self.shell_dirty = true;
                }
                self.needs_present = true;
            }
            Screen::Recents if self.foreground == Foreground::Shell => {
                self.recents_wheel(dx + dy);
            }
            Screen::Home if self.foreground == Foreground::Shell => {
                if dx.abs() >= dy.abs() {
                    self.home_wheel(dx);
                }
                // Vertical wheel on Home: nothing scrolls — consumed.
            }
            _ => {}
        }
        self.needs_present = true;
        true
    }

    /// One wheel notch over Recents pages the card carousel (drag + release
    /// through the same `Pager` the touch path uses, so the settle spring
    /// animates exactly like a flick).
    fn recents_wheel(&mut self, delta: f32) {
        if delta == 0.0 || self.morph.is_some() {
            return;
        }
        let size = de_api::Size::new(self.output.width as i32, self.output.height as i32);
        let stride = de_ui::recents::card_stride(size) as f32;
        if stride <= 0.0 {
            return;
        }
        // One notch = one card: drag a full stride opposite the scroll
        // direction, then release with a matching velocity.
        let dir = if delta > 0.0 { -1.0 } else { 1.0 };
        self.navigation.recents_pager.begin();
        self.navigation.recents_pager.drag(dir * stride, stride);
        self.navigation.recents_pager.release(dir * 900.0, stride);
        self.navigation.recents_last_tick = Some(Instant::now());
        self.navigation.recents_axis = None;
        self.shell_dirty = true;
        self.needs_present = true;
    }

    /// Pointer motion + hit-test — shared by libinput and the virtual
    /// pointer protocol.
    pub(crate) fn pointer_motion(
        &mut self,
        time: u32,
        pos: Point<f64, Logical>,
        delta: Point<f64, Logical>,
    ) {
        self.pointer_active = true;
        // The cursor sprite is a composited element: motion must schedule
        // a frame or it would only move on unrelated damage.
        self.needs_present = true;
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
}

/// Scroll delta in logical px for shell surfaces, `(dx, dy)` with positive
/// = scroll down/right. `AxisFrame.axis`/`v120` are `(horizontal,
/// vertical)`; wheel events arrive in 1/120-notch units → 56 px per notch,
/// while finger/continuous sources carry their raw values through.
fn wheel_delta(frame: &AxisFrame) -> (f32, f32) {
    const NOTCH_PX: f32 = 56.0;
    if let Some((h120, v120)) = frame.v120 {
        (
            h120 as f32 / 120.0 * NOTCH_PX,
            v120 as f32 / 120.0 * NOTCH_PX,
        )
    } else {
        (frame.axis.0 as f32, frame.axis.1 as f32)
    }
}
