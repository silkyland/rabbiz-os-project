//! Viewer geometry: fit-to-width page layout, zoom/pan clamps, swipe paging,
//! and the overview-grid cell math. Pure functions so the state machine in
//! `main.rs` stays unit-testable headless.
//!
//! Conventions (same model as `rabbiz-photos`):
//! - `pan` = offset of the page *center* from the content-area center, in
//!   canvas px. At zoom 1 on a page taller than the viewport this is the
//!   vertical scroll position.
//! - Display sizes are in *screen* px; the rendered pixmap is larger by the
//!   supersample factor and `blit_scaled` maps it down.

use de_api::{Point, Rect};

/// Double-tap window and slop.
pub const DOUBLE_TAP_MS: u128 = 300;
pub const DOUBLE_TAP_DIST: i32 = 48;
/// Zoom toggled to on double-tap (no pinch events exist in `AppEvent`).
pub const DOUBLE_TAP_ZOOM: f32 = 2.0;
/// Finger travel before a touch becomes a drag (tap slop).
pub const TOUCH_SLOP: i32 = 10;
/// Fraction of viewport width (or flick speed) that commits a page swipe.
pub const PAGE_DIST_FRAC: f32 = 0.25;
pub const PAGE_VELOCITY: f32 = 0.9; // px per millisecond
/// Edge rubber-band resistance for swipes past the first/last page.
pub const RUBBER: f32 = 0.35;
/// Zoom steps offered by the +/- buttons, in order.
pub const ZOOM_STEPS: [f32; 4] = [1.0, 1.5, 2.0, 3.0];
/// Supersample factor: pages are rasterized at `display_scale * SS` px/pt so
/// zoomed-in regions stay sharp; `blit_scaled` downsamples to the screen.
pub const SUPERSAMPLE: f32 = 2.0;
/// Gap between overview-grid cells and around the grid.
pub const GRID_GAP: i32 = 12;
pub const GRID_COLS: i32 = 3;

/// Is this `up` a double-tap following `last_up`?
#[must_use]
pub fn is_double_tap(
    last: Option<(std::time::Instant, Point)>,
    now: std::time::Instant,
    pos: Point,
) -> bool {
    let Some((t, p)) = last else {
        return false;
    };
    now.duration_since(t).as_millis() <= DOUBLE_TAP_MS
        && (pos.x - p.x).abs() <= DOUBLE_TAP_DIST
        && (pos.y - p.y).abs() <= DOUBLE_TAP_DIST
}

/// Fit scale that maps a page of `page_pt` (w,h in points) onto a content
/// rect of `view_w`×`view_h` screen px at zoom 1: the page *width* always
/// fills `view_w`; height may overflow (scroll) or letterbox (centered).
#[must_use]
pub fn fit_width_scale(view_w: i32, page_w_pt: f32) -> f32 {
    if page_w_pt <= 0.0 || view_w <= 0 {
        return 1.0;
    }
    view_w as f32 / page_w_pt
}

/// Displayed page size in screen px at `zoom`.
#[must_use]
pub fn display_size(page_w_pt: f32, page_h_pt: f32, base: f32, zoom: f32) -> (f32, f32) {
    (page_w_pt * base * zoom, page_h_pt * base * zoom)
}

/// Clamp `pan` so the displayed page still covers the whole content area.
/// On any axis where the page is smaller than the view, pan is forced to 0
/// (the page is centered instead).
#[must_use]
pub fn clamp_pan(
    pan: (f32, f32),
    view_w: i32,
    view_h: i32,
    page_w_pt: f32,
    page_h_pt: f32,
    base: f32,
    zoom: f32,
) -> (f32, f32) {
    let (dw, dh) = display_size(page_w_pt, page_h_pt, base, zoom);
    let max_x = ((dw - view_w as f32) / 2.0).max(0.0);
    let max_y = ((dh - view_h as f32) / 2.0).max(0.0);
    (
        if max_x <= 0.0 {
            0.0
        } else {
            pan.0.clamp(-max_x, max_x)
        },
        if max_y <= 0.0 {
            0.0
        } else {
            pan.1.clamp(-max_y, max_y)
        },
    )
}

