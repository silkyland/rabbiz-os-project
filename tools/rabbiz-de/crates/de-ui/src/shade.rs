//! P6 notification shade: header, quick tiles, sliders, media, grouped notifications.
//!
//! The sheet is pulled from the top edge over the dimmed canvas. It is authored
//! on the 720×1600 reference panel and mapped with `sx`/`sy`/`s`.

use de_api::{icons, Color, Layer, Point, Rect, Scene, Size, TextAlign, TextStyle, Weight};
use de_design::DARK;

const REF_W: f32 = 720.0;
const REF_H: f32 = 1600.0;
const LAYER: &str = "shade";
/// Sheet height at reference: the last op is the footer handle.
const SHEET_H: f32 = 1320.0;
const CONTENT_W: f32 = REF_W - DARK.page_margin * 2.0;

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

    // Plum base, the dim canvas the sheet covers, then the scrim.
    c.rect(0.0, 0.0, REF_W, REF_H, 0.0, DARK.bg);
    c.rect(0.0, 0.0, REF_W, REF_H, 0.0, DARK.canvas);
    c.rect(0.0, 0.0, REF_W, REF_H, 0.0, DARK.scrim);
    c.rect(0.0, 0.0, REF_W, SHEET_H, 0.0, DARK.surface);

    header(&mut c);
    tiles(&mut c);
    toggles(&mut c);
    sliders(&mut c);
    media(&mut c);
    notifications(&mut c);
    footer(&mut c);

    c.finish()
}

/// 96 px header: date, then settings and power circles.
fn header(c: &mut Canvas) {
    let f = c.f;
    let mid = 48.0;

    let date = TextStyle::new(f.d(DARK.h2_px), Weight::SemiBold, DARK.text, f.d(420.0));
    c.text(
        DARK.page_margin,
        mid - DARK.h2_px * 0.625,
        "Wed, 17 Nov",
        date,
    );

    c.square(588.0, 24.0, 48.0, DARK.radius_pill, DARK.surface_high);
    c.icon(612.0, 48.0, 36.0, icons::SETTINGS, DARK.text, false);

    c.square(648.0, 24.0, 48.0, DARK.radius_pill, DARK.surface_high);
    c.icon(672.0, 48.0, 36.0, icons::POWER_SETTINGS_NEW, DARK.text, false);
}

/// Two 328×112 connectivity tiles, Internet on and Bluetooth off.
fn tiles(c: &mut Canvas) {
    let f = c.f;
    let top = 120.0;
    let tile_w = 328.0;
    let tile_h = 112.0;
    let gap = 16.0;
    let x1 = DARK.page_margin;
    let x2 = x1 + tile_w + gap;

    c.rect(x1, top, tile_w, tile_h, DARK.radius_l, DARK.surface_high);
    c.rect(x2, top, tile_w, tile_h, DARK.radius_l, DARK.surface_high);

    c.icon(
        x1 + DARK.space_m + 18.0,
        top + 56.0,
        36.0,
        icons::WIFI,
        DARK.accent,
        true,
    );
    c.icon(
        x2 + DARK.space_m + 18.0,
        top + 56.0,
        36.0,
        icons::BLUETOOTH,
        DARK.text_dim,
        false,
    );

    let title = TextStyle::new(f.d(DARK.label_px), Weight::SemiBold, DARK.text, f.d(220.0));
    let sub = TextStyle::new(
        f.d(DARK.caption_px),
        Weight::Regular,
        DARK.text_dim,
        f.d(220.0),
    );
    c.text(x1 + 92.0, 150.0, "Internet", title);
    c.text(x1 + 92.0, 180.0, "Rabbiz 5G", sub);
    c.text(x2 + 92.0, 150.0, "Bluetooth", title);
    c.text(x2 + 92.0, 180.0, "Off", sub);
}

