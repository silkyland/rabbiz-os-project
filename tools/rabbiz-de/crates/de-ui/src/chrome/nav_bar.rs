//! Nav strip: the 32 px gesture-bar overlay composited on top of every
//! frame (M1-T8 shell stub reduced to its surviving piece — the clock home
//! it used to draw was replaced by the real `home` screen in M3).
//!
//! Drawn by `de-core` as a cached compositor texture (not a Wayland client).

use de_api::{Pixels, Rect, Size};
use de_design::tokens;
use de_paint::Canvas;

/// Rasterize the 32 px gesture bar (pill indicator centered near the bottom
/// edge). Transparent everywhere except the pill so it can overlay apps.
#[must_use]
pub fn gesture_bar(output: Size) -> Pixels {
    let theme = tokens::DEFAULT;
    let bar_h = tokens::GESTURE_BAR_HEIGHT;
    let mut c = Canvas::new(Size::new(output.w, bar_h));
    let pill_w = output.w / 3;
    let pill = Rect::new((output.w - pill_w) / 2, bar_h - 12, pill_w, 4);
    c.fill_rounded_rect(pill, 2.0, theme.text3);
    c.into_pixels()
}
