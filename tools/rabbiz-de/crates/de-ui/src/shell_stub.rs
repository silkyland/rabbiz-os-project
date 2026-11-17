//! M1 shell stub: the minimal visible "home" behind a closed app.
//!
//! Clock + date on the base surface plus the 32 px gesture bar with its pill.
//! Structure only — the real home screen lands in M3. Geometry helpers are
//! shared with the compositor's overlay so the stub bar and the over-app bar
//! are the same shape.

use de_api::{Point, Rect, Scene, Size, TextAlign, TextStyle, Weight};
use de_design::DARK;

/// Reference panel the stub is authored on.
const REF_W: f32 = 720.0;
const REF_H: f32 = 1600.0;

const LAYER: &str = "shell-stub";

/// Gesture bar height in physical px (P13).
pub const NAV_BAR_H: f32 = 32.0;
/// Nav pill spec: 200×8.
pub const NAV_PILL_W: f32 = 200.0;
pub const NAV_PILL_H: f32 = 8.0;

/// The bottom strip the shell owns for gestures.
pub fn nav_bar_rect(width: f32, height: f32) -> Rect {
    Rect::new(0.0, height - NAV_BAR_H, width, NAV_BAR_H)
}

/// The pill centered horizontally inside the bar.
pub fn nav_pill_rect(width: f32, height: f32) -> Rect {
    let bar = nav_bar_rect(width, height);
    Rect::new(
        (width - NAV_PILL_W) / 2.0,
        bar.y + (bar.h - NAV_PILL_H) / 2.0,
        NAV_PILL_W,
        NAV_PILL_H,
    )
}

pub fn build(size: Size, clock: &str, date: &str) -> Scene {
    let w = size.w as f32;
    let h = size.h as f32;
    let sx = w / REF_W;
    let sy = h / REF_H;
    let s = sx.min(sy);

    let mut scene = Scene::new(size);
    let mut layer = de_api::Layer::new(LAYER);

    layer.rect(Rect::new(0.0, 0.0, w, h), 0.0, DARK.bg);

    // Clock, centered: the stub's one live datum.
    let clock_style = TextStyle::new(DARK.display_px * s, Weight::Light, DARK.text, 600.0 * s)
        .with_align(TextAlign::Center);
    layer.text(
        Point::new((w - clock_style.max_width) / 2.0, 560.0 * sy),
        clock,
        clock_style,
    );

    let date_style = TextStyle::new(DARK.body_px * s, Weight::Regular, DARK.text_dim, 560.0 * s)
        .with_align(TextAlign::Center);
    layer.text(
        Point::new((w - date_style.max_width) / 2.0, 700.0 * sy),
        date,
        date_style,
    );

    // Gesture bar: translucent strip + centered pill, same shape the
    // compositor draws over a mapped app.
    let bar = nav_bar_rect(w, h);
    layer.rect(bar, 0.0, DARK.bg.with_alpha(217));
    let pill = nav_pill_rect(w, h);
    layer.rect(pill, NAV_PILL_H / 2.0, DARK.text.with_alpha(230));

    scene.push(layer);
    scene
}

#[cfg(test)]
mod tests {
    use super::*;
    use de_api::DrawOp;

    fn texts(scene: &Scene) -> Vec<&str> {
        scene
            .layers
            .iter()
            .flat_map(|l| l.ops.iter())
            .filter_map(|op| match op {
                DrawOp::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn stub_shows_clock_date_bar_and_pill() {
        let scene = build(Size::new(720, 1600), "21:47", "Tue, Nov 17");
        let ts = texts(&scene);
        assert!(ts.contains(&"21:47"));
        assert!(ts.contains(&"Tue, Nov 17"));
    }

    #[test]
    fn bar_occupies_the_bottom_32px() {
        let scene = build(Size::new(720, 1600), "09:41", "");
        let bar = scene.layers[0]
            .ops
            .iter()
            .filter_map(|op| match op {
                DrawOp::Rect { rect, .. } if rect.h == NAV_BAR_H => Some(*rect),
                _ => None,
            })
            .next()
            .expect("a 32 px bar rect");
        assert_eq!(bar.y, 1600.0 - NAV_BAR_H);
        assert_eq!(bar.w, 720.0);
    }

    #[test]
    fn pill_is_centered_in_the_bar() {
        let bar = nav_bar_rect(720.0, 1600.0);
        let pill = nav_pill_rect(720.0, 1600.0);
        assert_eq!(pill.w, 200.0);
        assert_eq!(pill.h, 8.0);
        assert_eq!(pill.x, 260.0);
        assert_eq!(pill.y + pill.h / 2.0, bar.y + bar.h / 2.0);
    }
}
