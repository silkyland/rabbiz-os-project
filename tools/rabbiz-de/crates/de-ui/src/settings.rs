//! P15 settings: the index and its six detail screens.
//!
//! Every screen is authored on the 720×1600 reference panel and mapped onto the
//! requested output the way `boot` does it: geometry by `sx`/`sy`, radii and
//! type by `s = min(sx, sy)`. Only `de_design::DARK` tokens are used, and every
//! glyph is a rounded rect, so the scenes rasterize without an icon font.
//!
//! Each screen is one layer named after its route and opens with a full-bleed
//! `DARK.bg` rect. The functions return a [`Scene`]; they never draw or read
//! the OS.

use de_api::{Color, Layer, Point, Rect, Scene, Size, TextAlign, TextStyle, Weight};
use de_design::DARK;

/// Reference panel the settings compositions were authored on.
const REF_W: f32 = 720.0;
const REF_H: f32 = 1600.0;
/// Page margin and the content-card width on the reference panel.
const PAGE: f32 = DARK.page_margin;
const CARD: f32 = REF_W - PAGE * 2.0;

/// P15 settings index: a title and eight 88px rows.
pub fn index(size: Size) -> Scene {
    let mut c = Canvas::new(size, "settings");

    let title = c.style(DARK.title_px, Weight::SemiBold, DARK.text, 480.0);
    c.text(PAGE, 84.0, "Settings", title);
    let device = c.style(DARK.label_px, Weight::Regular, DARK.text_dim, 480.0);
    c.text(PAGE, 138.0, "Galaxy A21s · RabbizOS", device);

    c.paint(PAGE, 208.0, CARD, 704.0, DARK.radius_card, DARK.surface);

    const ROWS: [(&str, Icon); 8] = [
        ("Network", Icon::Signal),
        ("Bluetooth", Icon::Bluetooth),
        ("Display", Icon::Display),
        ("Sound", Icon::Sound),
        ("Notifications", Icon::Bell),
        ("Apps", Icon::Apps),
        ("Battery", Icon::Battery),
        ("About", Icon::About),
    ];

    for (i, (name, glyph)) in ROWS.iter().enumerate() {
        let y = 208.0 + i as f32 * 88.0;

        // A 44px tile holds the rect-drawn glyph; copper ink keeps one accent
        // running down the column.
        c.paint(48.0, y + 22.0, 44.0, 44.0, DARK.radius_s, DARK.surface_high);
        icon(
            &mut c,
            70.0,
            y + 44.0,
            DARK.accent,
            DARK.surface_high,
            *glyph,
        );

        let row = c.style(DARK.body_px, Weight::Regular, DARK.text, 400.0);
        c.label(112.0, y, 88.0, *name, row);
        chevron(&mut c, 666.0, y + 42.0, true);

        if i + 1 < ROWS.len() {
            c.paint(48.0, y + 87.5, 624.0, 1.0, 0.0, DARK.line);
        }
    }

    c.finish()
}

