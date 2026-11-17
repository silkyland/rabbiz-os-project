//! Software rasterizer: scene -> premultiplied RGBA pixmap -> PNG.
//!
//! The public surface is frozen because `de-ui` and `de-sim` compile against
//! it: [`Renderer::new`], [`Renderer::render`] and [`save_png`]. Everything
//! else is private to this crate.
//!
//! Rects are filled by tiny-skia (rounded corners, alpha blended, clipped to
//! the pixmap). Text is shaped by cosmic-text (rustybuzz, so Thai clusters and
//! combining marks are shaped correctly) and rasterized by swash; glyph
//! placement stays inside `Buffer::draw`, the renderer never re-derives it.

use std::path::Path;

use cosmic_text::{
    Attrs, Buffer, Family, FontSystem, Metrics, Shaping, SwashCache, Weight as FontWeight,
};
use de_api::{Color, DrawOp, Point, Rect, Scene, TextAlign, TextStyle};
use swash::scale::{Render, ScaleContext, Source};
use swash::zeno::Format;
use swash::FontRef;
use tiny_skia::{FillRule, Paint, Path as SkiaPath, PathBuilder, Pixmap, Transform};

const NOTO_SANS: &[u8] = include_bytes!("../../../../../assets/fonts/NotoSans.ttf");
const NOTO_SANS_THAI: &[u8] = include_bytes!("../../../../../assets/fonts/NotoSansThai.ttf");
const MATERIAL_SYMBOLS_ROUNDED: &[u8] =
    include_bytes!("../../../../../assets/fonts/MaterialSymbolsRounded.ttf");

/// Scene -> pixmap painter. Holds the font database and the glyph cache, so
/// one renderer can paint many frames without re-reading system fonts.
pub struct Renderer {
    font_system: FontSystem,
    swash_cache: SwashCache,
    icon_scale_context: ScaleContext,
}

impl Renderer {
    pub fn new() -> Self {
        let mut font_system = FontSystem::new();
        font_system.db_mut().load_font_data(NOTO_SANS.to_vec());
        font_system.db_mut().load_font_data(NOTO_SANS_THAI.to_vec());
        Self {
            font_system,
            swash_cache: SwashCache::new(),
            icon_scale_context: ScaleContext::new(),
        }
    }

    /// Rasterize every layer, in scene order, onto one pixmap of `scene.size`.
    pub fn render(&mut self, scene: &Scene) -> Pixmap {
        let mut pixmap = Pixmap::new(scene.size.w.max(1), scene.size.h.max(1))
            .expect("scene size must fit in a pixmap");

        for layer in &scene.layers {
            for op in &layer.ops {
                match op {
                    DrawOp::Rect {
                        rect,
                        radius,
                        color,
                    } => fill_rounded_rect(&mut pixmap, *rect, *radius, *color),
                    DrawOp::Text {
                        origin,
                        text,
                        style,
                    } => self.draw_text(&mut pixmap, *origin, text, style),
                    DrawOp::Icon {
                        center,
                        size,
                        codepoint,
                        color,
                        filled,
                    } => self.draw_icon(&mut pixmap, *center, *size, *codepoint, *color, *filled),
                }
            }
        }

        pixmap
    }

    /// Rasterize a Material Symbols Rounded glyph centered at `center`.
    fn draw_icon(
        &mut self,
        pixmap: &mut Pixmap,
        center: Point,
        size: f32,
        codepoint: char,
        color: Color,
        filled: bool,
    ) {
        if size <= 0.0 || color.a == 0 {
            return;
        }
        let Some(font) = FontRef::from_index(MATERIAL_SYMBOLS_ROUNDED, 0) else {
            return;
        };
        let glyph_id = font.charmap().map(codepoint);
        if glyph_id == 0 {
            return;
        }
        let fill_val = if filled { 1.0 } else { 0.0 };
        let opsz_val = size.clamp(20.0, 48.0);
        let mut scaler = self
            .icon_scale_context
            .builder(font)
            .size(size)
            .hint(true)
            .variations(&[("FILL", fill_val), ("wght", 400.0), ("opsz", opsz_val)])
            .build();
        let Some(image) = Render::new(&[Source::Outline])
            .format(Format::Alpha)
            .render(&mut scaler, glyph_id)
        else {
            return;
        };

        let w = image.placement.width as i32;
        let h = image.placement.height as i32;
        if w <= 0 || h <= 0 {
            return;
        }
        // Center the rendered glyph bounding box at `center`.
        let start_x = (center.x - w as f32 / 2.0).round() as i32;
        let start_y = (center.y - h as f32 / 2.0).round() as i32;
        let alpha_scale = u32::from(color.a);

        for oy in 0..h {
            for ox in 0..w {
                let cov = image.data[(oy * w + ox) as usize];
                if cov == 0 {
                    continue;
                }
                blend_pixel(
                    pixmap,
                    start_x + ox,
                    start_y + oy,
                    [color.r, color.g, color.b, cov],
                    alpha_scale,
                );
            }
        }
    }

