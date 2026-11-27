//! Application state, decode pump, viewer navigation, and frame loop for
//! `rabbiz-photos`.

mod input;
mod render;

use std::collections::{HashMap, HashSet, VecDeque};
use std::time::{Duration, Instant};

use de_api::{Pixels, Point};
use de_design::motion::{bezier_eval, BEZIER_STANDARD};
use rabbiz_app_kit::ui::Tween;
use rabbiz_app_kit::{AppEvent, AppFrame, AppState, Canvas, ScrollView, TextInputCtl, Theme};

use crate::decode;
use crate::layout::{self, Group, Layout};
use crate::photos::{self, Photo};
use crate::viewer;

/// Grid thumbnail cache cap (~240 px RGBA ≈ 230 KB each → ~15 MB worst case).
const THUMB_CACHE_CAP: usize = 64;
const OPEN_MS: f32 = 240.0;
const CLOSE_MS: f32 = 180.0;
const SLIDE_MS: f32 = 210.0;
const SETTLE_MS: f32 = 140.0;
/// Viewer chrome fade duration (seconds).
const CHROME_FADE_S: f32 = 0.18;
/// How long a transient toast stays up.
const TOAST_MS: u64 = 2500;
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

pub(crate) struct PhotosApp {
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
    /// Scroll physics (drag, rubber-band, fling) — `offset()` is the scroll.
    sv: ScrollView,
    /// Monotonic epoch for `ScrollView`'s velocity sampling.
    clock0: Instant,
    pressed_cell: Option<usize>,
    pending: VecDeque<usize>,
    thumbs: HashMap<usize, Pixels>,
    thumb_failed: HashSet<usize>,
    refresh_pressed: bool,
    rescan_pressed: bool,

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
    info_pressed: bool,
    delete_pressed: bool,
    /// Overlay chrome visibility target; `chrome_fade` animates the alpha.
    chrome_vis: bool,
    chrome_fade: Tween,
    /// Expanded metadata line in the bottom strip (info action).
    show_info: bool,
    info_dimensions: Option<(i32, i32)>,
    /// Delete-confirmation dialog is up.
    del_dialog: bool,
    del_pressed: Option<usize>,
    del_scrim: bool,
    /// Transient snackbar message + expiry.
    toast: Option<(String, Instant)>,
}

pub fn run() {
    rabbiz_app_kit::run_app(PhotosApp::new());
}

impl PhotosApp {
    fn new() -> Self {
        let photos = photos::scan(&photos::default_dirs());
        let groups = Self::day_groups(&photos);
        Self {
            photos,
            groups,
            screen: Screen::Grid,
            cur_w: 0,
            cur_h: 0,
            layout: Layout::default(),
            view_top: 0,
            view_h: 0,
            sv: ScrollView::new(),
            clock0: Instant::now(),
            pressed_cell: None,
            pending: VecDeque::new(),
            thumbs: HashMap::new(),
            thumb_failed: HashSet::new(),
            refresh_pressed: false,
            rescan_pressed: false,
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
            info_pressed: false,
            delete_pressed: false,
            chrome_vis: true,
            chrome_fade: Tween::new(1.0, 1.0, CHROME_FADE_S, BEZIER_STANDARD),
            show_info: false,
            info_dimensions: None,
            del_dialog: false,
            del_pressed: None,
            del_scrim: false,
            toast: None,
        }
    }

