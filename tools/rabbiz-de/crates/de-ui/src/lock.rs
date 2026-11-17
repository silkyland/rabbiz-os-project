//! P1 lock screen: status line, clock, date, notification preview, media card,
//! and the bottom action row with the copper gesture pill.
//!
//! Authored on the 720×1600 reference panel and mapped onto the requested
//! [`Size`] with `sx`/`sy`, like every other screen. Only [`DARK`] tokens are
//! used: the plum ground is quiet, a low ember glow replaces a busy wallpaper,
//! and system UI icons use Material Symbols Rounded.

use de_api::{icons, Layer, Point, Rect, Scene, Size, TextAlign, TextStyle, Weight};
use de_design::DARK;

/// Reference panel the composition was authored on.
const REF_W: f32 = 720.0;
const REF_H: f32 = 1600.0;

/// A lock screen is one layer, named for the screen.
const LAYER: &str = "lock";

pub fn build(size: Size) -> Scene {
    let f = Frame::new(size);
    let mut scene = Scene::new(size);
    let mut layer = Layer::new(LAYER);

    // Full bleed: the first op always covers the whole output.
    layer.rect(Rect::new(0.0, 0.0, f.w, f.h), 0.0, DARK.bg);

    // A low ember glow instead of a wallpaper: fine alpha rings read as a soft
    // gradient without needing a gradient op.
    for step in 0..26 {
        let radius = 330.0 - 12.0 * step as f32;
        layer.rect(
            f.rect(360.0 - radius, 700.0 - radius, radius * 2.0, radius * 2.0),
            f.len(radius),
            DARK.accent.with_alpha(2),
        );
    }

    // Status line: time on the left, battery on the right.
    layer.text(
        f.at(24.0, 30.0),
        "20:17",
        TextStyle::new(
            f.len(DARK.caption_px),
            Weight::Regular,
            DARK.text_dim,
            200.0 * f.sx,
        ),
    );
    layer.icon(
        f.at(670.0, 49.0),
        f.len(30.0),
        icons::BATTERY_FULL,
        DARK.success,
        true,
    );
    centred(
        &mut layer,
        &f,
        588.0,
        30.0,
        "38%",
        TextStyle::new(
            f.len(DARK.caption_px),
            Weight::Regular,
            DARK.text_dim,
            96.0 * f.sx,
        ),
    );

    // Clock and date: the lock clock is the largest type in the system.
    layer.text(
        f.at(24.0, 104.0),
        "20:17",
        TextStyle::new(f.len(DARK.clock_px), Weight::Light, DARK.text, 672.0 * f.sx),
    );
    layer.text(
        f.at(24.0, 268.0),
        "Monday, 16 November",
        TextStyle::new(
            f.len(DARK.label_px),
            Weight::Regular,
            DARK.text_dim,
            672.0 * f.sx,
        ),
    );

    // Compact notification preview: avatar, sender, one line of body.
    layer.rect(
        f.rect(24.0, 360.0, 672.0, 116.0),
        f.len(DARK.radius_card),
        DARK.surface,
    );
    let avatar = f.rect(48.0, 386.0, 64.0, 64.0);
    layer.rect(avatar, f.len(DARK.radius_m), DARK.surface_high);
    centred(
        &mut layer,
        &f,
        80.0,
        402.0,
        "M",
        TextStyle::new(
            f.len(DARK.label_px),
            Weight::SemiBold,
            DARK.accent,
            64.0 * f.sx,
        ),
    );
    layer.text(
        f.at(128.0, 380.0),
        "Messages · Nok",
        TextStyle::new(
            f.len(DARK.label_px),
            Weight::SemiBold,
            DARK.text,
            470.0 * f.sx,
        ),
    );
    layer.text(
        f.at(128.0, 414.0),
        "Are we still on for dinner?",
        TextStyle::new(
            f.len(DARK.caption_px),
            Weight::Regular,
            DARK.text_dim,
            520.0 * f.sx,
        ),
    );
    layer.rect(f.rect(660.0, 378.0, 12.0, 12.0), f.len(6.0), DARK.accent);

    // Media card: album initial, title, artist, and a pause control.
    layer.rect(
        f.rect(24.0, 1124.0, 672.0, 148.0),
        f.len(DARK.radius_card),
        DARK.surface,
    );
    let album = f.rect(48.0, 1166.0, 64.0, 64.0);
    layer.rect(album, f.len(DARK.radius_m), DARK.surface_high);
    centred(
        &mut layer,
        &f,
        80.0,
        1182.0,
        "M",
        TextStyle::new(
            f.len(DARK.label_px),
            Weight::SemiBold,
            DARK.accent,
            64.0 * f.sx,
        ),
    );
    layer.text(
        f.at(128.0, 1160.0),
        "Midnight Drive",
        TextStyle::new(
            f.len(DARK.body_px),
            Weight::SemiBold,
            DARK.text,
            340.0 * f.sx,
        ),
    );
    layer.text(
        f.at(128.0, 1196.0),
        "Luna Ray",
        TextStyle::new(
            f.len(DARK.caption_px),
            Weight::Regular,
            DARK.text_dim,
            340.0 * f.sx,
        ),
    );
    layer.rect(f.rect(608.0, 1166.0, 64.0, 64.0), f.len(32.0), DARK.accent);
    layer.icon(
        f.at(640.0, 1198.0),
        f.len(34.0),
        icons::PAUSE,
        DARK.accent_ink,
        true,
    );

    // Bottom row: three 96 px action tiles with Material Symbols Rounded icons.
    let tile_y = 1352.0;
    for (x, fill) in [
        (96.0, DARK.surface),
        (312.0, DARK.accent),
        (528.0, DARK.surface),
    ] {
        layer.rect(f.rect(x, tile_y, 96.0, 96.0), f.len(DARK.radius_l), fill);
    }
    layer.icon(
        f.at(144.0, 1400.0),
        f.len(44.0),
        icons::FLASHLIGHT_ON,
        DARK.accent,
        false,
    );
    layer.icon(
        f.at(360.0, 1400.0),
        f.len(50.0),
        icons::FINGERPRINT,
        DARK.accent_ink,
        true,
    );
    layer.icon(
        f.at(576.0, 1400.0),
        f.len(44.0),
        icons::POWER_SETTINGS_NEW,
        DARK.text_dim,
        false,
    );

    let label_size = f.len(DARK.label_px);
    for (cx, label, ink) in [
        (144.0, "Flashlight", DARK.text_dim),
        (360.0, "Touch to unlock", DARK.text),
        (576.0, "Power", DARK.text_dim),
    ] {
        centred(
            &mut layer,
            &f,
            cx,
            1464.0,
            label,
            TextStyle::new(label_size, Weight::Regular, ink, 200.0 * f.sx),
        );
    }

    // Copper gesture pill: the unlock affordance sits at the very bottom.
    layer.rect(
        f.rect(260.0, 1548.0, 200.0, 8.0),
        f.len(DARK.radius_pill),
        DARK.accent,
    );

    scene.push(layer);
    scene
}

