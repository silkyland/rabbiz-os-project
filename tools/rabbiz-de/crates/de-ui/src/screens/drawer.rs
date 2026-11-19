//! P3 app drawer (M3-T3): live search field, Recent/Frequent row + 4-column
//! app grid, viewport-culled rendering (only visible rows are rasterized).

use de_api::{Pixels, Point, Rect, Size};
use de_design::tokens::{self, Theme};
use de_paint::{text, Canvas};

use crate::chrome::{self, style, IconEntry, StatusInfo};
use crate::i18n::Strings;

const COLS: i32 = 4;
const CELL_W: i32 = 168;
const CELL_H: i32 = 164;
const ICON: i32 = 96;
const HEADER_H: i32 = 40;
const SECTION_GAP: i32 = 12;

fn search_rect(size: Size) -> Rect {
    Rect::new(
        chrome::MARGIN,
        chrome::STATUS_H + 16,
        chrome::content_w(size),
        56,
    )
}

fn view_rect(size: Size) -> Rect {
    let sf = search_rect(size);
    let top = sf.y + sf.h + 12;
    Rect::new(0, top, size.w, (size.h - top - chrome::NAV_H - 8).max(0))
}

fn grid_x0(size: Size) -> i32 {
    (size.w - CELL_W * COLS) / 2
}

/// Filter `apps` by `query` (case-insensitive substring on `label` or `id`).
#[must_use]
pub fn filter_apps<'a>(apps: &'a [IconEntry], query: &str) -> Vec<&'a IconEntry> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        apps.iter().collect()
    } else {
        apps.iter()
            .filter(|e| e.label.to_lowercase().contains(&q) || e.id.to_lowercase().contains(&q))
            .collect()
    }
}

/// Section header group (kept for backwards compatibility with callers/tests).
#[must_use]
pub fn sections(apps: &[IconEntry]) -> Vec<(String, usize)> {
    let mut out: Vec<(String, usize)> = Vec::new();
    let mut last = String::new();
    for (i, e) in apps.iter().enumerate() {
        let key = e.label.chars().next().map_or_else(
            || "#".into(),
            |c| {
                if c.is_ascii_alphabetic() {
                    c.to_ascii_uppercase().to_string()
                } else {
                    "#".into()
                }
            },
        );
        if key != last {
            last.clone_from(&key);
            out.push((key, i));
        }
    }
    out
}

/// Total scrollable height of the grid (plus optional recent row).
#[must_use]
pub fn layout_height(apps: &[IconEntry]) -> i32 {
    layout_height_grid(0, apps.len())
}

fn rows_for(len: usize) -> i32 {
    if len == 0 {
        0
    } else {
        (len as i32 + COLS - 1) / COLS
    }
}

fn layout_height_grid(recent_len: usize, main_len: usize) -> i32 {
    let mut h = 0;
    if recent_len > 0 {
        h += HEADER_H + rows_for(recent_len) * CELL_H + SECTION_GAP;
    }
    if main_len > 0 {
        h += HEADER_H + rows_for(main_len) * CELL_H;
    }
    h
}

/// Render the drawer (backwards-compatible wrapper with empty recent & query).
#[must_use]
pub fn render(
    size: Size,
    s: &Strings,
    t: &Theme,
    status: &StatusInfo,
    apps: &[IconEntry],
    scroll_px: i32,
) -> Pixels {
    render_with_query(size, s, t, status, &[], apps, "", false, scroll_px)
}

