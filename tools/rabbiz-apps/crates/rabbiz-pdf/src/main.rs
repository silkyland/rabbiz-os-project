//! RabbizOS PDF — native PDF reader on `hayro` (pure-Rust CPU rasterizer,
//! no poppler/mupdf on the rootfs).
//!
//! Screens: file picker (~/Documents, ~/Downloads, ~, ~/Desktop) → page
//! viewer (fit-width, double-tap zoom, drag pan, swipe paging, prev/next +
//! zoom buttons) → page overview grid. Rendering is queued one page per
//! event/tick so the software rasterizer never blocks a frame for long.

mod doc;
mod headless;
mod view;

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::time::Instant;

use de_api::{Pixels, Point, Rect};
use de_design::motion::{bezier_eval, BEZIER_ENTER, BEZIER_EXIT, BEZIER_STANDARD};
use rabbiz_app_kit::ui;
use rabbiz_app_kit::{AppEvent, AppFrame, AppState, Canvas, TextInputCtl, Theme};

use doc::{Doc, DocError, PdfFile};

const OPEN_MS: f32 = 240.0;
const CLOSE_MS: f32 = 180.0;
const SLIDE_MS: f32 = 210.0;
const SETTLE_MS: f32 = 140.0;
/// Full-page pixmap cache cap in bytes (~120 MB worst case; typical use is
/// far below since only ±2 pages per zoom bucket are kept).
const PAGE_CACHE_BYTES: usize = 120 * 1024 * 1024;
/// Overview thumbnail cache cap (count, not bytes — thumbs are ~300 KB).
const THUMB_CACHE_CAP: usize = 64;
/// Picker row height.
const ROW_H: i32 = 92;
/// Y where the scrollable list/grid starts (below `ui::draw_header`).
const LIST_TOP: i32 = ui::TOP_SAFE + 104;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Screen {
    Picker,
    Viewer,
    Overview,
    Error,
}

#[derive(Debug, Clone, Copy)]
struct Anim {
    open: bool,
    t: f32,
}

#[derive(Debug, Clone, Copy)]
struct Slide {
    from: usize,
    to: usize,
    /// Finger offset at release; sign picks the direction.
    u: f32,
    t: f32,
}

/// One page to draw in the viewer: page index, zoom-step bucket, pixel
/// offsets for slide/open motion, and whether the live zoom/pan applies.
#[derive(Debug, Clone, Copy)]
struct PageDraw {
    idx: usize,
    z: usize,
    dx: f32,
    rise: f32,
    apply_transform: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Gesture {
    Undecided,
    Pan,
    Swipe,
    Ignore,
}

#[derive(Debug, Clone, Copy)]
struct Touch {
    id: i32,
    down: Point,
    prev: Point,
    mode: Gesture,
    last_t: Instant,
    vx: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NavBtn {
    Prev,
    Next,
    ZoomMinus,
    ZoomPlus,
    ZoomLabel,
}

/// Chrome geometry shared by hit-testing and drawing.
#[derive(Debug, Clone, Copy)]
struct Chrome {
    back: Rect,
    grid_btn: Rect,
    content: Rect,
    prev: Rect,
    next: Rect,
    zminus: Rect,
    zlabel: Rect,
    zplus: Rect,
}

fn chrome(w: i32, h: i32) -> Chrome {
    let top_y = ui::TOP_SAFE + 8;
    let back = Rect::new(ui::MARGIN, top_y, 104, 48);
    let grid_btn = Rect::new(w - ui::MARGIN - 104, top_y, 104, 48);
    let content_top = top_y + 48 + 12;
    let strip_h = 84;
    let strip_y = h - ui::BOTTOM_SAFE - strip_h;
    let bw = 64;
    let gap = 12;
    let row_w = bw * 5 + gap * 4;
    let x0 = (w - row_w) / 2;
    let by = strip_y + 16;
    Chrome {
        back,
        grid_btn,
        content: Rect::new(
            ui::MARGIN,
            content_top,
            w - ui::MARGIN * 2,
            strip_y - content_top,
        ),
        prev: Rect::new(x0, by, bw, 48),
        zminus: Rect::new(x0 + (bw + gap), by, bw, 48),
        zlabel: Rect::new(x0 + (bw + gap) * 2, by, bw, 48),
        zplus: Rect::new(x0 + (bw + gap) * 3, by, bw, 48),
        next: Rect::new(x0 + (bw + gap) * 4, by, bw, 48),
    }
}

fn nav_hit(ch: Chrome, pos: Point) -> Option<NavBtn> {
    if ch.prev.contains(pos) {
        Some(NavBtn::Prev)
    } else if ch.next.contains(pos) {
        Some(NavBtn::Next)
    } else if ch.zminus.contains(pos) {
        Some(NavBtn::ZoomMinus)
    } else if ch.zplus.contains(pos) {
        Some(NavBtn::ZoomPlus)
    } else if ch.zlabel.contains(pos) {
        Some(NavBtn::ZoomLabel)
    } else {
        None
    }
}

/// Scrollable grid area for the overview screen.
fn overview_view(w: i32, h: i32) -> Rect {
    Rect::new(
        ui::MARGIN,
        LIST_TOP,
        w - ui::MARGIN * 2,
        h - ui::BOTTOM_SAFE - LIST_TOP,
    )
}

struct PdfApp {
    files: Vec<PdfFile>,
    doc: Option<Doc>,
    /// Page sizes in points, fetched lazily per page.
    page_pts: HashMap<usize, (f32, f32)>,
    screen: Screen,
    error: String,