/// Reference-space frame: `sx`/`sy` map 720×1600 onto the output, `s` keeps
/// radii and type round on a panel with a different aspect.
struct Frame {
    w: f32,
    h: f32,
    sx: f32,
    sy: f32,
    s: f32,
}

impl Frame {
    fn new(size: Size) -> Self {
        let w = size.w as f32;
        let h = size.h as f32;
        let sx = w / REF_W;
        let sy = h / REF_H;
        Self {
            w,
            h,
            sx,
            sy,
            s: sx.min(sy),
        }
    }

    fn rect(&self, x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect::new(x * self.sx, y * self.sy, w * self.sx, h * self.sy)
    }

    fn at(&self, x: f32, y: f32) -> Point {
        Point::new(x * self.sx, y * self.sy)
    }

    fn len(&self, v: f32) -> f32 {
        v * self.s
    }
}

/// Place a centre-aligned text box on `cx`; `style.max_width` is the box width.
fn centred(layer: &mut Layer, f: &Frame, cx: f32, y: f32, text: &str, style: TextStyle) {
    let style = style.with_align(TextAlign::Center);
    let x = cx * f.sx - style.max_width / 2.0;
    layer.text(Point::new(x, y * f.sy), text, style);
}

#[cfg(test)]
mod tests {
    use super::*;
    use de_api::{Color, DrawOp};

    const REF: Size = Size::new(720, 1600);

    fn texts(scene: &Scene) -> impl Iterator<Item = (&str, Point, &TextStyle)> {
        scene
            .layers
            .iter()
            .flat_map(|layer| layer.ops.iter())
            .filter_map(|op| match op {
                DrawOp::Text {
                    origin,
                    text,
                    style,
                } => Some((text.as_str(), *origin, style)),
                DrawOp::Rect { .. } | DrawOp::Icon { .. } => None,
            })
    }

