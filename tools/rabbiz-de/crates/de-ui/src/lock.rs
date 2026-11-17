//! P1 lock screen, empty-state skeleton: clock, date and the unlock hint.
//!
//! The composition follows `docs/DE-PLAN.md` §7.18 P1: the date sits above a
//! display-size clock in the page column, and the unlock affordance lives in
//! the bottom band where a thumb reaches it. Status line, notifications, the
//! media card and the flash/camera row arrive with their milestones; this
//! scene is the shell those pieces will be added to.
//!
//! Like [`crate::boot`], geometry is authored on the 720×1600 reference panel
//! and mapped onto the requested [`Size`], so the same scene fills any output.
//! It returns drawing operations only: no font, no renderer, no OS.

use de_api::{Point, Rect, Scene, Size, TextAlign, TextStyle, Weight};
use de_design::DARK;

/// Reference panel the P1 composition was authored on.
const REF_W: f32 = 720.0;
const REF_H: f32 = 1600.0;

/// The lock frame is a single full-screen layer.
const LAYER: &str = "lock";

/// Page-column x used by P1's lockup (`x=40` in the plan), and the text column
/// width the boot frame also uses.
const COLUMN_X: f32 = 40.0;
const COLUMN_W: f32 = 560.0;

pub fn build(size: Size) -> Scene {
    let w = size.w as f32;
    let h = size.h as f32;
    let sx = w / REF_W;
    let sy = h / REF_H;
    // Corners and type keep their aspect on odd panels.
    let s = sx.min(sy);

    let mut scene = Scene::new(size);
    let mut layer = de_api::Layer::new(LAYER);

    // Full bleed: the first op always covers the whole output.
    layer.rect(Rect::new(0.0, 0.0, w, h), 0.0, DARK.bg);

    // Date above the clock, in the same left column. Dim cream keeps the clock
    // as the one primary object on the screen.
    let date = TextStyle::new(
        DARK.body_px * s,
        Weight::Regular,
        DARK.text_dim,
        COLUMN_W * s,
    );
    layer.text(Point::new(COLUMN_X * sx, 168.0 * sy), "Mon 16 Nov", date);

    // The clock is the display-size, cream, regular-weight time.
    let clock = TextStyle::new(
        DARK.display_px * s,
        Weight::Regular,
        DARK.text,
        COLUMN_W * s,
    );
    layer.text(Point::new(COLUMN_X * sx, 208.0 * sy), "20:17", clock);

    // A plum band closes the bottom of the frame, echoing the boot horizon and
    // giving the unlock affordance a surface to sit on.
    let horizon_y = 1264.0 * sy;
    let horizon = Rect::new(0.0, horizon_y, w, h - horizon_y);
    layer.rect(horizon, DARK.radius_card * s, DARK.canvas);

    // Copper gesture pill above the hint: the touch target reads at a glance.
    let pill = Rect::new((w - 72.0 * s) / 2.0, 1376.0 * sy, 72.0 * s, 8.0 * s);
    layer.rect(pill, 4.0 * s, DARK.accent);

    // Cream unlock hint, centred on the bottom band.
    let hint = TextStyle::new(DARK.body_px * s, Weight::Regular, DARK.text, COLUMN_W * s)
        .with_align(TextAlign::Center);
    let hint_x = (w - hint.max_width) / 2.0;
    layer.text(Point::new(hint_x, 1424.0 * sy), "Touch to unlock", hint);

    scene.push(layer);
    scene
}

#[cfg(test)]
mod tests {
    use super::*;
    use de_api::{Color, DrawOp};

    const REF: Size = Size::new(720, 1600);

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

    fn text_colors(scene: &Scene) -> Vec<Color> {
        scene.layers[0]
            .ops
            .iter()
            .filter_map(|op| match op {
                DrawOp::Text { style, .. } => Some(style.color),
                DrawOp::Rect { .. } => None,
            })
            .collect()
    }

    #[test]
    fn scene_is_the_requested_size_with_one_lock_layer() {
        let size = Size::new(1080, 2400);
        let scene = build(size);
        assert_eq!(scene.size, size);
        assert_eq!(scene.layers.len(), 1);
        assert_eq!(scene.layers[0].name, "lock");
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
    }

    #[test]
    fn clock_is_display_sized_cream_and_in_the_page_column() {
        let scene = build(REF);
        let (origin, style) = find_text(&scene, "20:17");
        assert_eq!(origin, Point::new(40.0, 208.0));
        assert_eq!(style.size, DARK.display_px);
        assert_eq!(style.weight, Weight::Regular);
        assert_eq!(style.color, DARK.text);
        assert_eq!(style.align, TextAlign::Left);
        assert_eq!(style.max_width, 560.0);
    }

    #[test]
    fn date_sits_above_the_clock_in_the_same_column() {
        let scene = build(REF);
        let (date_origin, date_style) = find_text(&scene, "Mon 16 Nov");
        let (clock_origin, _) = find_text(&scene, "20:17");

        assert!(
            date_origin.y < clock_origin.y,
            "date must be above the clock"
        );
        assert_eq!(date_origin.x, clock_origin.x);
        // The date box ends before the clock starts.
        assert!(date_origin.y + date_style.size * 1.25 <= clock_origin.y);
        assert_eq!(date_style.size, DARK.body_px);
        assert_eq!(date_style.color, DARK.text_dim);
        assert_eq!(date_style.weight, Weight::Regular);
    }

