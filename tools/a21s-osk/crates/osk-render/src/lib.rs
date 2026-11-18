//! Renders an `osk-core` layout to an RGBA pixmap — the host half of the renderer.
//!
//! The Wayland client paints the same scene into a `wl_shm` buffer; keeping the scene code
//! here means a keyboard layout can be reviewed as a PNG on the Mac before it ever touches
//! the phone. Text uses cosmic-text (rustybuzz shaping + swash glyphs), so Thai marks and
//! colour emoji go through the same path they will on the device.

use cosmic_text::{Attrs, Buffer, FontSystem, Metrics, Shaping, SwashCache};
use osk_core::layout::{Key, KeyAction, KeyPos, Layout, Rect};
use osk_core::state::ShiftState;
pub use tiny_skia::Pixmap;
use tiny_skia::{Color, FillRule, Paint, PathBuilder, PremultipliedColorU8, Transform};

#[derive(Clone, Copy)]
pub struct Theme {
    pub background: (u8, u8, u8),
    pub strip: (u8, u8, u8),
    pub letter_key: (u8, u8, u8),
    pub function_key: (u8, u8, u8),
    pub accent: (u8, u8, u8),
    pub label: (u8, u8, u8),
    pub label_dim: (u8, u8, u8),
}

impl Theme {
    /// Gboard dark: near-black surface, slightly lighter keys, blue accent action key.
    pub fn dark() -> Theme {
        Theme {
            background: (24, 25, 28),
            strip: (32, 33, 37),
            letter_key: (54, 56, 61),
            function_key: (38, 39, 44),
            accent: (138, 180, 248),
            label: (232, 234, 237),
            label_dim: (154, 160, 166),
        }
    }

    pub fn light() -> Theme {
        Theme {
            background: (238, 240, 243),
            strip: (248, 249, 250),
            letter_key: (255, 255, 255),
            function_key: (218, 221, 225),
            accent: (26, 115, 232),
            label: (32, 33, 36),
            label_dim: (95, 99, 104),
        }
    }
}

/// Long-press chooser drawn in the strip area.
pub struct AltBar<'a> {
    pub items: &'a [String],
    pub selected: usize,
}

/// Bundled so keys look like Gboard without installing a font package on the device.
const MATERIAL_ICONS: &[u8] = include_bytes!("../../../data/fonts/MaterialIcons-Regular.ttf");
const ICON_FAMILY: &str = "Material Icons";

pub struct Renderer {
    font_system: FontSystem,
    swash_cache: SwashCache,
}

impl Default for Renderer {
    fn default() -> Self {
        Renderer {
            font_system: FontSystem::new(),
            swash_cache: SwashCache::new(),
        }
    }
}

impl Renderer {
    pub fn new() -> Self {
        let mut renderer = Self::default();
        renderer
            .font_system
            .db_mut()
            .load_font_data(MATERIAL_ICONS.to_vec());
        renderer
    }

    /// Render one layer at `scale` (1.0 = device pixels, 2.0 = crisp review PNG).
    pub fn render(
        &mut self,
        layout: &Layout,
        layer_name: &str,
        shift: ShiftState,
        theme: &Theme,
        scale: f32,
    ) -> Pixmap {
        self.render_with_pressed(layout, layer_name, shift, theme, scale, None)
    }

    /// Same scene, with the key under the finger drawn highlighted.
    pub fn render_with_pressed(
        &mut self,
        layout: &Layout,
        layer_name: &str,
        shift: ShiftState,
        theme: &Theme,
        scale: f32,
        pressed: Option<KeyPos>,
    ) -> Pixmap {
        self.render_full(layout, layer_name, shift, theme, scale, pressed, None, &[])
    }