    cur_w: i32,
    cur_h: i32,

    // ---- picker ----
    picker_scroll: f32,
    pressed_file: Option<usize>,

    // ---- overview ----
    ov_scroll: f32,

    // ---- viewer ----
    page: usize,
    zoom: f32,
    /// Which `view::ZOOM_STEPS` index the current zoom is (cache key + label).
    zoom_i: usize,
    pan: (f32, f32),
    /// (page, zoom bucket) → pixels; `None` value = render failed.
    pages: HashMap<(usize, usize), Option<Pixels>>,
    page_queue: VecDeque<(usize, usize, f32)>, // idx, zoom bucket, px_per_pt
    queued: HashSet<(usize, usize)>,
    thumb_queue: VecDeque<usize>,
    thumbs: HashMap<usize, Option<Pixels>>,
    thumb_queued: HashSet<usize>,
    touch: Option<Touch>,
    last_up: Option<(Instant, Point)>,
    drag_dx: f32,
    slide: Option<Slide>,
    settle: Option<(f32, f32)>,
    anim: Option<Anim>,
    back_pressed: bool,
    grid_pressed: bool,
    nav_pressed: Option<NavBtn>,
}

impl PdfApp {
    fn new(initial: Option<PathBuf>) -> Self {
        let mut app = Self {
            files: doc::scan(&doc::default_dirs()),
            doc: None,
            page_pts: HashMap::new(),
            screen: Screen::Picker,
            error: String::new(),
            cur_w: 0,
            cur_h: 0,
            picker_scroll: 0.0,
            pressed_file: None,
            ov_scroll: 0.0,
            page: 0,
            zoom: 1.0,
            zoom_i: 0,
            pan: (0.0, 0.0),
            pages: HashMap::new(),
            page_queue: VecDeque::new(),
            queued: HashSet::new(),
            thumb_queue: VecDeque::new(),
            thumbs: HashMap::new(),
            thumb_queued: HashSet::new(),
            touch: None,
            last_up: None,
            drag_dx: 0.0,
            slide: None,
            settle: None,
            anim: None,
            back_pressed: false,
            grid_pressed: false,
            nav_pressed: None,
        };
        if let Some(path) = initial {
            app.open_path(&path);
        }
        app
    }

    // ---------------- document ----------------

    fn open_path(&mut self, path: &Path) {
        match Doc::open(path) {
            Ok(d) => {
                self.doc = Some(d);
                self.page_pts.clear();
                self.pages.clear();
                self.page_queue.clear();
                self.queued.clear();
                self.thumbs.clear();
                self.thumb_queue.clear();
                self.thumb_queued.clear();
                self.enter_viewer(0);
            }
            Err(e) => {
                self.error = match e {
                    DocError::Open(m) | DocError::Render(m) => m,
                };
                self.screen = Screen::Error;
            }
        }
    }

    fn open_file(&mut self, idx: usize) {
        let Some(f) = self.files.get(idx) else {
            return;
        };
        let path = f.path.clone();
        self.open_path(&path);
    }

