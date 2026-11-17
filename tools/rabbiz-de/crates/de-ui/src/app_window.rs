//! P13 app window chrome: the full 720×1600 app surface, a fake toolbar, one
//! content card, the status inset and the gesture bar.
//!
//! Frozen scaffold API: the frame is a pure function of the output [`Size`] and
//! returns drawing operations only. Geometry is authored on the 720×1600
//! reference panel and mapped with `sx`/`sy`, so the chrome keeps its inset on
//! the 1080×2400 panel.
//!
//! This is the frame a real app is composited into: no decorations, the status
//! inset is 48 px (P5/P13), and the 32 px gesture bar overlays the bottom.

use de_api::{icons, Layer, Point, Rect, Scene, Size, TextAlign, TextStyle, Weight};
use de_design::DARK;

/// `menu` from `assets/fonts/MaterialSymbolsRounded.codepoints`.
const ICON_MENU: char = '\u{e5d2}';

/// Reference panel the app frame was authored on.
const REF_W: f32 = 720.0;
const REF_H: f32 = 1600.0;

/// Toolbar band under the status inset (48 px + 88 px = 136 px of chrome).
const TOOLBAR_H: f32 = 88.0;

/// The demo content card: 672 wide (the page column), 420 tall.
const CARD_W: f32 = REF_W - 2.0 * DARK.page_margin;
const CARD_H: f32 = 420.0;

pub fn build(size: Size) -> Scene {
    let w = size.w as f32;
    let h = size.h as f32;
    let sx = w / REF_W;
    let sy = h / REF_H;
    // Corners, glyphs and type keep their aspect on odd panels.
    let s = sx.min(sy);

    let mut scene = Scene::new(size);
    let mut layer = Layer::new("app-window");

    // The first op is always the whole output; the app surface then fills it.
    layer.rect(Rect::new(0.0, 0.0, w, h), 0.0, DARK.bg);
    layer.rect(Rect::new(0.0, 0.0, w, h), 0.0, DARK.canvas);

    status_inset(&mut layer, w, sx, sy, s);
    toolbar(&mut layer, w, sx, sy, s);
    content_card(&mut layer, sx, sy, s);
    gesture_bar(&mut layer, w, h, sx, sy, s);

    scene.push(layer);
    scene
}

/// 48 px status inset: clock on the left, battery on the right, nothing in the
/// punch-hole cutout on the far left.
fn status_inset(layer: &mut Layer, w: f32, sx: f32, sy: f32, s: f32) {
    let band = Rect::new(0.0, 0.0, w, DARK.status_h * sy);
    layer.rect(band, 0.0, DARK.surface_low);

    let clock = TextStyle::new(DARK.label_px * s, Weight::Regular, DARK.text_dim, 160.0 * s);
    layer.text(Point::new(DARK.page_margin * sx, 12.0 * sy), "9:41", clock);

    layer.icon(
        Point::new(w - DARK.page_margin * sx - 12.0 * s, 24.0 * sy),
        24.0 * s,
        icons::BATTERY_FULL,
        DARK.text_faint,
        true,
    );
}

/// Fake toolbar: hamburger, title, search. The band is one token away from the
/// app surface so the chrome reads without a border.
fn toolbar(layer: &mut Layer, w: f32, sx: f32, sy: f32, s: f32) {
    let band = Rect::new(0.0, DARK.status_h * sy, w, TOOLBAR_H * sy);
    layer.rect(band, 0.0, DARK.surface);

    let bar_x = DARK.page_margin * sx;
    layer.icon(
        Point::new(bar_x + 12.0 * s, band.y + 44.0 * sy),
        26.0 * s,
        ICON_MENU,
        DARK.text_dim,
        false,
    );

    let title = TextStyle::new(DARK.label_px * s, Weight::SemiBold, DARK.text, 420.0 * s);
    layer.text(
        Point::new(bar_x + 44.0 * sx, band.y + 31.0 * sy),
        "Rabbiz Files",
        title,
    );

    layer.icon(
        Point::new(w - DARK.page_margin * sx - 38.0 * s, band.y + 44.0 * sy),
        26.0 * s,
        icons::SEARCH,
        DARK.text_dim,
        false,
    );

    // One hairline under the chrome; no shadows, no second surface.
    layer.rect(Rect::new(0.0, band.bottom(), w, 2.0 * s), 0.0, DARK.line);
}