/// "Home" pan for a freshly opened page: top-aligned when the page is taller
/// than the content area (so scroll position 0 = document start), centered
/// when it fits. `pan.y` sign convention: positive moves the page *down* on
/// screen, i.e. reveals content above — so the top of a tall page sits at
/// `pan.y = +max_y`.
#[must_use]
pub fn home_pan(view_h: i32, page_w_pt: f32, page_h_pt: f32, base: f32, zoom: f32) -> (f32, f32) {
    let (_, dh) = display_size(page_w_pt, page_h_pt, base, zoom);
    let max_y = ((dh - view_h as f32) / 2.0).max(0.0);
    (0.0, if dh > view_h as f32 { max_y } else { 0.0 })
}

/// Zoom `old_zoom -> new_zoom` keeping the content-area point `tap` stationary.
/// `center` is the content-area center in canvas coords.
#[must_use]
pub fn zoom_pan_at(
    pan: (f32, f32),
    tap: Point,
    center: (f32, f32),
    old_zoom: f32,
    new_zoom: f32,
) -> (f32, f32) {
    if old_zoom <= 0.0 || (new_zoom - old_zoom).abs() < f32::EPSILON {
        return pan;
    }
    let rx = tap.x as f32 - center.0;
    let ry = tap.y as f32 - center.1;
    let k = new_zoom / old_zoom;
    (rx - (rx - pan.0) * k, ry - (ry - pan.1) * k)
}

/// Screen rect the page occupies: centered on `center + pan` (center of the
/// content area), sized `display_size`.
#[must_use]
pub fn page_rect(
    view: Rect,
    page_w_pt: f32,
    page_h_pt: f32,
    base: f32,
    zoom: f32,
    pan: (f32, f32),
) -> Rect {
    let (dw, dh) = display_size(page_w_pt, page_h_pt, base, zoom);
    let cx = view.x as f32 + view.w as f32 / 2.0 + pan.0;
    let cy = view.y as f32 + view.h as f32 / 2.0 + pan.1;
    Rect::new(
        (cx - dw / 2.0) as i32,
        (cy - dh / 2.0) as i32,
        dw as i32,
        dh as i32,
    )
}

/// Decide whether a completed horizontal swipe pages, and which way.
/// `dx` = total horizontal travel (positive = finger moved right = previous
/// page), `vx` = smoothed release velocity in px/ms. Returns `Some(1)` for
/// previous, `Some(-1)` for next, `None` to snap back.
#[must_use]
pub fn page_dir(dx: f32, vx: f32, view_w: i32) -> Option<i32> {
    if view_w <= 0 {
        return None;
    }
    let dist_hit = dx.abs() > view_w as f32 * PAGE_DIST_FRAC;
    let vel_hit = vx.abs() > PAGE_VELOCITY;
    if dist_hit || vel_hit {
        Some(if dx > 0.0 { 1 } else { -1 })
    } else {
        None
    }
}

/// Rubber-band an overscroll delta (swipe past the first/last page): the
/// further you pull, the less the content follows.
#[must_use]
pub fn rubber_band(delta: f32) -> f32 {
    delta.signum() * (delta.abs() * RUBBER).min(120.0)
}

/// Next zoom step after `z` in the `ZOOM_STEPS` ladder (`dir` = +1/-1).
/// Returns the clamped nearest step; at the ends it stays put.
#[must_use]
pub fn zoom_step(z: f32, dir: i32) -> f32 {
    if dir >= 0 {
        ZOOM_STEPS
            .iter()
            .copied()
            .find(|s| *s > z + 0.01)
            .unwrap_or(*ZOOM_STEPS.last().unwrap())
    } else {
        ZOOM_STEPS
            .iter()
            .rev()
            .copied()
            .find(|s| *s < z - 0.01)
            .unwrap_or(ZOOM_STEPS[0])
    }
}

/// Grid cell rect for overview cell `idx` in a `cols`-column grid inside
/// `view` (already inset by margins), with `scroll` px of vertical offset.
/// Returns `None` for cells fully outside `view`'s vertical span.
#[must_use]
pub fn grid_cell(view: Rect, idx: usize, cols: i32, scroll: f32) -> Option<Rect> {
    if cols <= 0 || view.w <= 0 {
        return None;
    }
    let cols = cols.min(GRID_COLS.max(1));
    let gap = GRID_GAP;
    let cell_w = (view.w - gap * (cols - 1)) / cols;
    if cell_w <= 0 {
        return None;
    }
    // Uniform 3:4-ish cells (PDF pages are usually portrait).
    let cell_h = cell_w * 4 / 3 + gap;
    let col = (idx as i32) % cols;
    let row = (idx as i32) / cols;
    let x = view.x + col * (cell_w + gap);
    let y = view.y + row * cell_h - scroll as i32;
    if y + cell_h < view.y || y > view.y + view.h {
        return None;
    }
    Some(Rect::new(x, y, cell_w, cell_h - gap))
}