    /// Shape a text box at `origin` (its top-left corner) and blend the glyphs
    /// into `pixmap`. `style.max_width` is the wrap width.
    fn draw_text(&mut self, pixmap: &mut Pixmap, origin: Point, text: &str, style: &TextStyle) {
        if text.is_empty() || style.max_width <= 0.0 || style.size <= 0.0 {
            return;
        }

        let metrics = Metrics::new(style.size, style.size * 1.25);
        let mut buffer = Buffer::new(&mut self.font_system, metrics);
        buffer.set_size(&mut self.font_system, Some(style.max_width), None);
        buffer.set_text(
            &mut self.font_system,
            text,
            &Attrs::new()
                .family(Family::SansSerif)
                .weight(FontWeight(style.weight.numeric())),
            Shaping::Advanced,
            None,
        );
        buffer.shape_until_scroll(&mut self.font_system, false);

        // cosmic-text hands the closure straight (non-premultiplied) RGB plus a
        // coverage alpha; `blend_pixel` premultiplies before the src-over.
        let color = cosmic_text::Color::rgb(style.color.r, style.color.g, style.color.b);
        let alpha_scale = u32::from(style.color.a);
        // Center within the text box when the screen asks for it. cosmic-text
        // gives absolute glyph positions, so the offset is applied once here.
        let line_w = buffer
            .layout_runs()
            .next()
            .map(|run| run.line_w)
            .unwrap_or(style.max_width);
        let center_offset = match style.align {
            TextAlign::Left => 0.0,
            TextAlign::Center => ((style.max_width - line_w) / 2.0).max(0.0),
        };
        let origin_x = (origin.x + center_offset).round() as i32;
        let origin_y = origin.y.round() as i32;

        buffer.draw(
            &mut self.font_system,
            &mut self.swash_cache,
            color,
            |x, y, w, h, color| {
                let rgba = color.as_rgba();
                for oy in 0..h {
                    for ox in 0..w {
                        blend_pixel(
                            pixmap,
                            origin_x + x + ox as i32,
                            origin_y + y + oy as i32,
                            rgba,
                            alpha_scale,
                        );
                    }
                }
            },
        );
    }
}

impl Default for Renderer {
    fn default() -> Self {
        Self::new()
    }
}

/// Fill a rectangle whose corners are rounded by `radius`, clamped to half the
/// shorter side. Out-of-bounds areas are clipped by the pixmap.
fn fill_rounded_rect(pixmap: &mut Pixmap, rect: Rect, radius: f32, color: Color) {
    if rect.w <= 0.0 || rect.h <= 0.0 || color.a == 0 {
        return;
    }

    let radius = radius.max(0.0).min(rect.w / 2.0).min(rect.h / 2.0);
    let path = rounded_rect_path(rect, radius);

    let mut paint = Paint::default();
    paint.set_color_rgba8(color.r, color.g, color.b, color.a);
    paint.anti_alias = true;
    pixmap.fill_path(
        &path,
        &paint,
        FillRule::Winding,
        Transform::identity(),
        None,
    );
}

/// Rectangle path with the four corners replaced by quadratic arcs.
fn rounded_rect_path(rect: Rect, radius: f32) -> SkiaPath {
    let (x, y, w, h) = (rect.x, rect.y, rect.w, rect.h);
    let mut pb = PathBuilder::new();

    if radius <= 0.0 {
        pb.move_to(x, y);
        pb.line_to(x + w, y);
        pb.line_to(x + w, y + h);
        pb.line_to(x, y + h);
    } else {
        let r = radius;
        pb.move_to(x + r, y);
        pb.line_to(x + w - r, y);
        pb.quad_to(x + w, y, x + w, y + r);
        pb.line_to(x + w, y + h - r);
        pb.quad_to(x + w, y + h, x + w - r, y + h);
        pb.line_to(x + r, y + h);
        pb.quad_to(x, y + h, x, y + h - r);
        pb.line_to(x, y + r);
        pb.quad_to(x, y, x + r, y);
    }
    pb.close();

    pb.finish().expect("rounded rect is a closed path")
}

