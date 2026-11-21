//! Cross-platform simulator configuration, viewport mapping, chrome overlay,
//! and XKB keysym translation helpers.
//!
//! Kept free of windowing dependencies (`winit`, `softbuffer`, `smithay`) so
//! every rule and coordinate transform is unit-tested on both macOS and Linux.

use std::path::PathBuf;

use de_api::{Pixels, Point, Rect, Size};
use de_design::tokens::{self, Theme};
use de_paint::Canvas;

use crate::ui;

/// Dark bezel fill outside the simulated device viewport (`0x00RRGGBB`).
pub const BEZEL_XRGB: u32 = 0x0011_0E14;

// Canonical XKB keysyms matching `tools/rabbiz-apps` key handlers.
pub const XKB_BACKSPACE: u32 = 0xFF08;
pub const XKB_TAB: u32 = 0xFF09;
pub const XKB_RETURN: u32 = 0xFF0D;
pub const XKB_ESCAPE: u32 = 0xFF1B;
pub const XKB_HOME: u32 = 0xFF50;
pub const XKB_LEFT: u32 = 0xFF51;
pub const XKB_UP: u32 = 0xFF52;
pub const XKB_RIGHT: u32 = 0xFF53;
pub const XKB_DOWN: u32 = 0xFF54;
pub const XKB_PAGE_UP: u32 = 0xFF55;
pub const XKB_PAGE_DOWN: u32 = 0xFF56;
pub const XKB_END: u32 = 0xFF57;
pub const XKB_DELETE: u32 = 0xFFFF;

/// Simulated device screen preset.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum DevicePreset {
    /// Samsung Galaxy A21s portrait (`720x1600`, handset).
    #[default]
    A21sPortrait,
    /// Samsung Galaxy A21s landscape (`1600x720`, handset).
    A21sLandscape,
    /// Xiaomi Pad 7 Pro portrait (`1600x2560`, tablet).
    MuyuPortrait,
    /// Xiaomi Pad 7 Pro landscape (`2560x1600`, tablet).
    MuyuLandscape,
    /// Arbitrary physical pixel dimensions.
    Custom(Size),
}

impl DevicePreset {
    /// Physical canvas resolution in pixels.
    #[must_use]
    pub const fn size(self) -> Size {
        match self {
            Self::A21sPortrait => Size::new(720, 1600),
            Self::A21sLandscape => Size::new(1600, 720),
            Self::MuyuPortrait => Size::new(1600, 2560),
            Self::MuyuLandscape => Size::new(2560, 1600),
            Self::Custom(s) => Size::new(if s.w > 0 { s.w } else { 1 }, if s.h > 0 { s.h } else { 1 }),
        }
    }

