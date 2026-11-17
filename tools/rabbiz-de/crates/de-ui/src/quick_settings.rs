//! P7 expanded quick settings grid.
//!
//! A scrim over the dimmed canvas carries a 4×4 grid of 72 px circles with
//! labels and an Edit pill at the bottom. Authored on the 720×1600 reference
//! panel and mapped with `sx`/`sy`/`s`.

use de_api::{icons, Color, Layer, Point, Rect, Scene, Size, TextAlign, TextStyle, Weight};
use de_design::DARK;

const REF_W: f32 = 720.0;
const REF_H: f32 = 1600.0;
const LAYER: &str = "quick-settings";

// Material Symbols Rounded codepoints not in `de_api::icons`.
const BATTERY_SAVER: char = '\u{efde}';
const WIFI_TETHERING: char = '\u{e1e2}';
const CAST: char = '\u{e307}';
const LOCATION_ON: char = '\u{f1db}';
const NFC: char = '\u{e1bb}';
const BRIGHTNESS_AUTO: char = '\u{e1ab}';

/// Grid: four 168 px columns starting at x=108, 170 px row pitch.
const GRID_TOP: f32 = 470.0;
const ROW_PITCH: f32 = 170.0;
const COL_PITCH: f32 = 168.0;
const FIRST_CX: f32 = 108.0;
const CIRCLE: f32 = 72.0;

/// Reference geometry projected onto the requested output size.
#[derive(Clone, Copy)]
struct Frame {
    sx: f32,
    sy: f32,
    s: f32,
}

impl Frame {
    fn new(size: Size) -> Self {
        let sx = size.w as f32 / REF_W;
        let sy = size.h as f32 / REF_H;
        Self {
            sx,
            sy,
            s: sx.min(sy),
        }
    }

    fn x(self, v: f32) -> f32 {
        v * self.sx
    }

    fn y(self, v: f32) -> f32 {
        v * self.sy
    }

    /// A length that keeps its aspect ratio on odd panels: radii, type, icons.
    fn d(self, v: f32) -> f32 {
        v * self.s
    }

    fn rect(self, x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect::new(self.x(x), self.y(y), w * self.sx, h * self.sy)
    }

    fn square(self, x: f32, y: f32, side: f32) -> Rect {
        Rect::new(self.x(x), self.y(y), side * self.s, side * self.s)
    }
}

/// A layer plus the reference frame, so the grid stays declarative.
struct Canvas {
    size: Size,
    f: Frame,
    layer: Layer,
}

impl Canvas {
    fn new(size: Size, name: &str) -> Self {
        Self {
            size,
            f: Frame::new(size),
            layer: Layer::new(name),
        }
    }

    fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, radius: f32, color: Color) {
        self.layer
            .rect(self.f.rect(x, y, w, h), self.f.d(radius), color);
    }

    fn square(&mut self, x: f32, y: f32, side: f32, radius: f32, color: Color) {
        self.layer
            .rect(self.f.square(x, y, side), self.f.d(radius), color);
    }

    fn icon(&mut self, cx: f32, cy: f32, size: f32, codepoint: char, color: Color, filled: bool) {
        self.layer.icon(
            Point::new(self.f.x(cx), self.f.y(cy)),
            self.f.d(size),
            codepoint,
            color,
            filled,
        );
    }

    /// One line of type centred on the reference point (`cx`, `cy`).
    fn centered(&mut self, cx: f32, cy: f32, text: &str, style: TextStyle) {
        let style = style.with_align(TextAlign::Center);
        let x = self.f.x(cx) - style.max_width / 2.0;
        let y = self.f.y(cy) - style.size * 0.625;
        self.layer.text(Point::new(x, y), text, style);
    }

    fn finish(self) -> Scene {
        let mut scene = Scene::new(self.size);
        scene.push(self.layer);
        scene
    }
}

#[derive(Clone, Copy)]
enum Icon {
    Wifi,
    Bluetooth,
    Dnd,
    Flashlight,
    Airplane,
    Rotation,
    BatterySaver,
    Hotspot,
    Screenshot,
    NightLight,
    Cast,
    Location,
    MobileData,
    Nfc,
    AutoBrightness,
    Settings,
}

#[derive(Clone, Copy)]
struct Tile {
    label: &'static str,
    on: bool,
    icon: Icon,
}

/// The 4×4 grid in row-major order: label, state, glyph.
const TILES: [Tile; 16] = [
    Tile {
        label: "Wi-Fi",
        on: true,
        icon: Icon::Wifi,
    },
    Tile {
        label: "Bluetooth",
        on: true,
        icon: Icon::Bluetooth,
    },
    Tile {
        label: "DND",
        on: false,
        icon: Icon::Dnd,
    },
    Tile {
        label: "Flashlight",
        on: false,
        icon: Icon::Flashlight,
    },
    Tile {
        label: "Airplane",
        on: false,
        icon: Icon::Airplane,
    },
    Tile {
        label: "Rotation",
        on: true,
        icon: Icon::Rotation,
    },
    Tile {
        label: "Battery saver",
        on: false,
        icon: Icon::BatterySaver,
    },
    Tile {
        label: "Hotspot",
        on: false,
        icon: Icon::Hotspot,
    },
    Tile {
        label: "Screenshot",
        on: false,
        icon: Icon::Screenshot,
    },
    Tile {
        label: "Night light",
        on: true,
        icon: Icon::NightLight,
    },
    Tile {
        label: "Cast",
        on: false,
        icon: Icon::Cast,
    },
    Tile {
        label: "Location",
        on: true,
        icon: Icon::Location,
    },
    Tile {
        label: "Mobile data",
        on: true,
        icon: Icon::MobileData,
    },
    Tile {
        label: "NFC",
        on: false,
        icon: Icon::Nfc,
    },
    Tile {
        label: "Auto-brightness",
        on: true,
        icon: Icon::AutoBrightness,
    },
    Tile {
        label: "Settings",
        on: false,
        icon: Icon::Settings,
    },
];