    fn icon_ops(scene: &Scene) -> impl Iterator<Item = (char, bool, Color)> + '_ {
        scene
            .layers
            .iter()
            .flat_map(|layer| layer.ops.iter())
            .filter_map(|op| match op {
                DrawOp::Icon {
                    codepoint,
                    filled,
                    color,
                    ..
                } => Some((*codepoint, *filled, *color)),
                DrawOp::Rect { .. } | DrawOp::Text { .. } => None,
            })
    }

    fn find_text<'a>(scene: &'a Scene, needle: &str) -> (&'a str, Point, &'a TextStyle) {
        texts(scene)
            .find(|(text, _, _)| *text == needle)
            .unwrap_or_else(|| panic!("no text op for {needle:?}"))
    }

    fn palette(scene: &Scene) -> Vec<Color> {
        scene
            .layers
            .iter()
            .flat_map(|layer| layer.ops.iter())
            .flat_map(|op| match op {
                DrawOp::Rect { color, .. } => vec![*color],
                DrawOp::Text { style, .. } => vec![style.color],
                DrawOp::Icon { color, .. } => vec![*color],
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
    fn first_op_is_the_full_bleed_plum_background() {
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
    fn clock_uses_the_clock_token_in_light_type_and_the_date_is_dim() {
        let scene = build(REF);
        let clock = texts(&scene)
            .filter(|(text, _, _)| *text == "20:17")
            .map(|(_, _, style)| style)
            .max_by(|a, b| a.size.total_cmp(&b.size))
            .expect("a clock text op");
        assert!((clock.size - DARK.clock_px).abs() < 0.01);
        assert_eq!(clock.weight, Weight::Light);
        assert_eq!(clock.color, DARK.text);

        let (_, _, date) = find_text(&scene, "Monday, 16 November");
        assert_eq!(date.color, DARK.text_dim);
    }

    #[test]
    fn notification_and_media_cards_are_present() {
        let scene = build(REF);
        for line in [
            "Messages · Nok",
            "Are we still on for dinner?",
            "Midnight Drive",
            "Luna Ray",
        ] {
            find_text(&scene, line);
        }
    }

    #[test]
    fn action_row_and_copper_pill_close_the_screen() {
        let scene = build(REF);
        for label in ["Flashlight", "Touch to unlock", "Power"] {
            find_text(&scene, label);
        }

        let mut tiles = 0;
        for op in &scene.layers[0].ops {
            if let DrawOp::Rect { rect, .. } = op {
                if (rect.w - 96.0).abs() < 0.01 && (rect.h - 96.0).abs() < 0.01 {
                    tiles += 1;
                }
            }
        }
        assert_eq!(tiles, 3, "three 96 px action tiles");

        let icons_list: Vec<_> = icon_ops(&scene).collect();
        assert!(icons_list.contains(&(icons::BATTERY_FULL, true, DARK.success)));
        assert!(icons_list.contains(&(icons::PAUSE, true, DARK.accent_ink)));
        assert!(icons_list.contains(&(icons::FLASHLIGHT_ON, false, DARK.accent)));
        assert!(icons_list.contains(&(icons::FINGERPRINT, true, DARK.accent_ink)));
        assert!(icons_list.contains(&(icons::POWER_SETTINGS_NEW, false, DARK.text_dim)));

        match &scene.layers[0].ops.last().unwrap() {
            DrawOp::Rect {
                rect,
                radius,
                color,
            } => {
                assert_eq!(*rect, Rect::new(260.0, 1548.0, 200.0, 8.0));
                assert_eq!(*radius, DARK.radius_pill);
                assert_eq!(*color, DARK.accent);
            }
            other => panic!("the pill must be the last op, got {other:?}"),
        }
    }

    #[test]
    fn semantic_tokens_are_used() {
        let used = palette(&build(REF));
        assert!(used.contains(&DARK.success), "battery level is not success");
        assert!(used.contains(&DARK.accent), "no copper accent");
        assert!(
            texts(&build(REF)).any(|(_, _, style)| style.color == DARK.text_dim),
            "no dim secondary type"
        );
    }

    #[test]
    fn composition_scales_with_the_requested_size() {
        let scene = build(Size::new(1080, 2400));
        match &scene.layers[0].ops[0] {
            DrawOp::Rect { rect, .. } => assert_eq!(*rect, Rect::new(0.0, 0.0, 1080.0, 2400.0)),
            other => panic!("first op must be the background, got {other:?}"),
        }
        let clock = texts(&scene)
            .filter(|(text, _, _)| *text == "20:17")
            .map(|(_, _, style)| style)
            .max_by(|a, b| a.size.total_cmp(&b.size))
            .expect("a clock text op");
        assert!((clock.size - DARK.clock_px * 1.5).abs() < 0.01);
    }
}