    /// Matching `device.toml` form factor label.
    #[must_use]
    pub const fn form_factor(self) -> &'static str {
        match self {
            Self::A21sPortrait | Self::A21sLandscape => "handset",
            Self::MuyuPortrait | Self::MuyuLandscape => "tablet",
            Self::Custom(s) => {
                let (long, short) = if s.w >= s.h { (s.w, s.h) } else { (s.h, s.w) };
                if long >= 1200 && short >= 1000 {
                    "tablet"
                } else {
                    "handset"
                }
            }
        }
    }

    /// Concise human label for window title and chrome badge.
    #[must_use]
    pub fn short_label(self) -> String {
        let s = self.size();
        match self {
            Self::A21sPortrait | Self::A21sLandscape => format!("A21s {}x{}", s.w, s.h),
            Self::MuyuPortrait | Self::MuyuLandscape => format!("Muyu {}x{}", s.w, s.h),
            Self::Custom(_) => format!("Custom {}x{}", s.w, s.h),
        }
    }

    /// Cycle through standard presets (`Cmd+R` / rotate shortcut).
    #[must_use]
    pub const fn cycle(self) -> Self {
        match self {
            Self::A21sPortrait => Self::A21sLandscape,
            Self::A21sLandscape => Self::MuyuPortrait,
            Self::MuyuPortrait => Self::MuyuLandscape,
            Self::MuyuLandscape | Self::Custom(_) => Self::A21sPortrait,
        }
    }

    /// Parse preset name or `WxH` string (case-insensitive).
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        let norm = raw.trim().to_ascii_lowercase();
        match norm.as_str() {
            "a21s" | "a21s-portrait" | "handset" => Some(Self::A21sPortrait),
            "a21s-landscape" => Some(Self::A21sLandscape),
            "muyu" | "muyu-portrait" | "tablet" => Some(Self::MuyuPortrait),
            "muyu-landscape" => Some(Self::MuyuLandscape),
            _ => Self::parse_size(&norm).map(Self::Custom),
        }
    }

    /// Parse `WxH` dimensions (e.g. `"720x1600"`).
    #[must_use]
    pub fn parse_size(raw: &str) -> Option<Size> {
        let trimmed = raw.trim();
        let (w_str, h_str) = trimmed.split_once(['x', 'X'])?;
        let w: i32 = w_str.trim().parse().ok()?;
        let h: i32 = h_str.trim().parse().ok()?;
        (w > 0 && h > 0 && w <= 8192 && h <= 8192).then_some(Size::new(w, h))
    }

    /// Logical desktop window size tailored to fit desktop/Retina screens cleanly.
    ///
    /// On a 2x Retina macOS screen, `(360, 800)` produces a crisp `720x1600`
    /// 1:1 physical framebuffer. Larger tablet presets scale down uniformly to
    /// remain comfortably within typical desktop bounds.
    #[must_use]
    pub fn initial_logical_window_size(self) -> (u32, u32) {
        match self {
            Self::A21sPortrait => (360, 800),
            Self::A21sLandscape => (800, 360),
            Self::MuyuPortrait => (533, 853),
            Self::MuyuLandscape => (1080, 675),
            Self::Custom(s) => {
                let w = f64::from(s.w.max(1));
                let h = f64::from(s.h.max(1));
                let base_div = if s.w >= 640 && s.h >= 640 { 2.0 } else { 1.0 };
                let lw = w / base_div;
                let lh = h / base_div;
                let fit = (1080.0 / lw).min(860.0 / lh).min(1.0);
                (
                    (lw * fit).round().max(160.0) as u32,
                    (lh * fit).round().max(160.0) as u32,
                )
            }
        }
    }
}

/// Runtime options for the Tier 1 host simulator window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimOptions {
    pub preset: DevicePreset,
    pub theme_override: Option<String>,
    pub show_chrome: bool,
    pub mock_osk: bool,
    pub shot_dir: PathBuf,
}

impl Default for SimOptions {
    fn default() -> Self {
        Self {
            preset: DevicePreset::A21sPortrait,
            theme_override: None,
            show_chrome: true,
            mock_osk: false,
            shot_dir: default_shot_dir_from(|k| std::env::var(k).ok()),
        }
    }
}

impl SimOptions {
    /// Load simulator options from `RABBIZ_SIM_*` environment variables.
    #[must_use]
    pub fn from_env() -> Self {
        Self::from_lookup(|k| std::env::var(k).ok())
    }

    /// Pure lookup-driven parser used by both [`Self::from_env`] and tests.
    #[must_use]
    pub fn from_lookup<F: Fn(&str) -> Option<String>>(get: F) -> Self {
        let mut opts = Self {
            shot_dir: default_shot_dir_from(&get),
            ..Self::default()
        };
        if let Some(dev) = get("RABBIZ_SIM_DEVICE") {
            if let Some(preset) = DevicePreset::parse(&dev) {
                opts.preset = preset;
            }
        }
        if let Some(sz) = get("RABBIZ_SIM_SIZE") {
            if let Some(size) = DevicePreset::parse_size(&sz) {
                opts.preset = DevicePreset::Custom(size);
            }
        }
        if let Some(theme) = get("RABBIZ_SIM_THEME") {
            let norm = theme.trim().to_ascii_lowercase();
            if norm == "light" || norm == "dark" {
                opts.theme_override = Some(norm);
            }
        }
        if let Some(chrome) = get("RABBIZ_SIM_CHROME") {
            if let Some(b) = parse_bool(&chrome) {
                opts.show_chrome = b;
            }
        }
        if let Some(osk) = get("RABBIZ_SIM_OSK") {
            if let Some(b) = parse_bool(&osk) {
                opts.mock_osk = b;
            }
        }
        opts
    }
}

