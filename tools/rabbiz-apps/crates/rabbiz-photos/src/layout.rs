//! Grid layout math: day-group headers + square thumbnail cells.
//!
//! All geometry is derived from the live canvas size — nothing here assumes
//! 720x1600. Content coordinates start at y=0; the view window is
//! `[view_top, view_bottom)` and `scroll` shifts content upward.

use de_api::Rect;

/// Vertical gap between cells and around group headers.
pub const GAP: i32 = 2;
/// Height of a date-group header row.
pub const HEADER_H: i32 = 44;
/// Horizontal inset of group headers (matches `ui::MARGIN`).
pub const HEADER_X: i32 = 24;

/// One day group: label plus a half-open photo index range.
#[derive(Debug, Clone)]
pub struct Group {
    pub label: String,
    pub first: usize,
    pub end: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct Cell {
    /// Index into the photo list.
    pub photo: usize,
    pub rect: Rect,
}

#[derive(Debug, Clone, Copy)]
pub struct Header {
    /// Index into the group list.
    pub group: usize,
    pub rect: Rect,
}

#[derive(Debug, Clone, Default)]
pub struct Layout {
    pub cell: i32,
    pub cells: Vec<Cell>,
    pub headers: Vec<Header>,
    /// Total scrollable content height in content coordinates.
    pub content_h: i32,
}

/// Columns: aim for ~220 px cells, clamped to a sane phone range.
#[must_use]
pub fn cols_for(view_w: i32) -> i32 {
    (view_w / 220).clamp(2, 5)
}

/// Build the full content layout for `groups` inside `view_w`.
#[must_use]
pub fn build(view_w: i32, groups: &[Group]) -> Layout {
    let cols = cols_for(view_w);
    let cell = ((view_w - (cols - 1) * GAP) / cols).max(1);
    let mut cells = Vec::new();
    let mut headers = Vec::new();
    let mut y = 0;
    for (gi, g) in groups.iter().enumerate() {
        if g.end <= g.first {
            continue;
        }
        headers.push(Header {
            group: gi,
            rect: Rect::new(HEADER_X, y, view_w - HEADER_X * 2, HEADER_H),
        });
        y += HEADER_H;
        let n = g.end - g.first;
        let rows = n.div_ceil(cols as usize);
        for r in 0..rows {
            for c in 0..cols as usize {
                let idx = g.first + r * cols as usize + c;
                if idx >= g.end {
                    break;
                }
                cells.push(Cell {
                    photo: idx,
                    rect: Rect::new(
                        c as i32 * (cell + GAP),
                        y + r as i32 * (cell + GAP),
                        cell,
                        cell,
                    ),
                });
            }
        }
        // Row block: cells + inter-row gaps + one gap before the next header.
        y += rows as i32 * (cell + GAP);
    }
    Layout {
        cell,
        cells,
        headers,
        content_h: y,
    }
}

/// Clamp a scroll offset into `[0, max_scroll]`.
#[must_use]
pub fn clamp_scroll(scroll: f32, max_scroll: i32) -> f32 {
    scroll.clamp(0.0, max_scroll.max(0) as f32)
}

/// Cell index under `pos` (canvas coords), honoring `scroll` and `view_top`.
#[must_use]
pub fn hit_cell(layout: &Layout, view_top: i32, scroll: f32, pos: de_api::Point) -> Option<usize> {
    let sy = (pos.y - view_top) as f32;
    for c in &layout.cells {
        let top = c.rect.y as f32 - scroll;
        let bottom = top + c.rect.h as f32;
        if sy >= top && sy < bottom && pos.x >= c.rect.x && pos.x < c.rect.x + c.rect.w {
            return Some(c.photo);
        }
    }
    None
}

/// Photo indices whose cells intersect the visible window (for decode
/// scheduling). Returns indices in paint order.
#[must_use]
pub fn visible_indices(layout: &Layout, view_h: i32, scroll: f32) -> Vec<usize> {
    let mut out = Vec::new();
    for c in &layout.cells {
        let top = c.rect.y as f32 - scroll;
        let bottom = top + c.rect.h as f32;
        if bottom >= 0.0 && top <= view_h as f32 {
            out.push(c.photo);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn groups(n: usize) -> Vec<Group> {
        vec![Group {
            label: "Today".into(),
            first: 0,
            end: n,
        }]
    }

    #[test]
    fn three_cols_on_phone_width() {
        let l = build(720, &groups(10));
        assert_eq!(cols_for(720), 3);
        assert_eq!(l.cells.len(), 10);
        assert!(l.content_h > 0);
    }

    #[test]
    fn cell_rects_tile_without_overlap() {
        let l = build(720, &groups(7));
        for w in l.cells.windows(2) {
            let (a, b) = (w[0].rect, w[1].rect);
            // Same row neighbours must not overlap; rows advance in y.
            let overlap = a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;
            assert!(!overlap);
        }
    }

    #[test]
    fn headers_counted_once_per_group() {
        let gs = vec![
            Group {
                label: "Today".into(),
                first: 0,
                end: 3,
            },
            Group {
                label: "Yesterday".into(),
                first: 3,
                end: 5,
            },
        ];
        let l = build(720, &gs);
        assert_eq!(l.headers.len(), 2);
        assert_eq!(l.cells.len(), 5);
    }

    #[test]
    fn scroll_clamps() {
        assert_eq!(clamp_scroll(-50.0, 1000), 0.0);
        assert_eq!(clamp_scroll(5000.0, 1000), 1000.0);
        assert_eq!(clamp_scroll(500.0, 1000), 500.0);
        assert_eq!(clamp_scroll(10.0, 0), 0.0);
    }

    #[test]
    fn hit_and_visible_use_scroll() {
        let l = build(720, &groups(30));
        // First cell sits under the first header.
        let p = de_api::Point::new(l.cells[0].rect.x + 4, l.cells[0].rect.y + 4);
        assert_eq!(hit_cell(&l, 0, 0.0, p), Some(0));
        // Scrolled past it: same canvas point now hits a lower cell or none.
        let hit_scrolled = hit_cell(&l, 0, l.cells[3].rect.y as f32, p);
        assert!(hit_scrolled.is_none() || hit_scrolled != Some(0));
        let vis = visible_indices(&l, 800, 0.0);
        assert!(vis.len() >= 6 && vis.len() < 30);
        assert!(vis.contains(&0));
    }
}