    fn enter_viewer(&mut self, page: usize) {
        self.page = page.min(self.page_count().saturating_sub(1));
        self.zoom = 1.0;
        self.zoom_i = 0;
        self.pan = self.home_pan_for(self.page);
        self.drag_dx = 0.0;
        self.slide = None;
        self.settle = None;
        self.screen = Screen::Viewer;
        self.anim = Some(Anim { open: true, t: 0.0 });
        self.request_page(self.page, self.zoom_i);
        self.prefetch_neighbors();
    }

    fn go_picker(&mut self) {
        self.screen = Screen::Picker;
        self.doc = None;
        self.anim = None;
        self.touch = None;
        self.slide = None;
        self.settle = None;
        self.drag_dx = 0.0;
        self.back_pressed = false;
        self.grid_pressed = false;
        self.nav_pressed = None;
    }

    fn page_count(&self) -> usize {
        self.doc.as_ref().map_or(0, Doc::page_count)
    }

    fn page_pt(&mut self, idx: usize) -> (f32, f32) {
        if let Some(p) = self.page_pts.get(&idx) {
            return *p;
        }
        let p = self
            .doc
            .as_ref()
            .and_then(|d| d.page_size(idx))
            .unwrap_or((612.0, 792.0));
        self.page_pts.insert(idx, p);
        p
    }

    /// Fit scale (screen px per PDF pt) for page `idx`: page width fills the
    /// content area at zoom 1.
    fn base_scale(&mut self, idx: usize) -> f32 {
        let cw = chrome(self.cur_w.max(1), self.cur_h.max(1)).content.w;
        let (w_pt, _) = self.page_pt(idx);
        view::fit_width_scale(cw, w_pt)
    }

    /// px per pt to ask hayro for, for page `idx` at zoom bucket `z`
    /// (includes the supersample factor for zoom sharpness).
    fn render_scale(&mut self, idx: usize, z: usize) -> f32 {
        let zoom = view::ZOOM_STEPS[z.min(view::ZOOM_STEPS.len() - 1)];
        self.base_scale(idx) * zoom * view::SUPERSAMPLE
    }

    fn request_page(&mut self, idx: usize, z: usize) {
        let n = self.page_count();
        if idx >= n || self.pages.contains_key(&(idx, z)) || self.queued.contains(&(idx, z)) {
            return;
        }
        let scale = self.render_scale(idx, z);
        self.queued.insert((idx, z));
        self.page_queue.push_back((idx, z, scale));
    }

    fn prefetch_neighbors(&mut self) {
        let n = self.page_count();
        for d in [-2, -1, 1, 2] {
            let idx = self.page as i32 + d;
            if idx >= 0 && (idx as usize) < n {
                self.request_page(idx as usize, self.zoom_i);
            }
        }
        self.evict_page_cache();
    }

    fn request_thumb(&mut self, idx: usize) {
        let n = self.page_count();
        if idx >= n || self.thumbs.contains_key(&idx) || self.thumb_queued.contains(&idx) {
            return;
        }
        self.thumb_queued.insert(idx);
        self.thumb_queue.push_back(idx);
    }

    /// Render at most one queued item: viewer pages first (they gate the UI),
    /// then overview thumbs.
    fn pump(&mut self) {
        if let Some((idx, z, scale)) = self.page_queue.pop_front() {
            self.queued.remove(&(idx, z));
            let px = self
                .doc
                .as_ref()
                .and_then(|d| d.render_page(idx, scale).ok());
            self.pages.insert((idx, z), px);
            self.evict_page_cache();
            return;
        }
        if let Some(idx) = self.thumb_queue.pop_front() {
            self.thumb_queued.remove(&idx);
            let px = self.doc.as_ref().and_then(|d| {
                let (w_pt, _) = d.page_size(idx).unwrap_or((612.0, 792.0));
                let cw = (self.cur_w - ui::MARGIN * 2 - view::GRID_GAP * 2) / view::GRID_COLS;
                let scale = view::thumb_scale_for(w_pt, 792.0, cw.max(64));
                d.render_page(idx, scale).ok()
            });
            if self.thumbs.len() >= THUMB_CACHE_CAP {
                if let Some(k) = self.thumbs.keys().next().copied() {
                    self.thumbs.remove(&k);
                }
            }
            self.thumbs.insert(idx, px);
        }
    }

