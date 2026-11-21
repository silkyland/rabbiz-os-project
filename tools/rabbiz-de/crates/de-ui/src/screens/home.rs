//! P2 home (M3-T2): wallpaper + parallax, status bar, compact Smart Widget
//! (clock/date/charging line), 4×4 icon grid for the current page, page
//! dots, search pill, and the pinned dock.
//!
//! The render is pure data-in/pixels-out: the compositor passes a wallpaper
//! already cover-scaled and parallax-shifted (`de-paint::wallpaper`), icon
//! pixels when `de-apps` resolved them, and the page index — so golden
//! frames stay deterministic and device frames just feed real data.

use de_api::{Pixels, Point, Rect, Size};
use de_design::tokens::{self, Theme};
use de_paint::{text, Canvas};

use crate::chrome::{self, style, IconEntry, StatusInfo};
use crate::i18n::Strings;

/// Geometry follows the Stitch light home (docs/DE-DESIGN-LIGHT-TOKENS.md,
/// assets/design/light-v1/home.png) scaled to 720 px: a big clock and date
/// at the left, a rounded search card, 4 columns of 112 px tiles, a small
/// "Search" pill and a floating five-slot dock.
const CELL_W: i32 = 168;
const CELL_H: i32 = 172;
const GRID_Y: i32 = 580;
const ICON: i32 = 112;
const DOCK_ICON: i32 = 100;
/// Left edge of the clock, date and search card.
const LEFT: i32 = 44;
const SEARCH_CARD_Y: i32 = 414;
const SEARCH_CARD_H: i32 = 124;
/// Icons per home page: 4×4 portrait, 8×2 landscape (same count, so page
/// membership never changes when the device is turned).
pub const PAGE_SIZE: usize = 16;

/// Everything the layout depends on the output shape for. Portrait is the
/// original design (the constants above); landscape moves the clock, search
/// card and drawer pill into one header row, lays the 16 icons out 8×2 and
/// shrinks the dock so everything fits a ~720 px tall panel.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Geo {
    landscape: bool,
    cols: i32,
    cell_w: i32,
    cell_h: i32,
    grid_x: i32,
    grid_y: i32,
    icon: i32,
    dock_icon: i32,
    dock: Rect,
    dock_radius: f32,
    search: Rect,
    pill: Rect,
    clock_xy: (i32, i32),
    clock_size: f32,
    date_xy: (i32, i32),
    charging_xy: (i32, i32),
}

fn geo(size: Size) -> Geo {
    if size.w > size.h {
        let cell_w = 150;
        let cols = 8;
        let search = Rect::new(520, 78, (size.w - 520 - 260 - LEFT - 16).max(200), 92);
        Geo {
            landscape: true,
            cols,
            cell_w,
            cell_h: 160,
            grid_x: (size.w - cell_w * cols) / 2,
            grid_y: 220,
            icon: 104,
            dock_icon: 84,
            dock: Rect::new(16, size.h - 156, size.w - 32, 112),
            dock_radius: 48.0,
            search,
            pill: Rect::new(search.x + search.w + 16, search.y + 18, 220, 56),
            clock_xy: (LEFT - 2, 60),
            clock_size: 96.0,
            date_xy: (LEFT, 164),
            charging_xy: (LEFT, 204),
        }
    } else {
        Geo {
            landscape: false,
            cols: 4,
            cell_w: CELL_W,
            cell_h: CELL_H,
            grid_x: (size.w - CELL_W * 4) / 2,
            grid_y: GRID_Y,
            icon: ICON,
            dock_icon: DOCK_ICON,
            dock: Rect::new(16, size.h - 220, size.w - 32, 150),
            dock_radius: 60.0,
            search: Rect::new(LEFT, SEARCH_CARD_Y, size.w - LEFT * 2, SEARCH_CARD_H),
            pill: Rect::new((size.w - 200) / 2, size.h - 308, 200, 56),
            clock_xy: (LEFT - 4, 118),
            clock_size: 156.0,
            date_xy: (LEFT, 318),
            charging_xy: (LEFT, 360),
        }
    }
}