fn parse_bool(raw: &str) -> Option<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn default_shot_dir_from<F: Fn(&str) -> Option<String>>(get: F) -> PathBuf {
    if let Some(dir) = get("RABBIZ_SIM_SHOT_DIR") {
        let trimmed = dir.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }
    if let Some(home) = get("HOME") {
        let trimmed = home.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed).join("Pictures");
        }
    }
    std::env::temp_dir()
}

/// Centered aspect-ratio-preserving placement of a device canvas in a host window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ViewportFit {
    pub dst_x: i32,
    pub dst_y: i32,
    pub dst_w: i32,
    pub dst_h: i32,
    pub src: Size,
}

impl ViewportFit {
    /// Compute centered letterboxed/pillarboxed rect in physical window pixels.
    #[must_use]
    pub fn compute_fit(win_w: u32, win_h: u32, canvas: Size) -> Self {
        let src = Size::new(canvas.w.max(1), canvas.h.max(1));
        if win_w == 0 || win_h == 0 {
            return Self {
                dst_x: 0,
                dst_y: 0,
                dst_w: 0,
                dst_h: 0,
                src,
            };
        }
        let ww = f64::from(win_w);
        let wh = f64::from(win_h);
        let sw = f64::from(src.w);
        let sh = f64::from(src.h);
        let scale = (ww / sw).min(wh / sh);
        let dst_w = (sw * scale).round().clamp(1.0, ww) as i32;
        let dst_h = (sh * scale).round().clamp(1.0, wh) as i32;
        let dst_x = (win_w as i32 - dst_w) / 2;
        let dst_y = (win_h as i32 - dst_h) / 2;
        Self {
            dst_x,
            dst_y,
            dst_w,
            dst_h,
            src,
        }
    }

    /// Map physical window coordinates to device canvas coordinates, returning
    /// `None` if outside the viewport rect.
    #[must_use]
    pub fn window_to_app(self, phys_x: f64, phys_y: f64) -> Option<Point> {
        if self.dst_w <= 0 || self.dst_h <= 0 {
            return None;
        }
        let lx = phys_x - f64::from(self.dst_x);
        let ly = phys_y - f64::from(self.dst_y);
        if lx < 0.0 || ly < 0.0 || lx >= f64::from(self.dst_w) || ly >= f64::from(self.dst_h) {
            return None;
        }
        let ax = ((lx * f64::from(self.src.w)) / f64::from(self.dst_w))
            .floor()
            .clamp(0.0, f64::from(self.src.w - 1)) as i32;
        let ay = ((ly * f64::from(self.src.h)) / f64::from(self.dst_h))
            .floor()
            .clamp(0.0, f64::from(self.src.h - 1)) as i32;
        Some(Point::new(ax, ay))
    }

    /// Map physical window coordinates clamped inside the device canvas bounds
    /// (keeps active touch drags smooth if the cursor leaves the bezel).
    #[must_use]
    pub fn window_to_app_clamped(self, phys_x: f64, phys_y: f64) -> Point {
        if self.dst_w <= 0 || self.dst_h <= 0 {
            return Point::ZERO;
        }
        let lx = (phys_x - f64::from(self.dst_x)).clamp(0.0, f64::from(self.dst_w - 1));
        let ly = (phys_y - f64::from(self.dst_y)).clamp(0.0, f64::from(self.dst_h - 1));
        let ax = ((lx * f64::from(self.src.w)) / f64::from(self.dst_w))
            .floor()
            .clamp(0.0, f64::from(self.src.w - 1)) as i32;
        let ay = ((ly * f64::from(self.src.h)) / f64::from(self.dst_h))
            .floor()
            .clamp(0.0, f64::from(self.src.h - 1)) as i32;
        Point::new(ax, ay)
    }