/// P15 settings: network, connected card, nearby networks, saved networks.
pub fn network(size: Size) -> Scene {
    let mut c = Canvas::new(size, "settings-network");
    header(&mut c, "Network");

    // Wi-Fi master switch.
    c.paint(PAGE, 180.0, CARD, 96.0, DARK.radius_m, DARK.surface);
    two_line(
        &mut c,
        PAGE + PAGE,
        180.0,
        "Wi-Fi",
        "Visible as Galaxy A21s",
        DARK.text_dim,
    );
    toggle(&mut c, 596.0, 206.0, true);

    // The network we are on: the only card that carries the success colour.
    c.paint(
        PAGE,
        292.0,
        CARD,
        128.0,
        DARK.radius_card,
        DARK.surface_high,
    );
    c.paint(48.0, 332.0, 48.0, 48.0, DARK.radius_s, DARK.surface_low);
    icon(
        &mut c,
        72.0,
        356.0,
        DARK.accent,
        DARK.surface_low,
        Icon::Signal,
    );
    let joined = c.style(DARK.body_px, Weight::SemiBold, DARK.text, 420.0);
    c.text(116.0, 330.0, "Rabbiz-Home", joined);
    let meta = c.style(DARK.caption_px, Weight::Regular, DARK.success, 420.0);
    c.text(116.0, 366.0, "Connected · 5 GHz · WPA2", meta);
    signal(&mut c, 647.0, 372.0, 4, DARK.success, DARK.surface_low);

    // Nearby networks, each with signal strength and a lock where it is secured.
    const NETS: [(&str, u8, bool); 4] = [
        ("Rabbiz-Guest", 3, true),
        ("Nok's iPhone", 4, true),
        ("Cafe-A21S", 2, false),
        ("eduroam", 1, true),
    ];
    c.paint(PAGE, 464.0, CARD, 384.0, DARK.radius_card, DARK.surface);
    for (i, (name, bars, locked)) in NETS.iter().enumerate() {
        let y = 464.0 + i as f32 * 96.0;
        let style = c.style(DARK.body_px, Weight::Regular, DARK.text, 440.0);
        c.label(48.0, y, 96.0, *name, style);
        if *locked {
            icon(
                &mut c,
                616.0,
                y + 48.0,
                DARK.text_faint,
                DARK.surface,
                Icon::Lock,
            );
        }
        signal(&mut c, 647.0, y + 57.0, *bars, DARK.text, DARK.surface_high);
        if i + 1 < NETS.len() {
            c.paint(48.0, y + 95.5, 624.0, 1.0, 0.0, DARK.line);
        }
    }

    let saved = c.style(DARK.label_px, Weight::SemiBold, DARK.text_dim, 400.0);
    c.text(PAGE, 888.0, "Saved networks", saved);
    c.paint(PAGE, 920.0, CARD, 176.0, DARK.radius_card, DARK.surface);
    for (i, name) in ["Home-5G", "Office-IoT"].iter().enumerate() {
        let y = 920.0 + i as f32 * 88.0;
        let style = c.style(DARK.body_px, Weight::Regular, DARK.text, 440.0);
        c.label(48.0, y, 88.0, *name, style);
        chevron(&mut c, 666.0, y + 42.0, true);
        if i == 0 {
            c.paint(48.0, y + 87.5, 624.0, 1.0, 0.0, DARK.line);
        }
    }

    let foot = c.style(DARK.caption_px, Weight::Regular, DARK.text_faint, 600.0);
    c.text(
        PAGE,
        1116.0,
        "Networks are rescanned every 30 seconds.",
        foot,
    );

    c.finish()
}

/// P15 settings: Bluetooth switch, paired device and nearby pair prompts.
pub fn bluetooth(size: Size) -> Scene {
    let mut c = Canvas::new(size, "settings-bluetooth");
    header(&mut c, "Bluetooth");

    c.paint(PAGE, 180.0, CARD, 96.0, DARK.radius_m, DARK.surface);
    two_line(
        &mut c,
        PAGE + PAGE,
        180.0,
        "Bluetooth",
        "Visible as Galaxy A21s",
        DARK.text_dim,
    );
    toggle(&mut c, 596.0, 206.0, true);

    section(&mut c, 304.0, "Paired device");
    c.paint(PAGE, 344.0, CARD, 112.0, DARK.radius_card, DARK.surface);
    c.paint(48.0, 376.0, 48.0, 48.0, DARK.radius_s, DARK.surface_high);
    icon(
        &mut c,
        72.0,
        400.0,
        DARK.accent,
        DARK.surface_high,
        Icon::Sound,
    );
    let name = c.style(DARK.body_px, Weight::SemiBold, DARK.text, 420.0);
    c.text(116.0, 366.0, "Rabbiz Buds", name);
    let meta = c.style(DARK.caption_px, Weight::Regular, DARK.success, 420.0);
    c.text(116.0, 402.0, "Connected · 82%", meta);
    chevron(&mut c, 666.0, 398.0, true);

    section(&mut c, 484.0, "Nearby devices");
    const NEARBY: [&str; 3] = ["Nok's Speaker", "Pixel Buds", "Car Audio"];
    c.paint(PAGE, 520.0, CARD, 264.0, DARK.radius_card, DARK.surface);
    for (i, name) in NEARBY.iter().enumerate() {
        let y = 520.0 + i as f32 * 88.0;
        let style = c.style(DARK.body_px, Weight::Regular, DARK.text, 440.0);
        c.label(48.0, y, 88.0, *name, style);
        pair_pill(&mut c, 576.0, y + 22.0);
        if i + 1 < NEARBY.len() {
            c.paint(48.0, y + 87.5, 624.0, 1.0, 0.0, DARK.line);
        }
    }

    let foot = c.style(DARK.caption_px, Weight::Regular, DARK.text_faint, 600.0);
    c.text(
        PAGE,
        828.0,
        "Nearby devices stay discoverable for two minutes.",
        foot,
    );

    c.finish()
}