/// Everything the home screen paints.
pub struct HomeModel<'a> {
    pub status: StatusInfo,
    /// "09:41" — compositor-formatted.
    pub clock: &'a str,
    /// "Mon, 9 Jun" — compositor-formatted, localized.
    pub date: &'a str,
    /// Charging label under the clock ("" hides it).
    pub charging: &'a str,
    /// Wallpaper already cover-scaled + parallax-shifted to output size.
    /// `None` paints the theme canvas (sim/tests).
    pub wallpaper: Option<&'a Pixels>,
    /// Current-page icons (≤ 16, already ordered by `Catalog::home_order`).
    pub apps: &'a [IconEntry],
    /// Dock entries (≤ 6, from `Catalog::dock`).
    pub dock: &'a [IconEntry],
    /// Current page (0-based) and page count (≥ 1) for the dots.
    pub page: usize,
    pub page_count: usize,
}

impl<'a> HomeModel<'a> {
    /// Convenience for sim/tests: static data, no wallpaper.
    #[must_use]
    pub fn demo(clock: &'a str, apps: &'a [IconEntry]) -> Self {
        Self {
            status: StatusInfo::new(clock, "86"),
            clock,
            date: "",
            charging: "",
            wallpaper: None,
            apps,
            dock: &[],
            page: 0,
            page_count: 1,
        }
    }
}

/// Pages needed for `n` apps.
#[must_use]
pub const fn pages_for(n: usize) -> usize {
    if n == 0 {
        1
    } else {
        n.div_ceil(PAGE_SIZE)
    }
}

/// Slice of `apps` living on `page` (empty when out of range).
#[must_use]
pub fn page_slice(apps: &[IconEntry], page: usize) -> &[IconEntry] {
    let start = page * PAGE_SIZE;
    if start >= apps.len() {
        return &[];
    }
    &apps[start..(start + PAGE_SIZE).min(apps.len())]
}

/// What a tap on the home screen hit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HomeHit {
    /// Grid icon at page-local index (`0..apps_on_page`).
    Grid(usize),
    /// Dock slot index.
    Dock(usize),
    /// The search card (→ opens search).
    Search,
    /// The small "Search" pill above the dock (→ opens the app drawer).
    Drawer,
}

fn grid_cell(size: Size, index: usize) -> Rect {
    let g = geo(size);
    let x = g.grid_x + (index as i32 % g.cols) * g.cell_w;
    let y = g.grid_y + (index as i32 / g.cols) * g.cell_h;
    Rect::new(x, y, g.cell_w, g.cell_h)
}

pub fn dock_shelf(size: Size) -> Rect {
    geo(size).dock
}

fn search_card(size: Size) -> Rect {
    geo(size).search
}

/// The small "Search" pill above the dock: opens the app drawer.
fn drawer_pill(size: Size) -> Rect {
    geo(size).pill
}

fn grid_icon(size: Size, cell: Rect) -> Rect {
    let g = geo(size);
    Rect::new(cell.x + (g.cell_w - g.icon) / 2, cell.y + 6, g.icon, g.icon)
}

pub fn dock_icon(size: Size, dock: Rect, index: usize, slots: usize) -> Rect {
    let d = geo(size).dock_icon;
    let slots = slots.clamp(1, 6) as i32;
    let cell = dock.w as f32 / slots as f32;
    let cx = dock.x as f32 + cell * (index.min(slots as usize - 1) as f32 + 0.5);
    Rect::new(cx.round() as i32 - d / 2, dock.y + (dock.h - d) / 2, d, d)
}

/// Exact painted icon bounds for a launch from a grid cell or dock slot.
#[must_use]
pub fn icon_rect(size: Size, hit: HomeHit, dock_len: usize) -> Option<Rect> {
    match hit {
        HomeHit::Grid(i) => Some(grid_icon(size, grid_cell(size, i))),
        HomeHit::Dock(i) => Some(dock_icon(size, dock_shelf(size), i, dock_len.clamp(1, 6))),
        HomeHit::Search | HomeHit::Drawer => None,
    }
}

/// Hit-test `pos` against the same geometry [`render`] paints: grid cells,
/// the search pill, dock slots. Pure — `de-core` calls it on
/// [`crate::gestures::Gesture::Tap`].
#[must_use]
pub fn hit_test(size: Size, pos: Point, apps_on_page: usize, dock_len: usize) -> Option<HomeHit> {
    if search_card(size).contains(pos) {
        return Some(HomeHit::Search);
    }
    if drawer_pill(size).contains(pos) {
        return Some(HomeHit::Drawer);
    }
    for i in 0..apps_on_page.min(PAGE_SIZE) {
        if grid_cell(size, i).contains(pos) {
            return Some(HomeHit::Grid(i));
        }
    }
    let slots = dock_slots(dock_len);
    for i in 0..dock_len.min(slots) {
        if dock_icon(size, dock_shelf(size), i, slots).contains(pos) {
            return Some(HomeHit::Dock(i));
        }
    }
    None
}