/// The demo content: one 672×420 surface card with a kicker, a title, a
/// subtitle, four body lines and a copper action chip.
fn content_card(layer: &mut Layer, sx: f32, sy: f32, s: f32) {
    let card = Rect::new(DARK.page_margin * sx, 200.0 * sy, CARD_W * sx, CARD_H * sy);
    layer.rect(card, DARK.radius_card * s, DARK.surface);

    let inner_x = card.x + DARK.space_l * sx;
    let inner_w = card.w - 2.0 * DARK.space_l * sx;

    let kicker = TextStyle::new(DARK.micro_px * s, Weight::SemiBold, DARK.accent, inner_w);
    layer.text(Point::new(inner_x, card.y + 32.0 * sy), "RECENT", kicker);

    let title = TextStyle::new(DARK.h2_px * s, Weight::SemiBold, DARK.text, inner_w);
    layer.text(
        Point::new(inner_x, card.y + 62.0 * sy),
        "Camera roll",
        title,
    );

    let subtitle = TextStyle::new(DARK.caption_px * s, Weight::Regular, DARK.text_dim, inner_w);
    layer.text(
        Point::new(inner_x, card.y + 106.0 * sy),
        "128 photos · 1.2 GB",
        subtitle,
    );

    layer.rect(
        Rect::new(inner_x, card.y + 148.0 * sy, inner_w, 2.0 * s),
        1.0 * s,
        DARK.line,
    );

    let body = TextStyle::new(DARK.label_px * s, Weight::Regular, DARK.text_faint, inner_w);
    let lines = [
        "Saved in Pictures/RabbizOS",
        "Synced 2 minutes ago",
        "Available offline",
        "Shared with Rabbiz Mirror",
    ];
    for (i, line) in lines.into_iter().enumerate() {
        layer.text(
            Point::new(inner_x, card.y + (174.0 + 36.0 * i as f32) * sy),
            line,
            body,
        );
    }

    let chip = Rect::new(inner_x, card.y + 328.0 * sy, 132.0 * s, 44.0 * s);
    layer.rect(chip, DARK.radius_pill * s, DARK.accent);
    let chip_w = chip.w - 2.0 * DARK.space_s * s;
    let chip_text = TextStyle::new(DARK.label_px * s, Weight::SemiBold, DARK.accent_ink, chip_w)
        .with_align(TextAlign::Center);
    layer.text(
        Point::new(chip.x + (chip.w - chip_w) / 2.0, chip.y + 8.0 * s),
        "Open",
        chip_text,
    );
}

