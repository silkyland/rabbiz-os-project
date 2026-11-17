//! P3 app drawer: search field, recent row, and the 4×5 all-apps grid.
//!
//! The drawer is a full-screen plum canvas raised over the home surface: one
//! search card, four recent tiles, the section label, twenty app tiles with
//! labels, and the bottom handle. Authored on 720×1600 and scaled like every
//! other screen, with colours and sizes from [`DARK`] only.

use de_api::{icons, Color, Layer, Point, Rect, Scene, Size, TextAlign, TextStyle, Weight};
use de_design::DARK;

/// Reference panel the composition was authored on.
const REF_W: f32 = 720.0;
const REF_H: f32 = 1600.0;

/// A drawer is one layer, named for the screen.
const LAYER: &str = "drawer";

/// Codepoints verified against `assets/fonts/MaterialSymbolsRounded.codepoints`.
const ICON_CHAT: char = '\u{e0c9}';
const ICON_CONTACTS: char = '\u{e0ba}';
const ICON_EDIT_NOTE: char = '\u{e745}';
const ICON_MAIL: char = '\u{e159}';
const ICON_MAP: char = '\u{e55b}';
const ICON_MOVIE: char = '\u{e404}';
const ICON_MUSIC_NOTE: char = '\u{e405}';
const ICON_PHOTO_LIBRARY: char = '\u{e413}';
const ICON_PODCASTS: char = '\u{f048}';
const ICON_STORE: char = '\u{e8d1}';

