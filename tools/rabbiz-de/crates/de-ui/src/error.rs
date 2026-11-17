//! P18 error/recovery sheet.
//!
//! A dimmed app under a centred 620×360 sheet: the failing app's tile, what
//! happened, and the two ways out. Authored on the 720×1600 reference panel and
//! mapped with `sx`/`sy` for geometry and `s = min(sx, sy)` for radii and type.

use de_api::{Color, Layer, Point, Rect, Scene, Size, TextAlign, TextStyle, Weight};
use de_design::DARK;

const REF_W: f32 = 720.0;
const REF_H: f32 = 1600.0;
const LAYER: &str = "error";

/// P18 error/recovery sheet.
pub fn build(size: Size) -> Scene {
    let sc = Scale::new(size);
    let mut scene = Scene::new(size);
    let mut layer = Layer::new(LAYER);

    // Full bleed base, then the scrim that dims the app behind the sheet.
    let frame = Rect::new(0.0, 0.0, size.w as f32, size.h as f32);
    layer.rect(frame, 0.0, DARK.bg);
    layer.rect(frame, 0.0, DARK.scrim);

    // A centred 620×360 sheet: the only opaque surface on this frame.
    layer.rect(
        sc.rect(50.0, 620.0, 620.0, 360.0),
        DARK.radius_card * sc.s,
        DARK.surface,
    );

    // Calculator tile: a display line over four keys.
    layer.rect(
        sc.rect(332.0, 660.0, 56.0, 56.0),
        DARK.radius_m * sc.s,
        DARK.surface_low,
    );
    layer.rect(sc.rect(344.0, 672.0, 32.0, 5.0), 2.5 * sc.s, DARK.text_dim);
    for (dx, dy) in [(12.0, 30.0), (32.0, 30.0), (12.0, 42.0), (32.0, 42.0)] {
        layer.rect(
            sc.rect(332.0 + dx, 660.0 + dy, 10.0, 6.0),
            3.0 * sc.s,
            DARK.accent,
        );
    }

    let title = sc
        .style(DARK.h2_px, Weight::SemiBold, DARK.text, 556.0)
        .with_align(TextAlign::Center);
    layer.text(
        Point::new(82.0 * sc.sx, 744.0 * sc.sy),
        "Calculator isn't responding",
        title,
    );

    let body = sc
        .style(DARK.caption_px, Weight::Regular, DARK.text_dim, 556.0)
        .with_align(TextAlign::Center);
    layer.text(
        Point::new(82.0 * sc.sx, 800.0 * sc.sy),
        "You can wait for it to respond or close it.",
        body,
    );

    // Wait keeps the app; Close app is the primary, copper action.
    button(
        &mut layer,
        sc,
        sc.rect(90.0, 868.0, 258.0, 72.0),
        "Wait",
        DARK.surface_high,
        DARK.text,
    );
    button(
        &mut layer,
        sc,
        sc.rect(372.0, 868.0, 258.0, 72.0),
        "Close app",
        DARK.accent,
        DARK.accent_ink,
    );

    scene.push(layer);
    scene
}

/// Reference-panel scale factors for one output size.
#[derive(Clone, Copy)]
struct Scale {
    sx: f32,
    sy: f32,
    s: f32,
}

impl Scale {
    fn new(size: Size) -> Self {
        let sx = size.w as f32 / REF_W;
        let sy = size.h as f32 / REF_H;
        Self {
            sx,
            sy,
            s: sx.min(sy),
        }
    }

    fn rect(self, x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect::new(x * self.sx, y * self.sy, w * self.sx, h * self.sy)
    }

    fn style(self, size: f32, weight: Weight, color: Color, max_w: f32) -> TextStyle {
        TextStyle::new(size * self.s, weight, color, max_w * self.sx)
    }
}