    /// Blit premultiplied RGBA8 `src` into a `softbuffer` `0x00RRGGBB` slice.
    pub fn blit_to_xrgb(
        self,
        src: &Pixels,
        dst: &mut [u32],
        win_w: u32,
        win_h: u32,
        backdrop_xrgb: u32,
    ) {
        let total = win_w as usize * win_h as usize;
        if dst.len() < total || src.size.is_empty() {
            return;
        }
        let expected_src = (src.size.area() * 4) as usize;
        if src.data.len() < expected_src {
            return;
        }

        let exact_fill = self.dst_x == 0
            && self.dst_y == 0
            && self.dst_w == win_w as i32
            && self.dst_h == win_h as i32;
        if !exact_fill {
            dst[..total].fill(backdrop_xrgb);
        }

        if self.dst_w <= 0 || self.dst_h <= 0 {
            return;
        }

        let ww = win_w as usize;
        let wh = win_h as usize;
        let x0 = self.dst_x.max(0) as usize;
        let y0 = self.dst_y.max(0) as usize;
        let x1 = (self.dst_x + self.dst_w).min(win_w as i32).max(0) as usize;
        let y1 = (self.dst_y + self.dst_h).min(win_h as i32).max(0) as usize;
        if x0 >= x1 || y0 >= y1 || x1 > ww || y1 > wh {
            return;
        }

        let sw = src.size.w as usize;
        let sh = src.size.h as usize;

        if self.dst_w == src.size.w && self.dst_h == src.size.h {
            // 1:1 pixel-perfect fast path.
            for dy in y0..y1 {
                let sy = dy - self.dst_y as usize;
                let srow = sy * sw;
                let drow = dy * ww;
                for dx in x0..x1 {
                    let sx = dx - self.dst_x as usize;
                    let si = (srow + sx) * 4;
                    dst[drow + dx] = rgba_to_xrgb(
                        src.data[si],
                        src.data[si + 1],
                        src.data[si + 2],
                    );
                }
            }
            return;
        }

        // Precompute bilinear source X coordinates and 8-bit fractional weights.
        let dw = self.dst_w as usize;
        let dh = self.dst_h as usize;
        let sx_lut: Vec<(usize, usize, u32)> = (x0..x1)
            .map(|dx| {
                let rel_x = dx - self.dst_x as usize;
                let fx_256 = if dw > 1 && sw > 1 {
                    ((rel_x * (sw - 1) * 256) + (dw / 2)) / (dw - 1)
                } else {
                    0
                };
                let sx0 = (fx_256 >> 8).min(sw - 1);
                let sx1 = (sx0 + 1).min(sw - 1);
                let wx = (fx_256 & 0xFF) as u32;
                (sx0, sx1, wx)
            })
            .collect();

        for dy in y0..y1 {
            let rel_y = dy - self.dst_y as usize;
            let fy_256 = if dh > 1 && sh > 1 {
                ((rel_y * (sh - 1) * 256) + (dh / 2)) / (dh - 1)
            } else {
                0
            };
            let sy0 = (fy_256 >> 8).min(sh - 1);
            let sy1 = (sy0 + 1).min(sh - 1);
            let wy = (fy_256 & 0xFF) as u32;
            let iwy = 256 - wy;
            let srow0 = sy0 * sw;
            let srow1 = sy1 * sw;
            let drow = dy * ww;
            for (offset, &(sx0, sx1, wx)) in sx_lut.iter().enumerate() {
                let iwx = 256 - wx;
                let w00 = iwx * iwy;
                let w10 = wx * iwy;
                let w01 = iwx * wy;
                let w11 = wx * wy;
                let i00 = (srow0 + sx0) * 4;
                let i10 = (srow0 + sx1) * 4;
                let i01 = (srow1 + sx0) * 4;
                let i11 = (srow1 + sx1) * 4;
                let r = ((src.data[i00] as u32 * w00
                    + src.data[i10] as u32 * w10
                    + src.data[i01] as u32 * w01
                    + src.data[i11] as u32 * w11
                    + 32768)
                    >> 16) as u8;
                let g = ((src.data[i00 + 1] as u32 * w00
                    + src.data[i10 + 1] as u32 * w10
                    + src.data[i01 + 1] as u32 * w01
                    + src.data[i11 + 1] as u32 * w11
                    + 32768)
                    >> 16) as u8;
                let b = ((src.data[i00 + 2] as u32 * w00
                    + src.data[i10 + 2] as u32 * w10
                    + src.data[i01 + 2] as u32 * w01
                    + src.data[i11 + 2] as u32 * w11
                    + 32768)
                    >> 16) as u8;
                dst[drow + x0 + offset] = rgba_to_xrgb(r, g, b);
            }
        }
    }
}

