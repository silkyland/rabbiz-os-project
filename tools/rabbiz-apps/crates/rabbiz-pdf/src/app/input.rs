//! Viewer navigation, gesture recognition, animation tick, and `AppState` wiring.

use std::time::Instant;

use de_api::Point;
use de_design::motion::{bezier_eval, BEZIER_STANDARD};
use rabbiz_app_kit::ui;
use rabbiz_app_kit::{AppEvent, AppFrame, AppState, Canvas, TextInputCtl, Theme};

use crate::view;

use super::{
    chrome, nav_hit, overview_view, Anim, Gesture, NavBtn, PdfApp, Screen, Slide, Touch, CLOSE_MS,
    LIST_TOP, OPEN_MS, ROW_H, SETTLE_MS, SLIDE_MS,
};

impl PdfApp {
    // ---------------- viewer navigation ----------------

    pub(super) fn clamp_pan_now(&mut self) {
        let idx = self.page;
        let (w_pt, h_pt) = self.page_pt(idx);
        let ch = chrome(self.cur_w.max(1), self.cur_h.max(1)).content;
        let base = view::fit_width_scale(ch.w, w_pt);
        self.pan = view::clamp_pan(self.pan, ch.w, ch.h, w_pt, h_pt, base, self.zoom);
    }

    /// Pan that shows the top of `idx`'s page (home position).
    pub(super) fn home_pan_for(&mut self, idx: usize) -> (f32, f32) {
        let (w_pt, h_pt) = self.page_pt(idx);
        let ch = chrome(self.cur_w.max(1), self.cur_h.max(1)).content;
        let base = view::fit_width_scale(ch.w, w_pt);
        view::home_pan(ch.h, w_pt, h_pt, base, self.zoom)
    }

    fn commit_page(&mut self, delta: i32, u: f32) {
        let n = self.page_count() as i32;
        if n == 0 {
            return;
        }
        let to = (self.page as i32 + delta).clamp(0, n - 1) as usize;
        if to == self.page {
            self.settle = Some((u.signum() * 40.0, 0.0));
            return;
        }
        let u = if u.abs() < 1.0 {
            -(delta.signum() as f32)
        } else {
            u
        };
        // A page turn resets zoom — the incoming page starts fit-to-width.
        self.zoom = 1.0;
        self.zoom_i = 0;
        self.request_page(to, self.zoom_i);
        self.slide = Some(Slide {
            from: self.page,
            to,
            u,
            t: 0.0,
        });
        self.page = to;
        self.pan = self.home_pan_for(to);
        self.drag_dx = 0.0;
        self.prefetch_neighbors();
    }

    fn toggle_zoom(&mut self, tap: Point) {
        let ch = chrome(self.cur_w, self.cur_h);
        let center = (
            ch.content.x as f32 + ch.content.w as f32 / 2.0,
            ch.content.y as f32 + ch.content.h as f32 / 2.0,
        );
        let (target, zi) = if self.zoom > 1.001 {
            (1.0, 0usize)
        } else {
            (view::DOUBLE_TAP_ZOOM, 2usize) // ZOOM_STEPS[2] == 2.0
        };
        self.pan = view::zoom_pan_at(self.pan, tap, center, self.zoom, target);
        self.zoom = target;
        self.zoom_i = zi;
        self.clamp_pan_now();
        self.prefetch_neighbors();
    }

    fn set_zoom_step(&mut self, dir: i32) {
        let z = view::zoom_step(self.zoom, dir);
        let zi = view::ZOOM_STEPS
            .iter()
            .position(|s| (*s - z).abs() < 0.01)
            .unwrap_or(0);
        self.zoom = z;
        self.zoom_i = zi;
        self.clamp_pan_now();
        self.prefetch_neighbors();
    }

    fn reset_zoom(&mut self) {
        self.zoom = 1.0;
        self.zoom_i = 0;
        self.pan = self.home_pan_for(self.page);
        self.clamp_pan_now();
        self.prefetch_neighbors();
    }

    // ---------------- gestures ----------------

    fn new_touch(&self, id: i32, pos: Point) -> Touch {
        let now = Instant::now();
        Touch {
            id,
            down: pos,
            prev: pos,
            mode: Gesture::Undecided,
            last_t: now,
            vx: 0.0,
        }
    }