    /// Full scene: pressed key highlight plus an optional long-press chooser in the strip.
    pub fn render_full(
        &mut self,
        layout: &Layout,
        layer_name: &str,
        shift: ShiftState,
        theme: &Theme,
        scale: f32,
        pressed: Option<KeyPos>,
        alts: Option<AltBar<'_>>,
        suggestions: &[String],
    ) -> Pixmap {
        let w = (layout.geometry.width_px * scale).round() as u32;
        let h = (layout.height_px_for(layer_name) * scale).round() as u32;
        let mut pixmap = Pixmap::new(w, h).expect("pixmap");
        pixmap.fill(color(theme.background));

        let g = &layout.geometry;
        self.rect(
            &mut pixmap,
            0.0,
            0.0,
            w as f32,
            g.strip_height_px * scale,
            theme.strip,
            0.0,
        );
        if let Some(bar) = alts {
            self.alt_bar(&mut pixmap, layout, theme, scale, &bar);
        } else {
            self.toolbar(&mut pixmap, layout, theme, scale);
            self.strip_hints(&mut pixmap, layout, theme, scale, suggestions);
        }

        let rows = match layout.row_rects(layer_name) {
            Ok(rows) => rows,
            Err(_) => return pixmap,
        };
        let layer = match layout.layer(layer_name) {
            Some(layer) => layer,
            None => return pixmap,
        };
        for (row_i, row) in rows.iter().enumerate() {
            for (rect, pos) in row {
                let key = &layer.rows[row_i][pos.col];
                let r = Rect {
                    x: rect.x * scale,
                    y: rect.y * scale,
                    w: rect.w * scale,
                    h: rect.h * scale,
                };
                let base = match key.action {
                    KeyAction::Return => theme.accent,
                    KeyAction::Text(_) => theme.letter_key,
                    _ => theme.function_key,
                };
                let base = if pressed == Some(*pos) {
                    mix(base, (255, 255, 255), 0.18)
                } else {
                    base
                };
                self.rect(&mut pixmap, r.x, r.y, r.w, r.h, base, 8.0 * scale);
                let fg = if matches!(key.action, KeyAction::Return) {
                    theme.background
                } else {
                    theme.label
                };
                if let Some(icon) = key.icon.as_ref() {
                    self.text_in_family(
                        &mut pixmap,
                        icon,
                        r,
                        r.h * 0.52,
                        fg,
                        cosmic_text::Family::Name(ICON_FAMILY),
                    );
                    continue;
                }
                let label = layout.display_label(key, shift.is_shifted());
                let size = label_size(key, r.h / scale) * scale;
                self.text_centered(&mut pixmap, &label, r, size, fg);
                // Gboard shows what shift would produce as a small hint in the corner
                // (Thai digits carry their Arabic counterpart there).
                if !shift.is_shifted() {
                    if let Some(hint) = key.shifted.as_ref() {
                        if *hint != label {
                            // Gboard puts the shift hint in the top-left corner of the cap.
                            let hint_rect = Rect {
                                x: r.x + 4.0 * scale,
                                y: r.y + 3.0 * scale,
                                w: r.w * 0.45,
                                h: r.h * 0.32,
                            };
                            self.text_centered(
                                &mut pixmap,
                                hint,
                                hint_rect,
                                (r.h * 0.26).max(10.0),
                                theme.label_dim,
                            );
                        }
                    }
                }
            }
        }
        pixmap
    }

    fn alt_bar(&mut self, pixmap: &mut Pixmap, layout: &Layout, theme: &Theme, scale: f32, bar: &AltBar<'_>) {
        let g = &layout.geometry;
        let n = bar.items.len().max(1) as f32;
        let margin = g.margin_px * scale;
        let gap = 4.0 * scale;
        let width = (layout.geometry.width_px * scale - 2.0 * margin - gap * (n - 1.0)) / n;
        let height = g.strip_height_px * scale - 8.0 * scale;
        for (i, item) in bar.items.iter().enumerate() {
            let x = margin + i as f32 * (width + gap);
            let fill = if i == bar.selected {
                theme.accent
            } else {
                theme.letter_key
            };
            self.rect(pixmap, x, 4.0 * scale, width, height, fill, 6.0 * scale);
            let fg = if i == bar.selected { theme.background } else { theme.label };
            let rect = Rect { x, y: 4.0 * scale, w: width, h: height };
            self.text_centered(pixmap, item, rect, height * 0.6, fg);
        }
    }

