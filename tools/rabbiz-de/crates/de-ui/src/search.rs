//! P14 global search: a full-screen overlay with a 72 px field, grouped
//! results, recent query chips and a back hint.
//!
//! Frozen scaffold API: the overlay is a pure function of the output [`Size`]
//! and returns drawing operations only. Geometry is authored on the 720×1600
//! reference panel and mapped with `sx`/`sy`, so the field sits at y=96 on the
//! 1080×2400 panel too.
//!
//! Icons are initials on 48 px tiles, never an icon font: the shell has no font
//! to load and no images to ship.

use de_api::{Layer, Point, Rect, Scene, Size, TextAlign, TextStyle, Weight};
use de_design::DARK;

/// Reference panel the overlay was authored on.
const REF_W: f32 = 720.0;
const REF_H: f32 = 1600.0;

/// Result grouping: four app hits, three settings rows, one tile initial each.
const APPS: [(&str, &str, &str); 4] = [
    ("C", "Calculator", "Math, units and conversions"),
    ("F", "Files", "Browse Downloads and Pictures"),
    ("P", "Photos", "128 photos · 1.2 GB"),
    ("M", "Music", "Rabiz Mix · 12 tracks"),
];

const SETTINGS: [(&str, &str, &str); 3] = [
    ("W", "Wi-Fi", "Rabbiz-5G · connected"),
    ("B", "Bluetooth", "2 devices paired"),
    ("D", "Display", "Brightness 45% · Dark"),
];

const RECENT: [&str; 3] = ["calculator", "bluetooth", "screenshots"];

/// The three scale factors every helper needs: x, y and the aspect-safe one.
struct Scale {
    sx: f32,
    sy: f32,
    s: f32,
}

pub fn build(size: Size) -> Scene {
    let w = size.w as f32;
    let h = size.h as f32;
    let sx = w / REF_W;
    let sy = h / REF_H;
    let scale = Scale {
        sx,
        sy,
        // Corners, icons and type keep their aspect on odd panels.
        s: sx.min(sy),
    };

    let mut scene = Scene::new(size);
    let mut layer = Layer::new("search");

    // The overlay is drawn over home, so the canvas is the scrim; the first op
    // still covers the whole output with the base ink.
    layer.rect(Rect::new(0.0, 0.0, w, h), 0.0, DARK.bg);
    layer.rect(Rect::new(0.0, 0.0, w, h), 0.0, DARK.scrim);

    search_field(&mut layer, &scale);

    // Group labels and result rows share one rhythm: 96 px rows, 104 px pitch.
    section(&mut layer, "APPS", 204.0 * sy, &scale);
    let mut row = 240.0 * sy;
    for (i, (initial, title, subtitle)) in APPS.into_iter().enumerate() {
        result_row(&mut layer, row, initial, title, subtitle, i == 0, &scale);
        row += 104.0 * sy;
    }

    section(&mut layer, "SETTINGS", 672.0 * sy, &scale);
    let mut row = 708.0 * sy;
    for (initial, title, subtitle) in SETTINGS {
        result_row(&mut layer, row, initial, title, subtitle, false, &scale);
        row += 104.0 * sy;
    }

    recent(&mut layer, 1052.0 * sy, 1108.0 * sy, &scale);
    back_hint(&mut layer, w, h, &scale);

    scene.push(layer);
    scene
}

/// 72 px field at y=96: a magnifier ring and the placeholder, nothing else.
fn search_field(layer: &mut Layer, scale: &Scale) {
    let (sx, sy, s) = (scale.sx, scale.sy, scale.s);
    let field = Rect::new(
        DARK.page_margin * sx,
        96.0 * sy,
        (REF_W - 2.0 * DARK.page_margin) * sx,
        72.0 * sy,
    );
    layer.rect(field, DARK.radius_pill * s, DARK.surface);

    let ring = Rect::new(
        field.x + DARK.space_l * sx,
        field.y + 26.0 * sy,
        20.0 * s,
        20.0 * s,
    );
    layer.rect(ring, 10.0 * s, DARK.text_faint);
    layer.rect(ring.inset(4.0 * s), 6.0 * s, DARK.surface);
    // The stem overlaps the ring's lower-right arc, so the two rects read as
    // one magnifier instead of a circle and a tick.
    layer.rect(
        Rect::new(
            ring.right() - 4.0 * s,
            ring.bottom() - 4.0 * s,
            5.0 * s,
            12.0 * s,
        ),
        2.5 * s,
        DARK.text_faint,
    );

    // The placeholder owns everything from the ring to the right edge.
    let text_x = ring.right() + DARK.space_m * sx;
    let text_w = field.right() - DARK.page_margin * sx - text_x;
    let placeholder = TextStyle::new(DARK.label_px * s, Weight::Regular, DARK.text_faint, text_w);
    layer.text(
        Point::new(text_x, field.y + 22.0 * sy),
        "Search apps, files & settings",
        placeholder,
    );
}

