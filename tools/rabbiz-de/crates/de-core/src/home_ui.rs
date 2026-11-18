//! Live home-screen interaction: the finger-tracked page pager and the
//! long-press-and-drag icon rearrange (grid <-> dock). The maths lives in
//! `de_ui::pager` and `de_ui::home` (pure, tested); this module owns the
//! state, feeds it from the gesture stream and composes the frame.
//!
//! While a drag or settle is in progress the home screen is composed from
//! cached layers (base without icons + one transparent grid layer per page)
//! so each frame costs a few blits, not a full re-raster. At rest the
//! normal `de_ui::home::render` path is used unchanged.

use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::time::Instant;

use de_api::{Pixels, Point, Size};
use de_design::tokens;
use de_ui::home::{self, DropTarget, HomeHit};
use de_ui::nav::Screen;
use de_ui::pager::Pager;

use crate::state::{De, Foreground};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Axis {
    Undecided,
    Horizontal,
    Other,
}

#[derive(Clone, Copy)]
enum DragFrom {
    /// Global index in the grid order.
    Grid(usize),
    Dock(usize),
}

pub(crate) struct IconDrag {
    pub(crate) px: Pixels,
    dock_len: usize,
    pub(crate) id: String,
    from: DragFrom,
    pub(crate) pos: Point,
    pub(crate) target: Option<DropTarget>,
}

/// Pixel layers of the home screen while the pager moves: the icon-less
/// base, one icon grid and one dots strip per page. They are uploaded once
/// as textures; each frame only repositions them (no per-frame raster).
pub(crate) struct Layers {
    key: u64,
    pub(crate) base: Pixels,
    pub(crate) pages: HashMap<usize, Pixels>,
    pub(crate) dots: HashMap<usize, Pixels>,
}

pub(crate) struct HomeUi {
    pager: Pager,
    axis: Axis,
    last_tick: Option<Instant>,
    pub(crate) layers: Option<Layers>,
    pub(crate) drag: Option<IconDrag>,
    pub(crate) drag_tex: Option<Box<dyn std::any::Any>>,
    pub(crate) hl_tex: Option<Box<dyn std::any::Any>>,
    /// Uploaded copies of `layers` (renderer-specific texture ids).
    pub(crate) tex_key: u64,
    pub(crate) base_tex: Option<Box<dyn std::any::Any>>,
    pub(crate) page_tex: HashMap<usize, Box<dyn std::any::Any>>,
    pub(crate) dots_tex: HashMap<usize, Box<dyn std::any::Any>>,
}

impl HomeUi {
    /// Top-left of the lifted icon and the landing-spot highlight, if any.
    pub(crate) fn drag_overlay(&self, size: Size) -> Option<(Point, Option<de_api::Rect>)> {
        let d = self.drag.as_ref()?;
        let at = Point::new(d.pos.x - 56, d.pos.y - 112 - 24);
        let hl = d.target.map(|t| home::drop_rect(size, t, d.dock_len));
        Some((at, hl))
    }

    pub(crate) fn new() -> Self {
        Self {
            pager: Pager::new(1),
            axis: Axis::Undecided,
            last_tick: None,
            layers: None,
            drag: None,
            drag_tex: None,
            hl_tex: None,
            tex_key: 0,
            base_tex: None,
            page_tex: HashMap::new(),
            dots_tex: HashMap::new(),
        }
    }

    /// A pager drag or settle is in progress (compose from layers).
    pub(crate) fn paging(&self) -> bool {
        self.pager.animating()
    }

    /// Pager position in pages (for placing the layers).
    pub(crate) fn pos(&self) -> f32 {
        self.pager.pos()
    }

    pub(crate) fn layers_key(&self) -> Option<u64> {
        self.layers.as_ref().map(|l| l.key)
    }

    pub(crate) fn dragging_icon(&self) -> bool {
        self.drag.is_some()
    }
}

/// Everything the home screen shows, resolved once per use.
struct Geometry {
    size: Size,
    grid: Vec<String>,
    dock: Vec<String>,
}

