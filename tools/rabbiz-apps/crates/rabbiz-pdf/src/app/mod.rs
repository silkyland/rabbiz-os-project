//! PDF reader application state, chrome geometry, and document/page render cache.

mod input;
mod screens;
mod viewer;

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::time::Instant;

use de_api::{Pixels, Point, Rect};
use rabbiz_app_kit::ui;

use crate::doc::{self, Doc, DocError, PdfFile};
use crate::view;

pub(super) const OPEN_MS: f32 = 240.0;
pub(super) const CLOSE_MS: f32 = 180.0;
pub(super) const SLIDE_MS: f32 = 210.0;
pub(super) const SETTLE_MS: f32 = 140.0;
/// Viewer chrome fade duration (seconds).
pub(super) const CHROME_FADE_S: f32 = 0.18;
/// Full-page pixmap cache cap in bytes (~120 MB worst case; typical use is
/// far below since only ±2 pages per zoom bucket are kept).
const PAGE_CACHE_BYTES: usize = 120 * 1024 * 1024;
/// Overview thumbnail cache cap (count, not bytes — thumbs are ~300 KB).
const THUMB_CACHE_CAP: usize = 64;
/// Picker row height.
pub(super) const ROW_H: i32 = 72;
/// Y where the scrollable list/grid starts (below `ui::draw_header`).
pub(super) const LIST_TOP: i32 = ui::TOP_SAFE + 104;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Screen {
    Picker,
    Viewer,
    Overview,
    Error,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Anim {
    pub(super) open: bool,
    pub(super) t: f32,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Slide {
    pub(super) from: usize,
    pub(super) to: usize,
    /// Finger offset at release; sign picks the direction.
    pub(super) u: f32,
    pub(super) t: f32,
}

/// One page to draw in the viewer: page index, zoom-step bucket, pixel
/// offsets for slide/open motion, and whether the live zoom/pan applies.
#[derive(Debug, Clone, Copy)]
pub(super) struct PageDraw {
    pub(super) idx: usize,
    pub(super) z: usize,
    pub(super) dx: f32,
    pub(super) rise: f32,
    pub(super) apply_transform: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Gesture {
    Undecided,
    Pan,
    Swipe,
    Ignore,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Touch {
    pub(super) id: i32,
    pub(super) down: Point,
    pub(super) prev: Point,
    pub(super) mode: Gesture,
    pub(super) last_t: Instant,
    pub(super) vx: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NavBtn {
    Prev,
    Next,
    ZoomMinus,
    ZoomPlus,
    ZoomLabel,
}

/// Chrome geometry shared by hit-testing and drawing — keep this the single
/// source for every interactive viewer rect.
#[derive(Debug, Clone, Copy)]
pub(super) struct Chrome {
    pub(super) back: Rect,
    pub(super) grid_btn: Rect,
    /// Full-bleed page area — overlay chrome floats above it.
    pub(super) content: Rect,
    /// Floating bottom pill backing the nav buttons.
    pub(super) pill: Rect,
    pub(super) prev: Rect,
    pub(super) next: Rect,
    pub(super) zminus: Rect,
    /// Center of the pill: zoom percentage. Tap to reset.
    pub(super) zlabel: Rect,
    pub(super) zplus: Rect,
}

pub(super) fn chrome(w: i32, h: i32) -> Chrome {
    const BTN: i32 = 48;
    let counter_w = (w - BTN * 4 - 32).clamp(52, 92);
    const PAD: i32 = 8;
    const GAP: i32 = 4;
    const PILL_H: i32 = 64;

    let top_y = ui::TOP_SAFE + 8;
    let back = Rect::new(ui::MARGIN, top_y, BTN, BTN);
    let grid_btn = Rect::new(w - ui::MARGIN - BTN, top_y, BTN, BTN);

    // Floating bottom pill: previous, zoom controls, next.
    let pill_w = PAD * 2 + BTN * 4 + counter_w + GAP * 4;
    let pill = Rect::new(
        (w - pill_w) / 2,
        h - ui::BOTTOM_SAFE - 12 - PILL_H,
        pill_w,
        PILL_H,
    );
    let by = pill.y + (PILL_H - BTN) / 2;
    let mut x = pill.x + PAD;
    let prev = Rect::new(x, by, BTN, BTN);
    x += BTN + GAP;
    let zminus = Rect::new(x, by, BTN, BTN);
    x += BTN + GAP;
    let zlabel = Rect::new(x, by, counter_w, BTN);
    x += counter_w + GAP;
    let zplus = Rect::new(x, by, BTN, BTN);
    x += BTN + GAP;
    let next = Rect::new(x, by, BTN, BTN);

    Chrome {
        back,
        grid_btn,
        content: Rect::new(0, 0, w, h),
        pill,
        prev,
        next,
        zminus,
        zlabel,
        zplus,
    }
}

pub(super) fn nav_hit(ch: Chrome, pos: Point) -> Option<NavBtn> {
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
pub(super) fn overview_view(w: i32, h: i32) -> Rect {
    Rect::new(
        ui::MARGIN,
        LIST_TOP,
        w - ui::MARGIN * 2,
        h - ui::BOTTOM_SAFE - LIST_TOP,
    )
}

pub(super) struct PdfApp {
    pub(super) files: Vec<PdfFile>,
    pub(super) doc: Option<Doc>,
    /// Page sizes in points, fetched lazily per page.
    pub(super) page_pts: HashMap<usize, (f32, f32)>,
    pub(super) screen: Screen,
    pub(super) error: String,

    pub(super) cur_w: i32,
    pub(super) cur_h: i32,

    // ---- picker ----
    pub(super) picker_scroll: f32,
    pub(super) pressed_file: Option<usize>,

    // ---- overview ----
    pub(super) ov_scroll: f32,

    // ---- viewer ----
    pub(super) page: usize,
    pub(super) zoom: f32,
    /// Which `view::ZOOM_STEPS` index the current zoom is (cache key + label).
    pub(super) zoom_i: usize,
    pub(super) pan: (f32, f32),
    /// (page, zoom bucket) → pixels; `None` value = render failed.
    pub(super) pages: HashMap<(usize, usize), Option<Pixels>>,
    pub(super) page_queue: VecDeque<(usize, usize, f32)>, // idx, zoom bucket, px_per_pt
    pub(super) queued: HashSet<(usize, usize)>,
    pub(super) thumb_queue: VecDeque<usize>,
    pub(super) thumbs: HashMap<usize, Option<Pixels>>,
    pub(super) thumb_queued: HashSet<usize>,
    pub(super) touch: Option<Touch>,
    pub(super) last_up: Option<(Instant, Point)>,
    pub(super) drag_dx: f32,
    pub(super) slide: Option<Slide>,
    pub(super) settle: Option<(f32, f32)>,
    pub(super) anim: Option<Anim>,
    pub(super) back_pressed: bool,
    pub(super) grid_pressed: bool,
    pub(super) nav_pressed: Option<NavBtn>,
    /// Viewer overlay-chrome visibility target; `chrome_fade` animates it.
    pub(super) chrome_vis: bool,
    pub(super) chrome_fade: ui::Tween,
    /// Picker header rescan / empty-state rescan / error retry presses.
    pub(super) picker_action_pressed: bool,
    pub(super) rescan_pressed: bool,
    pub(super) retry_pressed: bool,
    /// Last path `open_path` was asked to load — the error screen's Retry.
    pub(super) last_path: Option<PathBuf>,
}

impl PdfApp {
    pub(super) fn new(initial: Option<PathBuf>) -> Self {
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
            chrome_vis: true,
            chrome_fade: ui::Tween::new(
                1.0,
                1.0,
                CHROME_FADE_S,
                de_design::motion::BEZIER_STANDARD,
            ),
            picker_action_pressed: false,
            rescan_pressed: false,
            retry_pressed: false,
            last_path: None,
        };
        if let Some(path) = initial {
            app.open_path(&path);
        }
        app
    }

    // ---------------- document ----------------

    pub(super) fn open_path(&mut self, path: &Path) {
        self.last_path = Some(path.to_path_buf());
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

    pub(super) fn open_file(&mut self, idx: usize) {
        let Some(f) = self.files.get(idx) else {
            return;
        };
        let path = f.path.clone();
        self.open_path(&path);
    }

    pub(super) fn enter_viewer(&mut self, page: usize) {
        self.page = page.min(self.page_count().saturating_sub(1));
        self.zoom = 1.0;
        self.zoom_i = 0;
        self.pan = self.home_pan_for(self.page);
        self.drag_dx = 0.0;
        self.slide = None;
        self.settle = None;
        self.screen = Screen::Viewer;
        self.anim = Some(Anim { open: true, t: 0.0 });
        // Chrome auto-shows on page open (fades in).
        self.chrome_vis = true;
        self.chrome_fade =
            ui::Tween::new(0.0, 1.0, CHROME_FADE_S, de_design::motion::BEZIER_STANDARD);
        self.request_page(self.page, self.zoom_i);
        self.prefetch_neighbors();
    }

    /// Re-scan the picker roots (header action + empty-state button).
    pub(super) fn rescan_picker(&mut self) {
        self.files = doc::scan(&doc::default_dirs());
        self.picker_scroll = 0.0;
        self.pressed_file = None;
    }

    /// Toggle viewer chrome visibility with a fade.
    pub(super) fn toggle_chrome(&mut self) {
        self.chrome_vis = !self.chrome_vis;
        let target = if self.chrome_vis { 1.0 } else { 0.0 };
        self.chrome_fade = ui::Tween::new(
            self.chrome_fade.value(),
            target,
            CHROME_FADE_S,
            de_design::motion::BEZIER_STANDARD,
        );
    }

    /// Current chrome opacity 0..=1 (drives every chrome draw).
    pub(super) fn chrome_alpha(&self) -> f32 {
        self.chrome_fade.value().clamp(0.0, 1.0)
    }

    pub(super) fn go_picker(&mut self) {
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
        self.picker_action_pressed = false;
        self.rescan_pressed = false;
        self.retry_pressed = false;
    }

    pub(super) fn page_count(&self) -> usize {
        self.doc.as_ref().map_or(0, Doc::page_count)
    }

    pub(super) fn page_pt(&mut self, idx: usize) -> (f32, f32) {
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

    pub(super) fn request_page(&mut self, idx: usize, z: usize) {
        let n = self.page_count();
        if idx >= n || self.pages.contains_key(&(idx, z)) || self.queued.contains(&(idx, z)) {
            return;
        }
        let scale = self.render_scale(idx, z);
        self.queued.insert((idx, z));
        self.page_queue.push_back((idx, z, scale));
    }

    pub(super) fn prefetch_neighbors(&mut self) {
        let n = self.page_count();
        for d in [-2, -1, 1, 2] {
            let idx = self.page as i32 + d;
            if idx >= 0 && (idx as usize) < n {
                self.request_page(idx as usize, self.zoom_i);
            }
        }
        self.evict_page_cache();
    }

    pub(super) fn request_thumb(&mut self, idx: usize) {
        let n = self.page_count();
        if idx >= n || self.thumbs.contains_key(&idx) || self.thumb_queued.contains(&idx) {
            return;
        }
        self.thumb_queued.insert(idx);
        self.thumb_queue.push_back(idx);
    }

    /// Render at most one queued item: viewer pages first (they gate the UI),
    /// then overview thumbs.
    pub(super) fn pump(&mut self) {
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

    pub(super) fn busy(&self) -> bool {
        self.anim.is_some()
            || self.slide.is_some()
            || self.settle.is_some()
            || !self.chrome_fade.done()
            || !self.page_queue.is_empty()
            || !self.thumb_queue.is_empty()
    }
}

pub fn run(initial: Option<PathBuf>) {
    rabbiz_app_kit::run_app(PdfApp::new(initial));
}

#[cfg(test)]
mod tests {
    use super::*;
    use rabbiz_app_kit::TestHarness;
    use std::time::Duration;

    /// Drive picker → viewer → chrome-toggle → overview → back and save PNG
    /// evidence to /tmp/rabbiz-shots/pdf-*.png.
    #[test]
    fn evidence_screens() {
        let home = std::env::temp_dir().join(format!("rabbiz-pdf-home-{}", std::process::id()));
        let docs = home.join("Documents");
        std::fs::create_dir_all(&docs).unwrap();
        let pdf_path = docs.join("hello.pdf");
        std::fs::write(&pdf_path, crate::doc::tiny_pdf()).unwrap();
        let shots = std::path::Path::new("/tmp/rabbiz-shots");
        let mut app = PdfApp::new(None);
        app.files = doc::scan(&[docs]);
        let mut h = TestHarness::new(app);
        let _ = h.render(); // sets cur_w/cur_h
        h.save_png(&shots.join("pdf-picker.png")).unwrap();

        // Open the first document row.
        h.tap(Point::new(360, LIST_TOP + ROW_H / 2));
        h.settle(120, Duration::from_millis(16));
        h.save_png(&shots.join("pdf-viewer-on.png")).unwrap();

        // Single tap on the page (chrome toggle after the restyle).
        h.tap(Point::new(360, 800));
        h.settle(60, Duration::from_millis(16));
        h.save_png(&shots.join("pdf-viewer-tap.png")).unwrap();

        assert!(!h.state().chrome_vis);

        // Reveal the chrome again with a separate tap, then open the page overview.
        h.tap(Point::new(240, 800));
        h.settle(60, Duration::from_millis(16));
        let ch = chrome(720, 1600);
        h.tap(ch.grid_btn.center());
        h.settle(120, Duration::from_millis(16));
        assert_eq!(h.state().screen, Screen::Overview);
        h.save_png(&shots.join("pdf-overview.png")).unwrap();

        // Back from overview — should return to the viewer.
        h.tap(ch.back.center());
        h.settle(120, Duration::from_millis(16));
        assert_eq!(h.state().screen, Screen::Viewer);
        h.save_png(&shots.join("pdf-after-back.png")).unwrap();

        let _ = std::fs::remove_dir_all(&home);
    }
}
