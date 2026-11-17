//! P2 home: status strip, clock and date, hero card, 4×2 app grid, search pill,
//! dock, and page dots.
//!
//! Authored on the 720×1600 reference panel and mapped onto the requested
//! [`Size`]. Icons are letter tiles cut from rounded rects, a low ember glow
//! stands in for the wallpaper, and every colour, radius and type size comes
//! from [`DARK`].

use de_api::{Color, Layer, Point, Rect, Scene, Size, TextAlign, TextStyle, Weight};
use de_design::DARK;

/// Reference panel the composition was authored on.
const REF_W: f32 = 720.0;
const REF_H: f32 = 1600.0;

/// A home surface is one layer, named for the screen.
const LAYER: &str = "home";

/// The 4×2 grid: label, glyph, tile fill, glyph ink.
const APPS: [(&str, &str, Color, Color); 8] = [
    ("Phone", "P", DARK.accent, DARK.accent_ink),
    ("Messages", "M", DARK.surface_high, DARK.accent_hi),
    ("Settings", "S", DARK.mark_body, DARK.text),
    ("Chrome", "C", DARK.surface_high, DARK.text),
    ("Files", "F", DARK.surface_high, DARK.accent),
    ("Calendar", "C", DARK.surface_high, DARK.text),
    ("Notes", "N", DARK.mark_body, DARK.text),
    ("Camera", "C", DARK.surface_high, DARK.text),
];

/// The dock repeats four pinned apps with shorter labels.
const DOCK: [(&str, &str, Color, Color); 4] = [
    ("Phone", "P", DARK.accent, DARK.accent_ink),
    ("Messages", "M", DARK.surface_high, DARK.accent_hi),
    ("Chrome", "C", DARK.surface_high, DARK.text),
    ("Files", "F", DARK.mark_body, DARK.text),
];

pub fn build(size: Size) -> Scene {
    let f = Frame::new(size);
    let mut scene = Scene::new(size);
    let mut layer = Layer::new(LAYER);

    // Full bleed: the first op always covers the whole output.
    layer.rect(Rect::new(0.0, 0.0, f.w, f.h), 0.0, DARK.bg);

    // A quiet ember low on the canvas: fine alpha rings read as the default
    // Rabbiz wallpaper without drawing one.
    for step in 0..14 {
        let radius = 210.0 - 15.0 * step as f32;
        layer.rect(
            f.rect(360.0 - radius, 1020.0 - radius, radius * 2.0, radius * 2.0),
            f.len(radius),
            DARK.accent.with_alpha(2),
        );
    }

    // Status strip: time on the left, battery on the right.
    layer.text(
        f.at(24.0, 26.0),
        "09:41",
        TextStyle::new(
            f.len(DARK.caption_px),
            Weight::Regular,
            DARK.text_dim,
            180.0 * f.sx,
        ),
    );
    layer.rect(
        f.rect(644.0, 32.0, 44.0, 22.0),
        f.len(6.0),
        DARK.surface_high,
    );
    layer.rect(f.rect(648.0, 36.0, 36.0, 14.0), f.len(4.0), DARK.success);
    layer.rect(f.rect(689.0, 38.0, 4.0, 10.0), f.len(2.0), DARK.success);
    centred(
        &mut layer,
        &f,
        588.0,
        26.0,
        "38%",
        TextStyle::new(
            f.len(DARK.caption_px),
            Weight::Regular,
            DARK.text_dim,
            96.0 * f.sx,
        ),
    );

    // Clock and date, centred on the panel.
    layer.text(
        f.at(24.0, 86.0),
        "09:41",
        centred_style(f.len(DARK.display_px), Weight::Light, DARK.text, &f, 672.0),
    );
    layer.text(
        f.at(24.0, 226.0),
        "Monday, 16 November",
        centred_style(
            f.len(DARK.label_px),
            Weight::Regular,
            DARK.text_dim,
            &f,
            672.0,
        ),
    );

    // Hero card: product line, greeting, and the pull-up affordance.
    layer.rect(
        f.rect(24.0, 300.0, 672.0, 180.0),
        f.len(DARK.radius_card),
        DARK.surface,
    );
    layer.rect(f.rect(48.0, 340.0, 6.0, 96.0), f.len(3.0), DARK.accent);
    layer.text(
        f.at(80.0, 328.0),
        "RabbizOS",
        TextStyle::new(
            f.len(DARK.micro_px),
            Weight::SemiBold,
            DARK.accent,
            520.0 * f.sx,
        ),
    );
    layer.text(
        f.at(80.0, 354.0),
        "Good evening",
        TextStyle::new(f.len(DARK.h2_px), Weight::SemiBold, DARK.text, 520.0 * f.sx),
    );
    layer.text(
        f.at(80.0, 400.0),
        "Swipe up for all apps",
        TextStyle::new(
            f.len(DARK.body_px),
            Weight::Regular,
            DARK.text_dim,
            520.0 * f.sx,
        ),
    );

    // 4×2 app grid: every cell is 168×156, every icon a 96 px letter tile.
    for (index, (label, glyph, fill, ink)) in APPS.iter().enumerate() {
        let cell_x = 24.0 + 168.0 * (index % 4) as f32;
        let cell_y = 520.0 + 156.0 * (index / 4) as f32;
        let tile = f.rect(cell_x + 36.0, cell_y + 10.0, 96.0, 96.0);
        layer.rect(tile, f.len(DARK.radius_l), *fill);
        if *label == "Messages" {
            layer.rect(
                f.rect(cell_x + 110.0, cell_y + 18.0, 14.0, 14.0),
                f.len(7.0),
                DARK.accent,
            );
        }
        centred(
            &mut layer,
            &f,
            cell_x + 84.0,
            cell_y + 10.0 + (96.0 - DARK.label_px * 1.25) / 2.0,
            glyph,
            TextStyle::new(f.len(DARK.label_px), Weight::SemiBold, *ink, tile.w),
        );
        centred(
            &mut layer,
            &f,
            cell_x + 84.0,
            cell_y + 116.0,
            label,
            TextStyle::new(
                f.len(DARK.label_px),
                Weight::Regular,
                DARK.text_dim,
                168.0 * f.sx,
            ),
        );
    }

    // Page dots: the copper pill is the current page.
    layer.rect(f.rect(328.0, 1200.0, 32.0, 8.0), f.len(4.0), DARK.accent);
    layer.rect(f.rect(368.0, 1200.0, 8.0, 8.0), f.len(4.0), DARK.text_faint);
    layer.rect(f.rect(384.0, 1200.0, 8.0, 8.0), f.len(4.0), DARK.text_faint);

    // Search pill: a copper magnifier ring and a quiet placeholder.
    layer.rect(
        f.rect(24.0, 1240.0, 672.0, 64.0),
        f.len(DARK.radius_pill),
        DARK.surface_high,
    );
    layer.rect(f.rect(56.0, 1258.0, 28.0, 28.0), f.len(14.0), DARK.accent);
    layer.rect(
        f.rect(61.0, 1263.0, 18.0, 18.0),
        f.len(9.0),
        DARK.surface_high,
    );
    layer.rect(f.rect(79.0, 1282.0, 10.0, 6.0), f.len(3.0), DARK.accent);
    layer.text(
        f.at(100.0, 1255.0),
        "Search apps",
        TextStyle::new(
            f.len(DARK.body_px),
            Weight::Regular,
            DARK.text_faint,
            520.0 * f.sx,
        ),
    );

    // Dock: 672×120 with four 88 px tiles.
    layer.rect(
        f.rect(24.0, 1332.0, 672.0, 120.0),
        f.len(DARK.radius_card),
        DARK.surface,
    );
    for (index, (_label, glyph, fill, ink)) in DOCK.iter().enumerate() {
        let x = 40.0 + 184.0 * index as f32;
        let tile = f.rect(x, 1348.0, 88.0, 88.0);
        layer.rect(tile, f.len(DARK.radius_l), *fill);
        centred(
            &mut layer,
            &f,
            x + 44.0,
            1348.0 + (88.0 - DARK.label_px * 1.25) / 2.0,
            glyph,
            TextStyle::new(f.len(DARK.label_px), Weight::SemiBold, *ink, tile.w),
        );
    }

    // Gesture bar, barely there so the dock keeps the focus.
    layer.rect(
        f.rect(260.0, 1556.0, 200.0, 8.0),
        f.len(DARK.radius_pill),
        DARK.line,
    );

    scene.push(layer);
    scene
}

