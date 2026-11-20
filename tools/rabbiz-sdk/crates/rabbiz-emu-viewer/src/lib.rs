//! Host Emulator Viewer (`rabbiz-emu-viewer`) for RabbizOS.
//!
//! Renders the simulated device bezel + live framebuffer (streamed over TCP
//! from `rabbiz-de --backend=virtual --emu-listen=127.0.0.1:5910` or rendered
//! via the built-in interactive RabbizOS preview screen when offline / `--demo`)
//! alongside a **320 px side control panel** with hardware buttons, navigation
//! gestures, and mock hardware toggles.

#![forbid(unsafe_code)]

use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::Path;
use std::time::Duration;

use de_api::{Pixels, Point, Rect, Size};
use de_design::tokens::{self, Theme};
use de_paint::{encode_png, Canvas};
use rabbiz_app_kit::DevicePreset;
use rabbiz_emu_proto::client_poll_frame;

/// Fixed width (px) of the side hardware & gesture control panel.
pub const SIDE_PANEL_WIDTH: i32 = 320;

/// Padding around the phone bezel inside the viewer canvas.
pub const BEZEL_PAD: i32 = 24;

/// Clickable actions in the 320 px side control panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlButton {
    // Hardware buttons
    Power,
    VolumeUp,
    VolumeDown,
    RotatePreset,
    // Navigation gestures
    SwipeHome,
    SwipeBack,
    RecentsHold,
    PullShade,
    // Theme & Mock Hardware
    ToggleTheme,
    Battery15,
    Battery85,
    BatteryCharge,
    WifiToggle,
    PushHunNotification,
    IncomingCall,
    SaveScreenshot,
}

impl ControlButton {
    pub const ALL: [Self; 16] = [
        Self::Power,
        Self::VolumeUp,
        Self::VolumeDown,
        Self::RotatePreset,
        Self::SwipeHome,
        Self::SwipeBack,
        Self::RecentsHold,
        Self::PullShade,
        Self::ToggleTheme,
        Self::Battery15,
        Self::Battery85,
        Self::BatteryCharge,
        Self::WifiToggle,
        Self::PushHunNotification,
        Self::IncomingCall,
        Self::SaveScreenshot,
    ];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Power => "Power",
            Self::VolumeUp => "Vol +",
            Self::VolumeDown => "Vol -",
            Self::RotatePreset => "Rotate (Preset)",
            Self::SwipeHome => "Swipe Home",
            Self::SwipeBack => "Swipe Back",
            Self::RecentsHold => "Recents Hold",
            Self::PullShade => "Pull Shade",
            Self::ToggleTheme => "Light/Dark",
            Self::Battery15 => "Battery 15%",
            Self::Battery85 => "Battery 85%",
            Self::BatteryCharge => "Charge Toggle",
            Self::WifiToggle => "Wi-Fi Toggle",
            Self::PushHunNotification => "Push HUN Notification",
            Self::IncomingCall => "Incoming Call",
            Self::SaveScreenshot => "Save Screenshot",
        }
    }
}

/// One laid-out button in the 320 px side control panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PanelButtonSlot {
    pub button: ControlButton,
    pub rect: Rect,
}

/// Complete viewer geometry: phone screen viewport + 320 px side control panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewerLayout {
    pub total_size: Size,
    pub phone_bezel_rect: Rect,
    pub phone_screen_rect: Rect,
    pub side_panel_rect: Rect,
    pub buttons: Vec<PanelButtonSlot>,
}