/// P15 settings: brightness, appearance, wallpapers and accent swatches.
pub fn display(size: Size) -> Scene {
    let mut c = Canvas::new(size, "settings-display");
    header(&mut c, "Display");

    section(&mut c, 180.0, "Brightness");
    // Sun glyphs at both ends of the track, filled to the current level.
    ring(&mut c, 36.0, 240.0, 16.0, DARK.accent);
    ring(&mut c, 660.0, 240.0, 24.0, DARK.accent_hi);
    slider(&mut c, 64.0, 236.0, 560.0, 0.7);

    section(&mut c, 292.0, "Appearance");
    c.paint(PAGE, 332.0, CARD, 64.0, DARK.radius_pill, DARK.surface);
    // Dark is the selected segment: one copper capsule, light stays a label.
    c.paint(32.0, 340.0, 328.0, 48.0, DARK.radius_pill, DARK.accent);
    let dark = c
        .style(DARK.label_px, Weight::SemiBold, DARK.accent_ink, 328.0)
        .with_align(TextAlign::Center);
    c.label(32.0, 340.0, 48.0, "Dark", dark);
    let light = c
        .style(DARK.label_px, Weight::Regular, DARK.text_dim, 328.0)
        .with_align(TextAlign::Center);
    c.label(360.0, 340.0, 48.0, "Light", light);

    section(&mut c, 436.0, "Wallpaper");
    let tiles = [
        (24.0, DARK.surface_low, DARK.accent, DARK.mark_body, true),
        (
            256.0,
            DARK.mark_body,
            DARK.accent_hi,
            DARK.surface_low,
            false,
        ),
        (
            488.0,
            DARK.surface_high,
            DARK.warning,
            DARK.surface_low,
            false,
        ),
    ];
    for (x, base, sun, hill, selected) in tiles {
        if selected {
            // The current wallpaper wears a copper ring.
            c.paint(x - 3.0, 473.0, 214.0, 134.0, 22.0, DARK.accent);
        }
        wallpaper(&mut c, x, 476.0, base, sun, hill);
    }

    section(&mut c, 660.0, "Accent colour");
    let swatches = [
        (24.0, DARK.accent, "Copper", true),
        (226.7, DARK.warning, "Sand", false),
        (429.3, DARK.mark_body, "Plum", false),
        (632.0, DARK.success, "Emerald", false),
    ];
    for (x, color, name, selected) in swatches {
        if selected {
            c.paint(x - 6.0, 694.0, 76.0, 76.0, 38.0, DARK.text);
        }
        c.paint(x, 700.0, 64.0, 64.0, 32.0, color);
        let label = c
            .style(DARK.micro_px, Weight::Regular, DARK.text_faint, 96.0)
            .with_align(TextAlign::Center);
        c.label(x + 32.0 - 48.0, 780.0, 24.0, name, label);
    }

    let foot = c.style(DARK.caption_px, Weight::Regular, DARK.text_faint, 600.0);
    c.text(
        PAGE,
        830.0,
        "The accent colour tints buttons, toggles and sliders.",
        foot,
    );

    c.finish()
}

/// P15 settings: volume, output, media volume and ringtone.
pub fn sound(size: Size) -> Scene {
    let mut c = Canvas::new(size, "settings-sound");
    header(&mut c, "Sound");

    section(&mut c, 180.0, "Volume");
    let value = c.style(DARK.label_px, Weight::SemiBold, DARK.text, 48.0);
    c.text(636.0, 180.0, "60%", value);
    icon(&mut c, 36.0, 240.0, DARK.accent, DARK.bg, Icon::Sound);
    slider(&mut c, 64.0, 236.0, 592.0, 0.6);

    const ROWS: [(&str, &str); 3] = [
        ("Output", "Speakers"),
        ("Media volume", "60%"),
        ("Ringtone", "Copper bell"),
    ];
    c.paint(PAGE, 300.0, CARD, 288.0, DARK.radius_card, DARK.surface);
    for (i, (title, sub)) in ROWS.iter().enumerate() {
        let y = 300.0 + i as f32 * 96.0;
        two_line(&mut c, 48.0, y, title, sub, DARK.text_dim);
        chevron(&mut c, 666.0, y + 46.0, true);
        if i + 1 < ROWS.len() {
            c.paint(48.0, y + 95.5, 624.0, 1.0, 0.0, DARK.line);
        }
    }

    let foot = c.style(DARK.caption_px, Weight::Regular, DARK.text_faint, 600.0);
    c.text(
        PAGE,
        636.0,
        "Media volume also follows the volume keys.",
        foot,
    );

    c.finish()
}