impl Geometry {
    fn on_page(&self, page: usize) -> usize {
        self.grid
            .len()
            .saturating_sub(page * home::PAGE_SIZE)
            .min(home::PAGE_SIZE)
    }
}

impl De {
    fn home_geometry(&self) -> Geometry {
        let prefs = self.app_prefs.prefs();
        Geometry {
            size: Size::new(self.output.width as i32, self.output.height as i32),
            grid: self
                .catalog
                .home_order(prefs)
                .iter()
                .map(|e| e.id.clone())
                .collect(),
            dock: self
                .catalog
                .dock(prefs)
                .iter()
                .map(|e| e.id.clone())
                .collect(),
        }
    }

    /// The home screen is on show and free for direct manipulation.
    fn home_live(&self) -> bool {
        self.foreground == Foreground::Shell
            && self.nav.current == Screen::Home
            && self.morph.is_none()
            && !self.menu_is_open()
    }

    fn home_dirty(&mut self) {
        self.shell_dirty = true;
        self.needs_present = true;
    }

    /// A new touch began: forget the previous gesture's axis decision.
    pub(crate) fn home_touch_started(&mut self) {
        self.home_ui.axis = Axis::Undecided;
    }

    /// Finger moved in the content area (past the slop).
    pub(crate) fn home_drag_progress(&mut self, dx: f32, dy: f32) {
        if !self.home_live() || self.home_ui.dragging_icon() {
            return;
        }
        if self.home_ui.axis == Axis::Undecided {
            if dx.abs() > dy.abs() {
                let pages = home::pages_for(self.home_geometry().grid.len());
                let page = self.home_page;
                let ui = &mut self.home_ui;
                ui.pager.set_pages(pages);
                if !ui.pager.animating() {
                    ui.pager.jump_to(page);
                }
                ui.pager.begin();
                ui.axis = Axis::Horizontal;
            } else {
                self.home_ui.axis = Axis::Other;
            }
        }
        if self.home_ui.axis == Axis::Horizontal {
            let w = self.output.width as f32;
            self.home_ui.pager.drag(dx, w);
            self.home_page = self.home_ui.pager.page();
            self.home_dirty();
        }
    }

    /// Finger lifted after a content drag, with its release velocity (px/s).
    pub(crate) fn home_drag_released(&mut self, vx: f32) {
        if self.home_ui.axis == Axis::Horizontal && self.home_ui.pager.dragging() {
            let w = self.output.width as f32;
            self.home_ui.pager.release(vx, w);
            self.home_ui.last_tick = None;
            self.home_dirty();
        }
        self.home_ui.axis = Axis::Undecided;
    }

    /// The touch was cancelled: settle where we are, drop any lifted icon.
    pub(crate) fn home_drag_cancelled(&mut self) {
        self.home_drag_released(0.0);
        if self.home_ui.drag.take().is_some() {
            self.home_ui.drag_tex = None;
            self.home_dirty();
        }
    }

    /// Per-frame tick of the settle spring (called from `present_once`).
    pub(crate) fn advance_home(&mut self, now: Instant) {
        let ui = &mut self.home_ui;
        if !ui.pager.animating() || ui.pager.dragging() {
            return;
        }
        let dt = ui
            .last_tick
            .map_or(1.0 / 60.0, |t| now.duration_since(t).as_secs_f32())
            .clamp(0.001, 0.05);
        ui.last_tick = Some(now);
        let moving = ui.pager.advance(dt, &self.motion_clock);
        self.home_page = self.home_ui.pager.page();
        if !moving {
            self.home_ui.last_tick = None;
        }
        self.home_dirty();
    }

    /// A pager drag or settle is running (frame-time logging hook).
    pub(crate) fn home_ui_paging(&self) -> bool {
        self.home_ui.paging()
    }