/// Four 88 px toggles with labels: Screenshot, Flashlight, DND, Rotate.
fn toggles(c: &mut Canvas) {
    let f = c.f;
    let xs = [108.0, 276.0, 444.0, 612.0];
    let top = 248.0;
    let side = 88.0;

    for x in xs {
        c.square(
            x - side / 2.0,
            top,
            side,
            DARK.radius_pill,
            DARK.surface_high,
        );
    }

    // 40 px glyphs sit on the 88 px circle centres (top + 44).
    let cy = top + 44.0;
    let glyph_codes = [
        icons::SCREENSHOT,
        icons::FLASHLIGHT_ON,
        icons::DO_NOT_DISTURB_ON,
        icons::SCREEN_ROTATION,
    ];
    for (x, code) in xs.into_iter().zip(glyph_codes) {
        c.icon(x, cy, 40.0, code, DARK.text, false);
    }

    let label = TextStyle::new(
        f.d(DARK.micro_px),
        Weight::Regular,
        DARK.text_dim,
        f.d(150.0),
    );
    for (x, text) in xs
        .into_iter()
        .zip(["Screenshot", "Flashlight", "DND", "Rotate"])
    {
        c.centered(x, 356.0, text, label);
    }
}

/// Two chunky vertical sliders, brightness and volume.
fn sliders(c: &mut Canvas) {
    let f = c.f;
    slider(c, 264.0, 392.0, 0.72, DARK.text, icons::BRIGHTNESS_HIGH);
    slider(c, 376.0, 392.0, 0.45, DARK.accent, icons::VOLUME_UP);

    let label = TextStyle::new(
        f.d(DARK.micro_px),
        Weight::Regular,
        DARK.text_dim,
        f.d(160.0),
    );
    c.centered(304.0, 772.0, "Brightness", label);
    c.centered(416.0, 772.0, "Volume", label);
}

fn slider(c: &mut Canvas, x: f32, y: f32, level: f32, fill: Color, codepoint: char) {
    let w = 80.0;
    let h = 360.0;

    c.rect(x, y, w, h, DARK.radius_pill, DARK.surface_high);
    let filled = h * level;
    c.rect(x, y + h - filled, w, filled, DARK.radius_pill, fill);
    c.icon(x + 40.0, y + 44.0, 32.0, codepoint, DARK.text_dim, true);
}

/// Now-playing card 672×160: art, title, progress and a pause button.
fn media(c: &mut Canvas) {
    let f = c.f;
    let top = 816.0;
    c.rect(
        DARK.page_margin,
        top,
        CONTENT_W,
        160.0,
        DARK.radius_card,
        DARK.surface_high,
    );

    // Abstract cover: three copper bars on the mascot's body plum.
    c.rect(40.0, top + 16.0, 96.0, 96.0, DARK.radius_m, DARK.mark_body);
    for (i, h) in [18.0, 30.0, 22.0].into_iter().enumerate() {
        c.rect(
            78.0 + i as f32 * 8.0,
            top + 64.0 - h / 2.0,
            4.0,
            h,
            2.0,
            DARK.accent,
        );
    }

    let title = TextStyle::new(f.d(DARK.label_px), Weight::SemiBold, DARK.text, f.d(340.0));
    c.text(152.0, top + 30.0, "Rabbiz Radio", title);
    let sub = TextStyle::new(
        f.d(DARK.caption_px),
        Weight::Regular,
        DARK.text_dim,
        f.d(340.0),
    );
    c.text(152.0, top + 60.0, "The morning show", sub);

    c.rect(152.0, top + 102.0, 440.0, 4.0, 2.0, DARK.surface_low);
    c.rect(152.0, top + 102.0, 180.0, 4.0, 2.0, DARK.accent);

    c.square(632.0, top + 56.0, 48.0, DARK.radius_pill, DARK.accent);
    c.icon(656.0, top + 80.0, 24.0, icons::PAUSE, DARK.accent_ink, true);
}

fn notifications(c: &mut Canvas) {
    notification(
        c,
        1000.0,
        DARK.accent,
        "M",
        "Rabbiz",
        "Build is green — screens landed.",
        "now",
    );
    notification(
        c,
        1092.0,
        DARK.success,
        "C",
        "Design review",
        "14:30 · Studio",
        "12m",
    );
}