    #[test]
    fn unlock_hint_is_cream_and_centred_on_the_bottom_band() {
        let scene = build(REF);
        let (origin, style) = find_text(&scene, "Touch to unlock");
        assert_eq!(origin, Point::new(80.0, 1424.0));
        assert_eq!(style.color, DARK.text);
        assert_eq!(style.align, TextAlign::Center);
        assert_eq!(style.size, DARK.body_px);
    }

    #[test]
    fn composition_uses_plum_copper_and_cream_tokens() {
        let scene = build(REF);
        let rects = rects(&scene);

        // The plum canvas: base background and the bottom band.
        assert!(rects.iter().any(|(_, _, color)| *color == DARK.bg));
        assert!(rects.iter().any(|(_, _, color)| *color == DARK.canvas));
        // Copper: the gesture pill is the screen's one accent.
        assert!(rects.iter().any(|(_, _, color)| *color == DARK.accent));

        let text = text_colors(&scene);
        assert!(text.contains(&DARK.text), "clock/hint must be cream");
        assert!(text.contains(&DARK.text_dim), "date must be dim cream");
    }

    #[test]
    fn bottom_band_carries_the_copper_pill_and_canvas_radius() {
        let scene = build(REF);
        let rects = rects(&scene);

        let (horizon, horizon_radius, horizon_color) = rects
            .iter()
            .find(|(rect, _, _)| rect.y == 1264.0)
            .copied()
            .expect("no bottom band at y=1264");
        assert_eq!(horizon, Rect::new(0.0, 1264.0, 720.0, 336.0));
        assert_eq!(horizon_radius, DARK.radius_card);
        assert_eq!(horizon_color, DARK.canvas);

        let (pill, pill_radius, pill_color) = rects
            .iter()
            .find(|(_, _, color)| *color == DARK.accent && color.a == 255)
            .copied()
            .expect("no solid copper pill");
        assert_eq!(pill, Rect::new(324.0, 1376.0, 72.0, 8.0));
        assert_eq!(pill_radius, 4.0);
        assert_eq!(pill_color, DARK.accent);
    }

    #[test]
    fn every_op_stays_inside_the_panel() {
        for (w, h) in [(720, 1600), (1080, 2400), (1440, 3200)] {
            let scene = build(Size::new(w, h));
            let (fw, fh) = (w as f32, h as f32);

            for (rect, _, _) in rects(&scene) {
                assert!(
                    rect.x >= 0.0 && rect.y >= 0.0,
                    "{w}x{h}: op starts off-panel at {rect:?}"
                );
                assert!(
                    rect.right() <= fw + 0.01 && rect.bottom() <= fh + 0.01,
                    "{w}x{h}: op overflows the panel at {rect:?}"
                );
            }

            for needle in ["Mon 16 Nov", "20:17", "Touch to unlock"] {
                let (origin, style) = find_text(&scene, needle);
                assert!(
                    origin.x + style.max_width <= fw + 0.01,
                    "{w}x{h}: {needle:?} overflows horizontally"
                );
                assert!(
                    origin.y + style.size * 1.25 <= fh + 0.01,
                    "{w}x{h}: {needle:?} overflows vertically"
                );
            }
        }
    }

    #[test]
    fn composition_scales_with_the_requested_size() {
        let scene = build(Size::new(1440, 3200));
        assert_eq!(scene.size, Size::new(1440, 3200));

        match &scene.layers[0].ops[0] {
            DrawOp::Rect { rect, .. } => assert_eq!(*rect, Rect::new(0.0, 0.0, 1440.0, 3200.0)),
            other => panic!("first op must be the background, got {other:?}"),
        }

        let (clock_origin, clock) = find_text(&scene, "20:17");
        assert_eq!(clock_origin, Point::new(80.0, 416.0));
        assert_eq!(clock.size, DARK.display_px * 2.0);

        let (hint_origin, hint) = find_text(&scene, "Touch to unlock");
        assert_eq!(hint_origin, Point::new(160.0, 2848.0));
        assert_eq!(hint.max_width, 1120.0);

        let rects = rects(&scene);
        let (horizon, horizon_radius, _) = rects
            .iter()
            .find(|(rect, _, _)| rect.y == 2528.0)
            .copied()
            .expect("no bottom band at 2x");
        assert_eq!(horizon, Rect::new(0.0, 2528.0, 1440.0, 672.0));
        assert_eq!(horizon_radius, DARK.radius_card * 2.0);

        let (pill, _, _) = rects
            .iter()
            .find(|(_, _, color)| *color == DARK.accent && color.a == 255)
            .copied()
            .expect("no solid copper pill at 2x");
        assert_eq!(pill, Rect::new(648.0, 2752.0, 144.0, 16.0));
    }
}