/// P15 settings: wordmark and the device's version facts.
pub fn about(size: Size) -> Scene {
    let mut c = Canvas::new(size, "settings-about");
    header(&mut c, "About");

    mark(&mut c, 360.0, 244.0);
    let wordmark = c
        .style(48.0, Weight::SemiBold, DARK.text, 600.0)
        .with_align(TextAlign::Center);
    c.text(60.0, 318.0, "RabbizOS", wordmark);
    let tagline = c
        .style(DARK.caption_px, Weight::Regular, DARK.text_dim, 600.0)
        .with_align(TextAlign::Center);
    c.text(60.0, 390.0, "native · quiet · yours", tagline);

    const FACTS: [(&str, &str); 3] = [
        ("Device", "Galaxy A21s"),
        ("Kernel", "7.3.0"),
        ("Build", "rabbiz-de 0.1.0"),
    ];
    c.paint(PAGE, 452.0, CARD, 352.0, DARK.radius_card, DARK.surface);
    for (i, (key, value)) in FACTS.iter().enumerate() {
        let y = 452.0 + i as f32 * 88.0;
        let key_style = c.style(DARK.label_px, Weight::Regular, DARK.text_dim, 240.0);
        c.label(48.0, y, 88.0, *key, key_style);
        let value_style = c.style(DARK.body_px, Weight::Regular, DARK.text, 380.0);
        c.label(300.0, y, 88.0, *value, value_style);
        c.paint(48.0, y + 87.5, 624.0, 1.0, 0.0, DARK.line);
    }

    // The licences row is a link, not a fact: label plus chevron.
    let y = 452.0 + 3.0 * 88.0;
    let key_style = c.style(DARK.label_px, Weight::Regular, DARK.text_dim, 560.0);
    c.label(48.0, y, 88.0, "Open source licenses", key_style);
    chevron(&mut c, 666.0, y + 42.0, true);

    let foot = c
        .style(DARK.micro_px, Weight::Regular, DARK.text_faint, 660.0)
        .with_align(TextAlign::Center);
    c.text(
        30.0,
        844.0,
        "RabbizOS 1.0 · Arch Linux ARM · GPL-3.0-only",
        foot,
    );

    c.finish()
}

/// P15 settings: session controls and the labwc fallback.
pub fn session(size: Size) -> Scene {
    let mut c = Canvas::new(size, "settings-session");
    header(&mut c, "Session");

    section(&mut c, 176.0, "Device");
    c.paint(PAGE, 208.0, CARD, 176.0, DARK.radius_card, DARK.surface);
    let rows = [
        ("Lock now", Icon::Lock),
        ("Restart Rabbiz DE", Icon::Restart),
    ];
    for (i, (name, glyph)) in rows.iter().enumerate() {
        let y = 208.0 + i as f32 * 88.0;
        c.paint(48.0, y + 22.0, 44.0, 44.0, DARK.radius_s, DARK.surface_high);
        icon(
            &mut c,
            70.0,
            y + 44.0,
            DARK.accent,
            DARK.surface_high,
            *glyph,
        );
        let style = c.style(DARK.body_px, Weight::Regular, DARK.text, 380.0);
        c.label(112.0, y, 88.0, *name, style);
        chevron(&mut c, 666.0, y + 42.0, true);
        if i == 0 {
            c.paint(48.0, y + 87.5, 624.0, 1.0, 0.0, DARK.line);
        }
    }

    section(&mut c, 424.0, "Recovery");
    c.paint(PAGE, 456.0, CARD, 144.0, DARK.radius_card, DARK.surface);
    // A danger edge marks the row that leaves this session.
    c.paint(PAGE, 456.0, 6.0, 144.0, 3.0, DARK.danger);
    let danger = c.style(DARK.body_px, Weight::SemiBold, DARK.danger, 520.0);
    c.text(48.0, 478.0, "Switch to labwc fallback", danger);
    let hint = c.style(DARK.caption_px, Weight::Regular, DARK.text_dim, 520.0);
    c.text(48.0, 516.0, "Ends this session and starts labwc", hint);
    chevron(&mut c, 666.0, 526.0, true);

    let confirm = c.style(DARK.caption_px, Weight::SemiBold, DARK.warning, 600.0);
    c.text(
        PAGE,
        620.0,
        "Tap again to confirm switching sessions.",
        confirm,
    );
    let foot = c.style(DARK.caption_px, Weight::Regular, DARK.text_faint, 600.0);
    c.text(
        PAGE,
        660.0,
        "You can return to Rabbiz DE from the fallback session.",
        foot,
    );

    c.finish()
}

