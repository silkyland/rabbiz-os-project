//! Pure layout math for the file list/grid: row & cell geometry, scroll
//! clamping, and index↔y mapping. Host-testable.

/// List row height (px) — icon + two text lines.
pub const ROW_H: i32 = 64;
/// Gap between rows / grid cells (px).
pub const GAP: i32 = 10;
/// Horizontal page margin (px) — matches `ui::MARGIN`-ish density for lists.
pub const PAD: i32 = 20;
/// Grid columns on the 720 px panel.
pub const GRID_COLS: usize = 3;
/// Grid cell height (px): big glyph + label.
pub const CELL_H: i32 = 168;
/// Top-of-content offset reserved for the ".." parent row — the parent row
/// uses the same ROW_H as entries.
#[must_use]
pub fn row_y(index: usize, scroll: f32) -> f32 {
    index as f32 * (ROW_H + GAP) as f32 - scroll
}

#[must_use]
pub fn cell_y(index: usize, scroll: f32) -> f32 {
    let row = index / GRID_COLS;
    row as f32 * (CELL_H + GAP) as f32 - scroll
}

#[must_use]
pub fn cell_x(index: usize, view_w: i32) -> i32 {
    let col = index % GRID_COLS;
    let cell_w = (view_w - PAD * 2 - GAP * (GRID_COLS as i32 - 1)) / GRID_COLS as i32;
    PAD + col as i32 * (cell_w + GAP)
}

#[must_use]
pub fn cell_w(view_w: i32) -> i32 {
    (view_w - PAD * 2 - GAP * (GRID_COLS as i32 - 1)) / GRID_COLS as i32
}

/// Total content height for `count` items in the given view.
#[must_use]
pub fn content_h(count: usize, grid: bool) -> i32 {
    if count == 0 {
        return 0;
    }
    if grid {
        let rows = count.div_ceil(GRID_COLS);
        rows as i32 * (CELL_H + GAP) - GAP
    } else {
        count as i32 * (ROW_H + GAP) - GAP
    }
}

/// Clamp scroll offset into `[0, max]`.
#[must_use]
pub fn clamp_scroll(scroll: f32, max: i32) -> f32 {
    scroll.clamp(0.0, max.max(0) as f32)
}

/// Largest valid scroll for `count` items in `view_h`.
#[must_use]
pub fn max_scroll(count: usize, view_h: i32, grid: bool) -> i32 {
    (content_h(count, grid) - view_h).max(0)
}

/// Which list row is under `y` (content-space, scroll already added back by
/// caller passing viewport y + scroll)? We take viewport `y` and `scroll`
/// and return the entry index, or `None` for gaps/empty.
#[must_use]
pub fn row_at(index_space_y: f32) -> Option<usize> {
    if index_space_y < 0.0 {
        return None;
    }
    let slot = ROW_H + GAP;
    let idx = (index_space_y / slot as f32) as usize;
    let within = index_space_y - idx as f32 * slot as f32;
    (within < ROW_H as f32).then_some(idx)
}

#[must_use]
pub fn cell_at(x: i32, y: f32, view_w: i32) -> Option<usize> {
    if y < 0.0 {
        return None;
    }
    let cw = cell_w(view_w);
    let slot_y = CELL_H + GAP;
    let row = (y / slot_y as f32) as usize;
    let within_y = y - row as f32 * slot_y as f32;
    if within_y >= CELL_H as f32 {
        return None;
    }
    // x → column
    let rel = (x - PAD) as f32;
    if rel < 0.0 {
        return None;
    }
    let pitch = (cw + GAP) as f32;
    let col = (rel / pitch) as usize;
    if col >= GRID_COLS {
        return None;
    }
    let within_x = rel - col as f32 * pitch;
    if within_x >= cw as f32 {
        return None;
    }
    Some(row * GRID_COLS + col)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_height_list() {
        assert_eq!(content_h(0, false), 0);
        assert_eq!(content_h(1, false), ROW_H);
        assert_eq!(content_h(2, false), 2 * ROW_H + GAP);
    }

    #[test]
    fn content_height_grid() {
        // 3 cols: 4 items → 2 rows.
        assert_eq!(content_h(3, true), CELL_H);
        assert_eq!(content_h(4, true), 2 * CELL_H + GAP);
        assert_eq!(content_h(7, true), 3 * CELL_H + 2 * GAP);
    }

    #[test]
    fn clamp_scroll_bounds() {
        assert_eq!(clamp_scroll(-50.0, 100), 0.0);
        assert_eq!(clamp_scroll(500.0, 100), 100.0);
        assert_eq!(clamp_scroll(50.0, 100), 50.0);
    }

    #[test]
    fn row_hit_testing() {
        assert_eq!(row_at(-5.0), None);
        assert_eq!(row_at(0.0), Some(0));
        assert_eq!(row_at((ROW_H + GAP) as f32), Some(1));
        // inside the gap between rows → None
        assert_eq!(row_at((ROW_H + 2) as f32), None);
        assert_eq!(row_at((ROW_H + GAP - 1) as f32), None);
    }

    #[test]
    fn cell_hit_testing() {
        let w = 720;
        // first cell center
        let cw = cell_w(w);
        assert!(cw > 200);
        assert_eq!(cell_at(PAD + 10, 10.0, w), Some(0));
        assert_eq!(cell_at(PAD + cw + GAP + 10, 10.0, w), Some(1));
        // second row first col
        assert_eq!(
            cell_at(PAD + 10, (CELL_H + GAP + 10) as f32, w),
            Some(GRID_COLS)
        );
        // gap → None
        assert_eq!(cell_at(PAD + 10, (CELL_H + 2) as f32, w), None);
    }
}