/// Sentinel `IconEntry.id` the compositor inserts into `HomeModel` lists
/// while an icon is being dragged: the cell renders as an empty drop
/// pocket instead of a tile.
pub const GAP_ID: &str = "\u{1}drop-gap";

/// Where a dragged icon would land.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropTarget {
    /// Page-local grid cell (`0..PAGE_SIZE`).
    Grid(usize),
    /// Dock slot (`0..dock slots`).
    Dock(usize),
}

/// Number of dock slots painted for `dock_len` pinned apps.
#[must_use]
pub fn dock_slots(dock_len: usize) -> usize {
    dock_len.min(6)
}

/// Which drop target `pos` points at while an icon is being dragged: the
/// dock (nearest slot, with a generous catch zone above the shelf) or a
/// grid cell. Pure, shares geometry with [`render`].
#[must_use]
pub fn drop_target(size: Size, pos: Point, dock_len: usize) -> Option<DropTarget> {
    let shelf = dock_shelf(size);
    let catch = Rect::new(shelf.x, shelf.y - 40, shelf.w, shelf.h + 40);
    let slots = (dock_len + 1).clamp(1, 6);
    if catch.contains(pos) {
        let best = (0..slots)
            .min_by_key(|&i| {
                let r = dock_icon(size, shelf, i, slots);
                (pos.x - (r.x + r.w / 2)).abs()
            })
            .unwrap_or(0);
        return Some(DropTarget::Dock(best));
    }
    let g = geo(size);
    let rows = PAGE_SIZE as i32 / g.cols;
    let area = Rect::new(g.grid_x, g.grid_y, g.cell_w * g.cols, g.cell_h * rows);
    if area.contains(pos) {
        let col = ((pos.x - g.grid_x) / g.cell_w).clamp(0, g.cols - 1) as usize;
        let row = ((pos.y - g.grid_y) / g.cell_h).clamp(0, rows - 1) as usize;
        return Some(DropTarget::Grid(row * g.cols as usize + col));
    }
    None
}

/// Painted bounds of a drop target (for the highlight under the finger).
#[must_use]
pub fn drop_rect(size: Size, target: DropTarget, dock_len: usize) -> Rect {
    match target {
        DropTarget::Grid(i) => grid_icon(size, grid_cell(size, i)),
        DropTarget::Dock(i) => dock_icon(size, dock_shelf(size), i, (dock_len + 1).clamp(1, 6)),
    }
}

/// Top of the page-dots strip (between the dock and the gesture bar).
#[must_use]
pub fn dots_y(size: Size) -> i32 {
    let dock = dock_shelf(size);
    dock.y + dock.h + 12
}

/// The page-dots row for `page` of `count`, as a strip `size.w` wide
/// painted at [`dots_y`]; the pager shows one per settled page.
#[must_use]
pub fn render_dots(size: Size, t: &Theme, count: usize, page: usize) -> Pixels {
    let mut c = Canvas::new(Size::new(size.w, 12));
    let dots_w = count as i32 * 20 - 12;
    let mut x = (size.w - dots_w) / 2;
    for i in 0..count {
        let (d, col) = if i == page {
            (10, t.accent)
        } else {
            (8, t.text3)
        };
        c.fill_rounded_rect(Rect::new(x, 1, d, d), 999.0, col);
        x += 20;
    }
    c.into_pixels()
}

/// Only the icon grid of one page, on a transparent canvas: the layer the
/// pager slides while the rest of the home screen stays put.
#[must_use]
pub fn render_grid(size: Size, t: &Theme, apps: &[IconEntry]) -> Pixels {
    let mut c = Canvas::new(Size::new(size.w, grid_band_h(size)));
    let top = grid_band_y(size);
    for (i, e) in apps.iter().take(PAGE_SIZE).enumerate() {
        let mut cell = grid_cell(size, i);
        cell.y -= top;
        if e.id == GAP_ID {
            draw_gap(&mut c, t, grid_icon(size, cell));
        } else {
            draw_icon(&mut c, t, size, e, cell);
        }
    }
    c.into_pixels()
}

