//! P0 boot frame: the first pixels the panel shows.
//!
//! Frozen scaffold API: the scene is built from design tokens and returns
//! drawing operations only. No font is loaded here; `de-paint` owns text.
//!
//! Geometry is authored on the 720×1600 reference panel and mapped onto the
//! requested [`Size`] with `sx`/`sy`, so the frame fills any output instead of
//! assuming the phone panel.

use de_api::{Point, Rect, Scene, Size, TextStyle, Weight};
use de_design::DARK;

/// Reference panel the P0 composition was authored on.
const REF_W: f32 = 720.0;
const REF_H: f32 = 1600.0;

/// The boot frame is a single full-screen layer.
const LAYER: &str = "boot";

pub fn build(size: Size) -> Scene {
    let w = size.w as f32;
    let h = size.h as f32;
    let sx = w / REF_W;
    let sy = h / REF_H;
    // Corners, the mark and type keep their aspect on odd panels.
    let s = sx.min(sy);

    let mut scene = Scene::new(size);
    let mut layer = de_api::Layer::new(LAYER);

    // Full bleed: the first op always covers the whole output.
    layer.rect(Rect::new(0.0, 0.0, w, h), 0.0, DARK.bg);

    // Low horizon: a canvas band with one surface card resting on it.
    let horizon_y = 864.0 * sy;
    let horizon = Rect::new(0.0, horizon_y, w, h - horizon_y);
    layer.rect(horizon, DARK.radius_card * s, DARK.canvas);

    let card = Rect::new(48.0 * sx, 920.0 * sy, 624.0 * sx, 360.0 * sy);
    layer.rect(card, DARK.radius_card * s, DARK.surface);

    // Rabbiz mark, centred on y = 640 of the reference panel.
    draw_mark(&mut layer, 360.0 * sx, 640.0 * sy, s);

    // Brand lockup: one centred 560 dp text column at rest.
    let wordmark = TextStyle::new(64.0 * s, Weight::SemiBold, DARK.text, 560.0 * s);
    let wordmark_x = (w - wordmark.max_width) / 2.0;
    layer.text(Point::new(wordmark_x, 960.0 * sy), "RabbizOS", wordmark);

    let subtitle = TextStyle::new(24.0 * s, Weight::Regular, DARK.text_dim, 560.0 * s);
    let subtitle_x = (w - subtitle.max_width) / 2.0;
    layer.text(Point::new(subtitle_x, 1040.0 * sy), "native · quiet · yours", subtitle);

    // Copper progress hairline: 160×4 at rest, no spinner.
    let hairline = Rect::new((w - 160.0 * s) / 2.0, 1120.0 * sy, 160.0 * s, 4.0 * s);
    layer.rect(hairline, 2.0 * s, DARK.accent);

    scene.push(layer);
    scene
}

/// Rabbiz as rounded rects: two upright ears behind a round head, a cream
/// muzzle and two dark eyes. Nothing here knows about fonts or pixels.
fn draw_mark(layer: &mut de_api::Layer, cx: f32, cy: f32, s: f32) {
    let ear_w = 52.0 * s;
    let ear_h = 160.0 * s;
    let ear_r = 26.0 * s;
    let ear_x = 29.0 * s; // ear centres sit this far either side of `cx`
    let ear_y = cy - 160.0 * s;

    // Ears first; the head tucks over their roots.
    let left_ear = Rect::new(cx - ear_x - ear_w / 2.0, ear_y, ear_w, ear_h);
    let right_ear = Rect::new(cx + ear_x - ear_w / 2.0, ear_y, ear_w, ear_h);
    layer.rect(left_ear, ear_r, DARK.mark_body);
    layer.rect(right_ear, ear_r, DARK.mark_body);
    // One ear carries the mascot's copper tip.
    let tip = Rect::new(right_ear.x, ear_y, ear_w, 56.0 * s);
    layer.rect(tip, ear_r, DARK.mark_copper);

    // Head centred on (cx, cy): 150 wide, radius ~70.
    let head = Rect::new(cx - 75.0 * s, cy - 75.0 * s, 150.0 * s, 150.0 * s);
    layer.rect(head, 70.0 * s, DARK.mark_body);

    // Cream face panel, then the eyes on top of it.
    let face = Rect::new(cx - 52.0 * s, cy - 22.0 * s, 104.0 * s, 76.0 * s);
    layer.rect(face, 38.0 * s, DARK.mark_cream);
    let eye = 14.0 * s;
    layer.rect(Rect::new(cx - 32.0 * s, cy - 10.0 * s, eye, 18.0 * s), 7.0 * s, DARK.bg);
    layer.rect(Rect::new(cx + 18.0 * s, cy - 10.0 * s, eye, 18.0 * s), 7.0 * s, DARK.bg);
}