impl ViewerLayout {
    /// Compute viewer layout for a given `screen_display_size` (e.g. `450x1000` scaled view of `720x1600`).
    #[must_use]
    pub fn compute(screen_display_size: Size) -> Self {
        let sw = screen_display_size.w.max(240);
        let sh = screen_display_size.h.max(480);

        let bezel_rect = Rect::new(BEZEL_PAD - 8, BEZEL_PAD - 8, sw + 16, sh + 16);
        let screen_rect = Rect::new(BEZEL_PAD, BEZEL_PAD, sw, sh);
        let panel_x = BEZEL_PAD * 2 + sw;
        let total_h = (sh + BEZEL_PAD * 2).max(760);
        let side_panel_rect = Rect::new(panel_x, 0, SIDE_PANEL_WIDTH, total_h);
        let total_size = Size::new(panel_x + SIDE_PANEL_WIDTH, total_h);

        let mut buttons = Vec::with_capacity(ControlButton::ALL.len());
        let col_w = (SIDE_PANEL_WIDTH - 36) / 2;
        let row_h = 40;

        // Group 1: Hardware buttons (4 buttons, 2x2 grid starting at y = 84)
        let hw = [
            ControlButton::Power,
            ControlButton::RotatePreset,
            ControlButton::VolumeUp,
            ControlButton::VolumeDown,
        ];
        for (i, &btn) in hw.iter().enumerate() {
            let col = (i % 2) as i32;
            let row = (i / 2) as i32;
            buttons.push(PanelButtonSlot {
                button: btn,
                rect: Rect::new(
                    panel_x + 12 + col * (col_w + 12),
                    84 + row * (row_h + 10),
                    col_w,
                    row_h,
                ),
            });
        }

        // Group 2: Navigation gestures (4 buttons, 2x2 grid starting at y = 224)
        let nav = [
            ControlButton::SwipeHome,
            ControlButton::SwipeBack,
            ControlButton::RecentsHold,
            ControlButton::PullShade,
        ];
        for (i, &btn) in nav.iter().enumerate() {
            let col = (i % 2) as i32;
            let row = (i / 2) as i32;
            buttons.push(PanelButtonSlot {
                button: btn,
                rect: Rect::new(
                    panel_x + 12 + col * (col_w + 12),
                    224 + row * (row_h + 10),
                    col_w,
                    row_h,
                ),
            });
        }

        // Group 3: Theme & Mock Hardware (8 buttons, 2x4 grid starting at y = 368)
        let mock = [
            ControlButton::ToggleTheme,
            ControlButton::WifiToggle,
            ControlButton::Battery15,
            ControlButton::Battery85,
            ControlButton::BatteryCharge,
            ControlButton::PushHunNotification,
            ControlButton::IncomingCall,
            ControlButton::SaveScreenshot,
        ];
        for (i, &btn) in mock.iter().enumerate() {
            let col = (i % 2) as i32;
            let row = (i / 2) as i32;
            buttons.push(PanelButtonSlot {
                button: btn,
                rect: Rect::new(
                    panel_x + 12 + col * (col_w + 12),
                    368 + row * (row_h + 10),
                    col_w,
                    row_h,
                ),
            });
        }

        Self {
            total_size,
            phone_bezel_rect: bezel_rect,
            phone_screen_rect: screen_rect,
            side_panel_rect,
            buttons,
        }
    }

    /// Hit-test a viewer click against the 320 px side control panel buttons.
    #[must_use]
    pub fn hit_button(&self, p: Point) -> Option<ControlButton> {
        self.buttons
            .iter()
            .find(|slot| slot.rect.contains(p))
            .map(|slot| slot.button)
    }

    /// Map a viewer click inside `phone_screen_rect` into device coordinates (`device_size`).
    #[must_use]
    pub fn viewer_to_device(&self, p: Point, device_size: Size) -> Option<Point> {
        if !self.phone_screen_rect.contains(p) || device_size.is_empty() {
            return None;
        }
        let rel_x = p.x - self.phone_screen_rect.x;
        let rel_y = p.y - self.phone_screen_rect.y;
        let dx = ((i64::from(rel_x) * i64::from(device_size.w))
            / i64::from(self.phone_screen_rect.w.max(1))) as i32;
        let dy = ((i64::from(rel_y) * i64::from(device_size.h))
            / i64::from(self.phone_screen_rect.h.max(1))) as i32;
        Some(Point::new(
            dx.clamp(0, device_size.w - 1),
            dy.clamp(0, device_size.h - 1),
        ))
    }
}

/// Interactive state of `rabbiz-emu-viewer` (both online TCP and offline `--demo` mode).
pub struct ViewerState {
    pub emu_addr: String,
    pub ctl_addr: String,
    pub preset: DevicePreset,
    pub dark_mode: bool,
    pub battery_pct: u8,
    pub charging: bool,
    pub wifi_on: bool,
    pub screen_name: String,
    pub banner: Option<String>,
    pub last_touch: Option<Point>,
    pub connected: bool,
    pub frame_pixels: Pixels,
    pub pressed_button: Option<ControlButton>,
}

