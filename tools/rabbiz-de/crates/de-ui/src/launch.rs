//! P12 starting window: the icon tile morphing into an app frame.
//!
//! Frozen scaffold API: the starting window is a pure function of the output
//! [`Size`] and returns drawing operations only. Geometry is authored on the
//! 720×1600 reference panel and mapped with `sx`/`sy`, so the tile keeps its
//! centre on the 1080×2400 panel.
//!
//! This is the mid-morph frame: a dimmed canvas, the 96 px icon tile at the
//! centre, the 360×360 surface card growing out of it, a copper progress
//! hairline and the caption. No app content yet — the first app frame replaces
//! this layer once the toplevel maps.

use de_api::{Layer, Point, Rect, Scene, Size, TextAlign, TextStyle, Weight};
use de_design::DARK;

/// Reference panel the starting window was authored on.
const REF_W: f32 = 720.0;
const REF_H: f32 = 1600.0;

/// The tile is 96 px at the centre of the panel; the card is 360×360 around it.
const TILE: f32 = 96.0;
const CARD: f32 = 360.0;

/// How much of the progress hairline is copper while the app warms up.
const PROGRESS: f32 = 0.6;

pub fn build(size: Size) -> Scene {
    let w = size.w as f32;
    let h = size.h as f32;
    let sx = w / REF_W;
    let sy = h / REF_H;
    // Corners, the glyph and type keep their aspect on odd panels.
    let s = sx.min(sy);

    let mut scene = Scene::new(size);
    let mut layer = Layer::new("launch");

    // Full bleed first: the morph sits over the home surface it came from.
    layer.rect(Rect::new(0.0, 0.0, w, h), 0.0, DARK.bg);
    layer.rect(Rect::new(0.0, 0.0, w, h), 0.0, DARK.scrim);

    // The surface card grows out of the tile: 360×360, centred on the panel.
    let cx = 360.0 * sx;
    let cy = 800.0 * sy;
    let card = Rect::new(
        cx - CARD / 2.0 * sx,
        cy - CARD / 2.0 * sy,
        CARD * sx,
        CARD * sy,
    );
    layer.rect(card, DARK.radius_card * s, DARK.surface);

    // The tile stays on top of the card while the frame expands under it.
    let tile = Rect::new(cx - TILE / 2.0 * s, cy - TILE / 2.0 * s, TILE * s, TILE * s);
    layer.rect(tile, DARK.radius_l * s, DARK.accent);
    calculator(&mut layer, tile, s);

    // Caption under the card: what the shell is waiting for, on the page column.
    let caption_w = (REF_W - 2.0 * DARK.page_margin) * s;
    let caption = TextStyle::new(DARK.body_px * s, Weight::Regular, DARK.text, caption_w)
        .with_align(TextAlign::Center);
    layer.text(
        Point::new((w - caption_w) / 2.0, 1004.0 * sy),
        "Opening Calculator",
        caption,
    );

    // Copper progress hairline: a track at the card width, 60 % filled.
    let track = Rect::new(card.x, 1052.0 * sy, CARD * sx, 4.0 * s);
    layer.rect(track, 2.0 * s, DARK.line);
    layer.rect(
        Rect::new(track.x, track.y, track.w * PROGRESS, track.h),
        2.0 * s,
        DARK.accent,
    );

    scene.push(layer);
    scene
}

