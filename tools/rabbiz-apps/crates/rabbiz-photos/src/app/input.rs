//! Touch gestures, keyboard navigation, and `AppEvent` dispatch for
//! `rabbiz-photos`.

use std::time::Instant;

use de_api::{Point, Rect};
use rabbiz_app_kit::ui;
use rabbiz_app_kit::{AppEvent, AppFrame, TextInputCtl};

use crate::layout;
use crate::viewer;

use super::{Gesture, PhotosApp, Screen, Touch, PUMP_EVENTS};

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

    fn touch_down(&mut self, id: i32, pos: Point) {
        // Single-finger model: ignore extra touches while one is tracked.
        if self.touch.is_some() {
            return;
        }
        match self.screen {
            Screen::Grid => {
                self.pressed_cell = layout::hit_cell(&self.layout, self.view_top, self.scroll, pos);
                self.scroll_start = self.scroll;
                self.scroll_down_y = pos.y;
                self.touch = Some(self.new_touch(id, pos));
            }
            Screen::Viewer => {
                let back = Rect::new(ui::MARGIN, ui::TOP_SAFE + 12, 120, 48);
                if back.contains(pos) {
                    self.back_pressed = true;
                    return;
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
            }
        }
        match t.mode {
            Gesture::Scroll => {
                let dy = pos.y - self.scroll_down_y;
                self.scroll =
                    layout::clamp_scroll(self.scroll_start - dy as f32, self.max_scroll());
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
        if self.back_pressed {
            self.back_pressed = false;
            self.close_viewer();
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
                let moved =
                    (pos.x - t.down.x).abs().max((pos.y - t.down.y).abs()) > viewer::TOUCH_SLOP;
                if !moved {
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
                    // Tap: double-tap zoom toggle, else arm the double-tap.
                    let now = Instant::now();
                    if viewer::is_double_tap(self.last_up, now, pos) {
                        self.toggle_zoom(pos);
                    }
                    self.last_up = Some((now, pos));
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
                        // u sign = content direction: positive = previous.
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