impl ViewerState {
    #[must_use]
    pub fn new(emu_addr: String, ctl_addr: String) -> Self {
        let preset = DevicePreset::A21sPortrait;
        let mut s = Self {
            emu_addr,
            ctl_addr,
            preset,
            dark_mode: false,
            battery_pct: 85,
            charging: false,
            wifi_on: true,
            screen_name: "Home · ~/Desktop".to_string(),
            banner: None,
            last_touch: None,
            connected: false,
            frame_pixels: Pixels::empty(preset.size()),
            pressed_button: None,
        };
        s.render_offline_demo_phone();
        s
    }

    #[must_use]
    pub fn active_theme(&self) -> Theme {
        if self.dark_mode {
            tokens::DARK
        } else {
            tokens::LIGHT
        }
    }

    #[must_use]
    pub fn display_phone_size(&self) -> Size {
        let dev = self.preset.size();
        if dev.w <= dev.h {
            Size::new(360, 800)
        } else {
            Size::new(800, 360)
        }
    }

    #[must_use]
    pub fn layout(&self) -> ViewerLayout {
        ViewerLayout::compute(self.display_phone_size())
    }

    /// Poll a live frame from `emu_addr`; if offline, falls back to the built-in interactive phone preview.
    pub fn poll_remote_frame(&mut self) -> bool {
        let Ok(addr) = self.emu_addr.parse::<SocketAddr>() else {
            self.connected = false;
            return false;
        };
        let Ok(mut stream) = TcpStream::connect_timeout(&addr, Duration::from_millis(80)) else {
            if self.connected {
                self.connected = false;
                self.render_offline_demo_phone();
            }
            return false;
        };
        let _ = stream.set_read_timeout(Some(Duration::from_millis(250)));
        let _ = stream.set_write_timeout(Some(Duration::from_millis(250)));
        if let Ok(frame) = client_poll_frame(&mut stream) {
            if frame.decode_into(&mut self.frame_pixels).is_ok() {
                self.connected = true;
                return true;
            }
        }
        false
    }

    /// Send a JSON IPC command line to `ctl_addr` when connected to a running `rabbiz-de` instance.
    pub fn send_ctl_json(&self, payload: &serde_json::Value) -> Result<String, String> {
        let addr: SocketAddr = self
            .ctl_addr
            .parse()
            .map_err(|e| format!("invalid ctl_addr {}: {e}", self.ctl_addr))?;
        let mut stream = TcpStream::connect_timeout(&addr, Duration::from_millis(120))
            .map_err(|e| format!("connect ctl {addr}: {e}"))?;
        let _ = stream.set_read_timeout(Some(Duration::from_millis(400)));
        let _ = stream.set_write_timeout(Some(Duration::from_millis(400)));
        let mut line = serde_json::to_string(payload).map_err(|e| e.to_string())?;
        line.push('\n');
        stream
            .write_all(line.as_bytes())
            .map_err(|e| format!("write ctl: {e}"))?;
        let mut reader = BufReader::new(stream);
        let mut reply = String::new();
        reader
            .read_line(&mut reply)
            .map_err(|e| format!("read ctl: {e}"))?;
        Ok(reply)
    }

    /// Inject a touch event (`"down"`, `"move"`, `"up"`) at device pixel `pos`.
    pub fn inject_touch(&mut self, phase: &str, pos: Point) {
        self.last_touch = Some(pos);
        let cmd = serde_json::json!({
            "id": 1,
            "cmd": "inject_touch",
            "phase": phase,
            "x": pos.x,
            "y": pos.y,
        });
        if self.send_ctl_json(&cmd).is_err() {
            self.render_offline_demo_phone();
        }
    }