/// Height of the strip [`render_grid`] returns (it is blitted at [`grid_band_y`]).
#[must_use]
pub fn grid_band_h(size: Size) -> i32 {
    let g = geo(size);
    g.cell_h * (PAGE_SIZE as i32 / g.cols)
}

/// Top of the grid strip on screen.
#[must_use]
pub fn grid_band_y(size: Size) -> i32 {
    geo(size).grid_y
}

/// Render the home screen.
#[allow(clippy::too_many_lines)] // one pass over home's visual layers
#[must_use]
pub fn render(size: Size, s: &Strings, t: &Theme, m: &HomeModel) -> Pixels {
    render_with_hidden(size, s, t, m, None, false)
}

/// Interaction layers own the pressed/lifted tile and dock while moving;
/// omit those pixels from the cached base once, then animate textures.
#[must_use]
pub fn render_with_hidden(
    size: Size,
    s: &Strings,
    t: &Theme,
    m: &HomeModel,
    hidden: Option<&str>,
    hide_dock: bool,
) -> Pixels {
    let mut c = match m.wallpaper {
        Some(wp) => {
            let mut c = Canvas::new(size);
            c.blit(wp, Point::ZERO);
            c
        }
        None => Canvas::filled(size, t.bg),
    };
    chrome::status_bar_on_wallpaper(&mut c, t, &m.status);

    // Clock and date, left-aligned, cream on wallpaper (matching status bar).
    let on_wp = [0xFF, 0xF8, 0xF0, 0xFF];
    let on_wp_sub = [0xFF, 0xF8, 0xF0, 0xD8];
    let g = geo(size);
    let clock_style = text::LabelStyle {
        size: g.clock_size,
        line_height: g.clock_size + 8.0,
        weight: 400,
        tnum: true,
        color: on_wp,
        ..text::LabelStyle::default()
    };
    chrome::draw(&mut c, m.clock, &clock_style, g.clock_xy.0, g.clock_xy.1, None);
    if !m.date.is_empty() {
        let date_style = text::LabelStyle {
            size: 30.0,
            line_height: 38.0,
            weight: 600,
            color: on_wp_sub,
            ..text::LabelStyle::default()
        };
        chrome::draw(&mut c, m.date, &date_style, g.date_xy.0, g.date_xy.1, None);
    }
    if !m.charging.is_empty() {
        chrome::draw(
            &mut c,
            m.charging,
            &style::caption(on_wp_sub),
            g.charging_xy.0,
            g.charging_xy.1,
            None,
        );
    }

    // Search card: glass + search glyph + hint text (single tap target).
    let card = search_card(size);
    c.fill_rounded_rect(card, (card.h as f32 / 2.0).min(44.0), t.glass);
    chrome::draw_ink_centered(
        &mut c,
        "\u{e8b6}",
        &text::LabelStyle::icon(32.0, t.text3),
        Rect::new(card.x + 28, card.y, 48, card.h),
    );
    chrome::draw(
        &mut c,
        s.get("home.search_hint"),
        &style::body(t.text2),
        card.x + 84,
        card.y + (card.h - 34) / 2,
        Some(card.w - 84 - 28),
    );

    // 4×4 grid for the current page.
    for (i, e) in m.apps.iter().take(PAGE_SIZE).enumerate() {
        let cell = grid_cell(size, i);
        if e.id == GAP_ID {
            draw_gap(&mut c, t, grid_icon(size, cell));
        } else if hidden == Some(e.id.as_str()) {
            let mut entry = e.clone();
            entry.icon = None;
            // The label remains in place; the tile is a separate texture.
            if let Some(px) = text::label_clamped(&entry.label, style::label(t.text), 150, 1) {
                c.blit(
                    &Pixels::new(
                        Size::new(px.width() as i32, px.height() as i32),
                        px.data().to_vec(),
                    ),
                    Point::new(
                        cell.x + (g.cell_w - px.width() as i32) / 2,
                        cell.y + g.icon + 14,
                    ),
                );
            }
        } else {
            draw_icon(&mut c, t, size, e, cell);
        }
    }

    // Small "Search" pill above the dock (opens the drawer).
    let pill = drawer_pill(size);
    c.fill_rounded_rect(pill, tokens::RADIUS_PILL, t.glass);
    chrome::draw_ink_centered(
        &mut c,
        "\u{e8b6}",
        &text::LabelStyle::icon(28.0, t.accent),
        Rect::new(pill.x + 22, pill.y, 40, pill.h),
    );
    chrome::draw(
        &mut c,
        s.get("home.search_pill"),
        &style::body(t.text),
        pill.x + 72,
        pill.y + 11,
        None,
    );

    // Page dots, tucked between the dock and the gesture bar.
    if m.page_count > 1 {
        c.blit(
            &render_dots(size, t, m.page_count, m.page),
            Point::new(0, dots_y(size)),
        );
    }

    if !hide_dock {
        let dock = dock_shelf(size);
        c.fill_rounded_rect(dock, g.dock_radius, t.glass);
        let slots = dock_slots(m.dock.len());
        for (i, e) in m.dock.iter().take(slots).enumerate() {
            let r = dock_icon(size, dock, i, slots);
            if e.id == GAP_ID {
                draw_gap(&mut c, t, r);
            } else if hidden != Some(e.id.as_str()) {
                chrome::draw_app_icon(&mut c, t, e, r);
            }
        }
    }
    chrome::nav_pill(&mut c, t);
    c.into_pixels()
}