    /// Clipboard history sheet: replaces the keys with a list of recent snippets.
    pub fn render_sheet(
        &mut self,
        layout: &Layout,
        theme: &Theme,
        scale: f32,
        title: &str,
        items: &[String],
    ) -> Pixmap {
        let w = (layout.geometry.width_px * scale).round() as u32;
        let h = (layout.height_px() * scale).round() as u32;
        let mut pixmap = Pixmap::new(w, h).expect("pixmap");
        pixmap.fill(color(theme.background));
        self.rect(&mut pixmap, 0.0, 0.0, w as f32, h as f32, theme.background, 0.0);
        let title_rect = Rect {
            x: 16.0 * scale,
            y: 10.0 * scale,
            w: (layout.geometry.width_px - 32.0) * scale,
            h: 40.0 * scale,
        };
        self.text_centered(&mut pixmap, title, title_rect, 20.0 * scale, theme.label_dim);
        if items.is_empty() {
            let empty = Rect {
                x: 16.0 * scale,
                y: 120.0 * scale,
                w: (layout.geometry.width_px - 32.0) * scale,
                h: 60.0 * scale,
            };
            self.text_centered(&mut pixmap, "clipboard history is empty", empty, 18.0 * scale, theme.label_dim);
            return pixmap;
        }
        for (index, item) in items.iter().take(5).enumerate() {
            let rect = sheet_row_rect(index, layout, scale);
            self.rect(&mut pixmap, rect.x, rect.y, rect.w, rect.h, theme.letter_key, 8.0 * scale);
            let preview: String = item.chars().take(48).collect();
            let text_rect = Rect {
                x: rect.x + 12.0 * scale,
                y: rect.y,
                w: rect.w - 24.0 * scale,
                h: rect.h,
            };
            self.text_centered(&mut pixmap, &preview, text_rect, 18.0 * scale, theme.label);
        }
        pixmap
    }

    fn toolbar(&mut self, pixmap: &mut Pixmap, layout: &Layout, theme: &Theme, scale: f32) {
        for (index, icon) in TOOLBAR_ICONS.iter().enumerate() {
            let rect = toolbar_button_rect(index, layout, scale);
            self.rect(pixmap, rect.x, rect.y, rect.w, rect.h, theme.function_key, 6.0 * scale);
            self.text_in_family(
                pixmap,
                icon,
                rect,
                rect.h * 0.55,
                theme.label,
                cosmic_text::Family::Name(ICON_FAMILY),
            );
        }
    }

    fn strip_hints(
        &mut self,
        pixmap: &mut Pixmap,
        layout: &Layout,
        theme: &Theme,
        scale: f32,
        suggestions: &[String],
    ) {
        let g = &layout.geometry;
        let row_h = g.strip_height_px * scale;
        let y = row_h * 0.18;
        let slot_h = row_h * 0.64;
        // Gboard layout: [left action] [three suggestions] [right action].
        let left = toolbar_button_rect(0, layout, scale);
        let right = toolbar_button_rect(1, layout, scale);
        let strip_x = left.x + left.w + 6.0 * scale;
        let strip_w = (right.x - 6.0 * scale) - strip_x;
        let slot_w = strip_w / 3.0;
        for i in 0..3 {
            let x = strip_x + i as f32 * slot_w;
            if i > 0 {
                self.rect(pixmap, x, y + 4.0 * scale, 1.0 * scale, slot_h - 8.0 * scale, theme.label_dim, 0.0);
            }
        }
        for (i, suggestion) in suggestions.iter().take(3).enumerate() {
            if suggestion.is_empty() {
                continue;
            }
            let rect = suggestion_rect(i, layout, scale);
            self.text_centered(pixmap, suggestion, rect, slot_h * 0.42, theme.label);
        }
    }