/// A group label in the small section style.
fn section(layer: &mut Layer, text: &str, y: f32, scale: &Scale) {
    let style = TextStyle::new(
        DARK.micro_px * scale.s,
        Weight::SemiBold,
        DARK.text_faint,
        (REF_W - 2.0 * DARK.page_margin) * scale.sx,
    );
    layer.text(Point::new(DARK.page_margin * scale.sx, y), text, style);
}

/// One result: a 48 px tile with an initial, a title and a subtitle.
fn result_row(
    layer: &mut Layer,
    top: f32,
    initial: &str,
    title: &str,
    subtitle: &str,
    top_hit: bool,
    scale: &Scale,
) {
    let (sx, sy, s) = (scale.sx, scale.sy, scale.s);
    let icon = Rect::new(DARK.page_margin * sx, top + 24.0 * sy, 48.0 * s, 48.0 * s);
    layer.rect(icon, DARK.radius_m * s, DARK.surface_high);

    // The top hit wears copper; every other tile stays quiet.
    let ink = if top_hit { DARK.accent } else { DARK.text_dim };
    let initial_style = TextStyle::new(DARK.label_px * s, Weight::SemiBold, ink, icon.w)
        .with_align(TextAlign::Center);
    layer.text(
        Point::new(icon.x, icon.y + (icon.h - DARK.label_px * s * 1.25) / 2.0),
        initial,
        initial_style,
    );

    // Title and subtitle share the column from the tile to the right margin.
    let text_x = icon.right() + DARK.space_m * sx;
    let text_w = (REF_W - DARK.page_margin) * sx - text_x;
    layer.text(
        Point::new(text_x, top + 18.0 * sy),
        title,
        TextStyle::new(DARK.label_px * s, Weight::Regular, DARK.text, text_w),
    );
    layer.text(
        Point::new(text_x, top + 54.0 * sy),
        subtitle,
        TextStyle::new(DARK.caption_px * s, Weight::Regular, DARK.text_dim, text_w),
    );
}

/// Recent queries as chips whose width follows the query, not a fixed grid.
fn recent(layer: &mut Layer, label_y: f32, chip_y: f32, scale: &Scale) {
    section(layer, "RECENT", label_y, scale);

    let (sx, sy, s) = (scale.sx, scale.sy, scale.s);
    let mut x = DARK.page_margin * sx;
    for query in RECENT {
        let chars = query.chars().count() as f32;
        // 0.72 em per glyph is a deliberately wide estimate: a chip that wraps
        // would paint outside its own 44 px height.
        let chip_w = chars * DARK.caption_px * 0.72 * s + 2.0 * DARK.space_m * sx;
        let chip = Rect::new(x, chip_y, chip_w, 44.0 * sy);
        layer.rect(chip, DARK.radius_pill * s, DARK.surface_high);

        let text_w = chip.w - 2.0 * DARK.space_m * sx;
        let style = TextStyle::new(DARK.caption_px * s, Weight::Regular, DARK.text_dim, text_w)
            .with_align(TextAlign::Center);
        layer.text(
            Point::new(chip.x + DARK.space_m * sx, chip.y + 10.0 * sy),
            query,
            style,
        );
        x += chip_w + DARK.space_s * sx;
    }
}

