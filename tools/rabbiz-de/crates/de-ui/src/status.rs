//! P5 status bar as a standalone scene: time, privacy dot, connectivity, battery.
//!
//! Authored on the 720×1600 reference panel and mapped onto the requested
//! [`Size`] with `sx`/`sy`/`s`, so the 48 px bar keeps its rhythm on 1080×2400.

use de_api::{icons, Color, Layer, Point, Rect, Scene, Size, TextAlign, TextStyle, Weight};
use de_design::DARK;

const REF_W: f32 = 720.0;
const REF_H: f32 = 1600.0;
const LAYER: &str = "status";
/// Material Symbols Rounded `battery_3_bar` (38% charge).
const BATTERY_3_BAR: char = '\u{f09e}';

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

    /// Layout rect: axis-scaled so the bar fills the panel edge to edge.
    fn rect(self, x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect::new(self.x(x), self.y(y), w * self.sx, h * self.sy)
    }

    fn square(self, x: f32, y: f32, side: f32) -> Rect {
        Rect::new(self.x(x), self.y(y), side * self.s, side * self.s)
    }
}

/// A layer plus the reference frame, so scenes stay declarative.
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

pub fn build(size: Size) -> Scene {
    let mut c = Canvas::new(size, LAYER);

    // Plum bar on top, the canvas it floats above, and a hairline seam.
    c.rect(0.0, 0.0, REF_W, REF_H, 0.0, DARK.bg);
    c.rect(
        0.0,
        DARK.status_h,
        REF_W,
        REF_H - DARK.status_h,
        0.0,
        DARK.canvas,
    );
    c.rect(0.0, DARK.status_h, REF_W, 1.0, 0.0, DARK.line);

    bar(&mut c);
    hint(&mut c);
    c.finish()
}

/// Time on the left; privacy dot, signal, Wi-Fi, Bluetooth and battery right.
fn bar(c: &mut Canvas) {
    let f = c.f;
    let mid = DARK.status_h / 2.0;

    let time = TextStyle::new(f.d(DARK.label_px), Weight::SemiBold, DARK.text, f.d(120.0));
    c.text(DARK.page_margin, mid - DARK.label_px * 0.625, "09:41", time);

    let percent = TextStyle::new(
        f.d(DARK.micro_px),
        Weight::Regular,
        DARK.text_dim,
        f.d(52.0),
    );
    c.text(644.0, mid - DARK.micro_px * 0.625, "38%", percent);

    // A camera or microphone is live: one soft green dot.
    c.square(484.0, mid - 4.0, 8.0, 4.0, DARK.success);

    let glyph = DARK.label_px;
    c.icon(
        504.0 + glyph / 2.0,
        mid,
        glyph,
        icons::SIGNAL_CELLULAR_ALT,
        DARK.text_dim,
        true,
    );
    c.icon(
        538.0 + glyph / 2.0,
        mid,
        glyph,
        icons::WIFI,
        DARK.text_dim,
        true,
    );
    c.icon(
        572.0 + glyph / 2.0,
        mid,
        glyph,
        icons::BLUETOOTH,
        DARK.text_dim,
        true,
    );

    let battery_side = 30.0;
    c.icon(
        606.0 + battery_side / 2.0,
        mid,
        battery_side,
        BATTERY_3_BAR,
        DARK.text_dim,
        true,
    );
}

/// The scene is a spec plate, so one faint line names what it shows.
fn hint(c: &mut Canvas) {
    let f = c.f;
    let style = TextStyle::new(
        f.d(DARK.caption_px),
        Weight::Regular,
        DARK.text_faint,
        f.d(REF_W - DARK.page_margin * 2.0),
    );
    c.centered(REF_W / 2.0, 800.0, "status bar · 48px", style);
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
    fn scene_is_the_requested_size_with_one_status_layer() {
        let size = Size::new(1080, 2400);
        let scene = build(size);
        assert_eq!(scene.size, size);
        assert_eq!(scene.layers.len(), 1);
        assert_eq!(scene.layers[0].name, "status");
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
    fn bar_shows_time_battery_and_the_reference_hint() {
        let scene = build(REF);
        let all = texts(&scene);
        for needle in ["09:41", "38%", "status bar · 48px"] {
            assert!(all.contains(&needle), "missing text op {needle:?}");
        }
    }

    #[test]
    fn canvas_band_starts_below_the_48px_bar() {
        let scene = build(Size::new(1080, 2400));
        let rs = rects(&scene);
        // Second op is the canvas band: 48 px at reference → 72 px at 1.5×.
        assert_eq!(rs[1].0, Rect::new(0.0, 72.0, 1080.0, 2328.0));
        assert_eq!(rs[1].1, DARK.canvas);
        assert_eq!(DARK.status_h, 48.0);
    }

    #[test]
    fn privacy_dot_and_glyphs_use_semantic_tokens() {
        let scene = build(REF);
        assert!(uses(&scene, DARK.success), "privacy dot must use success");
        assert!(uses(&scene, DARK.text_dim), "glyphs must use text_dim");
        assert!(uses(&scene, DARK.text_faint), "hint must use text_faint");
    }
}