    /// Trigger a side-panel [`ControlButton`] action.
    pub fn activate_button(&mut self, button: ControlButton) {
        self.pressed_button = Some(button);
        match button {
            ControlButton::Power => {
                self.screen_name = if self.screen_name.contains("Lock") {
                    "Home · ~/Desktop".to_string()
                } else {
                    "Keyguard · Locked".to_string()
                };
                let _ = self.send_ctl_json(&serde_json::json!({"id": 1, "cmd": "lock"}));
            }
            ControlButton::VolumeUp => {
                self.banner = Some("Volume: 75%".to_string());
                let _ = self.send_ctl_json(&serde_json::json!({
                    "id": 1,
                    "cmd": "show_osd",
                    "kind": "volume",
                    "percent": 75,
                    "muted": false
                }));
            }
            ControlButton::VolumeDown => {
                self.banner = Some("Volume: 35%".to_string());
                let _ = self.send_ctl_json(&serde_json::json!({
                    "id": 1,
                    "cmd": "show_osd",
                    "kind": "volume",
                    "percent": 35,
                    "muted": false
                }));
            }
            ControlButton::RotatePreset => {
                self.preset = self.preset.cycle();
                self.frame_pixels = Pixels::empty(self.preset.size());
            }
            ControlButton::SwipeHome => {
                self.screen_name = "Home · ~/Desktop".to_string();
                self.banner = None;
                let dev = self.preset.size();
                self.inject_touch("down", Point::new(dev.w / 2, dev.h - 8));
                self.inject_touch("up", Point::new(dev.w / 2, dev.h - 280));
            }
            ControlButton::SwipeBack => {
                self.screen_name = "Back Navigation".to_string();
                let dev = self.preset.size();
                self.inject_touch("down", Point::new(8, dev.h / 2));
                self.inject_touch("up", Point::new(180, dev.h / 2));
            }
            ControlButton::RecentsHold => {
                self.screen_name = "Recents Carousel".to_string();
            }
            ControlButton::PullShade => {
                self.screen_name = "Quick Settings & Shade".to_string();
                let _ = self.send_ctl_json(&serde_json::json!({
                    "id": 1,
                    "cmd": "set_panel",
                    "panel": "quick",
                    "open": true
                }));
            }
            ControlButton::ToggleTheme => {
                self.dark_mode = !self.dark_mode;
                let _ = self.send_ctl_json(&serde_json::json!({"id": 1, "cmd": "reload_theme"}));
            }
            ControlButton::Battery15 => {
                self.battery_pct = 15;
                self.charging = false;
            }
            ControlButton::Battery85 => {
                self.battery_pct = 85;
            }
            ControlButton::BatteryCharge => {
                self.charging = !self.charging;
            }
            ControlButton::WifiToggle => {
                self.wifi_on = !self.wifi_on;
            }
            ControlButton::PushHunNotification => {
                self.banner = Some("HUN: New message from RabbizOS Team".to_string());
            }
            ControlButton::IncomingCall => {
                self.banner = Some("Incoming Call · +66 81 234 5678".to_string());
            }
            ControlButton::SaveScreenshot => {
                let path = std::env::temp_dir().join("rabbiz-emu-viewer-shot.png");
                let _ = self.save_viewer_png(&path);
                self.banner = Some(format!("Saved {}", path.display()));
            }
        }
        if !self.connected {
            self.render_offline_demo_phone();
        }
    }

