//! P16 screenshot flash and thumbnail overlay.
//!
//! One translucent capture flash with the confirmation strip in the
//! bottom-left corner: a 160×96 thumbnail, the "Open"/"Share" actions and the
//! save location. Authored on the 720×1600 reference panel and mapped with
//! `sx`/`sy` for geometry and `s = min(sx, sy)` for radii and type.

use de_api::{Color, Layer, Point, Rect, Scene, Size, TextAlign, TextStyle, Weight};
use de_design::DARK;

const REF_W: f32 = 720.0;
const REF_H: f32 = 1600.0;
const LAYER: &str = "screenshot";

/// P16 screenshot flash and thumbnail overlay.
pub fn build(size: Size) -> Scene {
    let sc = Scale::new(size);
    let mut scene = Scene::new(size);
    let mut layer = Layer::new(LAYER);

    // Full bleed base, then the flash: one translucent cream veil.
    let frame = Rect::new(0.0, 0.0, size.w as f32, size.h as f32);
    layer.rect(frame, 0.0, DARK.bg);
    layer.rect(frame, 0.0, DARK.mark_cream.with_alpha(28));

    // 160×96 thumbnail: a mini frame with a copper title bar.
    layer.rect(
        sc.rect(24.0, 1400.0, 160.0, 96.0),
        DARK.radius_m * sc.s,
        DARK.surface_high,
    );
    layer.rect(
        sc.rect(32.0, 1408.0, 144.0, 80.0),
        DARK.radius_s * sc.s,
        DARK.surface_low,
    );
    layer.rect(
        sc.rect(32.0, 1408.0, 144.0, 10.0),
        DARK.radius_s * sc.s,
        DARK.accent,
    );
    layer.rect(
        sc.rect(40.0, 1432.0, 100.0, 6.0),
        3.0 * sc.s,
        DARK.text_faint,
    );
    layer.rect(
        sc.rect(40.0, 1446.0, 72.0, 6.0),
        3.0 * sc.s,
        DARK.text_faint,
    );

    // Open is the primary action; Share sits beside it.
    pill(
        &mut layer,
        sc,
        sc.rect(204.0, 1418.0, 112.0, 48.0),
        "Open",
        DARK.accent,
        DARK.accent_ink,
    );
    pill(
        &mut layer,
        sc,
        sc.rect(328.0, 1418.0, 112.0, 48.0),
        "Share",
        DARK.surface_high,
        DARK.text,
    );

    let caption = sc.style(DARK.caption_px, Weight::Regular, DARK.text_dim, 600.0);
    layer.text(
        Point::new(24.0 * sc.sx, 1512.0 * sc.sy),
        "Saved to Pictures/Screenshots",
        caption,
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

/// A 48dp pill with a centred label; `r` is already in output pixels.
fn pill(layer: &mut Layer, sc: Scale, r: Rect, label: &str, fill: Color, ink: Color) {
    layer.rect(r, DARK.radius_pill * sc.s, fill);
    let style = TextStyle::new(DARK.label_px * sc.s, Weight::SemiBold, ink, r.w)
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
        assert_eq!(scene.layers[0].name, "screenshot");
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
    fn the_flash_covers_the_output_and_is_translucent() {
        let scene = build(REF);
        match &scene.layers[0].ops[1] {
            DrawOp::Rect { rect, color, .. } => {
                assert_eq!(*rect, Rect::new(0.0, 0.0, 720.0, 1600.0));
                assert_eq!(*color, DARK.mark_cream.with_alpha(28));
                assert!(color.a < u8::MAX, "the flash must be translucent");
            }
            other => panic!("second op must be the flash, got {other:?}"),
        }
    }

    #[test]
    fn thumbnail_and_actions_are_present() {
        let scene = build(REF);
        assert!(
            rects(&scene)
                .iter()
                .any(|(rect, _, _)| *rect == Rect::new(24.0, 1400.0, 160.0, 96.0)),
            "no 160×96 thumbnail at the bottom-left"
        );
        for text in ["Open", "Share", "Saved to Pictures/Screenshots"] {
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
        let caption = texts(&scene)
            .into_iter()
            .find(|(text, _)| *text == "Saved to Pictures/Screenshots")
            .map(|(_, style)| style)
            .expect("caption must survive the scale");
        assert!((caption.size - DARK.caption_px * 1.5).abs() < 0.01);
    }
}