#[cfg(test)]
mod tests {
    use super::*;
    use de_api::DrawOp;

    const REF: Size = Size::new(720, 1600);

    fn find_text<'a>(scene: &'a Scene, needle: &str) -> (Point, &'a TextStyle) {
        scene
            .layers
            .iter()
            .flat_map(|layer| layer.ops.iter())
            .find_map(|op| match op {
                DrawOp::Text { origin, text, style } if text.as_str() == needle => Some((*origin, style)),
                _ => None,
            })
            .unwrap_or_else(|| panic!("no text op for {needle:?}"))
    }

    fn colors(scene: &Scene) -> Vec<de_api::Color> {
        scene.layers[0]
            .ops
            .iter()
            .filter_map(|op| match op {
                DrawOp::Rect { color, .. } => Some(*color),
                DrawOp::Text { .. } => None,
            })
            .collect()
    }

    #[test]
    fn scene_is_the_requested_size_with_one_boot_layer() {
        let size = Size::new(1080, 2400);
        let scene = build(size);
        assert_eq!(scene.size, size);
        assert_eq!(scene.layers.len(), 1);
        assert_eq!(scene.layers[0].name, "boot");
    }

    #[test]
    fn first_op_is_the_full_bleed_background() {
        let scene = build(REF);
        match &scene.layers[0].ops[0] {
            DrawOp::Rect { rect, radius, color } => {
                assert_eq!(*rect, Rect::new(0.0, 0.0, 720.0, 1600.0));
                assert_eq!(*radius, 0.0);
                assert_eq!(*color, DARK.bg);
            }
            other => panic!("first op must be the background, got {other:?}"),
        }
    }

    #[test]
    fn wordmark_is_present_with_the_brand_style() {
        let scene = build(REF);
        let (origin, style) = find_text(&scene, "RabbizOS");
        assert_eq!(origin, Point::new(80.0, 960.0));
        assert_eq!(style.weight, Weight::SemiBold);
        assert_eq!(style.color, DARK.text);
        assert!((style.size - 64.0).abs() < 0.01);
        assert!((style.max_width - 560.0).abs() < 0.01);
    }

    #[test]
    fn mark_uses_cream_face_and_copper_tip() {
        let used = colors(&build(REF));
        assert!(used.contains(&DARK.mark_body), "no mark body");
        assert!(used.contains(&DARK.mark_cream), "no cream face panel");
        assert!(used.contains(&DARK.mark_copper), "no copper ear tip");
    }

    #[test]
    fn composition_scales_with_the_requested_size() {
        let scene = build(Size::new(1440, 3200));
        assert_eq!(scene.size, Size::new(1440, 3200));

        match &scene.layers[0].ops[0] {
            DrawOp::Rect { rect, .. } => assert_eq!(*rect, Rect::new(0.0, 0.0, 1440.0, 3200.0)),
            other => panic!("first op must be the background, got {other:?}"),
        }

        let (origin, style) = find_text(&scene, "RabbizOS");
        assert_eq!(origin.y, 1920.0);
        assert!((style.size - 128.0).abs() < 0.01);

        match &scene.layers[0].ops.last().unwrap() {
            DrawOp::Rect { rect, radius, color } => {
                assert_eq!(*rect, Rect::new(560.0, 2240.0, 320.0, 8.0));
                assert_eq!(*radius, 4.0);
                assert_eq!(*color, DARK.accent);
            }
            other => panic!("hairline must be the last op, got {other:?}"),
        }
    }
}