fn notification(
    c: &mut Canvas,
    top: f32,
    ink: Color,
    initial: &str,
    title: &str,
    body: &str,
    time: &str,
) {
    let f = c.f;
    c.rect(
        DARK.page_margin,
        top,
        CONTENT_W,
        80.0,
        DARK.radius_m,
        DARK.surface_high,
    );

    // App badge: a filled circle with the app initial.
    c.square(40.0, top + 20.0, 40.0, DARK.radius_pill, ink);
    let badge = TextStyle::new(
        f.d(DARK.micro_px),
        Weight::SemiBold,
        DARK.accent_ink,
        f.d(40.0),
    );
    c.centered(60.0, top + 40.0, initial, badge);

    let title_style = TextStyle::new(f.d(DARK.label_px), Weight::SemiBold, DARK.text, f.d(400.0));
    c.text(92.0, top + 10.0, title, title_style);
    let body_style = TextStyle::new(
        f.d(DARK.caption_px),
        Weight::Regular,
        DARK.text_dim,
        f.d(420.0),
    );
    c.text(92.0, top + 38.0, body, body_style);

    let time_style = TextStyle::new(
        f.d(DARK.micro_px),
        Weight::Regular,
        DARK.text_faint,
        f.d(80.0),
    );
    c.centered(640.0, top + 20.0, time, time_style);
}

/// Clear-all pill and the drag handle at the sheet footer.
fn footer(c: &mut Canvas) {
    let f = c.f;
    c.rect(
        280.0,
        1196.0,
        160.0,
        50.0,
        DARK.radius_pill,
        DARK.surface_high,
    );
    let style = TextStyle::new(
        f.d(DARK.label_px),
        Weight::Regular,
        DARK.text_dim,
        f.d(160.0),
    );
    c.centered(360.0, 1221.0, "Clear all", style);

    c.rect(312.0, 1276.0, 96.0, 6.0, 3.0, DARK.line);
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
    fn scene_is_the_requested_size_with_one_shade_layer() {
        let size = Size::new(1080, 2400);
        let scene = build(size);
        assert_eq!(scene.size, size);
        assert_eq!(scene.layers.len(), 1);
        assert_eq!(scene.layers[0].name, "shade");
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
    fn sheet_carries_header_tiles_media_and_actions() {
        let scene = build(REF);
        let all = texts(&scene);
        for needle in [
            "Wed, 17 Nov",
            "Internet",
            "Bluetooth",
            "Rabbiz Radio",
            "Brightness",
            "Volume",
            "Clear all",
        ] {
            assert!(all.contains(&needle), "missing text op {needle:?}");
        }
    }

    #[test]
    fn scrim_covers_the_dim_canvas() {
        let rs = rects(&build(REF));
        assert_eq!(rs[1].1, DARK.canvas);
        assert_eq!(rs[2], (Rect::new(0.0, 0.0, 720.0, 1600.0), DARK.scrim));
        assert_eq!(rs[3], (Rect::new(0.0, 0.0, 720.0, 1320.0), DARK.surface));
    }

    #[test]
    fn sheet_tiles_and_notifications_use_semantic_tokens() {
        let scene = build(REF);
        assert!(uses(&scene, DARK.accent), "copper accents missing");
        assert!(uses(&scene, DARK.text_dim), "quiet ink missing");
        assert!(uses(&scene, DARK.success), "notification badge ink missing");
    }

    #[test]
    fn sheet_and_tiles_scale_with_the_panel() {
        let scene = build(Size::new(1080, 2400));
        let rs = rects(&scene);
        // The sheet is the fourth op (bg, canvas, scrim, sheet), 1320 → 1980.
        assert_eq!(rs[3].0, Rect::new(0.0, 0.0, 1080.0, 1980.0));
        // Two 328×112 connectivity tiles at 1.5×.
        let tiles = rs
            .iter()
            .filter(|(rect, _)| rect.w == 492.0 && rect.h == 168.0)
            .count();
        assert_eq!(tiles, 2);
    }
}
