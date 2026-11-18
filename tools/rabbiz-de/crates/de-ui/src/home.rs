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

/// Grid geometry (§7.18): 4 columns × 4 rows from y=312, 168×160 cells.
const CELL_W: i32 = 168;
const CELL_H: i32 = 160;
const GRID_Y: i32 = 312;
const SEARCH_Y: i32 = 1242;
const DOCK_Y: i32 = 1392;
/// Icons per home page (4×4).
pub const PAGE_SIZE: usize = 16;

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
    /// The search pill (→ opens search/drawer).
    Search,
}

/// Hit-test `pos` against the same geometry [`render`] paints: grid cells,
/// the search pill, dock slots. Pure — `de-core` calls it on
/// [`crate::gestures::Gesture::Tap`].
#[must_use]
pub fn hit_test(size: Size, pos: Point, apps_on_page: usize, dock_len: usize) -> Option<HomeHit> {
    let sp = Rect::new(
        chrome::MARGIN,
        SEARCH_Y.min(size.h - 420),
        chrome::content_w(size),
        64,
    );
    if sp.contains(pos) {
        return Some(HomeHit::Search);
    }
    let grid_w = CELL_W * 4;
    let gx = (size.w - grid_w) / 2;
    for i in 0..apps_on_page.min(PAGE_SIZE) {
        let (col, row) = ((i % 4) as i32, (i / 4) as i32);
        let r = Rect::new(gx + col * CELL_W, GRID_Y + row * CELL_H, CELL_W, CELL_H);
        if r.contains(pos) {
            return Some(HomeHit::Grid(i));
        }
    }
    let dock = Rect::new(
        chrome::MARGIN,
        DOCK_Y.min(size.h - 240),
        chrome::content_w(size),
        120,
    );
    let slots = dock_len.clamp(4, 6);
    for i in 0..dock_len.min(slots) {
        let ix = dock.x + 40 + i as i32 * ((dock.w - 80 - 88) / ((slots.max(2) - 1).max(1) as i32));
        if Rect::new(ix, dock.y + 16, 88, 88).contains(pos) {
            return Some(HomeHit::Dock(i));
        }
    }
    None
}

/// Render the home screen.
#[allow(clippy::too_many_lines)] // one pass over home's visual layers
#[must_use]
pub fn render(size: Size, s: &Strings, t: &Theme, m: &HomeModel) -> Pixels {
    let mut c = match m.wallpaper {
        Some(wp) => {
            let mut c = Canvas::new(size);
            c.blit(wp, Point::ZERO);
            c
        }
        None => Canvas::filled(size, t.bg),
    };
    chrome::status_bar_on_wallpaper(&mut c, t, &m.status);

    // Compact Smart Widget: time, date, charging note (glass card).
    let hero = Rect::new(chrome::MARGIN, 88, chrome::content_w(size), 184);
    c.fill_rounded_rect(hero, tokens::RADIUS_CARD, t.glass);
    chrome::draw(
        &mut c,
        m.clock,
        &style::h2(t.text),
        hero.x + 24,
        hero.y + 24,
        None,
    );
    if !m.date.is_empty() {
        chrome::draw(
            &mut c,
            m.date,
            &style::label(t.text2),
            hero.x + 24,
            hero.y + 76,
            None,
        );
    }
    if !m.charging.is_empty() {
        chrome::draw(
            &mut c,
            m.charging,
            &style::caption(t.accent),
            hero.x + 24,
            hero.y + hero.h - 44,
            None,
        );
    }

    // 4×4 grid for the current page.
    let grid_w = CELL_W * 4;
    let gx = (size.w - grid_w) / 2;
    for (i, e) in m.apps.iter().take(PAGE_SIZE).enumerate() {
        let (col, row) = ((i % 4) as i32, (i / 4) as i32);
        draw_icon(&mut c, t, e, gx + col * CELL_W, GRID_Y + row * CELL_H);
    }

    // Page dots between grid and search pill.
    if m.page_count > 1 {
        let dots_w = m.page_count as i32 * 20 - 12;
        let mut x = (size.w - dots_w) / 2;
        let y = SEARCH_Y - 56;
        for i in 0..m.page_count {
            let (d, col) = if i == m.page {
                (10, t.accent)
            } else {
                (8, t.text3)
            };
            c.fill_rounded_rect(Rect::new(x, y, d, d), 999.0, col);
            x += 20;
        }
    }

    // Search pill.
    let sp = Rect::new(
        chrome::MARGIN,
        SEARCH_Y.min(size.h - 420),
        chrome::content_w(size),
        64,
    );
    c.fill_rounded_rect(sp, tokens::RADIUS_PILL, t.glass);
    chrome::draw(
        &mut c,
        "\u{e8b6}",
        &text::LabelStyle::icon(26.0, t.text3),
        sp.x + 24,
        sp.y + 18,
        None,
    );
    chrome::draw(
        &mut c,
        s.get("home.search_hint"),
        &style::label(t.text3),
        sp.x + 64,
        sp.y + 18,
        None,
    );

    // Dock: glass shelf, pinned icons (or empty slots up to 4).
    let dock = Rect::new(
        chrome::MARGIN,
        DOCK_Y.min(size.h - 240),
        chrome::content_w(size),
        120,
    );
    c.fill_rounded_rect(dock, tokens::RADIUS_SHEET, t.glass);
    let slots = m.dock.len().clamp(4, 6);
    for i in 0..slots {
        let ix = dock.x + 40 + i as i32 * ((dock.w - 80 - 88) / ((slots.max(2) - 1).max(1) as i32));
        let r = Rect::new(ix, dock.y + 16, 88, 88);
        if let Some(e) = m.dock.get(i) {
            chrome::draw_app_icon(&mut c, t, e, r);
        } else {
            // Empty slot hint.
            c.fill_rounded_rect(r, tokens::ICON_TILE_RADIUS * (88.0 / 96.0), t.surface2);
        }
    }
    chrome::nav_pill(&mut c, t);
    c.into_pixels()
}

fn draw_icon(c: &mut Canvas, t: &Theme, e: &IconEntry, x: i32, y: i32) {
    chrome::draw_app_icon(c, t, e, Rect::new(x + (CELL_W - 96) / 2, y + 8, 96, 96));
    if let Some(px) = text::label_clamped(&e.label, style::label(t.text), 150, 2) {
        let (w, h) = (px.width() as i32, px.height() as i32);
        c.blit(
            &Pixels::new(Size::new(w, h), px.data().to_vec()),
            Point::new(x + (CELL_W - w) / 2, y + 108),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        // Search pill centre (y = min(SEARCH_Y, h-420) = 1180 at 1600).
        assert_eq!(
            hit_test(size, Point::new(360, 1180 + 32), 16, 4),
            Some(HomeHit::Search)
        );
        // First dock slot centre: dock.x=24, slot0 x = 24+40 = 64 → centre 108.
        assert_eq!(
            hit_test(size, Point::new(108, DOCK_Y + 16 + 44), 16, 4),
            Some(HomeHit::Dock(0))
        );
        // Wallpaper between grid and pill hits nothing.
        assert_eq!(hit_test(size, Point::new(360, 1100), 16, 4), None);
    }
}