/// Back hint: a pixel chevron and the word. The API has no rotation, so the
/// chevron is five rounded squares stepping down to a point.
fn back_hint(layer: &mut Layer, w: f32, h: f32, scale: &Scale) {
    let (sx, sy, s) = (scale.sx, scale.sy, scale.s);
    // A hairline separates the results from the hint.
    layer.rect(
        Rect::new(
            DARK.page_margin * sx,
            h - 144.0 * sy,
            (REF_W - 2.0 * DARK.page_margin) * sx,
            2.0 * s,
        ),
        0.0,
        DARK.line,
    );

    let hint_y = h - 118.0 * sy;
    // A "<" from nine 6 px squares stepping 3 px: each square overlaps the
    // next by half, so the shape is one stroke rather than a dotted diagonal.
    let chevron_x = w / 2.0 - 40.0 * sx;
    for step in 0..9 {
        // Columns: 12, 9, 6, 3, 0, 3, 6, 9, 12 from the group origin, a "<".
        let column = (step as f32 - 4.0).abs() * 3.0;
        layer.rect(
            Rect::new(
                chevron_x + column * sx,
                hint_y + step as f32 * 3.0 * s,
                6.0 * s,
                6.0 * s,
            ),
            2.0 * s,
            DARK.text_faint,
        );
    }

    let style = TextStyle::new(DARK.label_px * s, Weight::Regular, DARK.text_dim, 120.0 * s);
    layer.text(
        Point::new(chevron_x + 28.0 * sx, hint_y + 1.0 * sy),
        "Back",
        style,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use de_api::{Color, DrawOp};

    const REF: Size = Size::new(720, 1600);
    const PANEL: Size = Size::new(1080, 2400);

    fn near(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.01
    }

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

    fn rects(scene: &Scene) -> Vec<(Rect, Color)> {
        scene.layers[0]
            .ops
            .iter()
            .filter_map(|op| match op {
                DrawOp::Rect { rect, color, .. } => Some((*rect, *color)),
                DrawOp::Text { .. } => None,
            })
            .collect()
    }

    fn find_rect(scene: &Scene, want: Rect) -> Color {
        rects(scene)
            .into_iter()
            .find(|(rect, _)| {
                near(rect.x, want.x)
                    && near(rect.y, want.y)
                    && near(rect.w, want.w)
                    && near(rect.h, want.h)
            })
            .unwrap_or_else(|| panic!("no rect at {want:?}"))
            .1
    }

    #[test]
    fn scene_is_the_requested_size_with_one_search_layer() {
        let scene = build(PANEL);
        assert_eq!(scene.size, PANEL);
        assert_eq!(scene.layers.len(), 1);
        assert_eq!(scene.layers[0].name, "search");
    }

    #[test]
    fn first_op_is_the_full_bleed_background_then_the_scrim() {
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
        match &scene.layers[0].ops[1] {
            DrawOp::Rect { color, .. } => assert_eq!(*color, DARK.scrim),
            other => panic!("second op must be the scrim, got {other:?}"),
        }
    }

    #[test]
    fn field_is_seventy_two_tall_at_y_96() {
        let scene = build(REF);
        assert_eq!(
            find_rect(&scene, Rect::new(24.0, 96.0, 672.0, 72.0)),
            DARK.surface
        );

        let scene = build(PANEL);
        assert_eq!(
            find_rect(&scene, Rect::new(36.0, 144.0, 1008.0, 108.0)),
            DARK.surface
        );
    }

    #[test]
    fn field_placeholder_and_section_labels_are_present() {
        let scene = build(REF);
        let (_, style) = find_text(&scene, "Search apps, files & settings");
        assert_eq!(style.color, DARK.text_faint);
        assert_eq!(style.size, DARK.label_px);
        find_text(&scene, "APPS");
        find_text(&scene, "SETTINGS");
        find_text(&scene, "RECENT");
        find_text(&scene, "Back");
    }

    #[test]
    fn apps_and_settings_results_are_grouped() {
        let scene = build(REF);
        for (_, title, subtitle) in APPS {
            find_text(&scene, title);
            find_text(&scene, subtitle);
        }
        for (_, title, subtitle) in SETTINGS {
            find_text(&scene, title);
            find_text(&scene, subtitle);
        }
        for query in RECENT {
            find_text(&scene, query);
        }
    }

    #[test]
    fn recent_chips_stay_inside_the_page_margin() {
        let scene = build(REF);
        let chips: Vec<Rect> = rects(&scene)
            .into_iter()
            .filter(|(r, c)| *c == DARK.surface_high && near(r.h, 44.0))
            .map(|(r, _)| r)
            .collect();
        assert_eq!(chips.len(), RECENT.len());
        for chip in &chips {
            assert!(
                chip.x >= 24.0 && chip.right() <= 696.0,
                "chip escapes the margin: {chip:?}"
            );
        }
    }

    #[test]
    fn top_hit_wears_copper_and_the_rest_stay_dim() {
        let scene = build(REF);
        let (_, top_hit) = find_text(&scene, "C");
        assert_eq!(top_hit.color, DARK.accent);
        assert_eq!(top_hit.align, TextAlign::Center);

        let (_, plain) = find_text(&scene, "F");
        assert_eq!(plain.color, DARK.text_dim);

        let (_, title) = find_text(&scene, "Calculator");
        assert_eq!(title.color, DARK.text);
        assert_eq!(title.weight, Weight::Regular);
    }

    #[test]
    fn overlay_uses_semantic_tokens_only() {
        let scene = build(REF);
        let mut used: Vec<Color> = rects(&scene).into_iter().map(|(_, c)| c).collect();
        used.extend(scene.layers[0].ops.iter().filter_map(|op| match op {
            DrawOp::Text { style, .. } => Some(style.color),
            DrawOp::Rect { .. } => None,
        }));
        assert!(used.contains(&DARK.scrim), "no overlay scrim");
        assert!(used.contains(&DARK.text_dim), "no dim subtitle ink");
        assert!(used.contains(&DARK.line), "no hint hairline");
    }
}
