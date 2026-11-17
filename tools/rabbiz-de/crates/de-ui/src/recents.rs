//! P4 recents: three stacked app cards and the clear-all pill.
//!
//! The active card is opaque; its two neighbours drop to 60 % so depth reads
//! without a shadow op. Authored on 720×1600, scaled like every other screen,
//! and drawn from [`DARK`] tokens only.

use de_api::{Color, Layer, Point, Rect, Scene, Size, TextAlign, TextStyle, Weight};
use de_design::DARK;

/// Reference panel the composition was authored on.
const REF_W: f32 = 720.0;
const REF_H: f32 = 1600.0;

/// A recents stack is one layer, named for the screen.
const LAYER: &str = "recents";

/// Neighbours sit at 60 % to push the active card forward.
const NEIGHBOUR_ALPHA: u8 = 153;

/// Card height and the vertical rhythm of the stack.
const CARD_H: f32 = 360.0;
const CARD_Y: [f32; 3] = [180.0, 580.0, 980.0];

/// Codepoints verified against `assets/fonts/MaterialSymbolsRounded.codepoints`.
const ICON_CHAT: char = '\u{e0c9}';
const ICON_EDIT_NOTE: char = '\u{e745}';

#[derive(Clone, Copy)]
enum TileGlyph {
    Icon(char),
    Letter(&'static str),
}

/// The three windows, oldest first; the last one is the active card.
const CARDS: [Card; 3] = [
    Card {
        title: "Notes",
        subtitle: "Edited · 2m",
        glyph: TileGlyph::Icon(ICON_EDIT_NOTE),
        y: CARD_Y[0],
        active: false,
    },
    Card {
        title: "Chrome",
        subtitle: "3 tabs · rabbiz.os",
        glyph: TileGlyph::Letter("C"),
        y: CARD_Y[1],
        active: false,
    },
    Card {
        title: "Messages",
        subtitle: "Nok · now",
        glyph: TileGlyph::Icon(ICON_CHAT),
        y: CARD_Y[2],
        active: true,
    },
];

struct Card {
    title: &'static str,
    subtitle: &'static str,
    glyph: TileGlyph,
    y: f32,
    active: bool,
}

pub fn build(size: Size) -> Scene {
    let f = Frame::new(size);
    let mut scene = Scene::new(size);
    let mut layer = Layer::new(LAYER);

    // Full bleed: the first op always covers the whole output.
    layer.rect(Rect::new(0.0, 0.0, f.w, f.h), 0.0, DARK.bg);

    // Header: the stack has a name and one way out.
    layer.text(
        f.at(24.0, 56.0),
        "Recents",
        TextStyle::new(
            f.len(DARK.title_px),
            Weight::SemiBold,
            DARK.text,
            500.0 * f.sx,
        ),
    );
    layer.text(
        f.at(24.0, 108.0),
        "Swipe up to close",
        TextStyle::new(
            f.len(DARK.caption_px),
            Weight::Regular,
            DARK.text_dim,
            500.0 * f.sx,
        ),
    );

    for card in CARDS {
        let tone = |color: Color| {
            if card.active {
                color
            } else {
                color.with_alpha(NEIGHBOUR_ALPHA)
            }
        };
        let icon_fill = if card.active {
            DARK.accent
        } else {
            DARK.surface_high
        };
        let icon_ink = if card.active {
            DARK.accent_ink
        } else {
            DARK.text_dim
        };
        let title_ink = if card.active {
            DARK.text
        } else {
            DARK.text_dim
        };

        // Card ground, app icon, and two lines of identity.
        layer.rect(
            f.rect(24.0, card.y, 672.0, CARD_H),
            f.len(DARK.radius_card),
            tone(DARK.surface),
        );
        let icon = f.rect(48.0, card.y + 24.0, 64.0, 64.0);
        layer.rect(icon, f.len(DARK.radius_m), tone(icon_fill));
        match card.glyph {
            TileGlyph::Icon(codepoint) => {
                layer.icon(
                    f.at(80.0, card.y + 56.0),
                    f.len(32.0),
                    codepoint,
                    tone(icon_ink),
                    card.active,
                );
            }
            TileGlyph::Letter(letter) => {
                centred(
                    &mut layer,
                    &f,
                    80.0,
                    card.y + 24.0 + (64.0 - DARK.label_px * 1.25) / 2.0,
                    letter,
                    TextStyle::new(
                        f.len(DARK.label_px),
                        Weight::SemiBold,
                        tone(icon_ink),
                        icon.w,
                    ),
                );
            }
        }
        layer.text(
            f.at(128.0, card.y + 20.0),
            card.title,
            TextStyle::new(
                f.len(DARK.h2_px),
                Weight::SemiBold,
                tone(title_ink),
                460.0 * f.sx,
            ),
        );
        layer.text(
            f.at(128.0, card.y + 64.0),
            card.subtitle,
            TextStyle::new(
                f.len(DARK.caption_px),
                Weight::Regular,
                tone(DARK.text_dim),
                460.0 * f.sx,
            ),
        );

        // Rounded content area with a quiet window sketch rather than pixels.
        layer.rect(
            f.rect(48.0, card.y + 112.0, 624.0, 224.0),
            f.len(DARK.radius_m),
            tone(DARK.surface_low),
        );
        layer.rect(
            f.rect(72.0, card.y + 136.0, 240.0, 16.0),
            f.len(8.0),
            tone(DARK.surface_high),
        );
        for (index, width) in [520.0, 440.0, 480.0, 360.0].iter().enumerate() {
            layer.rect(
                f.rect(72.0, card.y + 172.0 + 24.0 * index as f32, *width, 12.0),
                f.len(6.0),
                tone(DARK.surface_high),
            );
        }
    }

    // Clear-all: the only action on the surface.
    layer.rect(
        f.rect(220.0, 1456.0, 280.0, 72.0),
        f.len(DARK.radius_pill),
        DARK.surface_high,
    );
    centred(
        &mut layer,
        &f,
        360.0,
        1476.0,
        "Clear all",
        TextStyle::new(
            f.len(DARK.body_px),
            Weight::SemiBold,
            DARK.accent,
            280.0 * f.sx,
        ),
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
    fn scene_is_the_requested_size_with_one_recents_layer() {
        let size = Size::new(1080, 2400);
        let scene = build(size);
        assert_eq!(scene.size, size);
        assert_eq!(scene.layers.len(), 1);
        assert_eq!(scene.layers[0].name, "recents");
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
    fn three_cards_stack_with_the_active_one_opaque() {
        let scene = build(REF);
        let mut opaque = 0;
        let mut dimmed = 0;
        for op in &scene.layers[0].ops {
            if let DrawOp::Rect { rect, color, .. } = op {
                if (rect.h - CARD_H).abs() < 0.01 {
                    if *color == DARK.surface {
                        opaque += 1;
                    } else if *color == DARK.surface.with_alpha(NEIGHBOUR_ALPHA) {
                        dimmed += 1;
                    }
                }
            }
        }
        assert_eq!(opaque, 1, "the active card is full opacity");
        assert_eq!(dimmed, 2, "two neighbours at 60 %");
    }

    #[test]
    fn every_card_has_an_icon_title_and_content_area() {
        let scene = build(REF);
        let mut icons = 0;
        for op in &scene.layers[0].ops {
            if let DrawOp::Rect { rect, .. } = op {
                if (rect.w - 64.0).abs() < 0.01 && (rect.h - 64.0).abs() < 0.01 {
                    icons += 1;
                }
            }
        }
        assert_eq!(icons, 3, "one 64 px icon per card");

        find_text(&scene, "Recents");
        for card in CARDS {
            find_text(&scene, card.title);
        }
        assert!(icon_ops(&scene).any(|(cp, filled, _)| cp == ICON_EDIT_NOTE && !filled));
        assert!(icon_ops(&scene).any(|(cp, filled, _)| cp == ICON_CHAT && filled));
    }

    #[test]
    fn clear_all_is_the_bottom_action() {
        let scene = build(REF);
        let (_, origin, style) = find_text(&scene, "Clear all");
        assert_eq!(origin.x, 220.0);
        assert_eq!(style.color, DARK.accent);
        assert!(origin.y > CARD_Y[2] + CARD_H, "below the card stack");
    }

    #[test]
    fn semantic_tokens_are_used() {
        let used = palette(&build(REF));
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
        let (_, origin, style) = find_text(&scene, "Clear all");
        assert!((style.size - DARK.body_px * 1.5).abs() < 0.01);
        assert!((origin.y - 1476.0 * 1.5).abs() < 0.01);
    }
}