/// Blend one straight-alpha source pixel over a premultiplied destination
/// pixel. `alpha_scale` is the text colour's own alpha in 0..=255; cosmic-text
/// drops it when it hands back a coverage alpha.
fn blend_pixel(pixmap: &mut Pixmap, x: i32, y: i32, src: [u8; 4], alpha_scale: u32) {
    let a = u32::from(src[3]) * alpha_scale / 255;
    if a == 0 || x < 0 || y < 0 {
        return;
    }
    let (x, y) = (x as u32, y as u32);
    if x >= pixmap.width() || y >= pixmap.height() {
        return;
    }

    let index = (y * pixmap.width() + x) as usize;
    let dst = pixmap.pixels()[index];
    let inv = 255 - a;

    let out_a = a + u32::from(dst.alpha()) * inv / 255;
    let out_r = u32::from(src[0]) * a / 255 + u32::from(dst.red()) * inv / 255;
    let out_g = u32::from(src[1]) * a / 255 + u32::from(dst.green()) * inv / 255;
    let out_b = u32::from(src[2]) * a / 255 + u32::from(dst.blue()) * inv / 255;

    if let Some(pixel) = tiny_skia::PremultipliedColorU8::from_rgba(
        out_r as u8,
        out_g as u8,
        out_b as u8,
        out_a as u8,
    ) {
        pixmap.pixels_mut()[index] = pixel;
    }
}

