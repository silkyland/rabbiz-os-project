//! P10 volume / P11 brightness OSD: a detached left-edge pill over the scrim.
//!
//! Frozen scaffold API: an OSD is a pure function of the output [`Size`] and
//! returns drawing operations only. The composition is authored on the
//! 720×1600 reference panel and mapped with `sx`/`sy`, so the pill sits at the
//! same left-edge point on the 1080×2400 panel.
//!
//! There is no icon font: the speaker and the sun are built from rounded
//! rects, and the meter is twenty real segments, not a texture.

use de_api::{Layer, Point, Rect, Scene, Size, TextAlign, TextStyle, Weight};
use de_design::DARK;

/// Reference panel the OSD composition was authored on.
const REF_W: f32 = 720.0;
const REF_H: f32 = 1600.0;

/// Pill geometry is fixed by DE-PLAN P10/P11: 96×420 at x=24/y=590.
const PILL_X: f32 = 24.0;
const PILL_Y: f32 = 590.0;
const PILL_W: f32 = 96.0;
const PILL_H: f32 = 420.0;

/// Segments in the meter; fourteen of twenty is 70 %.
const SEGMENTS: u32 = 20;

/// Which shape the pill wears at its head.
#[derive(Clone, Copy)]
enum Glyph {
    Speaker,
    Sun,
}

/// P10 volume OSD: speaker glyph, 70 % of twenty segments copper.
pub fn volume(size: Size) -> Scene {
    osd(size, "volume", Glyph::Speaker, 70, "Speakers")
}

/// P11 brightness OSD: the same pill with a sun glyph and 45 %.
pub fn brightness(size: Size) -> Scene {
    osd(size, "brightness", Glyph::Sun, 45, "Brightness")
}

/// One pill layout, two readings: only the glyph, the number and the caption
/// change, so the OSD never drifts between the two gestures.
fn osd(size: Size, layer_name: &str, glyph: Glyph, percent: u32, caption: &str) -> Scene {
    let w = size.w as f32;
    let h = size.h as f32;
    let sx = w / REF_W;
    let sy = h / REF_H;
    // Glyphs, radii and type keep their aspect on odd panels.
    let s = sx.min(sy);

    let mut scene = Scene::new(size);
    let mut layer = Layer::new(layer_name);

    // An OSD is composited over whatever is running, so the canvas is the dim
    // scrim. The first op still covers the whole output with the base ink.
    layer.rect(Rect::new(0.0, 0.0, w, h), 0.0, DARK.bg);
    layer.rect(Rect::new(0.0, 0.0, w, h), 0.0, DARK.scrim);

    let pill = Rect::new(PILL_X * sx, PILL_Y * sy, PILL_W * sx, PILL_H * sy);
    layer.rect(pill, DARK.radius_pill * s, DARK.glass);

    let cx = pill.x + pill.w / 2.0;
    match glyph {
        Glyph::Speaker => speaker(&mut layer, cx, 620.0 * sy, s),
        Glyph::Sun => sun(&mut layer, cx, 620.0 * sy, s),
    }

    // The number is the loudest thing in the pill. Its box is the usable pill
    // width, so "70%" never breaks into two lines over the meter.
    let inner_w = (PILL_W - 2.0 * DARK.space_xs) * s;
    let label = format!("{percent}%");
    let value = TextStyle::new(
        fitted_size(&label, DARK.title_px, inner_w, s),
        Weight::SemiBold,
        DARK.text,
        inner_w,
    )
    .with_align(TextAlign::Center);
    layer.text(
        Point::new(pill.x + (pill.w - inner_w) / 2.0, 676.0 * sy),
        label,
        value,
    );

    // Twenty segments, filled from the top like a ladder.
    let filled = SEGMENTS * percent / 100;
    let seg_w = 40.0 * sx;
    let seg_h = 8.0 * s;
    let gap = 3.0 * s;
    let seg_x = cx - seg_w / 2.0;
    let seg_top = 740.0 * sy;
    for i in 0..SEGMENTS {
        let y = seg_top + i as f32 * (seg_h + gap);
        let color = if i < filled { DARK.accent } else { DARK.line };
        layer.rect(Rect::new(seg_x, y, seg_w, seg_h), seg_h / 2.0, color);
    }

    // Device name under the ladder. The pill is 96 wide and the API cannot
    // rotate text, so the caption shrinks rather than wrapping out of the pill.
    layer.text(
        Point::new(pill.x + (pill.w - inner_w) / 2.0, 964.0 * sy),
        caption,
        fitted_caption(caption, inner_w, s),
    );

    scene.push(layer);
    scene
}