/// One output frame: the reference panel plus the factors that map onto it.
#[derive(Clone, Copy)]
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

    /// A reference-pixel rect in output pixels.
    fn rect(self, x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect::new(x * self.sx, y * self.sy, w * self.sx, h * self.sy)
    }
}

/// Scene builder: author in reference pixels, emit one layer, use tokens only.
struct Canvas {
    frame: Frame,
    scene: Scene,
}

impl Canvas {
    fn new(size: Size, layer: &str) -> Self {
        let frame = Frame::new(size);
        let mut scene = Scene::new(size);
        let mut first = Layer::new(layer);
        // Full bleed: the first op always covers the whole output.
        first.rect(Rect::new(0.0, 0.0, frame.w, frame.h), 0.0, DARK.bg);
        scene.push(first);
        Self { frame, scene }
    }

    fn paint(&mut self, x: f32, y: f32, w: f32, h: f32, radius: f32, color: Color) {
        let rect = self.frame.rect(x, y, w, h);
        let radius = radius * self.frame.s;
        self.scene.layers[0].rect(rect, radius, color);
    }

    fn style(&self, size: f32, weight: Weight, color: Color, max_w: f32) -> TextStyle {
        TextStyle::new(size * self.frame.s, weight, color, max_w * self.frame.sx)
    }

    fn text(&mut self, x: f32, y: f32, text: impl Into<String>, style: TextStyle) {
        let origin = Point::new(x * self.frame.sx, y * self.frame.sy);
        self.scene.layers[0].text(origin, text, style);
    }

    /// A single line centred vertically inside the `h`-tall band at `y`.
    fn label(&mut self, x: f32, y: f32, h: f32, text: impl Into<String>, style: TextStyle) {
        let origin_y = y + (h - style.size * 1.25) / 2.0;
        self.text(x, origin_y, text, style);
    }

    fn finish(self) -> Scene {
        self.scene
    }
}

fn header(c: &mut Canvas, title: &str) {
    chevron(c, PAGE, 104.0, false);
    let style = c.style(DARK.title_px, Weight::SemiBold, DARK.text, 560.0);
    c.text(72.0, 84.0, title, style);
}

fn section(c: &mut Canvas, y: f32, text: &str) {
    let style = c.style(DARK.label_px, Weight::SemiBold, DARK.text_dim, 480.0);
    c.text(PAGE, y, text, style);
}

/// Two-line card row: a title over a smaller, colour-coded status line.
fn two_line(c: &mut Canvas, x: f32, y: f32, title: &str, sub: &str, sub_color: Color) {
    let title_style = c.style(DARK.body_px, Weight::SemiBold, DARK.text, 420.0);
    c.text(x, y + 22.0, title, title_style);
    let sub_style = c.style(DARK.caption_px, Weight::Regular, sub_color, 420.0);
    c.text(x, y + 56.0, sub, sub_style);
}

fn toggle(c: &mut Canvas, x: f32, y: f32, on: bool) {
    let track = if on { DARK.accent } else { DARK.surface_high };
    c.paint(x, y, 76.0, 44.0, DARK.radius_pill, track);
    let (knob, knob_x) = if on {
        (DARK.bg, x + 38.0)
    } else {
        (DARK.text_faint, x + 6.0)
    };
    c.paint(knob_x, y + 6.0, 32.0, 32.0, 16.0, knob);
}

/// A hollow token ring of `d` reference pixels, used for the sun glyphs.
fn ring(c: &mut Canvas, cx: f32, cy: f32, d: f32, ink: Color) {
    c.paint(cx - d / 2.0, cy - d / 2.0, d, d, d / 2.0, ink);
    let inner = d - 4.0;
    c.paint(
        cx - inner / 2.0,
        cy - inner / 2.0,
        inner,
        inner,
        inner / 2.0,
        DARK.bg,
    );
}

fn slider(c: &mut Canvas, x: f32, y: f32, w: f32, fraction: f32) {
    c.paint(x, y, w, 8.0, 4.0, DARK.surface_high);
    let filled = w * fraction;
    c.paint(x, y, filled, 8.0, 4.0, DARK.accent);
    c.paint(
        x + filled - 15.0,
        y - 11.0,
        30.0,
        30.0,
        15.0,
        DARK.accent_hi,
    );
}

fn pair_pill(c: &mut Canvas, x: f32, y: f32) {
    c.paint(x, y, 96.0, 44.0, DARK.radius_pill, DARK.surface_high);
    let style = c
        .style(DARK.label_px, Weight::SemiBold, DARK.accent, 96.0)
        .with_align(TextAlign::Center);
    c.label(x, y, 44.0, "Pair", style);
}