/// Total content height of an `n`-cell overview grid.
#[must_use]
pub fn grid_content_h(view_w: i32, n: usize) -> i32 {
    if view_w <= 0 || n == 0 {
        return 0;
    }
    let cols = GRID_COLS.min(view_w / 80).max(1);
    let gap = GRID_GAP;
    let cell_w = (view_w - gap * (cols - 1)) / cols;
    if cell_w <= 0 {
        return 0;
    }
    let cell_h = cell_w * 4 / 3 + gap;
    let rows = (n as i32 + cols - 1) / cols;
    rows * cell_h
}

/// Fit a `src` pixmap inside `dst` keeping aspect (letterbox), returning the
/// inner rect — used for overview thumbnails of mixed page sizes.
#[must_use]
pub fn contain_rect(dst: Rect, src_w: i32, src_h: i32) -> Rect {
    if src_w <= 0 || src_h <= 0 || dst.w <= 0 || dst.h <= 0 {
        return Rect::new(dst.x, dst.y, 0, 0);
    }
    let s = (dst.w as f32 / src_w as f32).min(dst.h as f32 / src_h as f32);
    let w = (src_w as f32 * s) as i32;
    let h = (src_h as f32 * s) as i32;
    Rect::new(dst.x + (dst.w - w) / 2, dst.y + (dst.h - h) / 2, w, h)
}

/// Index of the grid cell under `pos`, or `None`.
#[must_use]
pub fn hit_cell(view: Rect, count: usize, scroll: f32, pos: Point) -> Option<usize> {
    for i in 0..count {
        if let Some(r) = grid_cell(view, i, GRID_COLS, scroll) {
            if r.contains(pos) {
                return Some(i);
            }
        }
    }
    None
}

/// Clamp `scroll` into `0..=max`.
#[must_use]
pub fn clamp_scroll(scroll: f32, max: i32) -> f32 {
    scroll.clamp(0.0, max.max(0) as f32)
}