/// Render the drawer with a Recent/Frequent row, live search query + caret,
/// and a 4-column app grid.
#[allow(clippy::too_many_arguments)]
#[must_use]
pub fn render_with_query(
    size: Size,
    s: &Strings,
    t: &Theme,
    _status: &StatusInfo,
    recent: &[IconEntry],
    apps: &[IconEntry],
    query: &str,
    show_caret: bool,
    scroll_px: i32,
) -> Pixels {
    let mut c = Canvas::filled(size, t.canvas);

    // Search field.
    let sf = search_rect(size);
    c.fill_rounded_rect(sf, tokens::RADIUS_PILL, t.surface2);
    chrome::draw_ink_centered(
        &mut c,
        "\u{e8b6}",
        &text::LabelStyle::icon(24.0, t.text3),
        Rect::new(sf.x + 16, sf.y, 40, sf.h),
    );
    let shown = if query.is_empty() {
        s.get("common.search")
    } else {
        query
    };
    let col = if query.is_empty() { t.text3 } else { t.text };
    let r = chrome::draw(
        &mut c,
        shown,
        &style::label(col),
        sf.x + 60,
        sf.y + 15,
        Some(sf.w - 84),
    );
    if show_caret {
        let caret_x = if query.is_empty() {
            sf.x + 60
        } else {
            r.x + r.w + 2
        };
        c.fill_rect(Rect::new(caret_x, sf.y + 14, 2, sf.h - 28), t.accent);
    }

    let view = view_rect(size);
    let filtered = filter_apps(apps, query);
    let show_recent = query.trim().is_empty() && !recent.is_empty();
    let recent_slice = if show_recent {
        &recent[..recent.len().min(COLS as usize)]
    } else {
        &[]
    };

    let total = layout_height_grid(recent_slice.len(), filtered.len());
    let clamped_scroll = scroll_px.max(0).min((total - view.h).max(0));
    if total > 0 && view.h > 0 {
        let mut gc = Canvas::new(Size::new(size.w, view.h));
        let vis_top = clamped_scroll;
        let vis_bot = clamped_scroll + view.h;
        let x0 = grid_x0(size);
        let mut y = 0;

        if !recent_slice.is_empty() {
            if y + HEADER_H > vis_top && y < vis_bot {
                chrome::draw(
                    &mut gc,
                    "Recent",
                    &style::caption(t.text3),
                    x0 + 8,
                    y - vis_top + 8,
                    None,
                );
            }
            y += HEADER_H;
            draw_grid_slice(
                &mut gc,
                t,
                recent_slice.iter().collect::<Vec<_>>().as_slice(),
                x0,
                y,
                vis_top,
                vis_bot,
            );
            y += rows_for(recent_slice.len()) * CELL_H + SECTION_GAP;
        }

        if !filtered.is_empty() {
            let hdr = if query.trim().is_empty() {
                "Apps"
            } else {
                "Results"
            };
            if y + HEADER_H > vis_top && y < vis_bot {
                chrome::draw(
                    &mut gc,
                    hdr,
                    &style::caption(t.text3),
                    x0 + 8,
                    y - vis_top + 8,
                    None,
                );
            }
            y += HEADER_H;
            draw_grid_slice(&mut gc, t, &filtered, x0, y, vis_top, vis_bot);
        }

        c.blit(&gc.into_pixels(), view.origin());
    } else if !query.trim().is_empty() {
        chrome::draw_centered(
            &mut c,
            s.get("search.none"),
            &style::body(t.text3),
            size.h / 2,
        );
    }

    chrome::nav_pill(&mut c, t);
    c.into_pixels()
}