    /// Group an mtime-sorted photo list into contiguous day buckets.
    fn day_groups(photos: &[Photo]) -> Vec<Group> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
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
        groups
    }

    /// Milliseconds since app start for `ScrollView` velocity sampling.
    fn now_ms(&self) -> u64 {
        self.clock0.elapsed().as_millis() as u64
    }

    /// Re-scan the library roots and reset every derived cache — wired to
    /// the grid header action, the empty-state button, and delete.
    fn rescan(&mut self) {
        self.photos = photos::scan(&photos::default_dirs());
        self.show_info = false;
        self.info_dimensions = None;
        self.last_up = None;
        self.groups = Self::day_groups(&self.photos);
        self.layout = Layout::default();
        self.pending.clear();
        self.thumbs.clear();
        self.thumb_failed.clear();
        self.v_px.clear();
        self.v_queued.clear();
        self.pressed_cell = None;
        self.sv.scroll_to(0.0);
        if self.screen == Screen::Viewer {
            if self.photos.is_empty() {
                self.screen = Screen::Grid;
                self.anim = None;
                self.del_dialog = false;
            } else {
                self.v_index = self.v_index.min(self.photos.len() - 1);
                self.v_zoom = 1.0;
                self.v_pan = (0.0, 0.0);
                self.request_viewer(self.v_index);
            }
        }
    }

    /// Delete the currently viewed photo from disk (dialog-confirmed), then
    /// rescan so the grid and viewer indexes stay consistent.
    fn delete_current(&mut self) {
        let Some(p) = self.photos.get(self.v_index).cloned() else {
            return;
        };
        let msg = match std::fs::remove_file(&p.path) {
            Ok(()) => {
                self.rescan();
                format!("Deleted {}", p.name)
            }
            Err(e) => format!("Couldn't delete {}: {e}", p.name),
        };
        self.toast = Some((msg, Instant::now() + Duration::from_millis(TOAST_MS)));
    }

    /// Toggle viewer chrome visibility with a fade.
    fn toggle_chrome(&mut self) {
        self.chrome_vis = !self.chrome_vis;
        let target = if self.chrome_vis { 1.0 } else { 0.0 };
        self.chrome_fade = Tween::new(
            self.chrome_fade.value(),
            target,
            CHROME_FADE_S,
            BEZIER_STANDARD,
        );
    }

    /// Current chrome opacity 0..=1 (drives every chrome draw).
    fn chrome_alpha(&self) -> f32 {
        self.chrome_fade.value().clamp(0.0, 1.0)
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
            || !self.chrome_fade.done()
            || self.toast.is_some()
            || (self.screen == Screen::Grid && (!self.pending.is_empty() || self.sv.is_animating()))
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
        self.show_info = false;
        self.info_dimensions = None;
        self.last_up = None;
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
        // Chrome auto-shows on page open (fades in).
        self.chrome_vis = true;
        self.chrome_fade = Tween::new(0.0, 1.0, CHROME_FADE_S, BEZIER_STANDARD);
        self.show_info = false;
        self.info_dimensions = None;
        self.last_up = None;
        self.del_dialog = false;
        self.del_pressed = None;
        self.del_scrim = false;
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
        self.info_pressed = false;
        self.delete_pressed = false;
        self.del_dialog = false;
        self.del_pressed = None;
        self.del_scrim = false;
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

    // ---------------- per-frame ----------------

    fn tick(&mut self, dt: Duration) -> AppFrame {
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
        if !self.chrome_fade.done() {
            self.chrome_fade.tick(dt);
            dirty = true;
        }
        if self.screen == Screen::Grid {
            let was = self.sv.offset_i32();
            let flinging = self.sv.tick(dt.as_secs_f32());
            dirty |= flinging || self.sv.offset_i32() != was;
        }
        if let Some((_, until)) = &self.toast {
            if Instant::now() >= *until {
                self.toast = None;
                dirty = true;
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
        let vis = layout::visible_indices(&self.layout, self.view_h, self.sv.offset());
        self.pending = vis
            .into_iter()
            .filter(|i| !self.thumbs.contains_key(i) && !self.thumb_failed.contains(i))
            .collect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use de_api::Size;
    use rabbiz_app_kit::TestHarness;
    use std::path::Path;
    use std::time::Duration;

    fn write_photo(dir: &Path, name: &str, w: i32, h: i32, rgb: [u8; 3]) {
        let mut px = Pixels::empty(Size::new(w, h));
        for i in (0..px.data.len()).step_by(4) {
            px.data[i] = rgb[0];
            px.data[i + 1] = rgb[1];
            px.data[i + 2] = rgb[2];
            px.data[i + 3] = 255;
        }
        std::fs::write(dir.join(name), de_paint::encode_png(&px).unwrap()).unwrap();
    }

    /// Drive the app through grid → viewer → chrome-toggle and save PNG
    /// evidence to /tmp/rabbiz-shots/photos-*.png.
    #[test]
    fn evidence_screens() {
        let home = std::env::temp_dir().join(format!("rabbiz-photos-home-{}", std::process::id()));
        let pics = home.join("Pictures");
        std::fs::create_dir_all(&pics).unwrap();
        for i in 0..6i32 {
            write_photo(
                &pics,
                &format!("p{i}.png"),
                40 + i * 8,
                30 + i * 4,
                [40 + (i as u8) * 30, 90, 220 - (i as u8) * 20],
            );
        }
        let shots = Path::new("/tmp/rabbiz-shots");

        // --- populated library ---
        let mut app = PhotosApp::new();
        app.photos = photos::scan(&[pics]);
        app.groups = PhotosApp::day_groups(&app.photos);
        let mut h = TestHarness::new(app);
        let _ = h.render(); // builds layout + queues thumbs
        for _ in 0..60 {
            h.tick(Duration::from_millis(16));
            if !h.last_frame().animating {
                break;
            }
        }
        h.save_png(&shots.join("photos-grid.png")).unwrap();

        let cell = h.state().layout.cells[0].rect;
        let top = h.state().view_top;
        let scroll = h.state().sv.offset();
        h.tap(Point::new(
            cell.x + cell.w / 2,
            top + cell.y - scroll as i32 + cell.h / 2,
        ));
        h.settle(80, Duration::from_millis(16));
        h.save_png(&shots.join("photos-viewer-on.png")).unwrap();

        // Single tap on the photo (chrome toggle after the restyle).
        h.tap(Point::new(360, 800));
        h.settle(60, Duration::from_millis(16));
        h.save_png(&shots.join("photos-viewer-tap.png")).unwrap();

        assert!(!h.state().chrome_vis);

        // Restore the controls with a separate tap before going back.
        h.tap(Point::new(240, 800));
        h.settle(60, Duration::from_millis(16));
        assert!(h.state().chrome_vis);
        let m = rabbiz_app_kit::ui::MARGIN;
        let ts = rabbiz_app_kit::ui::TOP_SAFE;
        h.tap(Point::new(m + 24, ts + 36));
        h.settle(80, Duration::from_millis(16));
        assert_eq!(h.state().screen, Screen::Grid);
        h.save_png(&shots.join("photos-back.png")).unwrap();

        // --- empty library ---
        let mut app = PhotosApp::new();
        app.photos.clear();
        app.groups.clear();
        let mut h2 = TestHarness::new(app);
        h2.save_png(&shots.join("photos-empty.png")).unwrap();

        let _ = std::fs::remove_dir_all(&home);
    }
}

impl AppState for PhotosApp {
    fn app_id(&self) -> &'static str {
        "rabbiz-photos"
    }

    fn title(&self) -> &'static str {
        "Photos"
    }

    fn handle_event(&mut self, event: AppEvent, ti: &TextInputCtl) -> AppFrame {
        self.handle_app_event(event, ti)
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