    fn touch_down(&mut self, id: i32, pos: Point) {
        if self.touch.is_some() {
            return;
        }
        match self.screen {
            Screen::Picker => {
                if super::screens::refresh_rect(self.cur_w).contains(pos) {
                    self.picker_action_pressed = true;
                    return;
                }
                if self.files.is_empty()
                    && super::screens::state_action_rect(self.cur_w, self.cur_h).contains(pos)
                {
                    self.rescan_pressed = true;
                    return;
                }
                if pos.y >= LIST_TOP && pos.y < self.cur_h - ui::BOTTOM_SAFE {
                    let idx = ((pos.y - LIST_TOP + self.picker_scroll as i32) / ROW_H) as usize;
                    self.pressed_file = self.files.get(idx).map(|_| idx);
                }
                self.touch = Some(self.new_touch(id, pos));
            }
            Screen::Viewer => {
                let ch = chrome(self.cur_w, self.cur_h);
                if self.chrome_vis && ch.back.contains(pos) {
                    self.back_pressed = true;
                    return;
                }
                if self.chrome_vis && ch.grid_btn.contains(pos) {
                    self.grid_pressed = true;
                    return;
                }
                if let Some(b) = nav_hit(ch, pos).filter(|_| self.chrome_vis) {
                    self.nav_pressed = Some(b);
                    return;
                }
                // Taps outside the content area (top bar, margins) are not
                // gestures — don't arm double-tap zoom from chrome.
                if !ch.content.contains(pos) {
                    return;
                }
                self.touch = Some(self.new_touch(id, pos));
            }
            Screen::Overview => {
                let ch = chrome(self.cur_w, self.cur_h);
                if ch.back.contains(pos) {
                    self.back_pressed = true;
                    return;
                }
                self.touch = Some(self.new_touch(id, pos));
            }
            Screen::Error => {
                if self.last_path.is_some()
                    && super::screens::state_action_rect(self.cur_w, self.cur_h).contains(pos)
                {
                    self.retry_pressed = true;
                    return;
                }
                let ch = chrome(self.cur_w, self.cur_h);
                if ch.back.contains(pos) {
                    self.back_pressed = true;
                }
            }
        }
    }

