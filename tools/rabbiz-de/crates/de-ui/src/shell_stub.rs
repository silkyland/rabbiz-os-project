//! M1-T8 shell stub: a minimal in-process home surface — clock centered on
//! the brand background plus the 32 px gesture bar at the bottom edge.
//!
//! Drawn by `de-core` as cached compositor textures (not a Wayland client).

use de_api::{Pixels, Point, Rect, Size};
use de_design::tokens;
use de_paint::{text, Canvas};

/// Gesture-bar height in physical px (contract constant, M1-T2/M1-T8).
pub const GESTURE_BAR_H: i32 = 32;

/// Rasterize the home stub (clock + brand) for `output`.
/// Re-rasterize once per minute — the clock is the only changing part.
#[must_use]
pub fn home(output: Size) -> Pixels {
    let theme = tokens::DEFAULT;
    let mut c = Canvas::filled(output, theme.bg);

    // Clock: HH:MM local time, large and centered.
    let now = clock_hhmm();
    let style = text::LabelStyle {
        size: 96.0,
        line_height: 112.0,
        weight: 400,
        color: theme.text,
        ..text::LabelStyle::default()
    };
    if let Some(clock) = text::label(&now, style, None) {
        let cw = clock.width() as i32;
        let ch = clock.height() as i32;
        let px = Pixels::new(Size::new(cw, ch), clock.data().to_vec());
        c.blit(
            &px,
            Point::new((output.w - cw) / 2, (output.h - ch) / 2 - 60),
        );
    }

    c.into_pixels()
}

/// Rasterize the 32 px gesture bar (pill indicator centered near the bottom
/// edge). Transparent everywhere except the pill so it can overlay apps.
#[must_use]
pub fn gesture_bar(output: Size) -> Pixels {
    let theme = tokens::DEFAULT;
    let mut c = Canvas::new(Size::new(output.w, GESTURE_BAR_H));
    let pill_w = output.w / 3;
    let pill = Rect::new((output.w - pill_w) / 2, GESTURE_BAR_H - 12, pill_w, 4);
    c.fill_rounded_rect(pill, 2.0, theme.text3);
    c.into_pixels()
}

fn clock_hhmm() -> String {
    std::process::Command::new("date")
        .arg("+%H:%M")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map_or_else(|| "--:--".to_string(), |s| s.trim().to_string())
}
