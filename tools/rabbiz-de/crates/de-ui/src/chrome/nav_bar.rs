//! Nav strip: the 32 px gesture-bar overlay composited on top of every
//! frame in `DisplayArea::SystemBars`.
//!
//! Drawn by `de-core` as a cached compositor texture (not a Wayland client).

use de_api::{Pixels, Rect, Size};
use de_design::tokens::{self, Theme};
use de_paint::Canvas;

/// Resting gesture bar (200×8 pill centered near the bottom edge).
#[must_use]
pub fn gesture_bar(output: Size) -> Pixels {
    gesture_bar_at(output, &tokens::DEFAULT, 0.0)
}

/// Rasterize the 32 px gesture bar at touch-active progress `active` (`0.0` at
/// rest, `1.0` when touched): expands slightly and brightens with a subtle glow.
#[must_use]
pub fn gesture_bar_at(output: Size, theme: &Theme, active: f32) -> Pixels {
    let t = active.clamp(0.0, 1.0);
    let bar_h = tokens::GESTURE_BAR_HEIGHT;
    let mut c = Canvas::new(Size::new(output.w, bar_h));

    let base_w = 200.0_f32.min((output.w as f32 * 0.45).max(80.0));
    let w = (base_w + 36.0 * t).round() as i32;
    let h = (8.0 + 2.0 * t).round() as i32;
    let cy = bar_h - 20; // resting 8 px pill spans y = bar_h - 24 .. bar_h - 16
    let x = (output.w - w) / 2;
    let y = (cy - h / 2).max(0);

    // Subtle luminous halo while touched.
    if t > 0.01 {
        let glow_pad = 3;
        let glow = Rect::new(
            (x - glow_pad).max(0),
            (y - glow_pad).max(0),
            w + glow_pad * 2,
            h + glow_pad * 2,
        );
        let glow_alpha = (56.0 * t).round() as u8;
        c.fill_rounded_rect(
            glow,
            (glow.h as f32) * 0.5,
            [255, 255, 255, glow_alpha],
        );
    }

    let lerp_u8 = |a: u8, b: u8| -> u8 {
        (f32::from(a) + (f32::from(b) - f32::from(a)) * t).round() as u8
    };
    let color = [
        lerp_u8(theme.text3[0], 255),
        lerp_u8(theme.text3[1], 255),
        lerp_u8(theme.text3[2], 255),
        lerp_u8(theme.text3[3], 255),
    ];
    let pill = Rect::new(x, y, w, h);
    c.fill_rounded_rect(pill, (h as f32) * 0.5, color);
    c.into_pixels()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_gesture_bar_is_wider_and_brighter_than_resting() {
        let size = Size::new(720, 1600);
        let rest = gesture_bar_at(size, &tokens::DEFAULT, 0.0);
        let active = gesture_bar_at(size, &tokens::DEFAULT, 1.0);
        let count_lit = |px: &Pixels| -> usize {
            px.data.chunks_exact(4).filter(|p| p[3] > 200).count()
        };
        let center_luma = |px: &Pixels| -> u32 {
            let idx = ((12 * px.size.w + px.size.w / 2) * 4) as usize;
            u32::from(px.data[idx]) + u32::from(px.data[idx + 1]) + u32::from(px.data[idx + 2])
        };
        assert!(count_lit(&active) > count_lit(&rest));
        assert!(center_luma(&active) > center_luma(&rest));
    }
}