/// Centred text style whose box spans `width` reference pixels.
fn centred_style(size: f32, weight: Weight, color: Color, f: &Frame, width: f32) -> TextStyle {
    TextStyle::new(size, weight, color, width * f.sx).with_align(TextAlign::Center)
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
    use de_api::DrawOp;

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
                DrawOp::Rect { .. } => None,
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
            })
            .collect()
    }

    #[test]
    fn scene_is_the_requested_size_with_one_home_layer() {
        let size = Size::new(1080, 2400);
        let scene = build(size);
        assert_eq!(scene.size, size);
        assert_eq!(scene.layers.len(), 1);
        assert_eq!(scene.layers[0].name, "home");
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
    fn clock_uses_the_display_token_in_light_type() {
        let scene = build(REF);
        let clock = texts(&scene)
            .filter(|(text, _, _)| *text == "09:41")
            .map(|(_, _, style)| style)
            .max_by(|a, b| a.size.total_cmp(&b.size))
            .expect("a clock text op");
        assert!((clock.size - DARK.display_px).abs() < 0.01);
        assert_eq!(clock.weight, Weight::Light);
        assert_eq!(clock.align, TextAlign::Center);
        find_text(&scene, "Monday, 16 November");
    }

    #[test]
    fn hero_and_search_copy_are_present() {
        let scene = build(REF);
        for line in ["Good evening", "Swipe up for all apps", "Search apps"] {
            find_text(&scene, line);
        }
    }

    #[test]
    fn the_grid_carries_all_eight_apps_as_96px_tiles() {
        let scene = build(REF);
        for (label, glyph, _, _) in APPS {
            find_text(&scene, label);
            assert!(
                texts(&scene).any(|(text, _, _)| text == glyph),
                "no glyph tile for {label}"
            );
        }

        let mut icons = 0;
        for op in &scene.layers[0].ops {
            if let DrawOp::Rect { rect, .. } = op {
                if (rect.w - 96.0).abs() < 0.01 && (rect.h - 96.0).abs() < 0.01 {
                    icons += 1;
                }
            }
        }
        assert_eq!(icons, 8, "eight 96 px grid icons");
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
            .filter(|(text, _, _)| *text == "09:41")
            .map(|(_, _, style)| style)
            .max_by(|a, b| a.size.total_cmp(&b.size))
            .expect("a clock text op");
        assert!((clock.size - DARK.display_px * 1.5).abs() < 0.01);
    }
}