/// Speaker cabinet with a driver and two sound bars. Two dark eyes in a cream
/// face would be the mascot; this is the plain audio glyph.
fn speaker(layer: &mut Layer, cx: f32, top: f32, s: f32) {
    layer.rect(
        Rect::new(cx - 26.0 * s, top, 30.0 * s, 40.0 * s),
        8.0 * s,
        DARK.text_dim,
    );
    // The driver reads as a hole, so it borrows the canvas ink.
    layer.rect(
        Rect::new(cx - 18.0 * s, top + 12.0 * s, 14.0 * s, 16.0 * s),
        8.0 * s,
        DARK.bg,
    );
    layer.rect(
        Rect::new(cx + 8.0 * s, top + 10.0 * s, 6.0 * s, 20.0 * s),
        3.0 * s,
        DARK.text_dim,
    );
    layer.rect(
        Rect::new(cx + 19.0 * s, top + 2.0 * s, 6.0 * s, 36.0 * s),
        3.0 * s,
        DARK.text_dim,
    );
}

/// Sun: a disc and four cardinal rays. Diagonal rays would need rotation, which
/// [`de_api::DrawOp`] does not express, so the four are enough to read as light.
fn sun(layer: &mut Layer, cx: f32, top: f32, s: f32) {
    layer.rect(
        Rect::new(cx - 14.0 * s, top + 6.0 * s, 28.0 * s, 28.0 * s),
        14.0 * s,
        DARK.accent_hi,
    );
    let rays = [
        (-3.0, 0.0, 6.0, 13.0),
        (-3.0, 27.0, 6.0, 13.0),
        (-27.0, 14.0, 14.0, 6.0),
        (14.0, 14.0, 14.0, 6.0),
    ];
    for (dx, dy, rw, rh) in rays {
        layer.rect(
            Rect::new(cx + dx * s, top + dy * s, rw * s, rh * s),
            3.0 * s,
            DARK.accent_hi,
        );
    }
}

/// Fit type inside the pill. The scene API has no text metrics, so 0.66 em is
/// a deliberately wide per-glyph estimate: the result never wraps to a second
/// line that would paint over the rounded foot of the pill.
fn fitted_size(text: &str, preferred: f32, max_width: f32, s: f32) -> f32 {
    let chars = text.chars().count().max(1) as f32;
    (max_width / (chars * 0.66)).min(preferred * s)
}

/// The device name under the ladder, shrunk to fit rather than wrapped.
fn fitted_caption(text: &str, max_width: f32, s: f32) -> TextStyle {
    let size = fitted_size(text, DARK.caption_px, max_width, s);
    TextStyle::new(size, Weight::Regular, DARK.text_dim, max_width).with_align(TextAlign::Center)
}

#[cfg(test)]
mod tests {
    use super::*;
    use de_api::{Color, DrawOp};

    const REF: Size = Size::new(720, 1600);
    const PANEL: Size = Size::new(1080, 2400);

    fn near(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.01
    }

