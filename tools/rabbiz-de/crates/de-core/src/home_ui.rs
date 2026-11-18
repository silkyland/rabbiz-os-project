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

struct IconDrag {
    id: String,
    from: DragFrom,
    pos: Point,
    target: Option<DropTarget>,
}

struct Layers {
    key: u64,
    base: Pixels,
    pages: HashMap<usize, Pixels>,
}

pub(crate) struct HomeUi {
    pager: Pager,
    axis: Axis,
    last_tick: Option<Instant>,
    layers: Option<Layers>,
    drag: Option<IconDrag>,
}

impl HomeUi {
    pub(crate) fn new() -> Self {
        Self {
            pager: Pager::new(1),
            axis: Axis::Undecided,
            last_tick: None,
            layers: None,
            drag: None,
        }
    }

    /// A pager drag or settle is in progress (compose from layers).
    pub(crate) fn paging(&self) -> bool {
        self.pager.animating()
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

    /// Long-press on the home screen: lift the icon under the finger.
    /// Returns `false` when nothing icon-like is there (the caller opens
    /// the empty-home menu instead).
    pub(crate) fn home_lift_icon(&mut self, pos: Point) -> bool {
        if !self.home_live() {
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
        self.home_ui.drag = Some(IconDrag {
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
            d.target = home::drop_target(g.size, pos, g.dock.len());
            self.home_dirty();
        }
    }

    /// Finger lifted: apply the rearrangement (grid <-> dock, persisted).
    pub(crate) fn home_icon_dropped(&mut self, pos: Point) {
        self.home_icon_dragged(pos);
        let Some(d) = self.home_ui.drag.take() else {
            return;
        };
        let g = self.home_geometry();
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

    /// Compose the home frame from cached layers while paging.
    pub(crate) fn home_composed(
        &mut self,
        size: Size,
        s: &de_ui::i18n::Strings,
        t: &tokens::Theme,
        m: &home::HomeModel<'_>,
        grid_ids: &[String],
    ) -> Pixels {
        let mut h = DefaultHasher::new();
        (
            m.clock,
            m.date,
            &grid_ids,
            self.app_prefs.prefs().wallpaper_id.as_str(),
        )
            .hash(&mut h);
        m.dock.iter().for_each(|e| e.id.hash(&mut h));
        let key = h.finish();
        if self.home_ui.layers.as_ref().map(|l| l.key) != Some(key) {
            let mut base_model = home::HomeModel {
                status: m.status.clone(),
                clock: m.clock,
                date: m.date,
                charging: m.charging,
                wallpaper: m.wallpaper,
                apps: &[],
                dock: m.dock,
                page: self.home_ui.pager.page(),
                page_count: m.page_count,
            };
            base_model.page = self.home_ui.pager.page();
            self.home_ui.layers = Some(Layers {
                key,
                base: home::render(size, s, t, &base_model),
                pages: HashMap::new(),
            });
        }
        let pos = self.home_ui.pager.pos();
        let first = pos.floor() as i64;
        let mut canvas = de_paint::Canvas::new(size);
        let layers = self.home_ui.layers.as_ref().expect("layers just built");
        canvas.blit(&layers.base, Point::ZERO);
        let mut needed = Vec::new();
        for p in [first, first + 1] {
            if p >= 0 && (p as usize) < m.page_count {
                needed.push(p as usize);
            }
        }
        for p in needed {
            if !self
                .home_ui
                .layers
                .as_ref()
                .is_some_and(|l| l.pages.contains_key(&p))
            {
                let ids: Vec<String> = grid_ids
                    .iter()
                    .skip(p * home::PAGE_SIZE)
                    .take(home::PAGE_SIZE)
                    .cloned()
                    .collect();
                let icons = self.app_icons(&ids);
                let px = home::render_grid(size, t, &icons);
                if let Some(l) = self.home_ui.layers.as_mut() {
                    l.pages.insert(p, px);
                }
            }
            let x = ((p as f32 - pos) * size.w as f32).round() as i32;
            if let Some(px) = self.home_ui.layers.as_ref().and_then(|l| l.pages.get(&p)) {
                canvas.blit(px, Point::new(x, 0));
            }
        }
        canvas.into_pixels()
    }

    /// Paint the lifted icon and its landing spot over the home frame.
    pub(crate) fn overlay_icon_drag(&mut self, base: Pixels) -> Pixels {
        let Some(d) = self.home_ui.drag.as_ref() else {
            return base;
        };
        let (id, pos, target) = (d.id.clone(), d.pos, d.target);
        let size = base.size;
        let dock_len = self.home_geometry().dock.len();
        let t = tokens::DEFAULT;
        let mut canvas = de_paint::Canvas::new(size);
        canvas.blit(&base, Point::ZERO);
        if let Some(target) = target {
            let r = home::drop_rect(size, target, dock_len);
            let mut c = t.accent;
            c[3] = 70;
            canvas.fill_rounded_rect(r, tokens::ICON_TILE_RADIUS, c);
        }
        if let Some(icon) = self.app_icon(&id).icon {
            let icon_side = 112;
            let r = de_api::Rect::new(
                pos.x - icon_side / 2,
                pos.y - icon_side - 24,
                icon_side,
                icon_side,
            );
            canvas.blit_scaled(&icon, r, 0.95);
        }
        canvas.into_pixels()
    }
}