    fn evict_page_cache(&mut self) {
        // Keep only the current zoom bucket within ±2 pages; drop the rest.
        let cur = self.page;
        let z = self.zoom_i;
        self.pages
            .retain(|(idx, zz), _| *zz == z && idx.abs_diff(cur) <= 2);
        self.queued
            .retain(|(idx, zz)| *zz == z && idx.abs_diff(cur) <= 2);
        self.page_queue
            .retain(|(idx, zz, _)| *zz == z && idx.abs_diff(cur) <= 2);
        // Byte budget: drop farthest pages first if over cap.
        let mut total: usize = self.pages.values().flatten().map(|p| p.data.len()).sum();
        if total > PAGE_CACHE_BYTES {
            let mut keys: Vec<(usize, usize)> = self.pages.keys().copied().collect();
            keys.sort_by_key(|(idx, _)| idx.abs_diff(cur));
            for k in keys.into_iter().rev() {
                if let Some(Some(p)) = self.pages.remove(&k) {
                    total = total.saturating_sub(p.data.len());
                }
                if total <= PAGE_CACHE_BYTES {
                    break;
                }
            }
        }
    }

    fn busy(&self) -> bool {
        self.anim.is_some()
            || self.slide.is_some()
            || self.settle.is_some()
            || !self.page_queue.is_empty()
            || !self.thumb_queue.is_empty()
    }

    // ---------------- viewer navigation ----------------

    fn clamp_pan_now(&mut self) {
        let idx = self.page;
        let (w_pt, h_pt) = self.page_pt(idx);
        let ch = chrome(self.cur_w.max(1), self.cur_h.max(1)).content;
        let base = view::fit_width_scale(ch.w, w_pt);
        self.pan = view::clamp_pan(self.pan, ch.w, ch.h, w_pt, h_pt, base, self.zoom);
    }

    /// Pan that shows the top of `idx`'s page (home position).
    fn home_pan_for(&mut self, idx: usize) -> (f32, f32) {
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
                let idx = ((pos.y - LIST_TOP + self.picker_scroll as i32) / ROW_H).max(0) as usize;
                self.pressed_file = Some(idx);
                self.touch = Some(self.new_touch(id, pos));
            }
            Screen::Viewer => {
                let ch = chrome(self.cur_w, self.cur_h);
                if ch.back.contains(pos) {
                    self.back_pressed = true;
                    return;
                }
                if ch.grid_btn.contains(pos) {
                    self.grid_pressed = true;
                    return;
                }
                if let Some(b) = nav_hit(ch, pos) {
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
        if self.back_pressed {
            self.back_pressed = false;
            self.leave_chrome();
            return;
        }
        if self.grid_pressed {
            self.grid_pressed = false;
            match self.screen {
                Screen::Viewer => self.enter_overview(),
                Screen::Overview => {
                    self.screen = Screen::Viewer;
                    self.anim = Some(Anim { open: true, t: 0.0 });
                }
                _ => {}
            }
            return;
        }
        if let Some(btn) = self.nav_pressed.take() {
            self.nav_action(btn);
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
                    }
                    self.last_up = Some((now, pos));
                }
                _ => {}
            },
            Screen::Error => {}
        }
    }