    fn touch_move(&mut self, id: i32, pos: Point) {
        let Some(mut t) = self.touch.take() else {
            return;
        };
        if t.id != id {
            self.touch = Some(t);
            return;
        }
        if t.mode == Gesture::Undecided {
            let dx = pos.x - t.down.x;
            let dy = pos.y - t.down.y;
            if dx.abs().max(dy.abs()) > view::TOUCH_SLOP {
                t.mode = match self.screen {
                    Screen::Picker | Screen::Overview => Gesture::Pan,
                    Screen::Viewer => {
                        if self.zoom > 1.001 {
                            Gesture::Pan
                        } else if dx.abs() > dy.abs() * 6 / 5 {
                            Gesture::Swipe
                        } else {
                            // Vertical drag: scroll a tall page via the same
                            // pan channel (x stays clamped to 0 at zoom 1).
                            Gesture::Pan
                        }
                    }
                    Screen::Error => Gesture::Ignore,
                };
            }
        }
        match (self.screen, t.mode) {
            (Screen::Picker, Gesture::Pan) => {
                let dy = pos.y - t.prev.y;
                self.picker_scroll =
                    view::clamp_scroll(self.picker_scroll - dy as f32, self.picker_max_scroll());
            }
            (Screen::Overview, Gesture::Pan) => {
                let dy = pos.y - t.prev.y;
                self.ov_scroll =
                    view::clamp_scroll(self.ov_scroll - dy as f32, self.ov_max_scroll());
            }
            (Screen::Viewer, Gesture::Pan) => {
                self.pan.0 += (pos.x - t.prev.x) as f32;
                self.pan.1 += (pos.y - t.prev.y) as f32;
                self.clamp_pan_now();
            }
            (Screen::Viewer, Gesture::Swipe) => {
                let raw = (pos.x - t.down.x) as f32;
                let at_start = self.page == 0 && raw > 0.0;
                let at_end = self.page + 1 >= self.page_count() && raw < 0.0;
                self.drag_dx = if at_start || at_end {
                    view::rubber_band(raw)
                } else {
                    raw
                };
            }
            _ => {}
        }
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
        let ch = chrome(self.cur_w, self.cur_h);
        let action = super::screens::state_action_rect(self.cur_w, self.cur_h);
        if std::mem::take(&mut self.picker_action_pressed) {
            if super::screens::refresh_rect(self.cur_w).contains(pos) {
                self.rescan_picker();
            }
            return;
        }
        if std::mem::take(&mut self.rescan_pressed) {
            if action.contains(pos) {
                self.rescan_picker();
            }
            return;
        }
        if std::mem::take(&mut self.retry_pressed) {
            if let Some(path) = self.last_path.clone().filter(|_| action.contains(pos)) {
                self.open_path(&path);
            }
            return;
        }
        if std::mem::take(&mut self.back_pressed) {
            if ch.back.contains(pos) {
                self.leave_chrome();
            }
            return;
        }
        if std::mem::take(&mut self.grid_pressed) {
            if ch.grid_btn.contains(pos) {
                self.enter_overview();
            }
            return;
        }
        if let Some(btn) = self.nav_pressed.take() {
            if nav_hit(ch, pos) == Some(btn) {
                self.nav_action(btn);
            }
            return;
        }
        let Some(t) = self.touch.take() else {
            return;
        };
        if t.id != id {
            self.touch = Some(t);
            return;
        }
        match self.screen {
            Screen::Picker => {
                let moved =
                    (pos.x - t.down.x).abs().max((pos.y - t.down.y).abs()) > view::TOUCH_SLOP;
                if !moved {
                    if let Some(idx) = self.pressed_file {
                        if idx < self.files.len() {
                            self.open_file(idx);
                        }
                    }
                }
                self.pressed_file = None;
            }
            Screen::Overview => {
                let moved =
                    (pos.x - t.down.x).abs().max((pos.y - t.down.y).abs()) > view::TOUCH_SLOP;
                if !moved {
                    let view = overview_view(self.cur_w, self.cur_h);
                    if let Some(idx) = view::hit_cell(view, self.page_count(), self.ov_scroll, pos)
                    {
                        self.enter_viewer(idx);
                    }
                }
            }
            Screen::Viewer => match t.mode {
                Gesture::Swipe => {
                    let dx = (pos.x - t.down.x) as f32;
                    let cw = chrome(self.cur_w, self.cur_h).content.w;
                    if let Some(dir) = view::page_dir(dx, t.vx, cw) {
                        // dir +1 = finger right = previous page.
                        self.commit_page(-dir, self.drag_dx);
                    } else {
                        self.settle = Some((self.drag_dx, 0.0));
                    }
                    self.drag_dx = 0.0;
                }
                Gesture::Undecided => {
                    let now = Instant::now();
                    if view::is_double_tap(self.last_up, now, pos) {
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
            Screen::Error => {}
        }
    }

    fn leave_chrome(&mut self) {
        match self.screen {
            Screen::Viewer | Screen::Error => self.go_picker(),
            Screen::Overview => self.screen = Screen::Viewer,
            Screen::Picker => {}
        }
    }

    fn nav_action(&mut self, btn: NavBtn) {
        match btn {
            NavBtn::Prev => self.commit_page(-1, 0.0),
            NavBtn::Next => self.commit_page(1, 0.0),
            NavBtn::ZoomMinus => self.set_zoom_step(-1),
            NavBtn::ZoomPlus => self.set_zoom_step(1),
            NavBtn::ZoomLabel => self.reset_zoom(),
        }
    }

    fn enter_overview(&mut self) {
        self.screen = Screen::Overview;
        self.anim = Some(Anim { open: true, t: 0.0 });
        self.ov_scroll = 0.0;
        for i in 0..self.page_count() {
            self.request_thumb(i);
        }
    }

    // ---------------- per-frame ----------------

    fn tick(&mut self, dt: std::time::Duration) -> AppFrame {
        let ms = dt.as_secs_f32() * 1000.0;
        let mut dirty = !self.chrome_fade.done();
        if dirty {
            self.chrome_fade.tick(dt);
        }
        if let Some(a) = &mut self.anim {
            a.t += ms / if a.open { OPEN_MS } else { CLOSE_MS };
            dirty = true;
            if a.t >= 1.0 {
                self.anim = None;
            }
        }
        if let Some(s) = &mut self.slide {
            s.t += ms / SLIDE_MS;
            dirty = true;
            if s.t >= 1.0 {
                self.slide = None;
            }
        }
        if let Some((from, t)) = &mut self.settle {
            *t += ms / SETTLE_MS;
            dirty = true;
            if *t >= 1.0 {
                self.settle = None;
                self.drag_dx = 0.0;
            } else {
                let k = bezier_eval(BEZIER_STANDARD, (*t).min(1.0));
                self.drag_dx = *from * (1.0 - k);
            }
        }
        self.pump();
        AppFrame {
            dirty,
            animating: self.busy(),
            quit: false,
        }
    }

    pub(super) fn picker_max_scroll(&self) -> i32 {
        let view_h = self.cur_h - ui::BOTTOM_SAFE - LIST_TOP;
        (self.files.len() as i32 * ROW_H - view_h).max(0)
    }

    pub(super) fn ov_max_scroll(&self) -> i32 {
        let v = overview_view(self.cur_w, self.cur_h);
        (view::grid_content_h(v.w, self.page_count()) - v.h).max(0)
    }
}

impl AppState for PdfApp {
    fn app_id(&self) -> &'static str {
        "rabbiz-pdf"
    }
    fn title(&self) -> &'static str {
        "PDF"
    }

    fn handle_event(&mut self, event: AppEvent, _ti: &TextInputCtl) -> AppFrame {
        match event {
            AppEvent::TouchDown { id, pos } => {
                self.pump();
                self.touch_down(id, pos);
                AppFrame {
                    dirty: true,
                    animating: self.busy(),
                    quit: false,
                }
            }
            AppEvent::TouchMove { id, pos } => {
                self.touch_move(id, pos);
                AppFrame {
                    dirty: true,
                    animating: self.busy(),
                    quit: false,
                }
            }
            AppEvent::TouchUp { id, pos } => {
                self.touch_up(id, pos);
                self.pump();
                AppFrame {
                    dirty: true,
                    animating: self.busy(),
                    quit: false,
                }
            }
            AppEvent::Key { keysym, .. } => {
                match self.screen {
                    Screen::Viewer => match keysym {
                        0xff1b | 0xff08 => self.go_picker(), // Esc / Backspace
                        0xff51 => self.commit_page(-1, 0.0), // Left
                        0xff53 => self.commit_page(1, 0.0),  // Right
                        0xff55 => self.set_zoom_step(1),     // Page_Up? no—keep simple
                        0xff56 => self.set_zoom_step(-1),
                        _ => {}
                    },
                    Screen::Overview => {
                        if keysym == 0xff1b || keysym == 0xff08 {
                            self.screen = Screen::Viewer;
                        }
                    }
                    Screen::Error => {
                        if keysym == 0xff1b || keysym == 0xff08 {
                            self.go_picker();
                        }
                    }
                    Screen::Picker => {
                        if keysym == 0xff1b || keysym == 0xff08 {
                            // Nothing to close; ignore.
                        }
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

    fn render(&mut self, canvas: &mut Canvas, theme: &Theme) {
        let size = canvas.size();
        if size.w != self.cur_w || size.h != self.cur_h {
            // Window resize invalidates every cached scale.
            self.cur_w = size.w;
            self.cur_h = size.h;
            self.pages.clear();
            self.queued.clear();
            self.page_queue.clear();
            self.thumbs.clear();
            self.thumb_queue.clear();
            self.thumb_queued.clear();
            self.page_pts.clear();
            if self.screen == Screen::Viewer || self.screen == Screen::Overview {
                self.request_page(self.page, self.zoom_i);
                self.prefetch_neighbors();
            }
        }
        self.cur_w = size.w;
        self.cur_h = size.h;
        self.pump();

        match self.screen {
            Screen::Picker => self.render_picker(canvas, theme),
            Screen::Viewer => self.render_viewer(canvas, theme),
            Screen::Overview => self.render_overview(canvas, theme),
            Screen::Error => self.render_error(canvas, theme),
        }
    }
}
