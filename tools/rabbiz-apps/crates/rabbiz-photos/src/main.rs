//! RabbizOS Photos — grid of thumbnails from ~/Pictures + ~/Downloads with a
//! full-screen viewer (double-tap zoom, drag pan, swipe paging).
//!
//! Decode path: `de-paint`'s resvg pipeline (zune-jpeg / image-webp / gif /
//! PNG) wrapped in a one-image SVG — see `decode.rs`. Thumbnails decode
//! lazily into a bounded cache; the viewer decodes on demand through the
//! same pump so the UI never blocks on a 12 MP JPEG.

mod decode;
mod headless;
mod layout;
mod photos;
mod viewer;

use std::collections::{HashMap, HashSet, VecDeque};
use std::time::Instant;

use de_api::{Pixels, Point, Rect};
use de_design::motion::{bezier_eval, BEZIER_ENTER, BEZIER_EXIT, BEZIER_STANDARD};
use rabbiz_app_kit::ui;
use rabbiz_app_kit::{AppEvent, AppFrame, AppState, Canvas, TextInputCtl, Theme};

use layout::{Group, Layout};
use photos::Photo;

/// Grid thumbnail cache cap (~240 px RGBA ≈ 230 KB each → ~15 MB worst case).
const THUMB_CACHE_CAP: usize = 64;
const OPEN_MS: f32 = 240.0;
const CLOSE_MS: f32 = 180.0;
const SLIDE_MS: f32 = 210.0;
const SETTLE_MS: f32 = 140.0;
/// Decodes per event/tick — keeps each frame under ~50 ms of decode work.
const PUMP_EVENTS: usize = 2;
const PUMP_TICK: usize = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Screen {
    Grid,
    Viewer,
}

#[derive(Debug, Clone, Copy)]
struct Anim {
    open: bool,
    t: f32, // 0..1
}

#[derive(Debug, Clone, Copy)]
struct Slide {
    from: usize,
    to: usize,
    /// Finger offset at release; sign picks the direction.
    u: f32,
    t: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Gesture {
    Undecided,
    Scroll,
    Pan,
    Swipe,
    Ignore,
}

#[derive(Debug, Clone, Copy)]
struct Touch {
    id: i32,
    /// Original touch-down position (never mutated).
    down: Point,
    /// Position where the current gesture mode was decided.
    anchor: Point,
    /// Previous move position (for incremental pan).
    prev: Point,
    mode: Gesture,
    last_t: Instant,
    vx: f32, // smoothed px/ms
}

struct PhotosApp {
    photos: Vec<Photo>,
    groups: Vec<Group>,
    screen: Screen,

    // Canvas size, refreshed every render — hit tests never hardcode it.
    cur_w: i32,
    cur_h: i32,

    // ---- grid ----
    layout: Layout,
    view_top: i32,
    view_h: i32,
    scroll: f32,
    scroll_start: f32,
    scroll_down_y: i32,
    pressed_cell: Option<usize>,
    pending: VecDeque<usize>,
    thumbs: HashMap<usize, Pixels>,
    thumb_failed: HashSet<usize>,

    // ---- viewer ----
    v_index: usize,
    v_zoom: f32,
    v_pan: (f32, f32),
    /// index → pixels; `None` value = decode failed. Absence = not decoded.
    v_px: HashMap<usize, Option<Pixels>>,
    v_queued: HashSet<usize>,
    touch: Option<Touch>,
    last_up: Option<(Instant, Point)>,
    drag_dx: f32,
    slide: Option<Slide>,
    settle: Option<(f32, f32)>, // (start_dx, t)
    anim: Option<Anim>,
    back_pressed: bool,
}

impl PhotosApp {
    fn new() -> Self {
        let photos = photos::scan(&photos::default_dirs());
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        // Photos arrive mtime-sorted, so equal days are contiguous.
        let mut groups: Vec<Group> = Vec::new();
        for (i, p) in photos.iter().enumerate() {
            let label = photos::format_day(p.mtime, now);
            match groups.last_mut() {
                Some(g) if g.label == label => g.end = i + 1,
                _ => groups.push(Group {
                    label,
                    first: i,
                    end: i + 1,
                }),
            }
        }
        Self {
            photos,
            groups,
            screen: Screen::Grid,
            cur_w: 0,
            cur_h: 0,
            layout: Layout::default(),
            view_top: 0,
            view_h: 0,
            scroll: 0.0,
            scroll_start: 0.0,
            scroll_down_y: 0,
            pressed_cell: None,
            pending: VecDeque::new(),
            thumbs: HashMap::new(),
            thumb_failed: HashSet::new(),
            v_index: 0,
            v_zoom: 1.0,
            v_pan: (0.0, 0.0),
            v_px: HashMap::new(),
            v_queued: HashSet::new(),
            touch: None,
            last_up: None,
            drag_dx: 0.0,
            slide: None,
            settle: None,
            anim: None,
            back_pressed: false,
        }
    }