    fn rect(
        &self,
        pixmap: &mut Pixmap,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        rgb: (u8, u8, u8),
        radius: f32,
    ) {
        if w <= 0.0 || h <= 0.0 {
            return;
        }
        let path = rounded_rect(x, y, w, h, radius.min(h / 2.0).min(w / 2.0));
        let mut paint = Paint::default();
        paint.set_color(color(rgb));
        paint.anti_alias = true;
        pixmap.fill_path(
            &path,
            &paint,
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }

    fn text_centered(
        &mut self,
        pixmap: &mut Pixmap,
        text: &str,
        rect: Rect,
        size: f32,
        rgb: (u8, u8, u8),
    ) {
        self.text_in_family(pixmap, text, rect, size, rgb, cosmic_text::Family::SansSerif);
    }

    fn text_in_family(
        &mut self,
        pixmap: &mut Pixmap,
        text: &str,
        rect: Rect,
        size: f32,
        rgb: (u8, u8, u8),
        family: cosmic_text::Family<'_>,
    ) {
        if text.is_empty() || rect.w <= 0.0 || rect.h <= 0.0 {
            return;
        }
        let mut buffer = Buffer::new(&mut self.font_system, Metrics::new(size, size * 1.25));
        buffer.set_size(&mut self.font_system, Some(rect.w - 4.0), Some(rect.h));
        buffer.set_text(
            &mut self.font_system,
            text,
            Attrs::new().family(family),
            Shaping::Advanced,
        );
        buffer.shape_until_scroll(&mut self.font_system, false);

        let line_w = buffer
            .layout_runs()
            .next()
            .map(|r| r.line_w)
            .unwrap_or(rect.w);
        let line_h = buffer.metrics().line_height;
        let dx = rect.x + (rect.w - line_w) / 2.0;
        let dy = rect.y + (rect.h - line_h) / 2.0;

        // cosmic-text's own `draw` owns glyph placement (baseline, hints, subpixel bins), so the
        // renderer never re-derives it. Each callback is one coverage/colour pixel.
        let pixmap_ref = &mut *pixmap;
        let width = pixmap_ref.width() as i32;
        let height = pixmap_ref.height() as i32;
        let mut blend = |x: i32, y: i32, w: u32, h: u32, color: cosmic_text::Color| {
            let [r, g, b, a] = color.as_rgba();
            if a == 0 {
                return;
            }
            for oy in 0..h as i32 {
                for ox in 0..w as i32 {
                    let px = x + ox;
                    let py = y + oy;
                    if px < 0 || py < 0 || px >= width || py >= height {
                        continue;
                    }
                    let index = (py as u32 * width as u32 + px as u32) as usize;
                    let dst = pixmap_ref.pixels()[index];
                    let inv = 255u32 - a as u32;
                    let out_a = a as u32 + dst.alpha() as u32 * inv / 255;
                    let out_r = r as u32 + dst.red() as u32 * inv / 255;
                    let out_g = g as u32 + dst.green() as u32 * inv / 255;
                    let out_b = b as u32 + dst.blue() as u32 * inv / 255;
                    if let Some(c) = PremultipliedColorU8::from_rgba(
                        out_r.min(255) as u8,
                        out_g.min(255) as u8,
                        out_b.min(255) as u8,
                        out_a.min(255) as u8,
                    ) {
                        pixmap_ref.pixels_mut()[index] = c;
                    }
                }
            }
        };
        buffer.draw(
            &mut self.font_system,
            &mut self.swash_cache,
            cosmic_text::Color::rgb(rgb.0, rgb.1, rgb.2),
            |x, y, w, h, color| {
                blend(x + dx as i32, y + dy as i32, w, h, color);
            },
        );
    }
}

/// Toolbar buttons drawn in the suggestion strip and hit-tested by the Wayland client.
pub const TOOLBAR_ICONS: [&str; 2] = ["content_paste", "mic"];
const TOOLBAR_BUTTON: f32 = 34.0;

/// Height of one row in a sheet (clipboard history), in surface pixels.
pub const SHEET_ROW_H: f32 = 52.0;

/// Rect of sheet row `index`, stacked from the top of the keyboard surface.
pub fn sheet_row_rect(index: usize, layout: &Layout, scale: f32) -> Rect {
    Rect {
        x: 8.0 * scale,
        y: (56.0 + index as f32 * (SHEET_ROW_H + 6.0)) * scale,
        w: (layout.geometry.width_px - 16.0) * scale,
        h: SHEET_ROW_H * scale,
    }
}

/// Rect of suggestion slot `index` (0..3) between the two toolbar buttons.
pub fn suggestion_rect(index: usize, layout: &Layout, scale: f32) -> Rect {
    let left = toolbar_button_rect(0, layout, scale);
    let right = toolbar_button_rect(1, layout, scale);
    let strip_x = left.x + left.w + 6.0 * scale;
    let strip_w = (right.x - 6.0 * scale) - strip_x;
    let slot_w = strip_w / 3.0;
    let y = (layout.geometry.strip_height_px * scale) * 0.18;
    let h = (layout.geometry.strip_height_px * scale) * 0.64;
    Rect {
        x: strip_x + index as f32 * slot_w + 4.0 * scale,
        y,
        w: slot_w - 8.0 * scale,
        h,
    }
}

/// Rect of toolbar button `index` (0 = left action, 1 = right action), in surface pixels.
pub fn toolbar_button_rect(index: usize, layout: &Layout, scale: f32) -> Rect {
    let h = (layout.geometry.strip_height_px - 8.0) * scale;
    let w = TOOLBAR_BUTTON * scale;
    let y = 4.0 * scale;
    let x = if index == 0 {
        6.0 * scale
    } else {
        layout.geometry.width_px * scale - w - 6.0 * scale
    };
    Rect { x, y, w, h }
}

fn mix(a: (u8, u8, u8), b: (u8, u8, u8), t: f32) -> (u8, u8, u8) {
    let f = |x: u8, y: u8| ((x as f32) * (1.0 - t) + (y as f32) * t).round() as u8;
    (f(a.0, b.0), f(a.1, b.1), f(a.2, b.2))
}

fn color(rgb: (u8, u8, u8)) -> Color {
    Color::from_rgba8(rgb.0, rgb.1, rgb.2, 255)
}

fn label_size(key: &Key, key_height_px: f32) -> f32 {
    // Emoji glyphs read as flat discs at text size (the globe key), so give them more room.
    if key.label.chars().any(|c| c as u32 >= 0x1F300) {
        return key_height_px * 0.52;
    }
    match key.action {
        KeyAction::Text(_) => key_height_px * 0.40,
        _ => key_height_px * 0.34,
    }
}

fn rounded_rect(x: f32, y: f32, w: f32, h: f32, r: f32) -> tiny_skia::Path {
    let mut pb = PathBuilder::new();
    if r <= 0.0 {
        pb.move_to(x, y);
        pb.line_to(x + w, y);
        pb.line_to(x + w, y + h);
        pb.line_to(x, y + h);
    } else {
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
    pb.finish().expect("path")
}

pub fn render_to_png(
    renderer: &mut Renderer,
    layout: &Layout,
    layer: &str,
    shift: ShiftState,
    theme: &Theme,
    scale: f32,
    path: &std::path::Path,
) -> Result<(), String> {
    let pixmap = renderer.render(layout, layer, shift, theme, scale);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    pixmap
        .save_png(path)
        .map_err(|e| format!("{}: {e}", path.display()))
}