/// The Calculator glyph on the tile: a display bar over two rows of keys. All
/// rounded rects, no icon font, and it reads at 96 px.
fn calculator(layer: &mut Layer, tile: Rect, s: f32) {
    let pad = 24.0 * s;
    let display = Rect::new(tile.x + pad, tile.y + pad, tile.w - 2.0 * pad, 18.0 * s);
    layer.rect(display, 6.0 * s, DARK.accent_ink);

    // Three columns of two keys, centred under the display.
    let key = 14.0 * s;
    let gap = 8.0 * s;
    let grid_w = 3.0 * key + 2.0 * gap;
    let grid_x = tile.x + (tile.w - grid_w) / 2.0;
    let rows = [display.bottom() + 10.0 * s, display.bottom() + 30.0 * s];
    for row in rows {
        for col in 0..3 {
            let x = grid_x + col as f32 * (key + gap);
            layer.rect(Rect::new(x, row, key, 12.0 * s), 4.0 * s, DARK.accent_ink);
        }
    }
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

    fn rects(scene: &Scene) -> Vec<(Rect, Color)> {
        scene.layers[0]
            .ops
            .iter()
            .filter_map(|op| match op {
                DrawOp::Rect { rect, color, .. } => Some((*rect, *color)),
                DrawOp::Text { .. } => None,
            })
            .collect()
    }

    fn find_rect(scene: &Scene, want: Rect) -> Color {
        rects(scene)
            .into_iter()
            .find(|(rect, _)| {
                near(rect.x, want.x)
                    && near(rect.y, want.y)
                    && near(rect.w, want.w)
                    && near(rect.h, want.h)
            })
            .unwrap_or_else(|| panic!("no rect at {want:?}"))
            .1
    }

    #[test]
    fn scene_is_the_requested_size_with_one_launch_layer() {
        let scene = build(PANEL);
        assert_eq!(scene.size, PANEL);
        assert_eq!(scene.layers.len(), 1);
        assert_eq!(scene.layers[0].name, "launch");
    }

    #[test]
    fn first_op_is_the_full_bleed_background_then_a_dim_scrim() {
        let scene = build(REF);
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

    #[test]
    fn tile_and_card_land_on_the_centre_of_the_panel() {
        let scene = build(REF);
        assert_eq!(
            find_rect(&scene, Rect::new(180.0, 620.0, 360.0, 360.0)),
            DARK.surface
        );
        assert_eq!(
            find_rect(&scene, Rect::new(312.0, 752.0, 96.0, 96.0)),
            DARK.accent
        );
    }

    #[test]
    fn caption_and_hairline_are_present() {
        let scene = build(REF);
        let (origin, style) = find_text(&scene, "Opening Calculator");
        assert_eq!(origin, Point::new(24.0, 1004.0));
        assert_eq!(style.align, TextAlign::Center);
        assert_eq!(style.color, DARK.text);
        assert!((style.size - DARK.body_px).abs() < 0.01);

        assert_eq!(
            find_rect(&scene, Rect::new(180.0, 1052.0, 360.0, 4.0)),
            DARK.line
        );
        assert_eq!(
            find_rect(&scene, Rect::new(180.0, 1052.0, 216.0, 4.0)),
            DARK.accent
        );
    }

    #[test]
    fn morph_scales_with_the_requested_size() {
        let scene = build(PANEL);
        assert_eq!(scene.size, PANEL);
        assert_eq!(
            find_rect(&scene, Rect::new(270.0, 930.0, 540.0, 540.0)),
            DARK.surface
        );
        assert_eq!(
            find_rect(&scene, Rect::new(468.0, 1128.0, 144.0, 144.0)),
            DARK.accent
        );

        let (origin, style) = find_text(&scene, "Opening Calculator");
        assert!(near(origin.x, 36.0), "caption x was {}", origin.x);
        assert!(near(origin.y, 1506.0), "caption y was {}", origin.y);
        assert!((style.size - DARK.body_px * 1.5).abs() < 0.01);
    }

    #[test]
    fn morph_uses_the_copper_accent_and_the_scrim() {
        let used: Vec<Color> = rects(&build(REF)).into_iter().map(|(_, c)| c).collect();
        assert!(used.contains(&DARK.accent), "no copper tile or hairline");
        assert!(used.contains(&DARK.scrim), "no dim scrim");
        assert!(used.contains(&DARK.accent_ink), "no ink inside the tile");
    }
}