#[inline]
const fn rgba_to_xrgb(r: u8, g: u8, b: u8) -> u32 {
    ((r as u32) << 16) | ((g as u32) << 8) | (b as u32)
}

/// Draw translucent simulator chrome (status safe-zone indicators, gesture pill,
/// and optional mock OSK sheet) over a rendered app frame.
pub fn draw_sim_chrome(
    canvas: &mut Canvas,
    theme: &Theme,
    preset: DevicePreset,
    theme_mode: &str,
    ime_active: bool,
    mock_osk: bool,
) {
    let size = canvas.size();
    let w = size.w;
    let h = size.h;
    if w < 120 || h < 120 {
        return;
    }

    // Subtle status bar safe-area backdrop inside `0..ui::TOP_SAFE`.
    let top_bg = [theme.surface1[0], theme.surface1[1], theme.surface1[2], 185];
    canvas.fill_rect(Rect::new(0, 0, w, ui::TOP_SAFE), top_bg);
    canvas.fill_rect(Rect::new(0, ui::TOP_SAFE - 1, w, 1), theme.line);

    let time_style = ui::styled(18.0, 600, theme.text);
    ui::draw_text(canvas, "09:41", &time_style, 20, 12, Some(90));

    let badge = format!(
        "SIM · {} · {}",
        preset.short_label(),
        if theme_mode == "dark" { "dark" } else { "light" }
    );
    let badge_style = ui::styled(15.0, 500, theme.text3);
    ui::draw_text(canvas, &badge, &badge_style, 96, 14, Some((w - 200).max(60)));

    if ime_active {
        let pill = Rect::new((w - 86).max(0), 9, 66, 26);
        canvas.fill_rounded_rect(pill, 13.0, theme.accent);
        ui::draw_centered(canvas, "IME", &ui::styled(14.0, 600, theme.text_on_accent), pill);
    }

    // Optional mock OSK overlay so developers see bottom IME occlusion.
    if mock_osk {
        let osk_h = (h / 3).clamp(240, 520).min(h - ui::TOP_SAFE - 24);
        let osk_y = h - osk_h;
        let osk_bg = [theme.surface2[0], theme.surface2[1], theme.surface2[2], 238];
        canvas.fill_rect(Rect::new(0, osk_y, w, osk_h), osk_bg);
        canvas.fill_rect(Rect::new(0, osk_y, w, 2), theme.line);

        ui::draw_text(
            canvas,
            "Mock OSK (Cmd+K to hide · type with host keyboard)",
            &ui::styled(16.0, 500, theme.text2),
            20,
            osk_y + 12,
            Some((w - 40).max(40)),
        );

        // Simple 4-row key grid outline to match visual density of rabbiz-osk.
        let key_top = osk_y + 46;
        let rows = 4;
        let row_area_h = (osk_h - 46 - ui::BOTTOM_SAFE).max(80);
        let row_h = (row_area_h / rows - 8).max(24);
        let cols = 10;
        let pad_x = 12;
        let key_w = ((w - pad_x * 2 - (cols - 1) * 6) / cols).max(16);
        let key_fill = [theme.surface1[0], theme.surface1[1], theme.surface1[2], 230];
        for r in 0..rows {
            let ky = key_top + r * (row_h + 8);
            for c in 0..cols {
                let kx = pad_x + c * (key_w + 6);
                canvas.fill_rounded_rect(Rect::new(kx, ky, key_w, row_h), 8.0, key_fill);
            }
        }
    }

    // Bottom navigation gesture pill inside `h - ui::BOTTOM_SAFE..h`.
    let pill_w = (w / 4).clamp(110, 220);
    let pill_h = 8;
    let pill_x = (w - pill_w) / 2;
    let pill_y = h - (tokens::GESTURE_BAR_HEIGHT / 2) - (pill_h / 2);
    let pill_color = [theme.text[0], theme.text[1], theme.text[2], 140];
    canvas.fill_rounded_rect(Rect::new(pill_x, pill_y, pill_w, pill_h), 4.0, pill_color);
}

