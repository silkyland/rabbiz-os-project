//! Viewer gesture math: double-tap zoom, drag pan, swipe paging.
//!
//! Everything here is pure so the state machine is unit-testable headless;
//! `main.rs` owns the `Instant`s and event plumbing.
//!
//! Conventions:
//! - `view` = viewport size (canvas), `img` = decoded image size in px.
//! - `base` = fit scale (`min(view/img)`), so at `zoom == 1` the image
//!   exactly fills the view along its limiting axis.
//! - `pan` = offset of the image *center* from the view center, in canvas px.
//!   At zoom 1 pan is always (0, 0); zooming may move it to keep the tapped
//!   point stationary.

use de_api::Point;

/// Double-tap window and slop.
pub const DOUBLE_TAP_MS: u128 = 300;
pub const DOUBLE_TAP_DIST: i32 = 48;
/// Zoom toggled to on double-tap (no pinch events exist in `AppEvent`).
pub const DOUBLE_TAP_ZOOM: f32 = 2.5;
/// Finger travel before a touch becomes a drag (tap slop).
pub const TOUCH_SLOP: i32 = 10;
/// Fraction of viewport width (or flick speed) that commits a page swipe.
pub const PAGE_DIST_FRAC: f32 = 0.25;
pub const PAGE_VELOCITY: f32 = 0.9; // px per millisecond
/// Edge rubber-band resistance for swipes past the first/last photo.
pub const RUBBER: f32 = 0.35;
/// Fallback viewer decode width cap (canvas px); zoom headroom without
/// decoding full sensor resolution into the cache.
pub const VIEW_DECODE_W: i32 = 1440;

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

/// Fit scale that makes `img` fill `view` (letterboxed on the long axis).
#[must_use]
pub fn fit_scale(view_w: i32, view_h: i32, img_w: i32, img_h: i32) -> f32 {
    if img_w <= 0 || img_h <= 0 || view_w <= 0 || view_h <= 0 {
        return 1.0;
    }
    (view_w as f32 / img_w as f32).min(view_h as f32 / img_h as f32)
}

/// Displayed image size at `zoom` (zoom multiplies the fit scale).
#[must_use]
pub fn display_size(img_w: i32, img_h: i32, base: f32, zoom: f32) -> (f32, f32) {
    (img_w as f32 * base * zoom, img_h as f32 * base * zoom)
}