    fn max_scroll(&self) -> i32 {
        (self.layout.content_h - self.view_h).max(0)
    }

    // ---------------- decode pump ----------------

    /// Decode queued work: viewer images first (they gate the UI), then
    /// grid thumbnails. At most `budget` decodes per call.
    fn pump(&mut self, budget: usize) {
        let mut n = 0;
        // Viewer queue: one decode per call is plenty — each is expensive.
        if n < budget {
            if let Some(idx) = self.v_queued.iter().next().copied() {
                self.v_queued.remove(&idx);
                self.decode_viewer(idx);
                n += 1;
            }
        }
        while n < budget {
            let Some(idx) = self.pending.pop_front() else {
                break;
            };
            if self.thumbs.contains_key(&idx) || self.thumb_failed.contains(&idx) {
                continue;
            }
            let Some(p) = self.photos.get(idx) else {
                continue;
            };
            let cell = self.layout.cell.max(1);
            match decode::rasterize(&p.path, cell, cell, decode::Fit::Slice) {
                Ok(px) => {
                    if self.thumbs.len() >= THUMB_CACHE_CAP {
                        if let Some(k) = self.thumbs.keys().next().copied() {
                            self.thumbs.remove(&k);
                        }
                    }
                    self.thumbs.insert(idx, px);
                }
                Err(_) => {
                    self.thumb_failed.insert(idx);
                }
            }
            n += 1;
        }
    }

    fn decode_viewer(&mut self, idx: usize) {
        let px = self
            .photos
            .get(idx)
            .and_then(|p| decode::rasterize_content(&p.path, viewer::VIEW_DECODE_W).ok());
        self.v_px.insert(idx, px);
        // Keep only neighbours of the current index.
        let cur = self.v_index;
        self.v_px.retain(|k, _| k.abs_diff(cur) <= 2);
        self.v_queued.retain(|k| k.abs_diff(cur) <= 2);
    }

    /// Queue a viewer decode if not already done/queued.
    fn request_viewer(&mut self, idx: usize) {
        if idx >= self.photos.len() || self.v_px.contains_key(&idx) || self.v_queued.contains(&idx)
        {
            return;
        }
        self.v_queued.insert(idx);
    }

    fn busy(&self) -> bool {
        self.anim.is_some()
            || self.slide.is_some()
            || self.settle.is_some()
            || (self.screen == Screen::Grid && !self.pending.is_empty())
            || !self.v_queued.is_empty()
    }

    // ---------------- viewer navigation ----------------

    /// Commit a page turn by index delta (-1 prev, +1 next). Slides even if
    /// the target pixels are still decoding — the placeholder slides with it.
    fn commit_page(&mut self, delta: i32, u: f32) {
        let n = self.photos.len() as i32;
        if n == 0 {
            return;
        }
        let to = (self.v_index as i32 + delta).clamp(0, n - 1) as usize;
        if to == self.v_index {
            // Edge nudge feedback.
            self.settle = Some((u.signum() * 40.0, 0.0));
            return;
        }
        // A pure-velocity flick can release with ~zero drag offset; give the
        // slide a minimal directional seed so the math never sees u == 0.
        let u = if u.abs() < 1.0 {
            -(delta.signum() as f32)
        } else {
            u
        };
        self.request_viewer(to);
        self.slide = Some(Slide {
            from: self.v_index,
            to,
            u,
            t: 0.0,
        });
        self.v_index = to;
        self.v_zoom = 1.0;
        self.v_pan = (0.0, 0.0);
        self.drag_dx = 0.0;
    }

