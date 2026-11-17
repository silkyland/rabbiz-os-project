//! P9 power menu sheet: lock, restart, power off, screenshot, emergency.
//!
//! Full scrim, a 720×620 bottom sheet and five 88 px rows. The Power off row
//! carries a copper hold ring. Authored on the 720×1600 reference panel and
//! mapped with `sx`/`sy`/`s`.

use de_api::{icons, Color, Layer, Point, Rect, Scene, Size, TextStyle, Weight};
use de_design::DARK;

const REF_W: f32 = 720.0;
const REF_H: f32 = 1600.0;
const LAYER: &str = "power-menu";
/// Bottom sheet height at reference; it starts at `REF_H - SHEET_H`.
const SHEET_H: f32 = 620.0;
const SHEET_TOP: f32 = REF_H - SHEET_H;
const ROW_H: f32 = 88.0;
const ROWS_TOP: f32 = SHEET_TOP + 96.0;

const ROWS: [&str; 5] = ["Lock", "Restart", "Power off", "Screenshot", "Emergency"];

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

/// A layer plus the reference frame, so the sheet stays declarative.
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

    fn text(&mut self, x: f32, y: f32, text: &str, style: TextStyle) {
        self.layer
            .text(Point::new(self.f.x(x), self.f.y(y)), text, style);
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

    fn finish(self) -> Scene {
        let mut scene = Scene::new(self.size);
        scene.push(self.layer);
        scene
    }
}

pub fn build(size: Size) -> Scene {
    let mut c = Canvas::new(size, LAYER);

    // Plum base, the dim canvas the menu covers, then the scrim.
    c.rect(0.0, 0.0, REF_W, REF_H, 0.0, DARK.bg);
    c.rect(0.0, 0.0, REF_W, REF_H, 0.0, DARK.canvas);
    c.rect(0.0, 0.0, REF_W, REF_H, 0.0, DARK.scrim);

    // Bottom sheet: rounded top corners, squared off at the screen edge.
    c.rect(
        0.0,
        SHEET_TOP,
        REF_W,
        SHEET_H,
        DARK.radius_card,
        DARK.surface,
    );
    c.rect(
        0.0,
        REF_H - DARK.radius_card,
        REF_W,
        DARK.radius_card,
        0.0,
        DARK.surface,
    );

    // Drag handle, then the five rows.
    c.rect(312.0, SHEET_TOP + 24.0, 96.0, 6.0, 3.0, DARK.line);
    rows(&mut c);

    c.finish()
}

fn rows(c: &mut Canvas) {
    let f = c.f;

    for (i, label) in ROWS.iter().enumerate() {
        let top = ROWS_TOP + i as f32 * ROW_H;

        if i > 0 {
            c.rect(
                DARK.page_margin,
                top - 1.0,
                REF_W - DARK.page_margin * 2.0,
                1.0,
                0.0,
                DARK.line,
            );
        }

        let icon_ink = if *label == "Emergency" {
            DARK.danger
        } else {
            DARK.text_dim
        };
        let codepoint = match i {
            0 => icons::LOCK,
            1 => icons::RESTART_ALT,
            2 => icons::POWER_SETTINGS_NEW,
            3 => icons::SCREENSHOT,
            _ => icons::EMERGENCY,
        };
        c.icon(DARK.page_margin + 20.0, top + 44.0, 40.0, codepoint, icon_ink, false);

        let ink = if *label == "Emergency" {
            DARK.danger
        } else {
            DARK.text
        };
        let style = TextStyle::new(f.d(DARK.body_px), Weight::Regular, ink, f.d(320.0));
        c.text(84.0, top + 28.0, label, style);
    }

    // Power off is hold-to-confirm: a copper ring around the power glyph.
    let mid = ROWS_TOP + 2.0 * ROW_H + ROW_H / 2.0;
    c.square(640.0, mid - 28.0, 56.0, DARK.radius_pill, DARK.accent);
    c.square(644.0, mid - 24.0, 48.0, DARK.radius_pill, DARK.surface);
    c.icon(668.0, mid, 28.0, icons::POWER_SETTINGS_NEW, DARK.text, true);
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
    fn scene_is_the_requested_size_with_one_power_menu_layer() {
        let size = Size::new(1080, 2400);
        let scene = build(size);
        assert_eq!(scene.size, size);
        assert_eq!(scene.layers.len(), 1);
        assert_eq!(scene.layers[0].name, "power-menu");
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
    fn five_rows_are_labelled_in_order() {
        let scene = build(REF);
        assert_eq!(texts(&scene), ROWS.to_vec());
    }

    #[test]
    fn sheet_is_720x620_flush_with_the_bottom_edge() {
        let rs = rects(&build(REF));
        // bg, canvas, scrim, sheet.
        assert_eq!(rs[3], (Rect::new(0.0, 980.0, 720.0, 620.0), DARK.surface));
        assert_eq!(DARK.radius_card, 28.0);

        let scaled = rects(&build(Size::new(1080, 2400)));
        assert_eq!(
            scaled[3].0,
            Rect::new(0.0, 1470.0, 1080.0, 930.0),
            "sheet keeps its 620 px height at 1.5×"
        );
    }

    #[test]
    fn hold_ring_is_copper_and_emergency_reads_danger() {
        let scene = build(REF);
        assert!(uses(&scene, DARK.accent), "hold ring must use accent");
        assert!(uses(&scene, DARK.danger), "emergency must use danger");

        let ring = rects(&scene)
            .into_iter()
            .find(|(rect, color)| *color == DARK.accent && rect.w == 56.0);
        let ring = ring.unwrap_or_else(|| panic!("no 56 px copper hold ring"));
        assert_eq!(ring.0, Rect::new(640.0, 1268.0, 56.0, 56.0));
    }
}