/// Convert a Unicode scalar into an XKB keysym (matching xkbcommon's convention).
#[must_use]
pub const fn char_to_xkb(ch: char) -> u32 {
    let cp = ch as u32;
    if cp <= 0xFF {
        cp
    } else {
        0x0100_0000 | cp
    }
}

/// Filter `AppEvent::Key::utf8` so control keys (`Backspace`, `Escape`, `Delete`,
/// `Tab`, navigation keys) do NOT trigger `utf8: Some(...)` arms ahead of
/// `keysym` match arms in apps such as `rabbiz-calculator`.
///
/// `Return` (`0xFF0D`) keeps `Some("\r")` because apps match either
/// `keysym: 0xff0d` or `utf8: Some(s)` containing `'\r'`.
#[must_use]
pub fn filter_key_utf8(keysym: u32, raw_text: Option<&str>) -> Option<String> {
    match keysym {
        XKB_RETURN => Some("\r".to_string()),
        XKB_BACKSPACE
        | XKB_TAB
        | XKB_ESCAPE
        | XKB_DELETE
        | XKB_HOME..=XKB_END => None,
        _ => raw_text.and_then(|s| {
            if s.is_empty() || s.chars().all(char::is_control) {
                None
            } else {
                Some(s.to_string())
            }
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn device_preset_parsing_and_cycling() {
        assert_eq!(DevicePreset::parse("a21s"), Some(DevicePreset::A21sPortrait));
        assert_eq!(
            DevicePreset::parse("A21S-LANDSCAPE"),
            Some(DevicePreset::A21sLandscape)
        );
        assert_eq!(DevicePreset::parse("muyu"), Some(DevicePreset::MuyuPortrait));
        assert_eq!(
            DevicePreset::parse("1080x2400"),
            Some(DevicePreset::Custom(Size::new(1080, 2400)))
        );
        assert_eq!(DevicePreset::parse("0x500"), None);
        assert_eq!(DevicePreset::parse("not-a-size"), None);

        let p0 = DevicePreset::A21sPortrait;
        assert_eq!(p0.size(), Size::new(720, 1600));
        assert_eq!(p0.form_factor(), "handset");
        assert_eq!(p0.initial_logical_window_size(), (360, 800));
        let p1 = p0.cycle();
        assert_eq!(p1, DevicePreset::A21sLandscape);
        assert_eq!(p1.cycle(), DevicePreset::MuyuPortrait);
        assert_eq!(p1.cycle().form_factor(), "tablet");
        assert_eq!(p1.cycle().cycle().cycle(), DevicePreset::A21sPortrait);
    }

    #[test]
    fn sim_options_from_lookup_parses_env_overrides() {
        let mut env = HashMap::new();
        env.insert("RABBIZ_SIM_DEVICE", "muyu-landscape");
        env.insert("RABBIZ_SIM_THEME", "DARK");
        env.insert("RABBIZ_SIM_CHROME", "false");
        env.insert("RABBIZ_SIM_OSK", "1");
        env.insert("RABBIZ_SIM_SHOT_DIR", "/tmp/rabbiz-shots");

        let opts = SimOptions::from_lookup(|k| env.get(k).map(|s| (*s).to_string()));
        assert_eq!(opts.preset, DevicePreset::MuyuLandscape);
        assert_eq!(opts.theme_override.as_deref(), Some("dark"));
        assert!(!opts.show_chrome);
        assert!(opts.mock_osk);
        assert_eq!(opts.shot_dir, PathBuf::from("/tmp/rabbiz-shots"));

        // Explicit RABBIZ_SIM_SIZE overrides device preset.
        env.insert("RABBIZ_SIM_SIZE", "800x1280");
        let opts2 = SimOptions::from_lookup(|k| env.get(k).map(|s| (*s).to_string()));
        assert_eq!(opts2.preset, DevicePreset::Custom(Size::new(800, 1280)));
    }

    #[test]
    fn viewport_fit_maps_exact_and_letterboxed_coordinates() {
        // Exact 1:1 fit
        let fit_exact = ViewportFit::compute_fit(720, 1600, Size::new(720, 1600));
        assert_eq!(fit_exact.dst_x, 0);
        assert_eq!(fit_exact.dst_y, 0);
        assert_eq!(fit_exact.dst_w, 720);
        assert_eq!(fit_exact.dst_h, 1600);
        assert_eq!(fit_exact.window_to_app(0.0, 0.0), Some(Point::new(0, 0)));
        assert_eq!(
            fit_exact.window_to_app(719.0, 1599.0),
            Some(Point::new(719, 1599))
        );
        assert_eq!(fit_exact.window_to_app(720.0, 0.0), None);

        // Letterboxed 2x wider window (pillarboxed)
        let fit_wide = ViewportFit::compute_fit(1440, 1600, Size::new(720, 1600));
        assert_eq!(fit_wide.dst_x, 360);
        assert_eq!(fit_wide.dst_y, 0);
        assert_eq!(fit_wide.dst_w, 720);
        assert_eq!(fit_wide.dst_h, 1600);
        assert_eq!(fit_wide.window_to_app(100.0, 500.0), None);
        assert_eq!(fit_wide.window_to_app(360.0, 100.0), Some(Point::new(0, 100)));
        assert_eq!(fit_wide.window_to_app_clamped(10.0, 100.0), Point::new(0, 100));

        // Downscaled half-size window
        let fit_half = ViewportFit::compute_fit(360, 800, Size::new(720, 1600));
        assert_eq!(fit_half.window_to_app(180.0, 400.0), Some(Point::new(360, 800)));
    }

    #[test]
    fn viewport_blit_to_xrgb_writes_bezel_and_pixels() {
        let mut src = Pixels::empty(Size::new(2, 2));
        // (0,0)=red, (1,0)=green, (0,1)=blue, (1,1)=white
        src.data = vec![
            0xFF, 0x00, 0x00, 0xFF, 0x00, 0xFF, 0x00, 0xFF, 0x00, 0x00, 0xFF, 0xFF, 0xFF, 0xFF,
            0xFF, 0xFF,
        ];
        let fit = ViewportFit::compute_fit(4, 2, src.size);
        let mut dst = vec![0u32; 8];
        fit.blit_to_xrgb(&src, &mut dst, 4, 2, BEZEL_XRGB);
        // dst_w = 2, dst_h = 2, dst_x = 1, dst_y = 0
        assert_eq!(dst[0], BEZEL_XRGB);
        assert_eq!(dst[1], 0x00FF_0000);
        assert_eq!(dst[2], 0x0000_FF00);
        assert_eq!(dst[3], BEZEL_XRGB);
        assert_eq!(dst[4], BEZEL_XRGB);
        assert_eq!(dst[5], 0x0000_00FF);
        assert_eq!(dst[6], 0x00FF_FFFF);
        assert_eq!(dst[7], BEZEL_XRGB);
    }

    #[test]
    fn chrome_overlay_draws_without_panicking_in_light_and_dark() {
        for (theme, mode) in [(&tokens::LIGHT, "light"), (&tokens::DARK, "dark")] {
            let mut canvas = Canvas::filled(Size::new(720, 1600), theme.bg);
            let before = canvas.pixmap().data().to_vec();
            draw_sim_chrome(
                &mut canvas,
                theme,
                DevicePreset::A21sPortrait,
                mode,
                true,
                true,
            );
            assert_ne!(canvas.pixmap().data(), before.as_slice());
        }
    }

    #[test]
    fn xkb_and_utf8_filtering_protects_backspace_and_handles_thai() {
        assert_eq!(char_to_xkb('A'), 0x41);
        assert_eq!(char_to_xkb('ก'), 0x0100_0E01);
        assert_eq!(filter_key_utf8(XKB_BACKSPACE, Some("\u{8}")), None);
        assert_eq!(filter_key_utf8(XKB_ESCAPE, Some("\u{1b}")), None);
        assert_eq!(filter_key_utf8(XKB_DELETE, Some("\u{7f}")), None);
        assert_eq!(filter_key_utf8(XKB_LEFT, None), None);
        assert_eq!(filter_key_utf8(XKB_RETURN, Some("\n")), Some("\r".into()));
        assert_eq!(filter_key_utf8(char_to_xkb('7'), Some("7")), Some("7".into()));
        assert_eq!(
            filter_key_utf8(char_to_xkb('ก'), Some("ก")),
            Some("ก".into())
        );
    }
}