    /// When the event loop must wake to draw the next settle frame. A
    /// finished frame with no damage requests no vblank, so the spring
    /// needs its own timer (same contract as the launch morph).
    pub(crate) fn home_wake_at(&self, interval: std::time::Duration) -> Option<Instant> {
        let ui = &self.home_ui;
        (ui.pager.animating() && !ui.pager.dragging()).then(|| Instant::now() + interval)
    }

    /// Long-press on the home screen: lift the icon under the finger.
    /// Returns `false` when nothing icon-like is there (the caller opens
    /// the empty-home menu instead).
    pub(crate) fn home_lift_icon(&mut self, pos: Point) -> bool {
        if !self.home_live() {
            eprintln!("home: long-press ignored (home not live)");
            return false;
        }
        let g = self.home_geometry();
        let on_page = g.on_page(self.home_page);
        let (id, from) = match home::hit_test(g.size, pos, on_page, g.dock.len()) {
            Some(HomeHit::Grid(i)) => {
                let global = self.home_page * home::PAGE_SIZE + i;
                (g.grid[global].clone(), DragFrom::Grid(global))
            }
            Some(HomeHit::Dock(i)) => match g.dock.get(i) {
                Some(id) => (id.clone(), DragFrom::Dock(i)),
                None => return false,
            },
            _ => return false,
        };
        eprintln!("home: lifted {id}");
        let px = {
            let side = 112;
            let mut c = de_paint::Canvas::new(Size::new(side, side));
            if let Some(icon) = self.app_icon(&id).icon {
                c.blit_scaled(&icon, de_api::Rect::new(0, 0, side, side), 0.95);
            }
            c.into_pixels()
        };
        self.home_ui.drag = Some(IconDrag {
            px,
            dock_len: g.dock.len(),
            id,
            from,
            pos,
            target: None,
        });
        self.home_dirty();
        true
    }

    pub(crate) fn home_icon_dragged(&mut self, pos: Point) {
        let g = self.home_geometry();
        if let Some(d) = self.home_ui.drag.as_mut() {
            d.pos = pos;
            let target = home::drop_target(g.size, pos, g.dock.len());
            if target != d.target {
                d.target = target;
                // The gap pocket moved: re-raster the home frame once per
                // slot transition (never per finger pixel).
                self.shell_dirty = true;
            }
            self.needs_present = true;
        }
    }

    /// Finger lifted: apply the rearrangement (grid <-> dock, persisted).
    pub(crate) fn home_icon_dropped(&mut self, pos: Point) {
        self.home_icon_dragged(pos);
        let Some(d) = self.home_ui.drag.take() else {
            return;
        };
        self.home_ui.drag_tex = None;
        let g = self.home_geometry();
        eprintln!("home: dropped {} on {:?}", d.id, d.target);
        match d.target {
            Some(DropTarget::Grid(i)) => {
                let global = self.home_page * home::PAGE_SIZE + i;
                let unchanged = matches!(d.from, DragFrom::Grid(o) if o == global);
                if !unchanged {
                    self.app_prefs.drop_on_grid(&d.id, global, &g.grid);
                }
            }
            Some(DropTarget::Dock(slot)) => {
                let unchanged = matches!(d.from, DragFrom::Dock(o) if o == slot);
                let swap_to = match d.from {
                    DragFrom::Grid(o) => o,
                    DragFrom::Dock(_) => 0,
                };
                if !unchanged {
                    self.app_prefs.drop_on_dock(&d.id, slot, swap_to, &g.grid);
                }
            }
            None => {}
        }
        // The grid may have shrunk below the current page.
        let pages = home::pages_for(self.home_geometry().grid.len());
        self.home_page = self.home_page.min(pages - 1);
        self.home_ui.pager.set_pages(pages);
        self.home_ui.pager.jump_to(self.home_page);
        self.home_dirty();
    }