    fn open_viewer(&mut self, idx: usize) {
        self.v_index = idx;
        self.v_zoom = 1.0;
        self.v_pan = (0.0, 0.0);
        self.drag_dx = 0.0;
        self.slide = None;
        self.settle = None;
        self.screen = Screen::Viewer;
        self.anim = Some(Anim { open: true, t: 0.0 });
        // Grid decode queue is irrelevant while the viewer is up.
        self.pending.clear();
        self.request_viewer(idx);
        if idx > 0 {
            self.request_viewer(idx - 1);
        }
        if idx + 1 < self.photos.len() {
            self.request_viewer(idx + 1);
        }
    }

    fn close_viewer(&mut self) {
        self.anim = Some(Anim {
            open: false,
            t: 0.0,
        });
        self.touch = None;
        self.slide = None;
        self.settle = None;
        self.drag_dx = 0.0;
        self.back_pressed = false;
    }

    fn toggle_zoom(&mut self, tap: Point) {
        let center = (self.cur_w as f32 / 2.0, self.cur_h as f32 / 2.0);
        let target = if self.v_zoom > 1.001 {
            1.0
        } else {
            viewer::DOUBLE_TAP_ZOOM
        };
        self.v_pan = viewer::zoom_pan_at(self.v_pan, tap, center, self.v_zoom, target);
        self.v_zoom = target;
        self.clamp_pan_now();
    }

    fn clamp_pan_now(&mut self) {
        if let Some(Some(px)) = self.v_px.get(&self.v_index) {
            let base = viewer::fit_scale(self.cur_w, self.cur_h, px.size.w, px.size.h);
            self.v_pan = viewer::clamp_pan(
                self.v_pan,
                self.cur_w,
                self.cur_h,
                px.size.w,
                px.size.h,
                base,
                self.v_zoom,
            );
        }
    }

    // ---------------- gesture plumbing ----------------

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

    // ---------------- per-frame ----------------

    fn tick(&mut self, dt: std::time::Duration) -> AppFrame {
        let ms = dt.as_secs_f32() * 1000.0;
        let mut dirty = false;
        if let Some(a) = &mut self.anim {
            a.t += ms / if a.open { OPEN_MS } else { CLOSE_MS };
            dirty = true;
            let (open, done) = (a.open, a.t >= 1.0);
            if done {
                self.anim = None;
                if !open {
                    self.screen = Screen::Grid;
                }
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
        self.pump(PUMP_TICK);
        AppFrame {
            dirty,
            animating: self.busy(),
            quit: false,
        }
    }

    fn rebuild_pending(&mut self) {
        let vis = layout::visible_indices(&self.layout, self.view_h, self.scroll);
        self.pending = vis
            .into_iter()
            .filter(|i| !self.thumbs.contains_key(i) && !self.thumb_failed.contains(i))
            .collect();
    }
}

impl AppState for PhotosApp {
    fn app_id(&self) -> &'static str {
        "rabbiz-photos"
    }
    fn title(&self) -> &'static str {
        "Photos"
    }