/// Encode `pixmap` as PNG at `path`, creating parent directories as needed.
/// tiny-skia un-premultiplies on encode, so the file is straight RGBA8.
pub fn save_png(pixmap: &Pixmap, path: &Path) -> Result<(), String> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)
            .map_err(|err| format!("create {}: {err}", parent.display()))?;
    }
    pixmap
        .save_png(path)
        .map_err(|err| format!("encode {}: {err}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use de_api::{Layer, Size, Weight};

    fn render(scene: &Scene) -> Pixmap {
        Renderer::new().render(scene)
    }

    #[test]
    fn filled_rect_produces_the_exact_pixel() {
        let mut scene = Scene::new(Size::new(8, 8));
        let mut layer = Layer::new("fill");
        layer.rect(
            Rect::new(2.0, 2.0, 4.0, 4.0),
            0.0,
            Color::rgb(0x20, 0x40, 0x60),
        );
        scene.push(layer);

        let pixmap = render(&scene);
        let inside = pixmap.pixel(3, 3).unwrap();
        assert_eq!(
            (inside.red(), inside.green(), inside.blue(), inside.alpha()),
            (0x20, 0x40, 0x60, 255)
        );

        // The pixmap starts transparent, so the pixel beside the rect is clean.
        assert_eq!(pixmap.pixel(1, 3).unwrap().alpha(), 0);
    }

    #[test]
    fn out_of_bounds_rect_is_clipped() {
        let mut scene = Scene::new(Size::new(16, 16));
        let mut layer = Layer::new("clip");
        // Runs off the right edge and above the top edge.
        layer.rect(
            Rect::new(12.0, -8.0, 40.0, 20.0),
            0.0,
            Color::rgb(0xF0, 0x00, 0x00),
        );
        // Entirely outside: must be a no-op, not a panic.
        layer.rect(
            Rect::new(-100.0, -100.0, 10.0, 10.0),
            0.0,
            Color::rgb(0x00, 0xF0, 0x00),
        );
        scene.push(layer);

        let pixmap = render(&scene);
        assert_eq!((pixmap.width(), pixmap.height()), (16, 16));

        // Visible part: inside the pixmap at the right edge.
        let inside = pixmap.pixel(15, 0).unwrap();
        assert_eq!(
            (inside.red(), inside.green(), inside.blue(), inside.alpha()),
            (0xF0, 0x00, 0x00, 255)
        );
        // Just left of the rect, and the bottom half, stay transparent.
        assert_eq!(pixmap.pixel(11, 8).unwrap().alpha(), 0);
        assert_eq!(pixmap.pixel(15, 15).unwrap().alpha(), 0);
    }

    #[test]
    fn alpha_blending_is_correct() {
        let mut scene = Scene::new(Size::new(4, 4));
        let mut background = Layer::new("background");
        background.rect(
            Rect::new(0.0, 0.0, 4.0, 4.0),
            0.0,
            Color::rgb(0x00, 0x00, 0x00),
        );
        scene.push(background);

        let mut foreground = Layer::new("foreground");
        foreground.rect(
            Rect::new(0.0, 0.0, 4.0, 4.0),
            0.0,
            Color::rgba(0xFF, 0xFF, 0xFF, 128),
        );
        scene.push(foreground);

        let pixmap = render(&scene);
        // src-over with a 50% white source over opaque black: 128-ish, alpha 255.
        let blended = pixmap.pixel(2, 2).unwrap();
        for (name, got) in [
            ("red", blended.red()),
            ("green", blended.green()),
            ("blue", blended.blue()),
        ] {
            let diff = i32::from(got) - 128;
            assert!(
                diff.abs() <= 2,
                "{name}: got {got}, want 128 +/- 2 (src-over of 50% white on black)"
            );
        }
        assert_eq!(blended.alpha(), 255);
    }

    #[test]
    fn text_op_paints_at_least_one_pixel() {
        let mut scene = Scene::new(Size::new(360, 96));
        let mut layer = Layer::new("text");
        // Latin and Thai in one run: exercises advanced shaping (Thai clusters).
        layer.text(
            Point::new(8.0, 8.0),
            "Rabbiz \u{0e01}\u{0e23}\u{0e30}\u{0e15}\u{0e48}\u{0e32}\u{0e22}",
            TextStyle::new(32.0, Weight::Regular, Color::rgb(0xF4, 0xED, 0xE1), 340.0),
        );
        scene.push(layer);

        let pixmap = render(&scene);
        let painted = pixmap.pixels().iter().filter(|px| px.alpha() > 0).count();
        assert!(
            painted > 0,
            "expected the text op to paint pixels, scene stayed empty"
        );
    }

    #[test]
    fn renders_a_720x1600_scene() {
        let mut scene = Scene::new(Size::new(720, 1600));
        let mut background = Layer::new("background");
        background.rect(
            Rect::new(0.0, 0.0, 720.0, 1600.0),
            0.0,
            Color::rgb(0x16, 0x10, 0x1A),
        );
        scene.push(background);

        let mut card = Layer::new("card");
        card.rect(
            Rect::new(24.0, 640.0, 672.0, 320.0),
            28.0,
            Color::rgb(0x24, 0x1A, 0x2A),
        );
        card.text(
            Point::new(48.0, 700.0),
            "RabbizOS",
            TextStyle::new(40.0, Weight::SemiBold, Color::rgb(0xF4, 0xED, 0xE1), 600.0),
        );
        scene.push(card);

        let pixmap = render(&scene);
        assert_eq!(pixmap.width(), 720);
        assert_eq!(pixmap.height(), 1600);

        // Background is opaque everywhere, including the far corner.
        let corner = pixmap.pixel(719, 1599).unwrap();
        assert_eq!(corner.alpha(), 255);
        // The card paints over the background.
        let card_pixel = pixmap.pixel(100, 700).unwrap();
        assert_eq!(
            (card_pixel.red(), card_pixel.green(), card_pixel.blue()),
            (0x24, 0x1A, 0x2A)
        );
    }

    #[test]
    fn save_png_creates_parent_directories() {
        let mut scene = Scene::new(Size::new(2, 2));
        let mut layer = Layer::new("fill");
        layer.rect(
            Rect::new(0.0, 0.0, 2.0, 2.0),
            0.0,
            Color::rgb(0x10, 0x20, 0x30),
        );
        scene.push(layer);
        let pixmap = render(&scene);

        let dir = std::env::temp_dir().join(format!("de-paint-save-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("nested").join("deep").join("out.png");

        save_png(&pixmap, &path).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn icon_op_paints_outlined_and_filled_glyphs() {
        let render_icon = |filled: bool| {
            let mut scene = Scene::new(Size::new(64, 64));
            let mut layer = Layer::new("icon");
            layer.icon(
                Point::new(32.0, 32.0),
                40.0,
                de_api::icons::NOTIFICATIONS,
                Color::rgb(255, 255, 255),
                filled,
            );
            scene.push(layer);
            render(&scene)
        };

        let outlined = render_icon(false);
        let filled = render_icon(true);
        let outlined_lit = outlined.pixels().iter().filter(|p| p.alpha() > 0).count();
        let filled_lit = filled.pixels().iter().filter(|p| p.alpha() > 0).count();
        assert!(outlined_lit > 20, "outlined icon should paint pixels");
        assert!(filled_lit > outlined_lit, "filled icon should paint more pixels than outlined");
    }
}