/// The drop pocket under the finger during a rearrange drag: a faint tile
/// outline where the icon will land.
fn draw_gap(c: &mut Canvas, t: &Theme, r: Rect) {
    c.fill_rounded_rect(r, 28.0, t.glass_hi);
    let b = 3;
    let inner = Rect::new(r.x + b, r.y + b, r.w - b * 2, r.h - b * 2);
    c.fill_rounded_rect(inner, 25.0, t.scrim);
}

fn draw_icon(c: &mut Canvas, t: &Theme, size: Size, e: &IconEntry, cell: Rect) {
    let g = geo(size);
    chrome::draw_app_icon(c, t, e, grid_icon(size, cell));
    if let Some(px) = text::label_clamped(&e.label, style::label(t.text), 150, 1) {
        let (w, h) = (px.width() as i32, px.height() as i32);
        c.blit(
            &Pixels::new(Size::new(w, h), px.data().to_vec()),
            Point::new(cell.x + (g.cell_w - w) / 2, cell.y + g.icon + 14),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dock_uses_only_occupied_slots_and_centres_every_count() {
        let shelf = dock_shelf(Size::new(720, 1600));
        for n in 1..=6 {
            assert_eq!(dock_slots(n), n);
            let first = dock_icon(Size::new(720, 1600), shelf, 0, n);
            let last = dock_icon(Size::new(720, 1600), shelf, n - 1, n);
            assert!((first.x - shelf.x - (shelf.x + shelf.w - last.x - last.w)).abs() <= 1);
            for i in 0..n {
                assert!(shelf.contains(dock_icon(Size::new(720, 1600), shelf, i, n).origin()));
            }
        }
        assert_eq!(dock_slots(0), 0);
        assert_eq!(dock_slots(7), 6);
        assert_eq!(dock_icon(Size::new(720, 1600), shelf, 0, 1).x + DOCK_ICON / 2, 360);
    }

    #[test]
    fn landscape_geometry_fits_and_hit_tests() {
        let size = Size::new(1600, 720);
        let screen = Rect::new(0, 0, size.w, size.h);
        let dock = dock_shelf(size);
        let band_end = grid_band_y(size) + grid_band_h(size);
        assert!(band_end <= dock.y, "grid {band_end} must clear the dock {}", dock.y);
        assert!(dots_y(size) + 12 <= size.h);
        for i in 0..PAGE_SIZE {
            let cell = grid_cell(size, i);
            assert!(screen.contains(cell.origin()) && cell.x + cell.w <= size.w);
            let c = cell.center();
            assert_eq!(hit_test(size, c, PAGE_SIZE, 4), Some(HomeHit::Grid(i)), "cell {i}");
            let icon = icon_rect(size, HomeHit::Grid(i), 4).unwrap();
            assert_eq!(drop_target(size, icon.center(), 4), Some(DropTarget::Grid(i)));
        }
        let (card, pill) = (search_card(size), drawer_pill(size));
        assert_eq!(hit_test(size, card.center(), 16, 4), Some(HomeHit::Search));
        assert_eq!(hit_test(size, pill.center(), 16, 4), Some(HomeHit::Drawer));
        assert!(card.x + card.w < pill.x && pill.x + pill.w <= size.w);
        for n in 1..=6 {
            for i in 0..n {
                let r = dock_icon(size, dock, i, n);
                assert!(dock.contains(r.origin()) && r.x + r.w <= dock.x + dock.w);
                let c = r.center();
                assert_eq!(hit_test(size, c, 16, n), Some(HomeHit::Dock(i)));
            }
        }
        let px = render(size, &Strings::default(), &tokens::DEFAULT, &HomeModel::demo("09:41", &[]));
        assert_eq!(px.size, size);
    }

    #[test]
    fn paging_math() {
        assert_eq!(pages_for(0), 1);
        assert_eq!(pages_for(16), 1);
        assert_eq!(pages_for(17), 2);
        let apps: Vec<IconEntry> = (0..20)
            .map(|_i| IconEntry::new("a", "x"))
            .enumerate()
            .map(|(i, mut e)| {
                e.id = format!("a{i}");
                e
            })
            .collect();
        assert_eq!(page_slice(&apps, 0).len(), 16);
        assert_eq!(page_slice(&apps, 1).len(), 4);
        assert!(page_slice(&apps, 2).is_empty());
    }

    #[test]
    fn drop_targets_cover_grid_and_dock() {
        let size = Size::new(720, 1600);
        let gx = (720 - CELL_W * 4) / 2;
        assert_eq!(
            drop_target(
                size,
                Point::new(gx + CELL_W + 5, GRID_Y + CELL_H * 2 + 5),
                4
            ),
            Some(DropTarget::Grid(9))
        );
        // Far left of the dock catches slot 0, far right the last slot.
        let y = dock_shelf(size).y + 60;
        assert_eq!(
            drop_target(size, Point::new(40, y), 4),
            Some(DropTarget::Dock(0))
        );
        assert_eq!(
            drop_target(size, Point::new(690, y), 4),
            Some(DropTarget::Dock(4))
        );
        assert_eq!(
            drop_target(size, Point::new(690, y), 6),
            Some(DropTarget::Dock(5))
        );
        // Above the dock's catch zone, outside the grid: nothing.
        assert_eq!(drop_target(size, Point::new(360, 100), 4), None);
        // The rect of a target is where hit_test finds the same icon.
        let r = drop_rect(size, DropTarget::Dock(2), 4);
        assert_eq!(
            hit_test(size, Point::new(r.x + r.w / 2, r.y + r.h / 2), 16, 5),
            Some(HomeHit::Dock(2))
        );
    }

    #[test]
    fn grid_layer_is_transparent_outside_icons() {
        let size = Size::new(720, 1600);
        let apps = vec![IconEntry::new("a", "A")];
        let px = render_grid(size, &tokens::DEFAULT, &apps);
        assert_eq!(px.size, Size::new(720, grid_band_h(Size::new(720, 1600))));
        // The strip corner away from the first icon is untouched.
        assert_eq!(
            &px.data[(px.size.w as usize - 1) * 4..px.size.w as usize * 4],
            &[0, 0, 0, 0]
        );
    }

    #[test]
    fn hit_test_grid_dock_search() {
        let size = Size::new(720, 1600);
        // First grid cell centre.
        let gx = (720 - CELL_W * 4) / 2;
        assert_eq!(
            hit_test(
                size,
                Point::new(gx + CELL_W / 2, GRID_Y + CELL_H / 2),
                16,
                4
            ),
            Some(HomeHit::Grid(0))
        );
        // Last grid cell on the first row.
        assert_eq!(
            hit_test(
                size,
                Point::new(gx + 3 * CELL_W + CELL_W / 2, GRID_Y + CELL_H / 2),
                16,
                4
            ),
            Some(HomeHit::Grid(3))
        );
        // Search card centre.
        assert_eq!(
            hit_test(
                size,
                Point::new(360, SEARCH_CARD_Y + SEARCH_CARD_H / 2),
                16,
                4
            ),
            Some(HomeHit::Search)
        );
        // The small pill above the dock opens the drawer.
        let p = drawer_pill(size);
        assert_eq!(
            hit_test(size, Point::new(p.x + p.w / 2, p.y + p.h / 2), 16, 4),
            Some(HomeHit::Drawer)
        );
        // First dock slot centre.
        let d0 = dock_icon(size, dock_shelf(size), 0, 4);
        assert_eq!(
            hit_test(size, Point::new(d0.x + d0.w / 2, d0.y + d0.h / 2), 16, 4),
            Some(HomeHit::Dock(0))
        );
        // Wallpaper beside the pill hits nothing.
        assert_eq!(hit_test(size, Point::new(100, 1330), 16, 4), None);
    }
}
