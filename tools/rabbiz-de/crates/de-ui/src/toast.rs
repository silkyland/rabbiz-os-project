//! P8 notification toast.
//!
//! A 672×112 glass card 72dp from the top: the app, the sender, the message and
//! when it arrived. Authored on the 720×1600 reference panel and mapped with
//! `sx`/`sy` for geometry and `s = min(sx, sy)` for radii and type.

use de_api::{Color, Layer, Point, Rect, Scene, Size, TextStyle, Weight};
use de_design::DARK;

const CHAT_BUBBLE: char = '\u{e0cb}';

const REF_W: f32 = 720.0;
const REF_H: f32 = 1600.0;
const LAYER: &str = "toast";

/// P8 notification toast.
pub fn build(size: Size) -> Scene {
    let sc = Scale::new(size);
    let mut scene = Scene::new(size);
    let mut layer = Layer::new(LAYER);

    // Full bleed: the first op always covers the whole output.
    layer.rect(
        Rect::new(0.0, 0.0, size.w as f32, size.h as f32),
        0.0,
        DARK.bg,
    );

    // Hairline edge under a glass card, so the toast sits above the surface.
    layer.rect(
        sc.rect(23.0, 71.0, 674.0, 114.0),
        DARK.radius_l * sc.s + 1.0,
        DARK.line,
    );
    layer.rect(
        sc.rect(24.0, 72.0, 672.0, 112.0),
        DARK.radius_l * sc.s,
        DARK.glass,
    );

    // 48dp app tile with a speech bubble icon.
    layer.rect(
        sc.rect(40.0, 104.0, 48.0, 48.0),
        DARK.radius_m * sc.s,
        DARK.accent,
    );
    layer.icon(
        Point::new(64.0 * sc.sx, 128.0 * sc.sy),
        26.0 * sc.s,
        CHAT_BUBBLE,
        DARK.accent_ink,
        true,
    );

    let app = sc.style(DARK.label_px, Weight::SemiBold, DARK.text, 96.0);
    layer.text(Point::new(104.0 * sc.sx, 86.0 * sc.sy), "Messages", app);
    let sender = sc.style(DARK.micro_px, Weight::Regular, DARK.text_dim, 60.0);
    layer.text(Point::new(206.0 * sc.sx, 88.0 * sc.sy), "Nok", sender);

    let body = sc.style(DARK.label_px, Weight::Regular, DARK.text, 470.0);
    layer.text(
        Point::new(104.0 * sc.sx, 120.0 * sc.sy),
        "Are we still on for dinner?",
        body,
    );

    let when = sc.style(DARK.micro_px, Weight::Regular, DARK.text_faint, 48.0);
    layer.text(Point::new(644.0 * sc.sx, 88.0 * sc.sy), "now", when);

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
                DrawOp::Text { .. } | DrawOp::Icon { .. } => None,
            })
            .collect()
    }

    fn uses(scene: &Scene, color: Color) -> bool {
        rects(scene).iter().any(|(_, _, used)| *used == color)
            || texts(scene).iter().any(|(_, style)| style.color == color)
            || scene.layers.iter().flat_map(|l| l.ops.iter()).any(|op| {
                matches!(op, DrawOp::Icon { color: c, .. } if *c == color)
            })
    }

    #[test]
    fn scene_is_the_requested_size_with_one_named_layer() {
        let scene = build(REF);
        assert_eq!(scene.size, REF);
        assert_eq!(scene.layers.len(), 1);
        assert_eq!(scene.layers[0].name, "toast");
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
    fn the_card_is_672_by_112_at_y_72() {
        let scene = build(REF);
        assert!(
            rects(&scene).iter().any(|(rect, _, color)| *rect
                == Rect::new(24.0, 72.0, 672.0, 112.0)
                && *color == DARK.glass),
            "no 672×112 toast card at y=72"
        );
        assert!(
            rects(&scene)
                .iter()
                .any(|(rect, _, _)| *rect == Rect::new(40.0, 104.0, 48.0, 48.0)),
            "no 48dp icon tile"
        );
    }

    #[test]
    fn app_sender_message_and_time_are_present() {
        let scene = build(REF);
        for text in ["Messages", "Nok", "Are we still on for dinner?", "now"] {
            assert!(has(&scene, text), "missing {text:?}");
        }
    }

    #[test]
    fn sender_and_time_use_dim_ink() {
        let scene = build(REF);
        assert!(uses(&scene, DARK.text_dim));
        assert!(uses(&scene, DARK.text_faint));
        assert!(uses(&scene, DARK.accent));
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
        assert!(
            rects(&scene)
                .iter()
                .any(|(rect, _, _)| *rect == Rect::new(36.0, 108.0, 1008.0, 168.0)),
            "the card must scale with the panel"
        );
        for (rect, _, _) in rects(&scene) {
            assert!(rect.right() <= 1080.0 + 0.01 && rect.bottom() <= 2400.0 + 0.01);
        }
    }
}