    /// Render the built-in interactive RabbizOS phone screen when offline or `--demo`.
    pub fn render_offline_demo_phone(&mut self) {
        let size = self.preset.size();
        let theme = self.active_theme();
        let mut c = Canvas::filled(size, theme.bg);

        // Status bar
        c.fill_rect(Rect::new(0, 0, size.w, 56), theme.surface1);
        rabbiz_app_kit::ui::draw_text(
            &mut c,
            "09:41",
            &rabbiz_app_kit::ui::styled(20.0, 600, theme.text),
            24,
            16,
            Some(120),
        );
        let status_right = format!(
            "{} · {}%{}",
            if self.wifi_on { "Wi-Fi" } else { "Offline" },
            self.battery_pct,
            if self.charging { " ⚡" } else { "" }
        );
        rabbiz_app_kit::ui::draw_text_right(
            &mut c,
            &status_right,
            &rabbiz_app_kit::ui::styled(18.0, 500, theme.text2),
            Rect::new(size.w - 300, 0, 276, 56),
            12,
        );

        // Title & screen card
        let y = rabbiz_app_kit::ui::draw_header(
            &mut c,
            &theme,
            "RabbizOS Emulator",
            &format!("{} · {}", self.preset.short_label(), self.screen_name),
        );

        if let Some(banner) = &self.banner {
            let hun = Rect::new(24, y + 8, size.w - 48, 72);
            c.fill_rounded_rect(hun, 24.0, theme.accent);
            rabbiz_app_kit::ui::draw_centered(
                &mut c,
                banner,
                &rabbiz_app_kit::ui::styled(20.0, 600, theme.text_on_accent),
                hun,
            );
        }

        // App icon tiles grid preview
        let grid_top = y + 104;
        let apps = [
            "Files", "Notes", "Clock", "Calc", "Settings", "Terminal", "Music", "Photos",
        ];
        let cols = 4;
        let cell_w = (size.w - 48) / cols;
        for (idx, name) in apps.iter().enumerate() {
            let col = (idx as i32) % cols;
            let row = (idx as i32) / cols;
            let tx = 24 + col * cell_w + (cell_w - 96) / 2;
            let ty = grid_top + row * 160;
            let tile = Rect::new(tx, ty, 96, 96);
            c.fill_rounded_rect(tile, 28.0, theme.icon_tile);
            rabbiz_app_kit::ui::draw_centered(
                &mut c,
                &name[..1],
                &rabbiz_app_kit::ui::styled(34.0, 600, theme.icon_glyph),
                tile,
            );
            rabbiz_app_kit::ui::draw_centered(
                &mut c,
                name,
                &rabbiz_app_kit::ui::caption(theme.text),
                Rect::new(tx - 12, ty + 100, 120, 28),
            );
        }

        // Bottom dock + gesture pill
        let dock = Rect::new(24, size.h - 168, size.w - 48, 112);
        c.fill_rounded_rect(dock, 36.0, theme.surface1);
        if let Some(pt) = self.last_touch {
            rabbiz_app_kit::ui::draw_centered(
                &mut c,
                &format!("Last Touch: ({}, {})", pt.x, pt.y),
                &rabbiz_app_kit::ui::body(theme.text2),
                dock,
            );
        } else {
            rabbiz_app_kit::ui::draw_centered(
                &mut c,
                "Click phone screen or 320px side panel controls",
                &rabbiz_app_kit::ui::body(theme.text2),
                dock,
            );
        }

        let pill = Rect::new((size.w - 180) / 2, size.h - 24, 180, 8);
        c.fill_rounded_rect(pill, 4.0, theme.text2);

        self.frame_pixels = c.into_pixels();
    }

    /// Render the complete viewer window (phone bezel + scaled phone screen + 320 px side control panel).
    #[must_use]
    pub fn render_viewer_frame(&self) -> Pixels {
        let layout = self.layout();
        let theme = self.active_theme();
        let mut canvas = Canvas::filled(layout.total_size, [0x11, 0x0E, 0x14, 0xFF]);

        // Phone outer bezel
        canvas.fill_rounded_rect(layout.phone_bezel_rect, 28.0, [0x26, 0x20, 0x2C, 0xFF]);

        // Blit scaled phone framebuffer into `phone_screen_rect`
        canvas.blit_scaled(&self.frame_pixels, layout.phone_screen_rect, 1.0);

        // Side control panel background (320 px wide)
        canvas.fill_rect(layout.side_panel_rect, theme.surface1);
        canvas.fill_rect(
            Rect::new(layout.side_panel_rect.x, 0, 2, layout.side_panel_rect.h),
            theme.line,
        );

        let px = layout.side_panel_rect.x + 14;
        rabbiz_app_kit::ui::draw_text(
            &mut canvas,
            "RabbizOS Emulator",
            &rabbiz_app_kit::ui::styled(20.0, 600, theme.text),
            px,
            16,
            Some(SIDE_PANEL_WIDTH - 24),
        );
        let conn_str = if self.connected {
            format!("LIVE · {}", self.emu_addr)
        } else {
            format!("DEMO · {}", self.preset.short_label())
        };
        rabbiz_app_kit::ui::draw_text(
            &mut canvas,
            &conn_str,
            &rabbiz_app_kit::ui::styled(14.0, 500, theme.accent),
            px,
            42,
            Some(SIDE_PANEL_WIDTH - 24),
        );

        // Section headers
        for (title_str, sy) in [
            ("HARDWARE KEYS", 64),
            ("NAVIGATION GESTURES", 204),
            ("THEME & MOCK HARDWARE", 348),
        ] {
            rabbiz_app_kit::ui::draw_text(
                &mut canvas,
                title_str,
                &rabbiz_app_kit::ui::styled(13.0, 600, theme.text3),
                px,
                sy,
                Some(SIDE_PANEL_WIDTH - 24),
            );
        }

        for slot in &layout.buttons {
            let is_pressed = self.pressed_button == Some(slot.button);
            let bg = if is_pressed {
                theme.accent
            } else {
                theme.surface3
            };
            let fg = if is_pressed {
                theme.text_on_accent
            } else {
                theme.text
            };
            canvas.fill_rounded_rect(slot.rect, 12.0, bg);
            rabbiz_app_kit::ui::draw_centered(
                &mut canvas,
                slot.button.label(),
                &rabbiz_app_kit::ui::styled(14.0, 600, fg),
                slot.rect,
            );
        }

        canvas.into_pixels()
    }