    fn handle_event(&mut self, event: AppEvent, _ti: &TextInputCtl) -> AppFrame {
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

    fn render(&mut self, canvas: &mut Canvas, theme: &Theme) {
        let size = canvas.size();
        self.cur_w = size.w;
        self.cur_h = size.h;
        self.pump(1); // opportunistic progress even without touch events

        match self.screen {
            Screen::Grid => self.render_grid(canvas, theme),
            Screen::Viewer => self.render_viewer(canvas, theme),
        }
    }
}

// ---------------------------------------------------------------------------
// Grid
// ---------------------------------------------------------------------------

impl PhotosApp {
    fn render_grid(&mut self, canvas: &mut Canvas, theme: &Theme) {
        let w = canvas.size().w;
        let h = canvas.size().h;
        let after_header = ui::draw_header(
            canvas,
            theme,
            "Photos",
            &format!("{} items", self.photos.len()),
        );
        self.view_top = after_header + 8;
        self.view_h = (h - ui::BOTTOM_SAFE - self.view_top).max(0);
        self.layout = layout::build(w, &self.groups);
        self.scroll = layout::clamp_scroll(self.scroll, self.max_scroll());
        self.rebuild_pending();

        if self.photos.is_empty() {
            ui::draw_centered(
                canvas,
                "No photos in ~/Pictures or ~/Downloads",
                &ui::body(theme.text3),
                Rect::new(0, h / 2 - 40, w, 80),
            );
            return;
        }

        // Group headers (content coords → screen coords).
        for hd in &self.layout.headers {
            let g = &self.groups[hd.group];
            let sy = self.view_top + hd.rect.y - self.scroll as i32;
            if sy + hd.rect.h < self.view_top || sy > h {
                continue;
            }
            ui::draw_text(
                canvas,
                &g.label,
                &ui::caption(theme.text3),
                hd.rect.x,
                sy + 10,
                Some(hd.rect.w),
            );
        }

        for cell in &self.layout.cells {
            let sy = self.view_top + cell.rect.y - self.scroll as i32;
            if sy + cell.rect.h < self.view_top || sy > h {
                continue;
            }
            let r = Rect::new(cell.rect.x, sy, cell.rect.w, cell.rect.h);
            let idx = cell.photo;
            if let Some(px) = self.thumbs.get(&idx) {
                canvas.blit_scaled(px, r, 1.0);
            } else {
                // Placeholder: surface tile + format hint, like Android's
                // grey thumb while the decode is in flight.
                canvas.fill_rect(r, theme.surface1);
                let name = self.photos.get(idx).map(|p| p.name.as_str()).unwrap_or("");
                let ext = name.rsplit('.').next().unwrap_or("").to_uppercase();
                if self.thumb_failed.contains(&idx) {
                    ui::draw_centered(canvas, "!", &ui::title(theme.text3), r);
                } else {
                    ui::draw_centered(canvas, &ext, &ui::caption(theme.text3), r);
                }
            }
            if self.pressed_cell == Some(idx) {
                canvas.fill_rect(r, [255, 255, 255, 18]);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Viewer
// ---------------------------------------------------------------------------

impl PhotosApp {
    /// Progress of the open/close animation in 0..=1 (1 = fully shown).
    fn viewer_e(&self) -> f32 {
        match &self.anim {
            Some(a) => {
                let t = a.t.clamp(0.0, 1.0);
                if a.open {
                    bezier_eval(BEZIER_ENTER, t)
                } else {
                    1.0 - bezier_eval(BEZIER_EXIT, t)
                }
            }
            None => 1.0,
        }
    }

    /// Draw one decoded (or placeholder) photo into the viewer at `dx` extra
    /// horizontal offset and `rise` vertical offset (open/close motion).
    /// `apply_transform` is true only for the settled current image —
    /// sliding neighbours always draw at fit/zoom 1.
    fn draw_photo(
        &self,
        canvas: &mut Canvas,
        theme: &Theme,
        idx: usize,
        dx: f32,
        rise: f32,
        apply_transform: bool,
    ) {
        match self.v_px.get(&idx) {
            Some(Some(px)) => {
                let (zoom, pan) = if apply_transform {
                    (self.v_zoom, self.v_pan)
                } else {
                    (1.0, (0.0, 0.0))
                };
                let base = viewer::fit_scale(self.cur_w, self.cur_h, px.size.w, px.size.h);
                let (x, y, dw, dh) = viewer::image_rect(
                    self.cur_w, self.cur_h, px.size.w, px.size.h, base, zoom, pan,
                );
                let r = Rect::new((x + dx) as i32, (y + rise) as i32, dw as i32, dh as i32);
                canvas.blit_scaled(px, r, 1.0);
            }
            Some(None) => {
                let r = Rect::new(0, 0, self.cur_w, self.cur_h);
                canvas.fill_rect(r, theme.surface1);
                self.draw_placeholder(canvas, theme, idx, "Can't decode this image");
            }
            None => {
                let r = Rect::new(0, 0, self.cur_w, self.cur_h);
                canvas.fill_rect(r, theme.surface1);
                self.draw_placeholder(canvas, theme, idx, "Loading…");
            }
        }
    }

    fn draw_placeholder(&self, canvas: &mut Canvas, theme: &Theme, idx: usize, msg: &str) {
        let name = self
            .photos
            .get(idx)
            .map(|p| p.name.as_str())
            .unwrap_or("Photo");
        let cy = self.cur_h / 2;
        ui::draw_centered(
            canvas,
            name,
            &ui::body(theme.text),
            Rect::new(0, cy - 60, self.cur_w, 44),
        );
        ui::draw_centered(
            canvas,
            msg,
            &ui::caption(theme.text3),
            Rect::new(0, cy - 8, self.cur_w, 32),
        );
    }

    fn render_viewer(&mut self, canvas: &mut Canvas, theme: &Theme) {
        let e = self.viewer_e();
        let n = self.photos.len();
        let vw = self.cur_w as f32;

        // Background: translucent while the open/close anim plays.
        let bg = theme.bg;
        let alpha = (e * 255.0) as u8;
        canvas.fill_rect(
            Rect::new(0, 0, self.cur_w, self.cur_h),
            [bg[0], bg[1], bg[2], alpha],
        );

        // Slide offsets: during a committed page turn both images travel;
        // otherwise the current image follows drag_dx / settle.
        // Open/close motion: content rises ~48 px while fading in.
        let rise = (1.0 - e) * 48.0;
        if let Some(s) = self.slide {
            let k = bezier_eval(BEZIER_STANDARD, s.t.clamp(0.0, 1.0));
            // Content-space model: the outgoing image travels from the
            // release offset `u` to one viewport in the swipe direction;
            // the incoming image starts one viewport past `u` and lands at 0.
            let dir = s.u.signum();
            let old_x = s.u * (1.0 - k) + vw * dir * k;
            let new_x = (s.u - vw * dir) * (1.0 - k);
            self.draw_photo(canvas, theme, s.from, old_x, rise, false);
            self.draw_photo(canvas, theme, s.to, new_x, rise, false);
        } else {
            let dx = self.drag_dx;
            self.draw_photo(canvas, theme, self.v_index, dx, rise, true);
        }

        // ---- chrome (rides the same rise as the content) ----
        let ry = rise as i32;
        let back_r = Rect::new(ui::MARGIN, ui::TOP_SAFE + 12 + ry, 120, 48);
        ui::draw_pill_button(canvas, theme, back_r, "‹ Back", false, self.back_pressed);
        if n > 0 {
            let counter = format!("{} / {}", self.v_index + 1, n);
            ui::draw_text(
                canvas,
                &counter,
                &ui::caption(theme.text2),
                self.cur_w - ui::MARGIN - 80,
                ui::TOP_SAFE + 24 + ry,
                Some(80),
            );
        }

        // Bottom metadata: file name + date on a translucent strip.
        let strip_h = 76;
        let strip_y = self.cur_h - ui::BOTTOM_SAFE - strip_h + ry;
        canvas.fill_rect(
            Rect::new(0, strip_y, self.cur_w, strip_h + ui::BOTTOM_SAFE),
            theme.scrim,
        );
        if let Some(p) = self.photos.get(self.v_index) {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            let meta = format!(
                "{}  ·  {}  ·  {}",
                p.name,
                photos::format_day(p.mtime, now),
                photos::format_bytes(p.bytes)
            );
            ui::draw_text(
                canvas,
                &meta,
                &ui::caption(theme.text),
                ui::MARGIN,
                strip_y + 14,
                Some(self.cur_w - ui::MARGIN * 2),
            );
            ui::draw_text(
                canvas,
                "Double-tap to zoom · swipe to turn pages",
                &ui::caption(theme.text3),
                ui::MARGIN,
                strip_y + 42,
                Some(self.cur_w - ui::MARGIN * 2),
            );
        }
    }
}

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    // Headless modes run before the GUI is touched at all.
    if argv.iter().any(|a| a == "--mcp") {
        headless::serve_mcp();
    }
    if argv.first().map(String::as_str) == Some("--cli") {
        std::process::exit(headless::run_cli(&argv[1..]));
    }
    match argv.first().map(String::as_str) {
        Some("-h") | Some("--help") | Some("help") => {
            print!("{}", headless::help_text());
        }
        Some(flag) if flag.starts_with('-') => {
            eprintln!("error: unknown flag '{flag}'\n");
            eprint!("{}", headless::help_text());
            std::process::exit(2);
        }
        // `rabbiz-photos <file-or-dir>`: headless open/list contract.
        Some(path) if argv.len() == 1 => {
            std::process::exit(headless::run_path_arg(std::path::Path::new(path)));
        }
        Some(_) => {
            eprintln!("error: too many arguments\n");
            eprint!("{}", headless::help_text());
            std::process::exit(2);
        }
        None => {
            rabbiz_app_kit::run_app(PhotosApp::new());
        }
    }
}