/// A 72dp dialog button with a centred label; `r` is in output pixels.
fn button(layer: &mut Layer, sc: Scale, r: Rect, label: &str, fill: Color, ink: Color) {
    layer.rect(r, DARK.radius_m * sc.s, fill);
    let style = TextStyle::new(DARK.body_px * sc.s, Weight::SemiBold, ink, r.w)
        .with_align(TextAlign::Center);
    let origin_y = r.y + (r.h - style.size * 1.25) / 2.0;
    layer.text(Point::new(r.x, origin_y), label, style);
}

#[cfg(test)]
mod tests {
    use super::*;
    use de_api::DrawOp;

    const REF: Size = Size::new(720, 1600);

    fn texts(scene: &Scene) -> Vec<(&str, TextStyle)> {
        scene
            .layers
            .iter()
            .flat_map(|layer| layer.ops.iter())
            .filter_map(|op| match op {
                DrawOp::Text { text, style, .. } => Some((text.as_str(), *style)),
                _ => None,
            })
            .collect()
    }

    fn has(scene: &Scene, needle: &str) -> bool {
        texts(scene).iter().any(|(text, _)| *text == needle)
    }

    fn rects(scene: &Scene) -> Vec<(Rect, f32, Color)> {
        scene
            .layers
            .iter()
            .flat_map(|layer| layer.ops.iter())
            .filter_map(|op| match op {
                DrawOp::Rect {
                    rect,
                    radius,
                    color,
                } => Some((*rect, *radius, *color)),
                DrawOp::Text { .. } => None,
            })
            .collect()
    }

    fn uses(scene: &Scene, color: Color) -> bool {
        rects(scene).iter().any(|(_, _, used)| *used == color)
            || texts(scene).iter().any(|(_, style)| style.color == color)
    }

    #[test]
    fn scene_is_the_requested_size_with_one_named_layer() {
        let scene = build(REF);
        assert_eq!(scene.size, REF);
        assert_eq!(scene.layers.len(), 1);
        assert_eq!(scene.layers[0].name, "error");
        match &scene.layers[0].ops[0] {
            DrawOp::Rect {
                rect,
                radius,
                color,
            } => {
                assert_eq!(*rect, Rect::new(0.0, 0.0, 720.0, 1600.0));
                assert_eq!(*radius, 0.0);
                assert_eq!(*color, DARK.bg);
            }
            other => panic!("first op must be the background, got {other:?}"),
        }
    }

    #[test]
    fn scrim_and_centred_sheet_cover_the_frame() {
        let scene = build(REF);
        match &scene.layers[0].ops[1] {
            DrawOp::Rect { rect, color, .. } => {
                assert_eq!(*rect, Rect::new(0.0, 0.0, 720.0, 1600.0));
                assert_eq!(*color, DARK.scrim);
            }
            other => panic!("second op must be the scrim, got {other:?}"),
        }
        let sheet = Rect::new(50.0, 620.0, 620.0, 360.0);
        assert!(
            rects(&scene)
                .iter()
                .any(|(rect, _, color)| *rect == sheet && *color == DARK.surface),
            "no 620×360 sheet centred on the panel"
        );
    }

    #[test]
    fn title_body_and_both_actions_are_present() {
        let scene = build(REF);
        for text in [
            "Calculator isn't responding",
            "You can wait for it to respond or close it.",
            "Wait",
            "Close app",
        ] {
            assert!(has(&scene, text), "missing {text:?}");
        }
    }

    #[test]
    fn uses_the_accent_and_dim_ink_tokens() {
        let scene = build(REF);
        assert!(uses(&scene, DARK.accent));
        assert!(uses(&scene, DARK.text_dim));
    }

    #[test]
    fn scales_to_the_phone_panel_without_leaving_it() {
        let size = Size::new(1080, 2400);
        let scene = build(size);
        assert_eq!(scene.size, size);
        match &scene.layers[0].ops[0] {
            DrawOp::Rect { rect, .. } => assert_eq!(*rect, Rect::new(0.0, 0.0, 1080.0, 2400.0)),
            other => panic!("first op must be the background, got {other:?}"),
        }
        for (rect, _, _) in rects(&scene) {
            assert!(rect.right() <= 1080.0 + 0.01 && rect.bottom() <= 2400.0 + 0.01);
        }
    }
}