    fn find_text<'a>(scene: &'a Scene, needle: &str) -> (Point, &'a TextStyle) {
        scene
            .layers
            .iter()
            .flat_map(|layer| layer.ops.iter())
            .find_map(|op| match op {
                DrawOp::Text {
                    origin,
                    text,
                    style,
                } if text.as_str() == needle => Some((*origin, style)),
                _ => None,
            })
            .unwrap_or_else(|| panic!("no text op for {needle:?}"))
    }

    fn rects(scene: &Scene) -> Vec<(Rect, f32, Color)> {
        scene.layers[0]
            .ops
            .iter()
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

    fn colors(scene: &Scene) -> Vec<Color> {
        let mut used: Vec<Color> = rects(scene).into_iter().map(|(_, _, c)| c).collect();
        used.extend(scene.layers[0].ops.iter().filter_map(|op| match op {
            DrawOp::Text { style, .. } => Some(style.color),
            DrawOp::Rect { .. } => None,
        }));
        used
    }

    fn meter(scene: &Scene) -> Vec<(Rect, Color)> {
        rects(scene)
            .into_iter()
            .filter(|(r, _, _)| near(r.w, 40.0) && near(r.h, 8.0))
            .map(|(r, _, c)| (r, c))
            .collect()
    }

    #[test]
    fn both_osd_scenes_are_the_requested_size_with_one_layer() {
        for (scene, name) in [(volume(PANEL), "volume"), (brightness(PANEL), "brightness")] {
            assert_eq!(scene.size, PANEL);
            assert_eq!(scene.layers.len(), 1);
            assert_eq!(scene.layers[0].name, name);
        }
    }

    #[test]
    fn first_op_is_the_full_bleed_background_then_the_scrim() {
        for scene in [volume(REF), brightness(REF)] {
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
            match &scene.layers[0].ops[1] {
                DrawOp::Rect { rect, color, .. } => {
                    assert_eq!(*rect, Rect::new(0.0, 0.0, 720.0, 1600.0));
                    assert_eq!(*color, DARK.scrim);
                }
                other => panic!("second op must be the scrim, got {other:?}"),
            }
        }
    }

    #[test]
    fn pill_keeps_the_plan_geometry_at_the_reference_panel() {
        let scene = volume(REF);
        let pill = rects(&scene)
            .into_iter()
            .find(|(r, _, c)| near(r.x, 24.0) && near(r.y, 590.0) && *c == DARK.glass);
        let (rect, _, _) = pill.expect("no glass pill at x=24/y=590");
        assert!(near(rect.w, 96.0) && near(rect.h, 420.0));
    }

    #[test]
    fn pill_moves_and_grows_with_the_requested_size() {
        let scene = volume(PANEL);
        let pill = rects(&scene)
            .into_iter()
            .find(|(r, _, c)| near(r.x, 36.0) && near(r.y, 885.0) && *c == DARK.glass)
            .expect("no glass pill at the 1080x2400 position");
        assert!(near(pill.0.w, 144.0) && near(pill.0.h, 630.0));

        let (origin, style) = find_text(&scene, "70%");
        assert!(near(origin.x, 45.0), "value origin x was {}", origin.x);
        assert!(near(origin.y, 1014.0), "value origin y was {}", origin.y);
        assert!(near(style.size, DARK.title_px * 1.5));
    }

    #[test]
    fn volume_reads_seventy_percent_of_twenty_segments_in_copper() {
        let scene = volume(REF);
        let segments = meter(&scene);
        assert_eq!(segments.len(), 20, "the meter must be twenty rects");
        let filled = segments.iter().filter(|(_, c)| *c == DARK.accent).count();
        assert_eq!(filled, 14);
        assert!(
            segments.iter().any(|(_, c)| *c == DARK.line),
            "the empty track needs the line token, not a literal"
        );
        find_text(&scene, "Speakers");
    }

    #[test]
    fn brightness_reads_forty_five_percent() {
        let scene = brightness(REF);
        let segments = meter(&scene);
        assert_eq!(segments.len(), 20);
        assert_eq!(
            segments.iter().filter(|(_, c)| *c == DARK.accent).count(),
            9
        );
        let (_, caption) = find_text(&scene, "Brightness");
        assert_eq!(caption.color, DARK.text_dim);
        assert_eq!(caption.align, TextAlign::Center);
    }

    #[test]
    fn osd_uses_semantic_tokens_only() {
        let used = colors(&volume(REF));
        assert!(used.contains(&DARK.accent), "no copper in the meter");
        assert!(used.contains(&DARK.text_dim), "no dim ink in the glyph");
        assert!(used.contains(&DARK.scrim), "no scrim behind the pill");
    }
}
