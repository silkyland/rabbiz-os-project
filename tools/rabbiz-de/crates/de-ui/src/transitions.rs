//! Shared-element transition helpers (M3-T5, §7.17a): pure lerp/easing
//! functions shared by the launch morph, close morph, and crossfades.
//! Velocity lives in `de_paint::animation::SpringValue` — this module is
//! only the deterministic "where is the element at progress p" half.

use de_api::Rect;

/// Linear interpolation.
#[must_use]
pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t.clamp(0.0, 1.0)
}

/// Rect linear interpolation (all four fields).
#[must_use]
pub fn lerp_rect(a: Rect, b: Rect, t: f32) -> Rect {
    Rect::new(
        lerp(a.x as f32, b.x as f32, t) as i32,
        lerp(a.y as f32, b.y as f32, t) as i32,
        lerp(a.w as f32, b.w as f32, t) as i32,
        lerp(a.h as f32, b.h as f32, t) as i32,
    )
}

/// RGBA channel-wise lerp (premultiplied-safe for our use: colors here are
/// opaque or alpha-faded uniformly).
#[must_use]
pub fn lerp_color(a: [u8; 4], b: [u8; 4], t: f32) -> [u8; 4] {
    [
        lerp(a[0] as f32, b[0] as f32, t) as u8,
        lerp(a[1] as f32, b[1] as f32, t) as u8,
        lerp(a[2] as f32, b[2] as f32, t) as u8,
        lerp(a[3] as f32, b[3] as f32, t) as u8,
    ]
}

/// Standard bezier fallback (§7.17a degrade ladder step): cubic-out.
#[must_use]
pub fn ease_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

/// Fade-in window: 0 below `start`, ramps to 1 by `end`.
#[must_use]
pub fn fade_in(p: f32, start: f32, end: f32) -> f32 {
    if end <= start {
        return if p >= end { 1.0 } else { 0.0 };
    }
    ((p - start) / (end - start)).clamp(0.0, 1.0)
}

/// Alpha premultiply helper: applies `opacity` to `color`'s alpha.
#[must_use]
pub fn with_opacity(color: [u8; 4], opacity: f32) -> [u8; 4] {
    [color[0], color[1], color[2], (color[3] as f32 * opacity.clamp(0.0, 1.0)) as u8]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lerp_rect_endpoints() {
        let a = Rect::new(10, 20, 100, 60);
        let b = Rect::new(0, 0, 720, 1600);
        assert_eq!(lerp_rect(a, b, 0.0), a);
        assert_eq!(lerp_rect(a, b, 1.0), b);
        let m = lerp_rect(a, b, 0.5);
        assert_eq!(m, Rect::new(5, 10, 410, 830));
    }

    #[test]
    fn fade_window() {
        assert_eq!(fade_in(0.2, 0.5, 0.9), 0.0);
        assert_eq!(fade_in(0.95, 0.5, 0.9), 1.0);
        assert!((fade_in(0.7, 0.5, 0.9) - 0.5).abs() < 0.01);
    }
}