    /// Save the current full viewer frame (bezel + side panel) as a PNG image.
    pub fn save_viewer_png(&self, path: &Path) -> Result<(), String> {
        let px = self.render_viewer_frame();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("mkdir {}: {e}", parent.display()))?;
            }
        }
        let png = encode_png(&px).map_err(|e| format!("encode viewer PNG: {e}"))?;
        std::fs::write(path, png).map_err(|e| format!("write {}: {e}", path.display()))
    }
}

/// Convert premultiplied RGBA8 [`Pixels`] into a `0x00RRGGBB` buffer for `softbuffer`.
#[must_use]
pub fn pixels_to_xrgb(px: &Pixels) -> Vec<u32> {
    let mut out = Vec::with_capacity((px.size.area().max(0)) as usize);
    for c in px.data.as_chunks::<4>().0 {
        out.push(((c[0] as u32) << 16) | ((c[1] as u32) << 8) | (c[2] as u32));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use rabbiz_emu_proto::EmuFrame;

    #[test]
    fn viewer_layout_places_320px_side_panel_and_all_16_control_buttons() {
        let layout = ViewerLayout::compute(Size::new(360, 800));
        assert_eq!(layout.side_panel_rect.w, SIDE_PANEL_WIDTH);
        assert_eq!(layout.buttons.len(), ControlButton::ALL.len());

        // Verify every button is inside the 320px side panel and hit-tests cleanly
        for slot in &layout.buttons {
            let center = Point::new(slot.rect.x + slot.rect.w / 2, slot.rect.y + slot.rect.h / 2);
            assert!(layout.side_panel_rect.contains(center));
            assert_eq!(layout.hit_button(center), Some(slot.button));
        }

        // Verify phone screen coordinate mapping
        let top_left = Point::new(layout.phone_screen_rect.x, layout.phone_screen_rect.y);
        assert_eq!(
            layout.viewer_to_device(top_left, Size::new(720, 1600)),
            Some(Point::new(0, 0))
        );
        let mid = Point::new(
            layout.phone_screen_rect.x + 180,
            layout.phone_screen_rect.y + 400,
        );
        assert_eq!(
            layout.viewer_to_device(mid, Size::new(720, 1600)),
            Some(Point::new(360, 800))
        );
    }

    #[test]
    fn viewer_state_renders_full_frame_and_handles_all_control_buttons() {
        let mut state = ViewerState::new("127.0.0.1:5910".to_string(), "127.0.0.1:5911".to_string());
        for btn in ControlButton::ALL {
            state.activate_button(btn);
        }
        let frame = state.render_viewer_frame();
        assert!(frame.size.w >= 360 + SIDE_PANEL_WIDTH);
        assert!(frame.data.iter().any(|&b| b != 0));
        let _ = EmuFrame::encode_full_png(1, 0, &frame).expect("encode viewer frame PNG");
    }
}