/// Four signal bars, bottom-aligned on `baseline`, filled left to right.
fn signal(c: &mut Canvas, x: f32, baseline: f32, lit: u8, ink: Color, idle: Color) {
    for i in 0..4u8 {
        let h = 6.0 + 4.0 * i as f32;
        let color = if i < lit { ink } else { idle };
        c.paint(x + i as f32 * 7.0, baseline - h, 4.0, h, 2.0, color);
    }
}

/// Chevron drawn as a four-step staircase of rounded bars. `right` points it
/// at the forward direction; the back glyph is the mirror.
fn chevron(c: &mut Canvas, x: f32, cy: f32, right: bool) {
    for i in 0..4 {
        let t = i as f32 * 4.0;
        let upper_x = if right { x + t } else { x + 12.0 - t };
        c.paint(upper_x, cy - 12.0 + t, 8.0, 4.0, 2.0, DARK.text_faint);
        if i < 3 {
            let lower_x = if right { x + 8.0 - t } else { x + 4.0 + t };
            c.paint(lower_x, cy + 4.0 + t, 8.0, 4.0, 2.0, DARK.text_faint);
        }
    }
}

/// Wallpaper thumbnail: a ground colour, a low sun and a horizon band.
fn wallpaper(c: &mut Canvas, x: f32, y: f32, base: Color, sun: Color, hill: Color) {
    c.paint(x, y, 208.0, 128.0, DARK.radius_m, base);
    c.paint(x + 24.0, y + 30.0, 48.0, 48.0, 24.0, sun);
    c.paint(x + 12.0, y + 84.0, 184.0, 32.0, 12.0, hill);
}

/// Rabbiz as rounded rects: two ears, a head, a cream muzzle and two eyes.
fn mark(c: &mut Canvas, cx: f32, cy: f32) {
    c.paint(cx - 44.0, cy - 84.0, 24.0, 76.0, 12.0, DARK.mark_body);
    c.paint(cx + 20.0, cy - 84.0, 24.0, 76.0, 12.0, DARK.mark_body);
    c.paint(cx + 20.0, cy - 84.0, 24.0, 26.0, 12.0, DARK.mark_copper);
    c.paint(cx - 38.0, cy - 24.0, 76.0, 72.0, 32.0, DARK.mark_body);
    c.paint(cx - 24.0, cy + 6.0, 48.0, 30.0, 15.0, DARK.mark_cream);
    c.paint(cx - 16.0, cy - 3.0, 7.0, 9.0, 3.5, DARK.bg);
    c.paint(cx + 9.0, cy - 3.0, 7.0, 9.0, 3.5, DARK.bg);
}

#[derive(Clone, Copy)]
enum Icon {
    Signal,
    Bluetooth,
    Display,
    Sound,
    Bell,
    Apps,
    Battery,
    About,
    Lock,
    Restart,
}