/// Cell size hint for a document page aspect, used to pre-size thumb renders.
#[must_use]
pub fn thumb_scale_for(page_w_pt: f32, page_h_pt: f32, cell_w: i32) -> f32 {
    // Render thumbs at ~1.5× the on-screen cell width for crispness.
    let _ = page_h_pt;
    let want_w_px = (cell_w as f32 * 1.5).max(64.0);
    want_w_px / page_w_pt.max(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn double_tap_window() {
        let t0 = Instant::now();
        let p = Point::new(100, 100);
        assert!(!is_double_tap(None, t0, p));
        assert!(is_double_tap(
            Some((t0, p)),
            t0 + Duration::from_millis(120),
            Point::new(110, 105)
        ));
        assert!(!is_double_tap(
            Some((t0, p)),
            t0 + Duration::from_millis(400),
            Point::new(110, 105)
        ));
        assert!(!is_double_tap(
            Some((t0, p)),
            t0 + Duration::from_millis(50),
            Point::new(200, 100)
        ));
    }

    #[test]
    fn fit_width_fills_width_only() {
        // A4 portrait 612x792 into 672-wide content: scale = 672/612.
        let s = fit_width_scale(672, 612.0);
        assert!((s - 672.0 / 612.0).abs() < 1e-6);
        let (dw, dh) = display_size(612.0, 792.0, s, 1.0);
        assert!((dw - 672.0).abs() < 0.01);
        // Height follows the page aspect: 792pt * (672/612) ≈ 871 px.
        assert!((dh - 792.0 * s).abs() < 0.5);
    }

    #[test]
    fn pan_clamped_per_axis() {
        // Page 612x792pt, content 672x1400, base≈1.098 → display 672x870:
        // width exactly covers → x pan forced 0; height under-covers → y 0.
        let base = fit_width_scale(672, 612.0);
        let pan = clamp_pan((9999.0, 9999.0), 672, 1400, 612.0, 792.0, base, 1.0);
        // Float dust: display width is 672±ε, so x pan is ~0 not exactly 0.
        assert!(pan.0.abs() < 0.01 && pan.1.abs() < 0.01, "{pan:?}");
        // Zoom 2: display 1344x1740 → x ±336, y ±170.
        let pan = clamp_pan((99999.0, 99999.0), 672, 1400, 612.0, 792.0, base, 2.0);
        assert!((pan.0 - 336.0).abs() < 1.0);
        assert!((pan.1 - 170.0).abs() < 1.0);
        // Tall page (receipt 612x3000pt): at zoom1 height overflows → y pans,
        // x stays 0.
        let base2 = fit_width_scale(672, 612.0);
        let pan = clamp_pan((50.0, 5000.0), 672, 1400, 612.0, 3000.0, base2, 1.0);
        assert!(pan.0.abs() < 0.01);
        assert!(pan.1 > 0.0 && pan.1 < 5000.0);
    }

    #[test]
    fn home_pan_top_aligns_tall_pages() {
        // Tall page: pan.y = +max_y shows the document top.
        let base = fit_width_scale(672, 612.0);
        let pan = home_pan(1400, 612.0, 3000.0, base, 1.0);
        let (_, dh) = display_size(612.0, 3000.0, base, 1.0);
        assert!((pan.1 - (dh - 1400.0) / 2.0).abs() < 0.5);
        // Verify: page_rect top edge lands on the content top.
        let view = Rect::new(24, 100, 672, 1400);
        let r = page_rect(view, 612.0, 3000.0, base, 1.0, pan);
        assert!((r.y - view.y).abs() < 2, "top={} view.y={}", r.y, view.y);
        // Short page: centered.
        let pan = home_pan(1400, 612.0, 792.0, base, 1.0);
        assert_eq!(pan, (0.0, 0.0));
    }

    #[test]
    fn zoom_keeps_tap_stationary() {
        let center = (360.0f32, 800.0f32);
        let pan0 = (10.0f32, -5.0f32);
        let tap_center = Point::new((center.0 + pan0.0) as i32, (center.1 + pan0.1) as i32);
        let pan = zoom_pan_at(pan0, tap_center, center, 1.0, 2.0);
        assert!((pan.0 - pan0.0).abs() < 0.6 && (pan.1 - pan0.1).abs() < 0.6);
    }

    #[test]
    fn page_thresholds() {
        assert_eq!(page_dir(200.0, 0.0, 672), Some(1));
        assert_eq!(page_dir(-200.0, 0.0, 672), Some(-1));
        assert_eq!(page_dir(50.0, 0.5, 672), None);
        assert_eq!(page_dir(50.0, 1.2, 672), Some(1));
        assert_eq!(page_dir(-50.0, -1.2, 672), Some(-1));
        assert_eq!(page_dir(10.0, 0.1, 672), None);
    }

    #[test]
    fn rubber_softens() {
        assert_eq!(rubber_band(0.0), 0.0);
        assert!(rubber_band(100.0).abs() < 40.0);
        assert_eq!(rubber_band(-1000.0), -120.0);
    }

    #[test]
    fn zoom_steps_ladder() {
        assert_eq!(zoom_step(1.0, 1), 1.5);
        assert_eq!(zoom_step(1.4, 1), 1.5);
        assert_eq!(zoom_step(3.0, 1), 3.0);
        assert_eq!(zoom_step(1.0, -1), 1.0);
        assert_eq!(zoom_step(2.0, -1), 1.5);
        assert_eq!(zoom_step(1.5, -1), 1.0);
    }

    #[test]
    fn grid_cells_tile() {
        let view = Rect::new(24, 100, 672, 1200);
        let c0 = grid_cell(view, 0, GRID_COLS, 0.0).unwrap();
        let c1 = grid_cell(view, 1, GRID_COLS, 0.0).unwrap();
        let c3 = grid_cell(view, 3, GRID_COLS, 0.0).unwrap();
        assert_eq!(c0.x, view.x);
        assert_eq!(c1.x, c0.x + c0.w + GRID_GAP);
        assert_eq!(c3.y, c0.y + c0.h + GRID_GAP);
        // Scrolled past → clipped out.
        assert!(grid_cell(view, 0, GRID_COLS, 5000.0).is_none());
        // Hit test finds the right cell.
        let pos = Point::new(c1.x + 4, c1.y + 4);
        assert_eq!(hit_cell(view, 9, 0.0, pos), Some(1));
        // Content height consistent with rows (15 cells = 5 rows > view).
        let h = grid_content_h(view.w, 15);
        assert!(h > view.h);
    }

    #[test]
    fn contain_letterboxes() {
        let dst = Rect::new(0, 0, 200, 200);
        let r = contain_rect(dst, 100, 200);
        assert_eq!(r.w, 100);
        assert_eq!(r.h, 200);
        assert_eq!(r.x, 50);
        let r = contain_rect(dst, 0, 100);
        assert_eq!(r.w, 0);
    }
}