pub fn build(size: Size) -> Scene {
    let mut c = Canvas::new(size, LAYER);

    // Plum base, the dim canvas, then the scrim the grid floats on.
    c.rect(0.0, 0.0, REF_W, REF_H, 0.0, DARK.bg);
    c.rect(0.0, 0.0, REF_W, REF_H, 0.0, DARK.canvas);
    c.rect(0.0, 0.0, REF_W, REF_H, 0.0, DARK.scrim);

    grid(&mut c);
    edit(&mut c);
    c.finish()
}

fn grid(c: &mut Canvas) {
    let f = c.f;
    let label = TextStyle::new(
        f.d(DARK.micro_px),
        Weight::Regular,
        DARK.text_dim,
        f.d(160.0),
    );

    for (i, tile) in TILES.iter().enumerate() {
        let cx = FIRST_CX + (i % 4) as f32 * COL_PITCH;
        let top = GRID_TOP + (i / 4) as f32 * ROW_PITCH;

        let fill = if tile.on {
            DARK.accent
        } else {
            DARK.surface_high
        };
        let ink = if tile.on { DARK.accent_ink } else { DARK.text };

        c.square(cx - CIRCLE / 2.0, top, CIRCLE, DARK.radius_pill, fill);
        c.icon(cx, top + 36.0, 36.0, codepoint(tile.icon), ink, tile.on);

        c.centered(cx, top + 90.0, tile.label, label);
    }
}

fn edit(c: &mut Canvas) {
    let f = c.f;
    c.rect(288.0, 1460.0, 144.0, 52.0, DARK.radius_pill, DARK.accent);
    let style = TextStyle::new(
        f.d(DARK.label_px),
        Weight::SemiBold,
        DARK.accent_ink,
        f.d(144.0),
    );
    c.centered(360.0, 1486.0, "Edit", style);
}

fn codepoint(icon: Icon) -> char {
    match icon {
        Icon::Wifi => icons::WIFI,
        Icon::Bluetooth => icons::BLUETOOTH,
        Icon::Dnd => icons::DO_NOT_DISTURB_ON,
        Icon::Flashlight => icons::FLASHLIGHT_ON,
        Icon::Airplane => icons::AIRPLANEMODE_ACTIVE,
        Icon::Rotation => icons::SCREEN_ROTATION,
        Icon::BatterySaver => BATTERY_SAVER,
        Icon::Hotspot => WIFI_TETHERING,
        Icon::Screenshot => icons::SCREENSHOT,
        Icon::NightLight => icons::NIGHTLIGHT,
        Icon::Cast => CAST,
        Icon::Location => LOCATION_ON,
        Icon::MobileData => icons::SIGNAL_CELLULAR_ALT,
        Icon::Nfc => NFC,
        Icon::AutoBrightness => BRIGHTNESS_AUTO,
        Icon::Settings => icons::SETTINGS,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use de_api::DrawOp;

    const REF: Size = Size::new(720, 1600);

    fn texts(scene: &Scene) -> Vec<&str> {
        scene
            .layers
            .iter()
            .flat_map(|layer| layer.ops.iter())
            .filter_map(|op| match op {
                DrawOp::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect()
    }

    fn rects(scene: &Scene) -> Vec<(Rect, Color)> {
        scene.layers[0]
            .ops
            .iter()
            .filter_map(|op| match op {
                DrawOp::Rect { rect, color, .. } => Some((*rect, *color)),
                _ => None,
            })
            .collect()
    }

    fn uses(scene: &Scene, color: Color) -> bool {
        scene.layers[0].ops.iter().any(|op| match op {
            DrawOp::Rect { color: c, .. } | DrawOp::Icon { color: c, .. } => *c == color,
            DrawOp::Text { style, .. } => style.color == color,
        })
    }

    #[test]
    fn scene_is_the_requested_size_with_one_quick_settings_layer() {
        let size = Size::new(1080, 2400);
        let scene = build(size);
        assert_eq!(scene.size, size);
        assert_eq!(scene.layers.len(), 1);
        assert_eq!(scene.layers[0].name, "quick-settings");
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
    fn grid_labels_and_edit_are_present() {
        let scene = build(REF);
        let all = texts(&scene);
        assert_eq!(all.len(), 17, "sixteen labels plus the Edit pill");
        for needle in [
            "Wi-Fi",
            "Bluetooth",
            "Battery saver",
            "Auto-brightness",
            "Settings",
            "Edit",
        ] {
            assert!(all.contains(&needle), "missing text op {needle:?}");
        }
    }

    #[test]
    fn sixteen_72px_circles_land_on_the_grid() {
        let scene = build(Size::new(1080, 2400));
        let circles = rects(&scene)
            .iter()
            .filter(|(rect, _)| rect.w == 108.0 && rect.h == 108.0)
            .count();
        assert_eq!(circles, 16);
    }

    #[test]
    fn active_tiles_use_copper_and_labels_use_text_dim() {
        let scene = build(REF);
        assert!(uses(&scene, DARK.accent), "active circles must use accent");
        assert!(
            uses(&scene, DARK.accent_ink),
            "active glyphs need accent ink"
        );
        assert!(uses(&scene, DARK.text_dim), "labels must use text_dim");
    }

    #[test]
    fn scrim_covers_the_dim_canvas() {
        let rs = rects(&build(REF));
        assert_eq!(rs[1].1, DARK.canvas);
        assert_eq!(rs[2], (Rect::new(0.0, 0.0, 720.0, 1600.0), DARK.scrim));
    }
}