fn draw_grid_slice(
    gc: &mut Canvas,
    t: &Theme,
    items: &[&IconEntry],
    x0: i32,
    start_y: i32,
    vis_top: i32,
    vis_bot: i32,
) {
    for (i, e) in items.iter().enumerate() {
        let row = i as i32 / COLS;
        let col = i as i32 % COLS;
        let cy = start_y + row * CELL_H;
        if cy + CELL_H <= vis_top {
            continue;
        }
        if cy >= vis_bot {
            break;
        }
        let ly = cy - vis_top;
        let cx = x0 + col * CELL_W;
        let icon_r = Rect::new(cx + (CELL_W - ICON) / 2, ly + 8, ICON, ICON);
        chrome::draw_app_icon(gc, t, e, icon_r);
        if let Some(px) = text::label_clamped(&e.label, style::label(t.text), CELL_W - 14, 1) {
            let pw = px.width() as i32;
            let ph = px.height() as i32;
            gc.blit(
                &Pixels::new(Size::new(pw, ph), px.data().to_vec()),
                Point::new(cx + (CELL_W - pw) / 2, ly + ICON + 16),
            );
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DrawerHit {
    Search,
    App(String),
    Letter(String),
}

/// Hit-test the drawer search bar and scrolled 4-column grid.
#[must_use]
pub fn hit_test(size: Size, apps: &[IconEntry], scroll_px: i32, pos: Point) -> Option<DrawerHit> {
    hit_test_with_query(size, &[], apps, "", scroll_px, pos)
}

/// Hit-test matching [`render_with_query`].
#[must_use]
pub fn hit_test_with_query(
    size: Size,
    recent: &[IconEntry],
    apps: &[IconEntry],
    query: &str,
    scroll_px: i32,
    pos: Point,
) -> Option<DrawerHit> {
    if search_rect(size).contains(pos) {
        return Some(DrawerHit::Search);
    }
    let view = view_rect(size);
    if !view.contains(pos) {
        return None;
    }
    let filtered = filter_apps(apps, query);
    let show_recent = query.trim().is_empty() && !recent.is_empty();
    let recent_slice = if show_recent {
        &recent[..recent.len().min(COLS as usize)]
    } else {
        &[]
    };
    let total = layout_height_grid(recent_slice.len(), filtered.len());
    let clamped_scroll = scroll_px.max(0).min((total - view.h).max(0));
    let local_y = pos.y - view.y + clamped_scroll;
    let x0 = grid_x0(size);
    let mut y = 0;

    if !recent_slice.is_empty() {
        y += HEADER_H;
        if let Some(id) = hit_grid_slice(
            recent_slice.iter().collect::<Vec<_>>().as_slice(),
            x0,
            y,
            pos.x,
            local_y,
        ) {
            return Some(DrawerHit::App(id));
        }
        y += rows_for(recent_slice.len()) * CELL_H + SECTION_GAP;
    }
    if !filtered.is_empty() {
        y += HEADER_H;
        if let Some(id) = hit_grid_slice(&filtered, x0, y, pos.x, local_y) {
            return Some(DrawerHit::App(id));
        }
    }
    None
}

fn hit_grid_slice(
    items: &[&IconEntry],
    x0: i32,
    start_y: i32,
    px: i32,
    local_y: i32,
) -> Option<String> {
    if local_y < start_y || px < x0 || px >= x0 + CELL_W * COLS {
        return None;
    }
    let row = (local_y - start_y) / CELL_H;
    let col = (px - x0) / CELL_W;
    let idx = (row * COLS + col) as usize;
    items.get(idx).map(|e| e.id.clone())
}

/// Scroll offset (px) that brings section `letter` to the top of the drawer.
#[must_use]
pub fn scroll_for_letter(apps: &[IconEntry], letter: &str) -> Option<i32> {
    let secs = sections(apps);
    secs.iter()
        .find(|(k, _)| k == letter)
        .map(|(_, idx)| HEADER_H + (*idx as i32 / COLS) * CELL_H)
}

/// Scroll offset (px) for `letter`.
#[must_use]
pub fn scroll_to_letter(apps: &[IconEntry], letter: &str) -> i32 {
    scroll_for_letter(apps, letter).unwrap_or_else(|| layout_height(apps))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apps(labels: &[&str]) -> Vec<IconEntry> {
        labels.iter().map(|l| IconEntry::new(l, l)).collect()
    }

    #[test]
    fn groups_by_first_letter() {
        let a = apps(&["Apple", "avocado", "Banana", "กา", "ข้าว"]);
        let s = sections(&a);
        assert_eq!(s[0], ("A".to_string(), 0));
        assert_eq!(s[1], ("B".to_string(), 2));
        assert_eq!(s[2], ("#".to_string(), 3));
    }

    #[test]
    fn grid_height_and_search_filter() {
        let a = apps(&["Alpha", "Beta", "Gamma", "Delta", "Epsilon"]);
        assert_eq!(layout_height(&a), HEADER_H + 2 * CELL_H);
        assert_eq!(filter_apps(&a, "gam").len(), 1);
        assert_eq!(filter_apps(&a, "gam")[0].id, "Gamma");
    }

    #[test]
    fn hit_test_matches_grid_and_recent_row() {
        let size = Size::new(720, 1600);
        let recent = apps(&["Term", "Browser"]);
        let all = apps(&["Alpha", "Beta", "Gamma", "Delta", "Epsilon"]);
        let sf = search_rect(size);
        assert_eq!(
            hit_test_with_query(size, &recent, &all, "", 0, Point::new(sf.x + 20, sf.y + 20)),
            Some(DrawerHit::Search)
        );
        let view = view_rect(size);
        let x0 = grid_x0(size);
        // Recent row item 1 ("Browser")
        assert_eq!(
            hit_test_with_query(
                size,
                &recent,
                &all,
                "",
                0,
                Point::new(x0 + CELL_W + 40, view.y + HEADER_H + 40)
            ),
            Some(DrawerHit::App("Browser".into()))
        );
        // All-apps row item 0 ("Alpha") below recent row + gap + header
        let all_y = view.y + HEADER_H + CELL_H + SECTION_GAP + HEADER_H + 40;
        assert_eq!(
            hit_test_with_query(size, &recent, &all, "", 0, Point::new(x0 + 40, all_y)),
            Some(DrawerHit::App("Alpha".into()))
        );
    }
}
