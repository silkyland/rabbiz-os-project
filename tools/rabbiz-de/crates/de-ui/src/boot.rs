//! P0 boot/brand frame (plan §7.18 P0).
//!
//! `bg`, centered Rabbiz mark 280 px wide, wordmark 360 px wide below it and a
//! 160x4 progress hairline at y=980. Rasterized once by `de-paint` — the
//! compositor uploads it as a single texture.

use de_api::{Pixels, Point, Rect, Size};
use de_design::tokens;
use de_paint::{svg, Canvas};

/// Rabbiz mark (bunny logo), embedded SVG.
const MARK_SVG: &[u8] = include_bytes!("../../../assets/branding/rabbiz-mark.svg");
/// `RabbizOS` wordmark for the dark theme (cream/copper variant of the b`rand`
/// SVG, same artwork — the light-bg original is invisible on `bg`).
const WORDMARK_SVG: &[u8] = include_bytes!("../../../assets/branding/rabbizos-wordmark-dark.svg");

/// Progress hairline y position (contract constant).
const HAIRLINE_Y: i32 = 980;
/// Mark width (contract constant).
const MARK_W: u32 = 280;
/// Wordmark width (contract constant).
const WORDMARK_W: u32 = 360;

/// Rasterize the P0 brand frame for `output` at `progress` (0.0..=1.0).
///
/// Deterministic for a given `output`+`progress` — this is what the golden
/// harness and the real first frame share.
#[must_use]
pub fn brand_frame(output: Size, progress: f32) -> Pixels {
    let theme = tokens::DEFAULT;
    let mut c = Canvas::filled(output, theme.bg);

    let mark = svg::rasterize(MARK_SVG, MARK_W).expect("brand mark svg");
    let wordmark = svg::rasterize(WORDMARK_SVG, WORDMARK_W).expect("wordmark svg");

    // Vertical layout: mark centered slightly above mid, wordmark below.
    let cx = output.w / 2;
    let mark_y = (output.h / 2) - (mark.size.h / 2) - 80;
    let mark_x = cx - mark.size.w / 2;
    let wm_x = cx - wordmark.size.w / 2;
    let wm_y = mark_y + mark.size.h + 32;

    c.blit(&mark, Point::new(mark_x, mark_y));
    c.blit(&wordmark, Point::new(wm_x, wm_y));

    // Progress hairline: track + fill.
    let track = Rect::new(cx - 80, HAIRLINE_Y, 160, 4);
    c.fill_rounded_rect(track, 2.0, theme.surface3);
    let fill_w = (160.0 * progress.clamp(0.0, 1.0)) as i32;
    if fill_w > 0 {
        c.fill_rounded_rect(Rect::new(track.x, track.y, fill_w, 4), 2.0, theme.accent);
    }

    c.into_pixels()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brand_frame_is_deterministic() {
        let a = brand_frame(Size::new(720, 1600), 0.5);
        let b = brand_frame(Size::new(720, 1600), 0.5);
        assert_eq!(a, b);
        // Background must be the theme bg at the corners.
        assert_eq!(&a.data[0..4], &tokens::DEFAULT.bg);
        // Mark area must not be background-colored (something was drawn).
        let center = a.data[((800 * 720 + 360) * 4) as usize..]
            .first_chunk::<4>()
            .unwrap();
        assert_ne!(*center, [0, 0, 0, 0]);
    }
}