#[derive(Clone, Copy)]
enum TileGlyph {
    Icon(char),
    Letter(&'static str),
}

/// The 4×5 all-apps grid: label, glyph, tile fill, glyph ink.
const APPS: [(&str, TileGlyph, Color, Color); 20] = [
    ("Phone", TileGlyph::Icon(icons::PHONE), DARK.surface, DARK.text),
    ("Messages", TileGlyph::Icon(ICON_CHAT), DARK.surface, DARK.text),
    ("Settings", TileGlyph::Icon(icons::SETTINGS), DARK.surface, DARK.text),
    ("Chrome", TileGlyph::Letter("C"), DARK.surface, DARK.text),
    ("Files", TileGlyph::Icon(icons::FOLDER), DARK.surface, DARK.text),
    ("Calendar", TileGlyph::Icon(icons::CALENDAR_TODAY), DARK.surface, DARK.text),
    ("Notes", TileGlyph::Icon(ICON_EDIT_NOTE), DARK.surface, DARK.text),
    ("Camera", TileGlyph::Icon(icons::PHOTO_CAMERA), DARK.surface, DARK.text),
    ("Clock", TileGlyph::Icon(icons::SCHEDULE), DARK.surface, DARK.text),
    ("Weather", TileGlyph::Icon(icons::WB_SUNNY), DARK.surface, DARK.text),
    ("Photos", TileGlyph::Icon(ICON_PHOTO_LIBRARY), DARK.surface, DARK.text),
    ("Music", TileGlyph::Icon(ICON_MUSIC_NOTE), DARK.surface, DARK.text),
    ("Calculator", TileGlyph::Icon(icons::CALCULATE), DARK.surface, DARK.text),
    ("Terminal", TileGlyph::Icon(icons::TERMINAL), DARK.surface, DARK.text),
    ("Maps", TileGlyph::Icon(ICON_MAP), DARK.surface, DARK.text),
    ("Mail", TileGlyph::Icon(ICON_MAIL), DARK.surface, DARK.text),
    ("Contacts", TileGlyph::Icon(ICON_CONTACTS), DARK.surface, DARK.text),
    ("Store", TileGlyph::Icon(ICON_STORE), DARK.surface, DARK.text),
    ("Podcasts", TileGlyph::Icon(ICON_PODCASTS), DARK.surface, DARK.text),
    ("Videos", TileGlyph::Icon(ICON_MOVIE), DARK.surface, DARK.text),
];

/// The four most recent apps: label, glyph.
const RECENT: [(&str, TileGlyph); 4] = [
    ("Phone", TileGlyph::Icon(icons::PHONE)),
    ("Chrome", TileGlyph::Letter("C")),
    ("Notes", TileGlyph::Icon(ICON_EDIT_NOTE)),
    ("Camera", TileGlyph::Icon(icons::PHOTO_CAMERA)),
];

pub fn build(size: Size) -> Scene {
    let f = Frame::new(size);
    let mut scene = Scene::new(size);
    let mut layer = Layer::new(LAYER);

    // Full bleed: the first op always covers the whole output.
    layer.rect(Rect::new(0.0, 0.0, f.w, f.h), 0.0, DARK.bg);

    // The drawer itself is a full-screen canvas sheet.
    layer.rect(Rect::new(0.0, 0.0, f.w, f.h), 0.0, DARK.canvas);

    // Search field: copper magnifier icon and a faint placeholder.
    layer.rect(
        f.rect(24.0, 40.0, 672.0, 72.0),
        f.len(DARK.radius_pill),
        DARK.surface,
    );
    layer.icon(
        f.at(66.0, 76.0),
        f.len(30.0),
        icons::SEARCH,
        DARK.accent,
        false,
    );
    layer.text(
        f.at(100.0, 60.0),
        "Search apps",
        TextStyle::new(
            f.len(DARK.body_px),
            Weight::Regular,
            DARK.text_faint,
            520.0 * f.sx,
        ),
    );

    // Recent row: four 72 px tiles on the grid columns.
    for (index, (label, glyph)) in RECENT.iter().enumerate() {
        let cell_x = 24.0 + 168.0 * index as f32;
        let tile = f.rect(cell_x + 48.0, 148.0, 72.0, 72.0);
        layer.rect(tile, f.len(DARK.radius_l), DARK.surface_high);
        match glyph {
            TileGlyph::Icon(codepoint) => {
                layer.icon(
                    f.at(cell_x + 84.0, 184.0),
                    f.len(34.0),
                    *codepoint,
                    DARK.text_dim,
                    false,
                );
            }
            TileGlyph::Letter(letter) => {
                centred(
                    &mut layer,
                    &f,
                    cell_x + 84.0,
                    148.0 + (72.0 - DARK.label_px * 1.25) / 2.0,
                    letter,
                    TextStyle::new(
                        f.len(DARK.label_px),
                        Weight::SemiBold,
                        DARK.text_dim,
                        tile.w,
                    ),
                );
            }
        }
        centred(
            &mut layer,
            &f,
            cell_x + 84.0,
            228.0,
            label,
            TextStyle::new(
                f.len(DARK.caption_px),
                Weight::Regular,
                DARK.text_faint,
                168.0 * f.sx,
            ),
        );
    }

    // Section label, then the grid.
    layer.text(
        f.at(24.0, 292.0),
        "ALL APPS",
        TextStyle::new(
            f.len(DARK.micro_px),
            Weight::SemiBold,
            DARK.text_faint,
            300.0 * f.sx,
        ),
    );
    layer.rect(f.rect(24.0, 322.0, 672.0, 1.0), 0.0, DARK.line);

    // 4×5 grid: every cell is 168×210, every icon a 96 px surface tile.
    for (index, (label, glyph, fill, ink)) in APPS.iter().enumerate() {
        let cell_x = 24.0 + 168.0 * (index % 4) as f32;
        let cell_y = 348.0 + 210.0 * (index / 4) as f32;
        let tile = f.rect(cell_x + 36.0, cell_y + 37.0, 96.0, 96.0);
        layer.rect(tile, f.len(DARK.radius_l), *fill);
        match glyph {
            TileGlyph::Icon(codepoint) => {
                layer.icon(
                    f.at(cell_x + 84.0, cell_y + 85.0),
                    f.len(42.0),
                    *codepoint,
                    *ink,
                    false,
                );
            }
            TileGlyph::Letter(letter) => {
                centred(
                    &mut layer,
                    &f,
                    cell_x + 84.0,
                    cell_y + 37.0 + (96.0 - DARK.label_px * 1.25) / 2.0,
                    letter,
                    TextStyle::new(f.len(DARK.label_px), Weight::SemiBold, *ink, tile.w),
                );
            }
        }
        centred(
            &mut layer,
            &f,
            cell_x + 84.0,
            cell_y + 145.0,
            label,
            TextStyle::new(
                f.len(DARK.label_px),
                Weight::Regular,
                DARK.text_faint,
                168.0 * f.sx,
            ),
        );
    }

    // Bottom handle: the drawer pulls back down from here.
    layer.rect(
        f.rect(280.0, 1520.0, 160.0, 8.0),
        f.len(DARK.radius_pill),
        DARK.line,
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
    fn scene_is_the_requested_size_with_one_drawer_layer() {
        let size = Size::new(1080, 2400);
        let scene = build(size);
        assert_eq!(scene.size, size);
        assert_eq!(scene.layers.len(), 1);
        assert_eq!(scene.layers[0].name, "drawer");
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
    fn search_section_label_and_handle_are_present() {
        let scene = build(REF);
        find_text(&scene, "Search apps");
        find_text(&scene, "ALL APPS");
        assert_eq!(
            scene.layers[0].ops.last(),
            Some(&DrawOp::Rect {
                rect: Rect::new(280.0, 1520.0, 160.0, 8.0),
                radius: DARK.radius_pill,
                color: DARK.line,
            })
        );
    }

    #[test]
    fn all_twenty_apps_have_a_tile_and_a_label() {
        let scene = build(REF);
        for (label, glyph, _, _) in APPS {
            find_text(&scene, label);
            match glyph {
                TileGlyph::Icon(codepoint) => assert!(
                    icon_ops(&scene).any(|(cp, _, _)| cp == codepoint),
                    "no icon op for {label}"
                ),
                TileGlyph::Letter(letter) => assert!(
                    texts(&scene).any(|(text, _, _)| text == letter),
                    "no letter tile for {label}"
                ),
            }
        }
        for (label, _) in RECENT {
            find_text(&scene, label);
        }
        assert!(icon_ops(&scene).any(|(cp, filled, _)| cp == icons::SEARCH && !filled));

        let mut icons = 0;
        for op in &scene.layers[0].ops {
            if let DrawOp::Rect { rect, .. } = op {
                if (rect.w - 96.0).abs() < 0.01 && (rect.h - 96.0).abs() < 0.01 {
                    icons += 1;
                }
            }
        }
        assert_eq!(icons, 20, "twenty 96 px app tiles");
    }

    #[test]
    fn semantic_tokens_are_used() {
        let used = palette(&build(REF));
        assert!(used.contains(&DARK.accent), "no copper accent");
        assert!(
            texts(&build(REF)).any(|(_, _, style)| style.color == DARK.text_dim),
            "no dim secondary type"
        );
        assert!(
            texts(&build(REF)).any(|(_, _, style)| style.color == DARK.text_faint),
            "no faint labels"
        );
    }

    #[test]
    fn composition_scales_with_the_requested_size() {
        let scene = build(Size::new(1080, 2400));
        match &scene.layers[0].ops[0] {
            DrawOp::Rect { rect, .. } => assert_eq!(*rect, Rect::new(0.0, 0.0, 1080.0, 2400.0)),
            other => panic!("first op must be the background, got {other:?}"),
        }
        let (_, origin, style) = find_text(&scene, "Search apps");
        assert!((style.size - DARK.body_px * 1.5).abs() < 0.01);
        assert!((origin.y - 90.0).abs() < 0.01);
    }
}
