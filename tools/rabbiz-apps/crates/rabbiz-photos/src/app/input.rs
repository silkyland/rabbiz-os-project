//! Touch gestures, keyboard navigation, and `AppEvent` dispatch for
//! `rabbiz-photos`.

use std::time::{Duration, Instant};

use de_api::{Point, Rect, Size};
use rabbiz_app_kit::widgets::{self, DialogLayout};
use rabbiz_app_kit::{AppEvent, AppFrame, TextInputCtl};

use crate::layout;
use crate::viewer;

use super::render::{header_action_rect, rescan_rect, viewer_chrome};
use super::{Gesture, PhotosApp, Screen, Touch, PUMP_EVENTS, TOAST_MS};

impl PhotosApp {
    fn new_touch(&self, id: i32, pos: Point) -> Touch {
        let now = Instant::now();
        Touch {
            id,
            down: pos,
            anchor: pos,
            prev: pos,
            mode: Gesture::Undecided,
            last_t: now,
            vx: 0.0,
        }
    }

    /// Dialog geometry must match `widgets::dialog` in `render_viewer`.
    fn delete_dialog_layout(&self) -> DialogLayout {
        let name = self.current_name();
        widgets::dialog_layout(
            Size::new(self.cur_w.max(1), self.cur_h.max(1)),
            &[name.as_str()],
            &["Cancel", "Delete"],
        )
    }