/// 32 px gesture bar overlaying the bottom edge, with the 200×8 nav pill from
/// DE-PLAN P13.
fn gesture_bar(layer: &mut Layer, w: f32, h: f32, sx: f32, sy: f32, s: f32) {
    let bar = Rect::new(0.0, h - DARK.nav_h * sy, w, DARK.nav_h * sy);
    layer.rect(bar, 0.0, DARK.bg);
    let pill = Rect::new(
        (w - 200.0 * sx) / 2.0,
        bar.y + (bar.h - 8.0 * s) / 2.0,
        200.0 * sx,
        8.0 * s,
    );
    layer.rect(pill, 4.0 * s, DARK.text_faint);
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
                DrawOp::Text { .. } | DrawOp::Icon { .. } => None,
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
    fn scene_is_the_requested_size_with_one_app_window_layer() {
        let scene = build(PANEL);
        assert_eq!(scene.size, PANEL);
        assert_eq!(scene.layers.len(), 1);
        assert_eq!(scene.layers[0].name, "app-window");
    }

    #[test]
    fn first_op_is_the_full_bleed_background() {
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
        // The app surface covers the same output.
        match &scene.layers[0].ops[1] {
            DrawOp::Rect { rect, color, .. } => {
                assert_eq!(*rect, Rect::new(0.0, 0.0, 720.0, 1600.0));
                assert_eq!(*color, DARK.canvas);
            }
            other => panic!("second op must be the app surface, got {other:?}"),
        }
    }

    #[test]
    fn status_inset_toolbar_and_gesture_bar_keep_their_heights() {
        let scene = build(REF);
        assert_eq!(
            find_rect(&scene, Rect::new(0.0, 0.0, 720.0, 48.0)),
            DARK.surface_low
        );
        assert_eq!(
            find_rect(&scene, Rect::new(0.0, 48.0, 720.0, 88.0)),
            DARK.surface
        );
        assert_eq!(
            find_rect(&scene, Rect::new(0.0, 1568.0, 720.0, 32.0)),
            DARK.bg
        );
        assert_eq!(
            find_rect(&scene, Rect::new(260.0, 1580.0, 200.0, 8.0)),
            DARK.text_faint
        );
    }

    #[test]
    fn content_card_is_672_by_420_inside_the_page_margin() {
        let scene = build(REF);
        assert_eq!(
            find_rect(&scene, Rect::new(24.0, 200.0, 672.0, 420.0)),
            DARK.surface
        );
    }

    #[test]
    fn toolbar_title_and_card_text_are_present() {
        let scene = build(REF);
        let (_, style) = find_text(&scene, "Rabbiz Files");
        assert_eq!(style.weight, Weight::SemiBold);
        assert_eq!(style.color, DARK.text);

        let (_, title) = find_text(&scene, "Camera roll");
        assert_eq!(title.color, DARK.text);
        find_text(&scene, "Saved in Pictures/RabbizOS");
        find_text(&scene, "Open");
        find_text(&scene, "9:41");
    }

    #[test]
    fn chrome_scales_with_the_requested_size() {
        let scene = build(PANEL);
        assert_eq!(
            find_rect(&scene, Rect::new(36.0, 300.0, 1008.0, 630.0)),
            DARK.surface
        );
        assert_eq!(
            find_rect(&scene, Rect::new(0.0, 72.0, 1080.0, 132.0)),
            DARK.surface
        );
        assert_eq!(
            find_rect(&scene, Rect::new(390.0, 2370.0, 300.0, 12.0)),
            DARK.text_faint
        );

        let (origin, size) = find_text(&scene, "Rabbiz Files");
        assert!(near(origin.x, 102.0), "title x was {}", origin.x);
        assert!(near(size.size, DARK.label_px * 1.5));
    }

    #[test]
    fn frame_uses_the_copper_accent_and_dim_ink() {
        let scene = build(REF);
        let mut used: Vec<Color> = rects(&scene).into_iter().map(|(_, c)| c).collect();
        used.extend(scene.layers[0].ops.iter().filter_map(|op| match op {
            DrawOp::Text { style, .. } => Some(style.color),
            DrawOp::Icon { color, .. } => Some(*color),
            DrawOp::Rect { .. } => None,
        }));
        assert!(used.contains(&DARK.accent), "no copper action chip");
        assert!(used.contains(&DARK.text_faint), "no faint body ink");
        assert!(used.contains(&DARK.accent_ink), "no ink on the chip");
    }

    #[test]
    fn chrome_uses_material_icons() {
        let scene = build(REF);
        let codepoints: Vec<char> = scene.layers[0]
            .ops
            .iter()
            .filter_map(|op| match op {
                DrawOp::Icon { codepoint, .. } => Some(*codepoint),
                DrawOp::Rect { .. } | DrawOp::Text { .. } => None,
            })
            .collect();
        assert!(codepoints.contains(&icons::BATTERY_FULL));
        assert!(codepoints.contains(&ICON_MENU));
        assert!(codepoints.contains(&icons::SEARCH));
    }
}
