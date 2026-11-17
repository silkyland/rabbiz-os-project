//! P19 first-run: language, Wi-Fi, fingerprint.
//!
//! The welcome frame: Rabbiz, the setup promise, a language choice, the two
//! rows worth asking about up front, and one Continue button. Authored on the
//! 720×1600 reference panel and mapped with `sx`/`sy` for geometry and
//! `s = min(sx, sy)` for radii and type.

use de_api::{Color, Layer, Point, Rect, Scene, Size, TextAlign, TextStyle, Weight};
use de_design::DARK;

const REF_W: f32 = 720.0;
const REF_H: f32 = 1600.0;
const LAYER: &str = "first-run";

/// P19 first-run: language, Wi-Fi, fingerprint.
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

    mark(&mut layer, sc, 360.0, 300.0);

    let title = sc
        .style(DARK.title_px, Weight::SemiBold, DARK.text, 600.0)
        .with_align(TextAlign::Center);
    layer.text(
        Point::new(60.0 * sc.sx, 430.0 * sc.sy),
        "Let's set up RabbizOS",
        title,
    );
    let subtitle = sc
        .style(DARK.caption_px, Weight::Regular, DARK.text_dim, 640.0)
        .with_align(TextAlign::Center);
    layer.text(
        Point::new(40.0 * sc.sx, 494.0 * sc.sy),
        "Language, network, unlock — about a minute.",
        subtitle,
    );

    section(&mut layer, sc, 580.0, "Language");
    // Thai is the selected chip; English waits beside it.
    pill(
        &mut layer,
        sc,
        sc.rect(24.0, 616.0, 200.0, 64.0),
        "ไทย",
        DARK.accent,
        DARK.accent_ink,
    );
    pill(
        &mut layer,
        sc,
        sc.rect(240.0, 616.0, 220.0, 64.0),
        "English",
        DARK.surface_high,
        DARK.text,
    );

    // Wi-Fi is already connected, so the row reports success.
    layer.rect(
        sc.rect(24.0, 744.0, 672.0, 96.0),
        DARK.radius_m * sc.s,
        DARK.surface,
    );
    layer.rect(
        sc.rect(48.0, 770.0, 44.0, 44.0),
        DARK.radius_s * sc.s,
        DARK.surface_high,
    );
    signal(&mut layer, sc, 70.0, 792.0, DARK.accent);
    two_line(
        &mut layer,
        sc,
        112.0,
        744.0,
        "Wi-Fi",
        "Rabbiz-Home · connected",
        DARK.success,
    );
    toggle(&mut layer, sc, 596.0, 770.0, true);

    // Fingerprint stays optional at setup; the row carries a chevron.
    layer.rect(
        sc.rect(24.0, 856.0, 672.0, 96.0),
        DARK.radius_m * sc.s,
        DARK.surface,
    );
    layer.rect(
        sc.rect(48.0, 882.0, 44.0, 44.0),
        DARK.radius_s * sc.s,
        DARK.surface_high,
    );
    fingerprint(&mut layer, sc, 70.0, 904.0, DARK.accent, DARK.surface_high);
    two_line(
        &mut layer,
        sc,
        112.0,
        856.0,
        "Fingerprint",
        "Add a fingerprint to unlock",
        DARK.text_dim,
    );
    chevron(&mut layer, sc, 666.0, 902.0);

    let note = sc.style(DARK.caption_px, Weight::Regular, DARK.text_faint, 600.0);
    layer.text(
        Point::new(24.0 * sc.sx, 988.0 * sc.sy),
        "Your fingerprint never leaves the device.",
        note,
    );

    button(
        &mut layer,
        sc,
        sc.rect(24.0, 1400.0, 672.0, 96.0),
        "Continue",
        DARK.accent,
        DARK.accent_ink,
    );
    let foot = sc
        .style(DARK.caption_px, Weight::Regular, DARK.text_faint, 640.0)
        .with_align(TextAlign::Center);
    layer.text(
        Point::new(40.0 * sc.sx, 1522.0 * sc.sy),
        "You can change these later in Settings.",
        foot,
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

fn section(layer: &mut Layer, sc: Scale, y: f32, text: &str) {
    let style = sc.style(DARK.label_px, Weight::SemiBold, DARK.text_dim, 480.0);
    layer.text(Point::new(24.0 * sc.sx, y * sc.sy), text, style);
}

/// Two-line card row: a title over a smaller, colour-coded status line.
fn two_line(
    layer: &mut Layer,
    sc: Scale,
    x: f32,
    y: f32,
    title: &str,
    sub: &str,
    sub_color: Color,
) {
    let title_style = sc.style(DARK.body_px, Weight::SemiBold, DARK.text, 420.0);
    layer.text(
        Point::new(x * sc.sx, (y + 22.0) * sc.sy),
        title,
        title_style,
    );
    let sub_style = sc.style(DARK.caption_px, Weight::Regular, sub_color, 420.0);
    layer.text(Point::new(x * sc.sx, (y + 56.0) * sc.sy), sub, sub_style);
}

/// A pill with a centred label; `r` is already in output pixels.
fn pill(layer: &mut Layer, sc: Scale, r: Rect, label: &str, fill: Color, ink: Color) {
    layer.rect(r, DARK.radius_pill * sc.s, fill);
    let style = TextStyle::new(DARK.label_px * sc.s, Weight::SemiBold, ink, r.w)
        .with_align(TextAlign::Center);
    let origin_y = r.y + (r.h - style.size * 1.25) / 2.0;
    layer.text(Point::new(r.x, origin_y), label, style);
}

/// The one primary action: 96dp tall, copper, centred label.
fn button(layer: &mut Layer, sc: Scale, r: Rect, label: &str, fill: Color, ink: Color) {
    layer.rect(r, DARK.radius_pill * sc.s, fill);
    let style = TextStyle::new(DARK.body_px * sc.s, Weight::SemiBold, ink, r.w)
        .with_align(TextAlign::Center);
    let origin_y = r.y + (r.h - style.size * 1.25) / 2.0;
    layer.text(Point::new(r.x, origin_y), label, style);
}

fn toggle(layer: &mut Layer, sc: Scale, x: f32, y: f32, on: bool) {
    let track = if on { DARK.accent } else { DARK.surface_high };
    layer.rect(sc.rect(x, y, 76.0, 44.0), DARK.radius_pill * sc.s, track);
    let (knob, knob_x) = if on {
        (DARK.bg, x + 38.0)
    } else {
        (DARK.text_faint, x + 6.0)
    };
    layer.rect(sc.rect(knob_x, y + 6.0, 32.0, 32.0), 16.0 * sc.s, knob);
}

/// Forward chevron drawn as a four-step staircase of rounded bars.
fn chevron(layer: &mut Layer, sc: Scale, x: f32, cy: f32) {
    for i in 0..4 {
        let t = i as f32 * 4.0;
        layer.rect(
            sc.rect(x + t, cy - 12.0 + t, 8.0, 4.0),
            2.0 * sc.s,
            DARK.text_faint,
        );
        if i < 3 {
            layer.rect(
                sc.rect(x + 8.0 - t, cy + 4.0 + t, 8.0, 4.0),
                2.0 * sc.s,
                DARK.text_faint,
            );
        }
    }
}

/// Four signal bars, bottom-aligned, the same glyph the network rows use.
fn signal(layer: &mut Layer, sc: Scale, cx: f32, cy: f32, ink: Color) {
    for i in 0..4u8 {
        let h = 6.0 + 4.0 * i as f32;
        layer.rect(
            sc.rect(cx - 12.5 + i as f32 * 7.0, cy + 9.0 - h, 4.0, h),
            2.0 * sc.s,
            ink,
        );
    }
}

/// Fingerprint as a sensor ring with a centre dot.
fn fingerprint(layer: &mut Layer, sc: Scale, cx: f32, cy: f32, ink: Color, surface: Color) {
    layer.rect(sc.rect(cx - 10.0, cy - 10.0, 20.0, 20.0), 10.0 * sc.s, ink);
    layer.rect(sc.rect(cx - 7.0, cy - 7.0, 14.0, 14.0), 7.0 * sc.s, surface);
    layer.rect(sc.rect(cx - 2.0, cy - 2.0, 4.0, 4.0), 2.0 * sc.s, ink);
}

/// Rabbiz as rounded rects: two ears, a head, a cream muzzle and two eyes.
fn mark(layer: &mut Layer, sc: Scale, cx: f32, cy: f32) {
    layer.rect(
        sc.rect(cx - 44.0, cy - 84.0, 24.0, 76.0),
        12.0 * sc.s,
        DARK.mark_body,
    );
    layer.rect(
        sc.rect(cx + 20.0, cy - 84.0, 24.0, 76.0),
        12.0 * sc.s,
        DARK.mark_body,
    );
    layer.rect(
        sc.rect(cx + 20.0, cy - 84.0, 24.0, 26.0),
        12.0 * sc.s,
        DARK.mark_copper,
    );
    layer.rect(
        sc.rect(cx - 38.0, cy - 24.0, 76.0, 72.0),
        32.0 * sc.s,
        DARK.mark_body,
    );
    layer.rect(
        sc.rect(cx - 24.0, cy + 6.0, 48.0, 30.0),
        15.0 * sc.s,
        DARK.mark_cream,
    );
    layer.rect(sc.rect(cx - 16.0, cy - 3.0, 7.0, 9.0), 3.5 * sc.s, DARK.bg);
    layer.rect(sc.rect(cx + 9.0, cy - 3.0, 7.0, 9.0), 3.5 * sc.s, DARK.bg);
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
        assert_eq!(scene.layers[0].name, "first-run");
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
    fn welcome_title_chips_rows_and_continue_are_present() {
        let scene = build(REF);
        for text in [
            "Let's set up RabbizOS",
            "ไทย",
            "English",
            "Wi-Fi",
            "Fingerprint",
            "Continue",
        ] {
            assert!(has(&scene, text), "missing {text:?}");
        }
    }

    #[test]
    fn uses_the_accent_success_and_dim_tokens() {
        let scene = build(REF);
        assert!(
            uses(&scene, DARK.accent),
            "the selected chip must be copper"
        );
        assert!(
            uses(&scene, DARK.success),
            "the Wi-Fi row must report success"
        );
        assert!(uses(&scene, DARK.text_dim));
    }

    #[test]
    fn selected_chip_and_primary_button_are_copper() {
        let scene = build(REF);
        let copper = rects(&scene)
            .into_iter()
            .filter(|(_, _, color)| *color == DARK.accent)
            .count();
        assert!(
            copper >= 3,
            "chip, toggle and Continue all carry the accent"
        );
        assert!(rects(&scene).iter().any(|(rect, _, color)| *rect
            == Rect::new(24.0, 1400.0, 672.0, 96.0)
            && *color == DARK.accent));
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