/// Every settings glyph, drawn from rounded rects around `(cx, cy)`.
fn icon(c: &mut Canvas, cx: f32, cy: f32, ink: Color, surface: Color, glyph: Icon) {
    match glyph {
        Icon::Signal => signal(c, cx - 12.0, cy + 9.0, 4, ink, ink.with_alpha(40)),
        Icon::Bluetooth => {
            c.paint(cx - 1.5, cy - 9.0, 3.0, 18.0, 1.5, ink);
            c.paint(cx - 1.5, cy - 9.0, 9.0, 3.0, 1.5, ink);
            c.paint(cx - 1.5, cy + 6.0, 9.0, 3.0, 1.5, ink);
            c.paint(cx - 6.0, cy - 5.0, 7.0, 3.0, 1.5, ink);
            c.paint(cx - 6.0, cy + 2.0, 7.0, 3.0, 1.5, ink);
        }
        Icon::Display => {
            c.paint(cx - 11.0, cy - 8.0, 22.0, 16.0, 3.0, ink);
            c.paint(cx - 9.0, cy - 6.0, 18.0, 12.0, 2.0, surface);
            c.paint(cx - 5.0, cy + 9.0, 10.0, 3.0, 1.5, ink);
        }
        Icon::Sound => {
            c.paint(cx - 10.0, cy - 3.0, 6.0, 6.0, 1.5, ink);
            c.paint(cx - 5.0, cy - 8.0, 8.0, 16.0, 3.0, ink);
            c.paint(cx + 6.0, cy - 7.0, 3.0, 3.0, 1.5, ink);
            c.paint(cx + 10.0, cy - 4.0, 3.0, 8.0, 1.5, ink);
        }
        Icon::Bell => {
            c.paint(cx - 3.0, cy - 12.0, 6.0, 4.0, 2.0, ink);
            c.paint(cx - 8.0, cy - 9.0, 16.0, 12.0, 7.0, ink);
            c.paint(cx - 10.0, cy + 2.0, 20.0, 4.0, 2.0, ink);
            c.paint(cx - 2.0, cy + 7.0, 4.0, 4.0, 2.0, ink);
        }
        Icon::Apps => {
            for (dx, dy) in [(-8.0, -8.0), (1.0, -8.0), (-8.0, 1.0), (1.0, 1.0)] {
                c.paint(cx + dx, cy + dy, 7.0, 7.0, 2.0, ink);
            }
        }
        Icon::Battery => {
            c.paint(cx - 10.0, cy - 6.0, 18.0, 12.0, 3.0, ink);
            c.paint(cx - 8.0, cy - 4.0, 14.0, 8.0, 2.0, surface);
            c.paint(cx - 8.0, cy - 4.0, 9.0, 8.0, 2.0, DARK.success);
            c.paint(cx + 9.0, cy - 3.0, 3.0, 6.0, 1.5, ink);
        }
        Icon::About => {
            c.paint(cx - 2.0, cy - 9.0, 4.0, 4.0, 2.0, ink);
            c.paint(cx - 2.0, cy - 3.0, 4.0, 12.0, 2.0, ink);
        }
        Icon::Lock => {
            c.paint(cx - 8.0, cy - 10.0, 16.0, 14.0, 7.0, ink);
            c.paint(cx - 5.0, cy - 7.0, 10.0, 10.0, 5.0, surface);
            c.paint(cx - 9.0, cy - 2.0, 18.0, 14.0, 3.0, ink);
        }
        Icon::Restart => {
            c.paint(cx - 9.0, cy - 9.0, 18.0, 18.0, 9.0, ink);
            c.paint(cx - 5.0, cy - 5.0, 10.0, 10.0, 5.0, surface);
            c.paint(cx + 1.0, cy - 11.0, 9.0, 4.0, 2.0, ink);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use de_api::DrawOp;

    const REF: Size = Size::new(720, 1600);
    const PHONE: Size = Size::new(1080, 2400);

    fn all(size: Size) -> Vec<(&'static str, Scene)> {
        vec![
            ("settings", index(size)),
            ("settings-network", network(size)),
            ("settings-bluetooth", bluetooth(size)),
            ("settings-display", display(size)),
            ("settings-sound", sound(size)),
            ("settings-about", about(size)),
            ("settings-session", session(size)),
        ]
    }

    fn texts(scene: &Scene) -> Vec<(&str, TextStyle)> {
        scene
            .layers
            .iter()
            .flat_map(|layer| layer.ops.iter())
            .filter_map(|op| match op {
                DrawOp::Text { text, style, .. } => Some((text.as_str(), *style)),
                _ => None,
            })
            .collect()
    }

    fn has(scene: &Scene, needle: &str) -> bool {
        texts(scene).iter().any(|(text, _)| *text == needle)
    }

    fn mentions(scene: &Scene, needle: &str) -> bool {
        texts(scene).iter().any(|(text, _)| text.contains(needle))
    }

    /// A colour is in use when a rect or a text style carries it.
    fn uses(scene: &Scene, color: Color) -> bool {
        let rect = scene
            .layers
            .iter()
            .flat_map(|layer| layer.ops.iter())
            .any(|op| matches!(op, DrawOp::Rect { color: c, .. } if *c == color));
        rect || texts(scene).iter().any(|(_, style)| style.color == color)
    }

    #[test]
    fn every_screen_is_one_named_layer_over_the_background() {
        for (name, scene) in all(REF) {
            assert_eq!(scene.size, REF, "{name}");
            assert_eq!(scene.layers.len(), 1, "{name}");
            assert_eq!(scene.layers[0].name, name);
            match &scene.layers[0].ops[0] {
                DrawOp::Rect {
                    rect,
                    radius,
                    color,
                } => {
                    assert_eq!(*rect, Rect::new(0.0, 0.0, 720.0, 1600.0), "{name}");
                    assert_eq!(*radius, 0.0, "{name}");
                    assert_eq!(*color, DARK.bg, "{name}");
                }
                other => panic!("{name}: first op must be the background, got {other:?}"),
            }
        }
    }

    #[test]
    fn every_screen_scales_to_the_phone_panel() {
        for (name, scene) in all(PHONE) {
            assert_eq!(scene.size, PHONE, "{name}");
            match &scene.layers[0].ops[0] {
                DrawOp::Rect { rect, .. } => {
                    assert_eq!(*rect, Rect::new(0.0, 0.0, 1080.0, 2400.0), "{name}");
                }
                other => panic!("{name}: first op must be the background, got {other:?}"),
            }
        }
    }

    #[test]
    fn no_operation_leaves_the_output() {
        for size in [REF, PHONE] {
            let (w, h) = (size.w as f32, size.h as f32);
            for (name, scene) in all(size) {
                for op in scene.layers.iter().flat_map(|layer| layer.ops.iter()) {
                    match op {
                        DrawOp::Rect { rect, .. } => {
                            assert!(rect.x >= -0.01 && rect.y >= -0.01, "{name}: {rect:?}");
                            assert!(
                                rect.right() <= w + 0.01 && rect.bottom() <= h + 0.01,
                                "{name}: {rect:?}"
                            );
                        }
                        DrawOp::Text {
                            origin,
                            text,
                            style,
                        } => {
                            assert!(origin.x >= 0.0 && origin.y >= 0.0, "{name}: {text:?}");
                            assert!(
                                origin.x + style.max_width <= w + 0.01,
                                "{name}: {text:?} leaves the output"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn index_lists_all_eight_rows() {
        let scene = index(REF);
        assert!(has(&scene, "Settings"));
        for row in [
            "Network",
            "Bluetooth",
            "Display",
            "Sound",
            "Notifications",
            "Apps",
            "Battery",
            "About",
        ] {
            assert!(has(&scene, row), "missing settings row {row:?}");
        }
    }

    #[test]
    fn index_uses_the_semantic_tokens() {
        let scene = index(REF);
        assert!(
            uses(&scene, DARK.success),
            "battery tile lost its success fill"
        );
        assert!(uses(&scene, DARK.accent), "rows lost their copper glyphs");
        assert!(uses(&scene, DARK.text_dim), "device line lost its dim ink");
    }

    #[test]
    fn network_shows_the_connection_and_the_saved_section() {
        let scene = network(REF);
        for text in ["Wi-Fi", "Rabbiz-Home", "Saved networks", "Home-5G"] {
            assert!(has(&scene, text), "missing {text:?}");
        }
        assert!(uses(&scene, DARK.success));
        assert!(uses(&scene, DARK.text_dim));
    }

    #[test]
    fn bluetooth_pairs_each_nearby_device() {
        let scene = bluetooth(REF);
        for text in ["Bluetooth", "Rabbiz Buds", "Nearby devices"] {
            assert!(has(&scene, text), "missing {text:?}");
        }
        let pairs = texts(&scene)
            .iter()
            .filter(|(text, _)| *text == "Pair")
            .count();
        assert_eq!(pairs, 3, "every nearby device needs a Pair pill");
        assert!(uses(&scene, DARK.accent));
    }

    #[test]
    fn display_selects_dark_and_names_the_accent_swatches() {
        let scene = display(REF);
        for text in [
            "Brightness",
            "Dark",
            "Light",
            "Wallpaper",
            "Copper",
            "Sand",
            "Plum",
            "Emerald",
        ] {
            assert!(has(&scene, text), "missing {text:?}");
        }
        assert!(uses(&scene, DARK.accent));
        assert!(uses(&scene, DARK.text_dim));
    }

    #[test]
    fn sound_lists_output_media_and_ringtone() {
        let scene = sound(REF);
        for text in ["Speakers", "Media volume", "Ringtone", "60%"] {
            assert!(has(&scene, text), "missing {text:?}");
        }
        assert!(uses(&scene, DARK.text_dim));
    }

    #[test]
    fn about_names_the_device_kernel_and_build() {
        let scene = about(REF);
        for text in [
            "RabbizOS",
            "Galaxy A21s",
            "7.3.0",
            "rabbiz-de 0.1.0",
            "Open source licenses",
        ] {
            assert!(has(&scene, text), "missing {text:?}");
        }
        assert!(uses(&scene, DARK.text_dim));
    }

    #[test]
    fn session_marks_the_labwc_fallback_as_danger() {
        let scene = session(REF);
        for text in ["Lock now", "Restart Rabbiz DE", "Switch to labwc fallback"] {
            assert!(has(&scene, text), "missing {text:?}");
        }
        assert!(
            mentions(&scene, "confirm"),
            "the fallback needs a confirm hint"
        );
        assert!(uses(&scene, DARK.danger));
    }
}