/// Clamp `pan` so the scaled image still covers the whole viewport.
/// When the image is smaller than the view on an axis (shouldn't happen
/// after cropping, but cheap insurance) pan on that axis is forced to 0.
#[must_use]
pub fn clamp_pan(
    pan: (f32, f32),
    view_w: i32,
    view_h: i32,
    img_w: i32,
    img_h: i32,
    base: f32,
    zoom: f32,
) -> (f32, f32) {
    let (dw, dh) = display_size(img_w, img_h, base, zoom);
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

/// Zoom `old_zoom -> new_zoom` keeping the canvas point `tap` stationary.
/// `center` is the view center; `pan`/`tap`/`center` are canvas coordinates.
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

/// Screen rect the image occupies: centered on `center + pan`, sized
/// `display_size`. Used both for drawing and for the open/close morph.
#[must_use]
pub fn image_rect(
    view_w: i32,
    view_h: i32,
    img_w: i32,
    img_h: i32,
    base: f32,
    zoom: f32,
    pan: (f32, f32),
) -> (f32, f32, f32, f32) {
    let (dw, dh) = display_size(img_w, img_h, base, zoom);
    (
        view_w as f32 / 2.0 + pan.0 - dw / 2.0,
        view_h as f32 / 2.0 + pan.1 - dh / 2.0,
        dw,
        dh,
    )
}

/// Decide whether a completed horizontal swipe pages, and which way.
/// `dx` = total horizontal travel (positive = finger moved right = previous
/// photo), `vx` = smoothed release velocity in px/ms. Returns `Some(1)` for
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

/// Rubber-band an overscroll delta (grid at an edge or viewer swipe past
/// the last photo): the further you pull, the less the content follows.
#[must_use]
pub fn rubber_band(delta: f32) -> f32 {
    delta.signum() * (delta.abs() * RUBBER).min(120.0)
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
    fn fit_scale_letterboxes() {
        // 4000x3000 into 720x1600 view → width-limited.
        let s = fit_scale(720, 1600, 4000, 3000);
        assert!((s - 0.18).abs() < 0.001);
        assert!((4000.0 * s - 720.0).abs() < 0.01);
    }

    #[test]
    fn pan_clamped_to_cover() {
        // img 1000x500, view 500x500, base 0.5 → displayed 500x250 at zoom1:
        // height under-covers → pan.y forced 0; width exactly covers → pan.x 0.
        let pan = clamp_pan((999.0, 999.0), 500, 500, 1000, 500, 0.5, 1.0);
        assert_eq!(pan, (0.0, 0.0));
        // zoom 2 → displayed 1000x500: x range ±250, y range 0.
        let pan = clamp_pan((9999.0, 9999.0), 500, 500, 1000, 500, 0.5, 2.0);
        assert_eq!(pan, (250.0, 0.0));
        let pan = clamp_pan((-9999.0, 0.0), 500, 500, 1000, 500, 0.5, 2.0);
        assert_eq!(pan, (-250.0, 0.0));
    }

    #[test]
    fn zoom_keeps_tap_stationary() {
        // Tapping the image's own center (view center + pan) leaves pan put.
        let center = (250.0f32, 400.0f32);
        let pan0 = (10.0f32, -5.0f32);
        let tap_center = Point::new((center.0 + pan0.0) as i32, (center.1 + pan0.1) as i32);
        let pan = zoom_pan_at(pan0, tap_center, center, 1.0, 2.0);
        assert!((pan.0 - pan0.0).abs() < 0.6 && (pan.1 - pan0.1).abs() < 0.6);
        // Tap offset from center: the image point under it stays put.
        // Verify by projecting an image point before/after.
        let view = (500, 800);
        let img = (1000, 1000);
        let base = fit_scale(view.0, view.1, img.0, img.1);
        let center = (view.0 as f32 / 2.0, view.1 as f32 / 2.0);
        let tap = Point::new(400, 300);
        let pan0 = (0.0f32, 0.0f32);
        let pan1 = zoom_pan_at(pan0, tap, center, 1.0, 2.0);
        // Image-space fraction under `tap` before/after must match:
        // fx = (tap - center - pan + disp/2) / disp.
        let project = |pan: (f32, f32), zoom: f32| -> f32 {
            let (dw, _dh) = display_size(img.0, img.1, base, zoom);
            (tap.x as f32 - center.0 - pan.0 + dw / 2.0) / dw
        };
        let fx0 = project(pan0, 1.0);
        let fx1 = project(pan1, 2.0);
        assert!((fx0 - fx1).abs() < 0.5, "{fx0} vs {fx1}");
    }

    #[test]
    fn page_thresholds() {
        assert_eq!(page_dir(200.0, 0.0, 720), Some(1)); // far, slow
        assert_eq!(page_dir(-200.0, 0.0, 720), Some(-1));
        assert_eq!(page_dir(50.0, 0.5, 720), None); // short, slow
        assert_eq!(page_dir(50.0, 1.2, 720), Some(1)); // short but fast flick
        assert_eq!(page_dir(-50.0, -1.2, 720), Some(-1));
        assert_eq!(page_dir(10.0, 0.1, 720), None);
        assert_eq!(page_dir(900.0, 0.0, 720), Some(1));
    }

    #[test]
    fn rubber_softens() {
        assert_eq!(rubber_band(0.0), 0.0);
        assert!(rubber_band(100.0).abs() < 40.0);
        assert_eq!(rubber_band(-1000.0), -120.0);
    }
}