    fn leave_chrome(&mut self) {
        match self.screen {
            Screen::Viewer | Screen::Overview | Screen::Error => self.go_picker(),
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
        let mut dirty = false;
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

    fn picker_max_scroll(&self) -> i32 {
        let view_h = self.cur_h - ui::BOTTOM_SAFE - LIST_TOP;
        (self.files.len() as i32 * ROW_H - view_h).max(0)
    }

    fn ov_max_scroll(&self) -> i32 {
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

// ---------------------------------------------------------------------------
// Picker
// ---------------------------------------------------------------------------

impl PdfApp {
    fn render_picker(&mut self, canvas: &mut Canvas, theme: &Theme) {
        let w = canvas.size().w;
        let h = canvas.size().h;
        let after = ui::draw_header(
            canvas,
            theme,
            "PDF",
            &format!(
                "{} document{}",
                self.files.len(),
                if self.files.len() == 1 { "" } else { "s" }
            ),
        );
        let list_top = after + 8;
        debug_assert_eq!(list_top, LIST_TOP);
        self.picker_scroll = view::clamp_scroll(self.picker_scroll, self.picker_max_scroll());

        if self.files.is_empty() {
            ui::draw_centered(
                canvas,
                "No PDFs found",
                &ui::body(theme.text3),
                Rect::new(0, h / 2 - 40, w, 80),
            );
            ui::draw_centered(
                canvas,
                "Drop a .pdf into ~/Documents or ~/Downloads,\nor open one from a file manager",
                &ui::caption(theme.text3),
                Rect::new(0, h / 2 + 40, w, 64),
            );
            return;
        }

        for (i, f) in self.files.iter().enumerate() {
            let sy = list_top + i as i32 * ROW_H - self.picker_scroll as i32;
            if sy + ROW_H < list_top || sy > h {
                continue;
            }
            let r = Rect::new(ui::MARGIN, sy, w - ui::MARGIN * 2, ROW_H - 12);
            ui::draw_card(canvas, theme, r, self.pressed_file == Some(i));
            let name = ui::body(theme.text);
            ui::draw_text(canvas, &f.name, &name, r.x + 20, r.y + 16, Some(r.w - 40));
            let sub = format!(
                "{}  ·  {}",
                f.path
                    .parent()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default(),
                doc::format_bytes(f.bytes)
            );
            ui::draw_text(
                canvas,
                &sub,
                &ui::caption(theme.text3),
                r.x + 20,
                r.y + 48,
                Some(r.w - 40),
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Viewer
// ---------------------------------------------------------------------------

impl PdfApp {
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

    /// Draw one page (or placeholder) into the viewer.
    /// `apply_transform` is true only for the settled current page — slide
    /// neighbours always draw at their own fit/zoom-1 layout.
    fn draw_page(&mut self, canvas: &mut Canvas, theme: &Theme, p: PageDraw) {
        let ch = chrome(self.cur_w, self.cur_h);
        let (w_pt, h_pt) = self.page_pt(p.idx);
        let base = view::fit_width_scale(ch.content.w, w_pt);
        let (zoom, pan) = if p.apply_transform {
            (self.zoom, self.pan)
        } else {
            let z = view::ZOOM_STEPS[p.z.min(view::ZOOM_STEPS.len() - 1)];
            // Slide neighbours show their own home position.
            (z, view::home_pan(ch.content.h, w_pt, h_pt, base, z))
        };
        let key = (p.idx, p.z);
        match self.pages.get(&key) {
            Some(Some(px)) => {
                let r = view::page_rect(ch.content, w_pt, h_pt, base, zoom, pan);
                let r = Rect::new(r.x + p.dx as i32, r.y + p.rise as i32, r.w, r.h);
                canvas.blit_scaled(px, r, 1.0);
            }
            Some(None) => {
                let r = ch.content;
                canvas.fill_rect(r, theme.surface1);
                self.page_placeholder(canvas, theme, p.idx, "Can't render this page");
            }
            None => {
                // Not yet rendered: page-shaped placeholder.
                let r = view::page_rect(ch.content, w_pt, h_pt, base, zoom, pan);
                let r = Rect::new(r.x + p.dx as i32, r.y + p.rise as i32, r.w, r.h);
                canvas.fill_rect(r, theme.surface1);
                canvas.fill_rect(
                    Rect::new(r.x, r.y, r.w, (r.h * 18 / 100).max(24)),
                    theme.surface2,
                );
                ui::draw_centered(
                    canvas,
                    &format!("Page {}", p.idx + 1),
                    &ui::caption(theme.text3),
                    Rect::new(r.x, r.y + r.h / 2 - 16, r.w, 32),
                );
            }
        }
    }

    fn page_placeholder(&self, canvas: &mut Canvas, theme: &Theme, idx: usize, msg: &str) {
        let cy = self.cur_h / 2;
        ui::draw_centered(
            canvas,
            &format!("Page {}", idx + 1),
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
        // Keep pan valid across window resizes and the pre-first-frame open.
        self.clamp_pan_now();
        let e = self.viewer_e();
        let n = self.page_count();
        let vw = self.cur_w as f32;
        let ch = chrome(self.cur_w, self.cur_h);

        let bg = theme.bg;
        let alpha = (e * 255.0) as u8;
        canvas.fill_rect(
            Rect::new(0, 0, self.cur_w, self.cur_h),
            [bg[0], bg[1], bg[2], alpha],
        );

        let rise = (1.0 - e) * 48.0;
        if let Some(s) = self.slide {
            let k = bezier_eval(BEZIER_STANDARD, s.t.clamp(0.0, 1.0));
            let dir = s.u.signum();
            let old_x = s.u * (1.0 - k) + vw * dir * k;
            let new_x = (s.u - vw * dir) * (1.0 - k);
            // Neighbours draw at zoom bucket 0 (fit) — a page turn resets zoom.
            self.draw_page(
                canvas,
                theme,
                PageDraw {
                    idx: s.from,
                    z: 0,
                    dx: old_x,
                    rise,
                    apply_transform: false,
                },
            );
            self.draw_page(
                canvas,
                theme,
                PageDraw {
                    idx: s.to,
                    z: self.zoom_i,
                    dx: new_x,
                    rise,
                    apply_transform: false,
                },
            );
        } else {
            self.draw_page(
                canvas,
                theme,
                PageDraw {
                    idx: self.page,
                    z: self.zoom_i,
                    dx: self.drag_dx,
                    rise,
                    apply_transform: true,
                },
            );
        }

        // ---- chrome (rides the same rise) ----
        let ry = rise as i32;
        let back_r = Rect::new(ch.back.x, ch.back.y + ry, ch.back.w, ch.back.h);
        ui::draw_pill_button(canvas, theme, back_r, "‹ Files", false, self.back_pressed);
        let grid_r = Rect::new(
            ch.grid_btn.x,
            ch.grid_btn.y + ry,
            ch.grid_btn.w,
            ch.grid_btn.h,
        );
        ui::draw_pill_button(canvas, theme, grid_r, "Pages", false, self.grid_pressed);
        if n > 0 {
            let counter = format!("{} / {}", self.page + 1, n);
            let mid_w = (grid_r.x - ch.back.right() - 16).max(40);
            let mid = Rect::new(ch.back.right() + 8, ch.back.y + ry, mid_w, ch.back.h);
            ui::draw_centered(canvas, &counter, &ui::label(theme.text), mid);
        }

        // ---- bottom nav strip ----
        let strip_y = ch.prev.y - 20 + ry;
        canvas.fill_rect(
            Rect::new(
                0,
                strip_y - 8,
                self.cur_w,
                self.cur_h - strip_y + ui::BOTTOM_SAFE,
            ),
            theme.scrim,
        );
        let zlabel = format!("{}%", (self.zoom * 100.0) as i32);
        let btns: [(Rect, &str, NavBtn, bool); 5] = [
            (ch.prev, "‹", NavBtn::Prev, true),
            (ch.zminus, "−", NavBtn::ZoomMinus, false),
            (ch.zlabel, zlabel.as_str(), NavBtn::ZoomLabel, false),
            (ch.zplus, "+", NavBtn::ZoomPlus, false),
            (ch.next, "›", NavBtn::Next, true),
        ];
        for (r, label, btn, primary) in btns {
            let rr = Rect::new(r.x, r.y + ry, r.w, r.h);
            ui::draw_pill_button(
                canvas,
                theme,
                rr,
                label,
                primary,
                self.nav_pressed == Some(btn),
            );
        }
        if let Some(d) = self.doc.as_ref() {
            ui::draw_text(
                canvas,
                &d.name,
                &ui::caption(theme.text3),
                ui::MARGIN,
                strip_y - 26 + ry,
                Some(self.cur_w - ui::MARGIN * 2),
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Overview
// ---------------------------------------------------------------------------

impl PdfApp {
    fn render_overview(&mut self, canvas: &mut Canvas, theme: &Theme) {
        let w = canvas.size().w;
        let h = canvas.size().h;
        let ch = chrome(w, h);
        let view = overview_view(w, h);
        self.ov_scroll = view::clamp_scroll(self.ov_scroll, self.ov_max_scroll());

        let back_r = Rect::new(ch.back.x, ch.back.y, ch.back.w, ch.back.h);
        ui::draw_pill_button(canvas, theme, back_r, "‹ Back", false, self.back_pressed);
        // Compact title beside the back button (no draw_header — it would
        // collide with the pill).
        ui::draw_text(
            canvas,
            "Pages",
            &ui::title(theme.text),
            back_r.right() + 16,
            back_r.y + 2,
            Some((ch.grid_btn.x - back_r.right() - 32).max(60)),
        );
        ui::draw_text(
            canvas,
            &format!("{} pages", self.page_count()),
            &ui::caption(theme.text3),
            back_r.right() + 16,
            back_r.y + 34,
            Some((ch.grid_btn.x - back_r.right() - 32).max(60)),
        );

        if self.page_count() == 0 {
            ui::draw_centered(
                canvas,
                "No pages",
                &ui::body(theme.text3),
                Rect::new(0, h / 2 - 40, w, 80),
            );
            return;
        }

        // Queue thumbs for visible cells (same metrics as view::grid_cell).
        let cols = view::GRID_COLS.min(view.w / 80).max(1);
        let cell_w = (view.w - view::GRID_GAP * (cols - 1)) / cols;
        let row_h = cell_w * 4 / 3 + view::GRID_GAP;
        let first_row = (self.ov_scroll as i32 / row_h.max(1))
            .saturating_sub(1)
            .max(0) as usize;
        let last_row = ((self.ov_scroll as i32 + view.h) / row_h.max(1) + 2) as usize;
        let first = first_row * cols as usize;
        let last = ((last_row * cols as usize) + cols as usize).min(self.page_count());
        for i in first..last {
            self.request_thumb(i);
        }

        for i in 0..self.page_count() {
            let Some(cell) = view::grid_cell(view, i, view::GRID_COLS, self.ov_scroll) else {
                continue;
            };
            match self.thumbs.get(&i) {
                Some(Some(px)) => {
                    let inner = view::contain_rect(cell, px.size.w, px.size.h);
                    canvas.blit_scaled(px, inner, 1.0);
                }
                Some(None) => {
                    canvas.fill_rect(cell, theme.surface1);
                    ui::draw_centered(
                        canvas,
                        &format!("{}", i + 1),
                        &ui::caption(theme.text3),
                        cell,
                    );
                }
                None => {
                    canvas.fill_rect(cell, theme.surface1);
                    canvas.fill_rect(
                        Rect::new(cell.x, cell.y, cell.w, cell.h / 5),
                        theme.surface2,
                    );
                }
            }
            // Page number badge.
            ui::draw_text(
                canvas,
                &format!("{}", i + 1),
                &ui::caption(theme.text3),
                cell.x + 8,
                cell.y + cell.h - 26,
                Some(cell.w - 16),
            );
            if i == self.page {
                // Accent ring on the current page.
                canvas.fill_rect(
                    Rect::new(cell.x - 2, cell.y - 2, cell.w + 4, 3),
                    theme.accent,
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Error
// ---------------------------------------------------------------------------

impl PdfApp {
    fn render_error(&mut self, canvas: &mut Canvas, theme: &Theme) {
        let w = canvas.size().w;
        let h = canvas.size().h;
        let ch = chrome(w, h);
        let back_r = Rect::new(ch.back.x, ch.back.y, ch.back.w, ch.back.h);
        ui::draw_pill_button(canvas, theme, back_r, "‹ Files", false, self.back_pressed);
        let cy = h / 2 - 40;
        ui::draw_centered(
            canvas,
            "Can't open PDF",
            &ui::title(theme.text),
            Rect::new(0, cy, w, 60),
        );
        ui::draw_centered(
            canvas,
            &self.error,
            &ui::body(theme.text2),
            Rect::new(ui::MARGIN * 2, cy + 70, w - ui::MARGIN * 4, 80),
        );
        ui::draw_centered(
            canvas,
            "The file may be corrupt or password-protected.",
            &ui::caption(theme.text3),
            Rect::new(0, cy + 160, w, 40),
        );
    }
}

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    // Headless modes run before the GUI is touched at all — they must win
    // over the `%f` file-arg path below.
    if argv.iter().any(|a| a == "--mcp") {
        headless::serve_mcp();
    }
    if let Some(i) = argv.iter().position(|a| a == "--cli") {
        std::process::exit(headless::run_cli(&argv[i + 1..]));
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
        // `rabbiz-pdf <file.pdf>`: the desktop entry passes the document
        // via `%f`; the Wayland runner does not consume argv, so grab it
        // here before `run_app`.
        Some(_) if argv.len() == 1 => {
            let initial = doc::file_arg(argv.iter().map(String::as_str));
            rabbiz_app_kit::run_app(PdfApp::new(initial));
        }
        Some(_) => {
            eprintln!("error: too many arguments\n");
            eprint!("{}", headless::help_text());
            std::process::exit(2);
        }
        None => {
            rabbiz_app_kit::run_app(PdfApp::new(None));
        }
    }
}