    /// The layered home as one image (screen captures while paging).
    pub(crate) fn home_layers_composed(&self) -> Option<Pixels> {
        let l = self.home_ui.layers.as_ref()?;
        let size = l.base.size;
        let pos = self.home_ui.pager.pos();
        let mut c = de_paint::Canvas::new(size);
        c.blit(&l.base, Point::ZERO);
        for (p, px) in &l.pages {
            let x = ((*p as f32 - pos) * size.w as f32).round() as i32;
            c.blit(px, Point::new(x, home::grid_band_y()));
        }
        let near = pos.round().max(0.0) as usize;
        if let Some(d) = l.dots.get(&near) {
            c.blit(d, Point::new(0, home::dots_y(size)));
        }
        Some(c.into_pixels())
    }

    /// Whether the compositor should show the layered home this frame.
    pub(crate) fn home_layers_active(&self) -> bool {
        self.home_ui.paging() && self.home_live()
    }

    /// Bring the layers up to date for the pager's current position
    /// (builds only what changed; the first frame of a drag pays once).
    pub(crate) fn home_layers_frame(&mut self) {
        let size = Size::new(self.output.width as i32, self.output.height as i32);
        let t = tokens::DEFAULT;
        let s = self.strings.clone();
        let (status, date) = self.shell_meta();
        let prefs = self.app_prefs.prefs().clone();
        let grid: Vec<String> = self
            .catalog
            .home_order(&prefs)
            .iter()
            .map(|e| e.id.clone())
            .collect();
        let dock_ids: Vec<String> = self.catalog.dock(&prefs).iter().map(|e| e.id.clone()).collect();
        let page_count = home::pages_for(grid.len());
        let mut h = DefaultHasher::new();
        (status.clock.as_str(), date.as_str(), &grid, &dock_ids, prefs.wallpaper_id.as_str()).hash(&mut h);
        let key = h.finish();
        if self.home_ui.layers.as_ref().map(|l| l.key) != Some(key) {
            let wallpaper = self.wallpaper_for(&prefs.wallpaper_id, size);
            let dock = self.app_icons(&dock_ids);
            let model = home::HomeModel {
                status: status.clone(),
                clock: &status.clock,
                date: &date,
                charging: "",
                wallpaper: Some(&wallpaper),
                apps: &[],
                dock: &dock,
                page: 0,
                page_count: 1, // dots travel as their own layer
            };
            self.home_ui.layers = Some(Layers {
                key,
                base: home::render(size, &s, &t, &model),
                pages: HashMap::new(),
                dots: HashMap::new(),
            });
        }
        let pos = self.home_ui.pager.pos();
        let first = pos.floor() as i64;
        let near = pos.round().max(0.0) as usize;
        let mut want: Vec<usize> = [first, first + 1]
            .into_iter()
            .filter(|p| *p >= 0 && (*p as usize) < page_count)
            .map(|p| p as usize)
            .collect();
        want.dedup();
        for p in want {
            if self.home_ui.layers.as_ref().is_some_and(|l| l.pages.contains_key(&p)) {
                continue;
            }
            let ids: Vec<String> = grid
                .iter()
                .skip(p * home::PAGE_SIZE)
                .take(home::PAGE_SIZE)
                .cloned()
                .collect();
            let icons = self.app_icons(&ids);
            let px = home::render_grid(size, &t, &icons);
            if let Some(l) = self.home_ui.layers.as_mut() {
                l.pages.insert(p, px);
            }
        }
        if page_count > 1 {
            if let Some(l) = self.home_ui.layers.as_mut() {
                let near = near.min(page_count - 1);
                l.dots
                    .entry(near)
                    .or_insert_with(|| home::render_dots(size, &t, page_count, near));
            }
        }
    }

    /// The cover-scaled wallpaper for `id` (cached).
    pub(crate) fn wallpaper_for(&mut self, id: &str, size: Size) -> Pixels {
        match &self.wallpaper {
            Some((cached, px)) if cached == id => px.clone(),
            _ => {
                let px = de_paint::wallpaper::bundled(id, size);
                self.wallpaper = Some((id.to_owned(), px.clone()));
                px
            }
        }
    }
}