    fn set_toast(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), Instant::now() + Duration::from_millis(TOAST_MS)));
    }

    fn touch_down(&mut self, id: i32, pos: Point) {
        // Single-finger model: ignore extra touches while one is tracked.
        if self.touch.is_some() {
            return;
        }
        match self.screen {
            Screen::Grid => {
                if header_action_rect(self.cur_w).contains(pos) {
                    self.refresh_pressed = true;
                    return;
                }
                if self.photos.is_empty() {
                    let area = Rect::new(0, self.view_top, self.cur_w, self.view_h);
                    if rescan_rect(area).contains(pos) {
                        self.rescan_pressed = true;
                        return;
                    }
                }
                self.pressed_cell =
                    layout::hit_cell(&self.layout, self.view_top, self.sv.offset(), pos);
                self.sv.on_touch_down(pos.y as f32, self.now_ms());
                self.touch = Some(self.new_touch(id, pos));
            }
            Screen::Viewer => {
                if self.del_dialog {
                    let l = self.delete_dialog_layout();
                    if let Some(i) = widgets::dialog_action_at(&l, pos) {
                        self.del_pressed = Some(i);
                    } else if widgets::dialog_on_scrim(&l, Size::new(self.cur_w, self.cur_h), pos) {
                        self.del_scrim = true;
                    }
                    return;
                }
                // Hidden chrome isn't tappable — those taps toggle it back.
                if self.chrome_vis {
                    let vc = viewer_chrome(self.cur_w, self.cur_h, self.viewer_rise());
                    if vc.back.contains(pos) {
                        self.back_pressed = true;
                        return;
                    }
                    if vc.info.contains(pos) {
                        self.info_pressed = true;
                        return;
                    }
                    if vc.delete.contains(pos) {
                        self.delete_pressed = true;
                        return;
                    }
                }
                self.touch = Some(self.new_touch(id, pos));
            }
        }
    }

    fn touch_move(&mut self, id: i32, pos: Point) {
        let Some(mut t) = self.touch else { return };
        if t.id != id {
            return;
        }
        if t.mode == Gesture::Undecided {
            let dx = pos.x - t.down.x;
            let dy = pos.y - t.down.y;
            if dx.abs().max(dy.abs()) > viewer::TOUCH_SLOP {
                t.mode = match self.screen {
                    Screen::Grid => Gesture::Scroll,
                    Screen::Viewer => {
                        if self.v_zoom > 1.001 {
                            Gesture::Pan
                        } else if dx.abs() > dy.abs() * 6 / 5 {
                            Gesture::Swipe
                        } else {
                            Gesture::Ignore
                        }
                    }
                };
                t.anchor = pos;
                if t.mode == Gesture::Scroll {
                    // A fling/scroll cancels the cell press flash.
                    self.pressed_cell = None;
                }
            }
        }
        match t.mode {
            Gesture::Scroll => {
                self.sv.on_touch_move(pos.y as f32, self.now_ms());
            }
            Gesture::Pan => {
                self.v_pan.0 += (pos.x - t.prev.x) as f32;
                self.v_pan.1 += (pos.y - t.prev.y) as f32;
                self.clamp_pan_now();
            }
            Gesture::Swipe => {
                let raw = (pos.x - t.down.x) as f32;
                let at_start = self.v_index == 0 && raw > 0.0;
                let at_end = self.v_index + 1 >= self.photos.len() && raw < 0.0;
                self.drag_dx = if at_start || at_end {
                    viewer::rubber_band(raw)
                } else {
                    raw
                };
            }
            _ => {}
        }
        // Smoothed horizontal velocity for flick detection.
        let now = Instant::now();
        let dt = now.duration_since(t.last_t).as_secs_f32() * 1000.0;
        if dt > 0.5 {
            let inst = (pos.x - t.prev.x) as f32 / dt;
            t.vx = t.vx * 0.7 + inst.clamp(-4.0, 4.0) * 0.3;
            t.last_t = now;
        }
        t.prev = pos;
        self.touch = Some(t);
    }

    fn touch_up(&mut self, id: i32, pos: Point) {
        // Dialog buttons win over everything else.
        if let Some(i) = self.del_pressed.take() {
            if widgets::dialog_action_at(&self.delete_dialog_layout(), pos) == Some(i) {
                self.del_dialog = false;
                if i == 1 {
                    self.delete_current();
                }
            }
            return;
        }
        if self.del_scrim {
            self.del_scrim = false;
            self.del_dialog = false;
            return;
        }
        if self.back_pressed {
            self.back_pressed = false;
            self.close_viewer();
            return;
        }
        if self.refresh_pressed {
            self.refresh_pressed = false;
            self.rescan();
            self.set_toast("Library updated");
            return;
        }
        if self.rescan_pressed {
            self.rescan_pressed = false;
            self.rescan();
            self.set_toast("Library updated");
            return;
        }
        let chrome = viewer_chrome(self.cur_w, self.cur_h, self.viewer_rise());
        if std::mem::take(&mut self.info_pressed) {
            if chrome.info.contains(pos) {
                self.show_info = !self.show_info;
                if self.show_info && self.info_dimensions.is_none() {
                    self.info_dimensions = self
                        .photos
                        .get(self.v_index)
                        .and_then(|photo| crate::headless::image_dimensions(&photo.path));
                }
            }
            return;
        }
        if std::mem::take(&mut self.delete_pressed) {
            if chrome.delete.contains(pos) {
                self.del_dialog = true;
            }
            return;
        }
        let Some(t) = self.touch.take() else { return };
        if t.id != id {
            // Stale up for an ignored second finger — restore tracking.
            self.touch = Some(t);
            return;
        }
        match self.screen {
            Screen::Grid => {
                // Releases a drag + may launch a fling; returns was-drag.
                self.sv.on_touch_up(self.now_ms());
                if t.mode != Gesture::Scroll {
                    if let Some(idx) = self.pressed_cell {
                        self.open_viewer(idx);
                    }
                }
                self.pressed_cell = None;
            }
            Screen::Viewer => match t.mode {
                Gesture::Swipe => {
                    let dx = (pos.x - t.down.x) as f32;
                    if let Some(dir) = viewer::page_dir(dx, t.vx, self.cur_w.max(1)) {
                        // dir +1 = finger right = previous photo.
                        self.commit_page(-dir, self.drag_dx);
                    } else {
                        self.settle = Some((self.drag_dx, 0.0));
                    }
                    self.drag_dx = 0.0;
                }
                Gesture::Undecided => {
                    // Double-tap zooms (and does not toggle chrome); a single
                    // tap fades the overlay chrome in or out — the AOSP
                    // contract where two quick taps net "zoom, chrome as-is".
                    let now = Instant::now();
                    if viewer::is_double_tap(self.last_up, now, pos) {
                        self.toggle_zoom(pos);
                        self.toggle_chrome();
                        self.last_up = None;
                    } else {
                        self.toggle_chrome();
                        self.last_up = Some((now, pos));
                    }
                }
                _ => {}
            },
        }
    }

    pub(super) fn handle_app_event(&mut self, event: AppEvent, _ti: &TextInputCtl) -> AppFrame {
        match event {
            AppEvent::TouchDown { id, pos } => {
                self.pump(PUMP_EVENTS);
                self.touch_down(id, pos);
                AppFrame {
                    dirty: true,
                    animating: self.busy(),
                    quit: false,
                }
            }
            AppEvent::TouchMove { id, pos } => {
                self.pump(PUMP_EVENTS);
                self.touch_move(id, pos);
                AppFrame {
                    dirty: true,
                    animating: self.busy(),
                    quit: false,
                }
            }
            AppEvent::TouchUp { id, pos } => {
                self.touch_up(id, pos);
                self.pump(PUMP_EVENTS);
                AppFrame {
                    dirty: true,
                    animating: self.busy(),
                    quit: false,
                }
            }
            AppEvent::Key { keysym, .. } => {
                if self.screen == Screen::Viewer {
                    match keysym {
                        0xff1b | 0xff08 => self.close_viewer(), // Esc / Backspace
                        // Keyboard pages use the same content-space sign as a
                        // rightward finger drag (u > 0 = previous).
                        0xff51 => self.commit_page(-1, 1.0),
                        0xff53 => self.commit_page(1, -1.0),
                        _ => {}
                    }
                }
                AppFrame {
                    dirty: true,
                    animating: self.busy(),
                    quit: false,
                }
            }
            AppEvent::Tick(dt) => self.tick(dt),
            _ => AppFrame::default(),
        }
    }
}
