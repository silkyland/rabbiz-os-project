//! Host Emulator Viewer (`rabbiz-emu-viewer`) & Host Paravirtual Driver Bridge for RabbizOS.
//!
//! Runs a real ARM64 Linux guest environment in the background (`rabbiz-de --backend=virtual`
//! + `rabbiz-osk` + all 12 compiled Linux `rabbiz-apps`) while driving Display, Touch/Input,
//! Power/Battery, Network, and Audio from the Host OS (macOS, Linux, Windows).

#![forbid(unsafe_code)]

use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use de_api::{Pixels, Point, Rect, Size};
use de_design::tokens::{self, Theme};
use de_paint::{encode_png, Canvas};
use rabbiz_app_kit::DevicePreset;
use rabbiz_emu_proto::{client_poll_frame, client_poll_raw_frame};

/// Physical width (px) of the right-hand studio dock (320 physical px = 160 logical pt on Retina 2x).
pub const SIDE_PANEL_WIDTH: i32 = 320;

/// Padding around the phone bezel inside the viewer canvas (physical px).
pub const BEZEL_PAD: i32 = 24;

/// Host-driven paravirtual driver telemetry bridged into the background Linux OS.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostDriverStatus {
    pub host_os: &'static str,
    pub battery_pct: u8,
    pub ac_charging: bool,
    pub network_online: bool,
    pub network_label: String,
    pub volume_pct: u8,
}

impl Default for HostDriverStatus {
    fn default() -> Self {
        Self {
            host_os: std::env::consts::OS,
            battery_pct: 100,
            ac_charging: true,
            network_online: true,
            network_label: "Host Bridge".to_string(),
            volume_pct: 65,
        }
    }
}

impl HostDriverStatus {
    /// Probe the real Host OS (macOS / Linux / Windows) power, network, and audio drivers.
    #[must_use]
    pub fn probe_host() -> Self {
        let mut s = Self::default();
        if cfg!(target_os = "macos") {
            // 1. Host Power / Battery driver via `pmset -g batt`
            if let Ok(out) = Command::new("pmset").args(["-g", "batt"]).output() {
                let text = String::from_utf8_lossy(&out.stdout);
                s.ac_charging = text.contains("AC Power") || text.contains("charging");
                if let Some(pct) = parse_percent_in_text(&text) {
                    s.battery_pct = pct;
                } else if s.ac_charging {
                    s.battery_pct = 100;
                }
            }
            // 2. Host Network driver via `route -n get default`
            if let Ok(out) = Command::new("route").args(["-n", "get", "default"]).output() {
                let text = String::from_utf8_lossy(&out.stdout);
                if let Some(iface) = text
                    .lines()
                    .find_map(|l| l.trim().strip_prefix("interface:"))
                    .map(str::trim)
                {
                    s.network_online = !iface.is_empty();
                    s.network_label = format!("Host ({iface})");
                } else {
                    s.network_online = false;
                    s.network_label = "Host Offline".to_string();
                }
            }
        } else if cfg!(target_os = "linux") {
            if let Ok(cap) = std::fs::read_to_string("/sys/class/power_supply/BAT0/capacity") {
                if let Ok(pct) = cap.trim().parse::<u8>() {
                    s.battery_pct = pct.min(100);
                }
            }
            if let Ok(st) = std::fs::read_to_string("/sys/class/power_supply/BAT0/status") {
                s.ac_charging = st.trim().eq_ignore_ascii_case("Charging")
                    || st.trim().eq_ignore_ascii_case("Full");
            }
            if let Ok(route) = std::fs::read_to_string("/proc/net/route") {
                s.network_online = route.lines().count() > 1;
            }
        }
        s
    }
}

fn parse_percent_in_text(s: &str) -> Option<u8> {
    for token in s.split_whitespace() {
        if let Some(num_part) = token.find('%').map(|idx| &token[..idx]) {
            let digits: String = num_part.chars().filter(char::is_ascii_digit).collect();
            if let Ok(v) = digits.parse::<u8>() {
                if v <= 100 {
                    return Some(v);
                }
            }
        }
    }
    None
}

/// Clickable actions in the 320 px studio dock.
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
    // Theme & Host/Mock Hardware
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
            Self::Power => "Power / Lock",
            Self::VolumeUp => "Volume +",
            Self::VolumeDown => "Volume -",
            Self::RotatePreset => "Rotate",
            Self::SwipeHome => "Home",
            Self::SwipeBack => "Back",
            Self::RecentsHold => "Recents",
            Self::PullShade => "Quick Shade",
            Self::ToggleTheme => "Light / Dark",
            Self::Battery15 => "Batt 15%",
            Self::Battery85 => "Host Power",
            Self::BatteryCharge => "AC Charge",
            Self::WifiToggle => "Host Wi-Fi",
            Self::PushHunNotification => "Push HUN",
            Self::IncomingCall => "Test Call",
            Self::SaveScreenshot => "Screenshot",
        }
    }
}

/// One laid-out button in the 320 px studio dock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PanelButtonSlot {
    pub button: ControlButton,
    pub rect: Rect,
}

/// Complete viewer geometry: 1:1 physical phone screen viewport + 320 px studio dock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewerLayout {
    pub total_size: Size,
    pub phone_bezel_rect: Rect,
    pub phone_screen_rect: Rect,
    pub side_panel_rect: Rect,
    pub buttons: Vec<PanelButtonSlot>,
}

impl ViewerLayout {
    /// Compute viewer layout for `screen_display_size` (e.g. `720x1600` 1:1 physical pixels).
    #[must_use]
    pub fn compute(screen_display_size: Size) -> Self {
        let sw = screen_display_size.w.max(240);
        let sh = screen_display_size.h.max(480);

        let bezel_rect = Rect::new(BEZEL_PAD - 12, BEZEL_PAD - 12, sw + 24, sh + 24);
        let screen_rect = Rect::new(BEZEL_PAD, BEZEL_PAD, sw, sh);
        let panel_x = BEZEL_PAD * 2 + sw;
        let total_h = (sh + BEZEL_PAD * 2).max(760);
        let side_panel_rect = Rect::new(panel_x, 0, SIDE_PANEL_WIDTH, total_h);
        let total_size = Size::new(panel_x + SIDE_PANEL_WIDTH, total_h);

        let mut buttons = Vec::with_capacity(ControlButton::ALL.len());
        let col_w = (SIDE_PANEL_WIDTH - 44) / 2;
        let row_h = 56;

        // Group 1: Hardware keys (4 buttons, 2x2 grid)
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
                    panel_x + 16 + col * (col_w + 12),
                    330 + row * (row_h + 12),
                    col_w,
                    row_h,
                ),
            });
        }

        // Group 2: System gestures (4 buttons, 2x2 grid)
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
                    panel_x + 16 + col * (col_w + 12),
                    524 + row * (row_h + 12),
                    col_w,
                    row_h,
                ),
            });
        }

        // Group 3: Host Drivers & Event Injection (8 buttons, 2x4 grid)
        let mock = [
            ControlButton::ToggleTheme,
            ControlButton::WifiToggle,
            ControlButton::Battery85,
            ControlButton::Battery15,
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
                    panel_x + 16 + col * (col_w + 12),
                    718 + row * (row_h + 12),
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

    /// Hit-test a viewer click against the 320 px studio dock buttons.
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

    /// Map a viewer coordinate clamped to `phone_screen_rect` so drags crossing the bezel edge finish cleanly.
    #[must_use]
    pub fn viewer_to_device_clamped(&self, p: Point, device_size: Size) -> Point {
        if device_size.is_empty() {
            return Point::ZERO;
        }
        let rel_x = (p.x - self.phone_screen_rect.x).clamp(0, self.phone_screen_rect.w.max(1) - 1);
        let rel_y = (p.y - self.phone_screen_rect.y).clamp(0, self.phone_screen_rect.h.max(1) - 1);
        let dx = ((i64::from(rel_x) * i64::from(device_size.w))
            / i64::from(self.phone_screen_rect.w.max(1))) as i32;
        let dy = ((i64::from(rel_y) * i64::from(device_size.h))
            / i64::from(self.phone_screen_rect.h.max(1))) as i32;
        Point::new(
            dx.clamp(0, device_size.w - 1),
            dy.clamp(0, device_size.h - 1),
        )
    }
}

/// Interactive state of `rabbiz-emu-viewer` (persistent TCP link to background Linux `rabbiz-de` + Host Drivers).
pub struct ViewerState {
    pub emu_addr: String,
    pub ctl_addr: String,
    pub preset: DevicePreset,
    pub dark_mode: bool,
    pub battery_pct: u8,
    pub charging: bool,
    pub wifi_on: bool,
    pub host_drivers: HostDriverStatus,
    pub screen_name: String,
    pub banner: Option<String>,
    pub last_touch: Option<Point>,
    pub connected: bool,
    pub frame_seq: u64,
    pub frame_pixels: Pixels,
    pub pressed_button: Option<ControlButton>,
    emu_stream: Option<TcpStream>,
    host_synced_once: bool,
}

impl ViewerState {
    #[must_use]
    pub fn new(emu_addr: String, ctl_addr: String) -> Self {
        let preset = DevicePreset::A21sPortrait;
        let host_drivers = HostDriverStatus::probe_host();
        let mut s = Self {
            emu_addr,
            ctl_addr,
            preset,
            dark_mode: false,
            battery_pct: host_drivers.battery_pct,
            charging: host_drivers.ac_charging,
            wifi_on: host_drivers.network_online,
            host_drivers,
            screen_name: "Home · ~/Desktop".to_string(),
            banner: None,
            last_touch: None,
            connected: false,
            frame_seq: 0,
            frame_pixels: Pixels::empty(preset.size()),
            pressed_button: None,
            emu_stream: None,
            host_synced_once: false,
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

    /// Full 1:1 physical device resolution (`720x1600` for A21s portrait) so Retina displays render razor-sharp.
    #[must_use]
    pub fn display_phone_size(&self) -> Size {
        let dev = self.preset.size();
        if dev.w <= dev.h {
            Size::new(720, 1600)
        } else {
            Size::new(1600, 720)
        }
    }

    #[must_use]
    pub fn layout(&self) -> ViewerLayout {
        ViewerLayout::compute(self.display_phone_size())
    }

    fn ensure_emu_stream(&mut self) -> Option<&mut TcpStream> {
        if self.emu_stream.is_none() {
            let addr = self.emu_addr.parse::<SocketAddr>().ok()?;
            let stream = TcpStream::connect_timeout(&addr, Duration::from_millis(150)).ok()?;
            let _ = stream.set_nodelay(true);
            let _ = stream.set_read_timeout(Some(Duration::from_millis(1200)));
            let _ = stream.set_write_timeout(Some(Duration::from_millis(500)));
            self.emu_stream = Some(stream);
        }
        self.emu_stream.as_mut()
    }

    /// Push the probed Host OS drivers (battery, AC power, Wi-Fi/network, volume) into the Linux guest.
    pub fn sync_host_drivers_to_linux(&mut self) {
        self.host_drivers = HostDriverStatus::probe_host();
        self.battery_pct = self.host_drivers.battery_pct;
        self.charging = self.host_drivers.ac_charging;
        self.wifi_on = self.host_drivers.network_online;
        let _ = self.send_ctl_json(&serde_json::json!({
            "id": 1,
            "cmd": "set_mock",
            "key": "battery",
            "value": self.battery_pct.to_string(),
        }));
        let _ = self.send_ctl_json(&serde_json::json!({
            "id": 2,
            "cmd": "set_mock",
            "key": "charging",
            "value": self.charging.to_string(),
        }));
        let _ = self.send_ctl_json(&serde_json::json!({
            "id": 3,
            "cmd": "set_mock",
            "key": "wifi",
            "value": self.wifi_on.to_string(),
        }));
    }

    /// Poll a live frame from `emu_addr` using the persistent TCP stream (`RAW` first, `FRAME` fallback).
    pub fn poll_remote_frame(&mut self) -> bool {
        let res = {
            let Some(stream) = self.ensure_emu_stream() else {
                if self.connected {
                    self.connected = false;
                    self.render_offline_demo_phone();
                }
                return false;
            };
            client_poll_raw_frame(stream).or_else(|_| client_poll_frame(stream))
        };

        match res {
            Ok(frame) => {
                if frame.decode_into(&mut self.frame_pixels).is_ok() {
                    self.connected = true;
                    self.frame_seq = frame.seq;
                    if !self.host_synced_once {
                        self.host_synced_once = true;
                        self.sync_host_drivers_to_linux();
                    }
                    return true;
                }
                false
            }
            Err(_) => {
                self.emu_stream = None;
                false
            }
        }
    }

    /// Send a JSON IPC command line to the running Linux `rabbiz-de` instance (reusing `emu_stream` or `ctl_addr`).
    pub fn send_ctl_json(&self, payload: &serde_json::Value) -> Result<String, String> {
        let addr: SocketAddr = self
            .ctl_addr
            .parse()
            .or_else(|_| self.emu_addr.parse())
            .map_err(|e| format!("invalid ctl_addr {}: {e}", self.ctl_addr))?;
        let mut stream = TcpStream::connect_timeout(&addr, Duration::from_millis(150))
            .map_err(|e| format!("connect ctl {addr}: {e}"))?;
        let _ = stream.set_nodelay(true);
        let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
        let _ = stream.set_write_timeout(Some(Duration::from_millis(500)));
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

    /// Fast-path JSON command over the persistent `emu_stream` (used for real-time touch drag & gestures).
    fn send_fast_emu_cmd(&mut self, payload: &serde_json::Value) -> bool {
        let Some(stream) = self.ensure_emu_stream() else {
            return false;
        };
        let Ok(mut line) = serde_json::to_string(payload) else {
            return false;
        };
        line.push('\n');
        if stream.write_all(line.as_bytes()).is_err() || stream.flush().is_err() {
            self.emu_stream = None;
            return false;
        }
        let mut reader = BufReader::new(stream);
        let mut reply = String::new();
        if reader.read_line(&mut reply).is_err() {
            self.emu_stream = None;
            return false;
        }
        true
    }

    /// Inject a touch event (`"down"`, `"move"`, `"up"`, `"cancel"`) at device pixel `pos`.
    pub fn inject_touch(&mut self, phase: &str, pos: Point) {
        self.last_touch = Some(pos);
        let cmd = serde_json::json!({
            "id": 1,
            "cmd": "inject_touch",
            "phase": phase,
            "slot": 0,
            "x": pos.x,
            "y": pos.y,
        });
        if !self.send_fast_emu_cmd(&cmd) && self.send_ctl_json(&cmd).is_err() && !self.connected {
            self.render_offline_demo_phone();
        }
    }

    /// Perform a smooth multi-step swipe gesture so `GestureTracker` registers real release velocity.
    pub fn inject_smooth_swipe(&mut self, from: Point, to: Point, steps: i32) {
        let steps = steps.max(2);
        self.inject_touch("down", from);
        for i in 1..steps {
            let x = from.x + ((to.x - from.x) * i) / steps;
            let y = from.y + ((to.y - from.y) * i) / steps;
            self.inject_touch("move", Point::new(x, y));
        }
        self.inject_touch("up", to);
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
                let _ = self.send_fast_emu_cmd(&serde_json::json!({
                    "id": 1,
                    "cmd": "inject_hw_button",
                    "button": "power"
                }));
            }
            ControlButton::VolumeUp => {
                self.banner = Some("Volume: 75%".to_string());
                let _ = self.send_fast_emu_cmd(&serde_json::json!({
                    "id": 1,
                    "cmd": "inject_hw_button",
                    "button": "volume_up"
                }));
            }
            ControlButton::VolumeDown => {
                self.banner = Some("Volume: 35%".to_string());
                let _ = self.send_fast_emu_cmd(&serde_json::json!({
                    "id": 1,
                    "cmd": "inject_hw_button",
                    "button": "volume_down"
                }));
            }
            ControlButton::RotatePreset => {
                self.preset = self.preset.cycle();
                self.frame_pixels = Pixels::empty(self.preset.size());
            }
            ControlButton::SwipeHome => {
                self.screen_name = "Home · ~/Desktop".to_string();
                self.banner = None;
                let _ = self.send_fast_emu_cmd(&serde_json::json!({
                    "id": 1,
                    "cmd": "set_panel",
                    "panel": "quick",
                    "open": false
                }));
                let dev = self.preset.size();
                self.inject_smooth_swipe(
                    Point::new(dev.w / 2, dev.h - 10),
                    Point::new(dev.w / 2, dev.h - 420),
                    6,
                );
            }
            ControlButton::SwipeBack => {
                self.screen_name = "Back Navigation".to_string();
                let dev = self.preset.size();
                self.inject_smooth_swipe(
                    Point::new(8, dev.h / 2),
                    Point::new(260, dev.h / 2),
                    6,
                );
            }
            ControlButton::RecentsHold => {
                self.screen_name = "Recents Carousel".to_string();
                let dev = self.preset.size();
                self.inject_touch("down", Point::new(dev.w / 2, dev.h - 10));
                self.inject_touch("move", Point::new(dev.w / 2, dev.h - 260));
            }
            ControlButton::PullShade => {
                self.screen_name = "Quick Settings & Shade".to_string();
                let _ = self.send_fast_emu_cmd(&serde_json::json!({
                    "id": 1,
                    "cmd": "set_panel",
                    "panel": "quick",
                    "open": true
                }));
            }
            ControlButton::ToggleTheme => {
                self.dark_mode = !self.dark_mode;
                let _ = self.send_fast_emu_cmd(&serde_json::json!({
                    "id": 1,
                    "cmd": "reload_theme"
                }));
            }
            ControlButton::Battery15 => {
                self.battery_pct = 15;
                self.charging = false;
                let _ = self.send_fast_emu_cmd(&serde_json::json!({
                    "id": 1,
                    "cmd": "set_mock",
                    "key": "battery",
                    "value": "15"
                }));
                let _ = self.send_fast_emu_cmd(&serde_json::json!({
                    "id": 1,
                    "cmd": "set_mock",
                    "key": "charging",
                    "value": "false"
                }));
            }
            ControlButton::Battery85 => {
                self.sync_host_drivers_to_linux();
            }
            ControlButton::BatteryCharge => {
                self.charging = !self.charging;
                let _ = self.send_fast_emu_cmd(&serde_json::json!({
                    "id": 1,
                    "cmd": "set_mock",
                    "key": "charging",
                    "value": self.charging.to_string()
                }));
            }
            ControlButton::WifiToggle => {
                self.wifi_on = !self.wifi_on;
                let _ = self.send_fast_emu_cmd(&serde_json::json!({
                    "id": 1,
                    "cmd": "set_mock",
                    "key": "wifi",
                    "value": self.wifi_on.to_string()
                }));
            }
            ControlButton::PushHunNotification => {
                self.banner = Some("HUN: New message from RabbizOS Team".to_string());
                let _ = self.send_fast_emu_cmd(&serde_json::json!({
                    "id": 1,
                    "cmd": "set_mock",
                    "key": "notif",
                    "value": "Messages|RabbizOS Dev|Host paravirtual notification bridge is live!"
                }));
            }
            ControlButton::IncomingCall => {
                self.banner = Some("Incoming Call · +66 81 234 5678".to_string());
                let _ = self.send_fast_emu_cmd(&serde_json::json!({
                    "id": 1,
                    "cmd": "set_mock",
                    "key": "notif",
                    "value": "Phone|Incoming Call · +66 81 234 5678|Tap to open Rabbiz Dialer"
                }));
            }
            ControlButton::SaveScreenshot => {
                let path = std::env::temp_dir().join("rabbiz-emu-viewer-shot.png");
                let _ = self.save_viewer_png(&path);
                self.banner = Some(format!("Saved {}", path.display()));
            }
        }
        let _ = self.poll_remote_frame();
        if !self.connected {
            self.render_offline_demo_phone();
        }
    }

    /// Render the fallback phone preview only when no Linux backend is reachable (or `--self-test`).
    pub fn render_offline_demo_phone(&mut self) {
        let size = self.preset.size();
        let theme = self.active_theme();
        let mut c = Canvas::filled(size, theme.bg);

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

        let y = rabbiz_app_kit::ui::draw_header(
            &mut c,
            &theme,
            "Starting Linux Guest…",
            &format!("Connecting to rabbiz-de ({})", self.emu_addr),
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

        let pill = Rect::new((size.w - 180) / 2, size.h - 24, 180, 8);
        c.fill_rounded_rect(pill, 4.0, theme.text2);

        self.frame_pixels = c.into_pixels();
    }

    /// Render the complete viewer window (1:1 phone bezel + phone screen + 320 px studio dock).
    #[must_use]
    pub fn render_viewer_frame(&self) -> Pixels {
        let layout = self.layout();
        let dock_theme = tokens::DARK;
        let mut canvas = Canvas::filled(layout.total_size, [0x0E, 0x0B, 0x11, 0xFF]);

        // Realistic hardware phone frame + physical side key nubs
        canvas.fill_rounded_rect(layout.phone_bezel_rect, 44.0, [0x2A, 0x22, 0x30, 0xFF]);
        let inner_bezel = layout.phone_bezel_rect.inset(3);
        canvas.fill_rounded_rect(inner_bezel, 41.0, [0x17, 0x12, 0x1B, 0xFF]);
        // Physical button nubs on right edge of bezel
        let nub_x = layout.phone_bezel_rect.right();
        canvas.fill_rounded_rect(Rect::new(nub_x, 300, 5, 96), 2.5, [0x3D, 0x32, 0x45, 0xFF]);
        canvas.fill_rounded_rect(Rect::new(nub_x, 420, 5, 96), 2.5, [0x3D, 0x32, 0x45, 0xFF]);
        canvas.fill_rounded_rect(Rect::new(nub_x, 570, 5, 64), 2.5, [0xA6, 0x5A, 0x2E, 0xFF]);

        // Blit phone framebuffer into `phone_screen_rect` (1:1 when 720x1600!)
        if self.frame_pixels.size == layout.phone_screen_rect.size() {
            canvas.blit(&self.frame_pixels, layout.phone_screen_rect.origin());
        } else {
            canvas.blit_scaled(&self.frame_pixels, layout.phone_screen_rect, 1.0);
        }

        // Studio dock background (320 px physical)
        canvas.fill_rect(layout.side_panel_rect, [0x16, 0x11, 0x1B, 0xFF]);
        canvas.fill_rect(
            Rect::new(layout.side_panel_rect.x, 0, 2, layout.side_panel_rect.h),
            dock_theme.line,
        );

        let px = layout.side_panel_rect.x + 18;
        rabbiz_app_kit::ui::draw_text(
            &mut canvas,
            "RabbizOS Emulator",
            &rabbiz_app_kit::ui::styled(26.0, 600, dock_theme.text),
            px,
            24,
            Some(SIDE_PANEL_WIDTH - 32),
        );

        // Live connection & Host Paravirtual Driver status card
        let status_card = Rect::new(layout.side_panel_rect.x + 16, 68, SIDE_PANEL_WIDTH - 32, 206);
        canvas.fill_rounded_rect(status_card, 20.0, dock_theme.surface1);

        let badge_color = if self.connected {
            dock_theme.success
        } else {
            dock_theme.warning
        };
        canvas.fill_rounded_rect(Rect::new(px + 12, 90, 10, 10), 5.0, badge_color);
        let conn_str = if self.connected {
            format!("LIVE LINUX · #{}", self.frame_seq)
        } else {
            "STARTING LINUX...".to_string()
        };
        rabbiz_app_kit::ui::draw_text(
            &mut canvas,
            &conn_str,
            &rabbiz_app_kit::ui::styled(17.0, 600, badge_color),
            px + 28,
            84,
            Some(SIDE_PANEL_WIDTH - 72),
        );

        let driver_lines = [
            format!("Guest: Linux arm64 ({})", self.preset.short_label()),
            format!("Host OS: {} (Paravirtual)", self.host_drivers.host_os),
            format!(
                "Power: {}% ({})",
                self.battery_pct,
                if self.charging { "AC Charging" } else { "Battery" }
            ),
            format!(
                "Network: {}",
                if self.wifi_on {
                    &self.host_drivers.network_label
                } else {
                    "Disconnected"
                }
            ),
            "Apps: 12 Native Rust + OSK".to_string(),
        ];
        for (idx, line) in driver_lines.iter().enumerate() {
            rabbiz_app_kit::ui::draw_text(
                &mut canvas,
                line,
                &rabbiz_app_kit::ui::styled(15.0, 400, dock_theme.text2),
                px + 12,
                116 + (idx as i32) * 28,
                Some(SIDE_PANEL_WIDTH - 56),
            );
        }

        // Section headers
        for (title_str, sy) in [
            ("HARDWARE KEYS", 296),
            ("SYSTEM GESTURES", 490),
            ("HOST DRIVERS & EVENTS", 684),
        ] {
            rabbiz_app_kit::ui::draw_text(
                &mut canvas,
                title_str,
                &rabbiz_app_kit::ui::styled(15.0, 600, dock_theme.text3),
                px,
                sy,
                Some(SIDE_PANEL_WIDTH - 32),
            );
        }

        for slot in &layout.buttons {
            let is_pressed = self.pressed_button == Some(slot.button);
            let bg = if is_pressed {
                dock_theme.accent
            } else {
                dock_theme.surface2
            };
            let fg = if is_pressed {
                dock_theme.text_on_accent
            } else {
                dock_theme.text
            };
            canvas.fill_rounded_rect(slot.rect, 16.0, bg);
            rabbiz_app_kit::ui::draw_centered(
                &mut canvas,
                slot.button.label(),
                &rabbiz_app_kit::ui::styled(17.0, 600, fg),
                slot.rect,
            );
        }

        // Footer hint
        rabbiz_app_kit::ui::draw_text(
            &mut canvas,
            "Tip: Tap app icons or drag on phone screen",
            &rabbiz_app_kit::ui::styled(14.0, 400, dock_theme.text3),
            px,
            layout.total_size.h - 42,
            Some(SIDE_PANEL_WIDTH - 32),
        );

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
        let layout = ViewerLayout::compute(Size::new(720, 1600));
        assert_eq!(layout.side_panel_rect.w, SIDE_PANEL_WIDTH);
        assert_eq!(layout.buttons.len(), ControlButton::ALL.len());

        for slot in &layout.buttons {
            let center = Point::new(slot.rect.x + slot.rect.w / 2, slot.rect.y + slot.rect.h / 2);
            assert!(layout.side_panel_rect.contains(center));
            assert_eq!(layout.hit_button(center), Some(slot.button));
        }

        let top_left = Point::new(layout.phone_screen_rect.x, layout.phone_screen_rect.y);
        assert_eq!(
            layout.viewer_to_device(top_left, Size::new(720, 1600)),
            Some(Point::new(0, 0))
        );
        let mid = Point::new(
            layout.phone_screen_rect.x + 360,
            layout.phone_screen_rect.y + 800,
        );
        assert_eq!(
            layout.viewer_to_device(mid, Size::new(720, 1600)),
            Some(Point::new(360, 800))
        );
    }

    #[test]
    fn viewer_state_renders_full_frame_and_handles_all_control_buttons() {
        let mut state =
            ViewerState::new("127.0.0.1:5910".to_string(), "127.0.0.1:5911".to_string());
        for btn in ControlButton::ALL {
            state.activate_button(btn);
        }
        let frame = state.render_viewer_frame();
        assert!(frame.size.w >= 720 + SIDE_PANEL_WIDTH);
        assert!(frame.data.iter().any(|&b| b != 0));
        let _ = EmuFrame::encode_full_png(1, 0, &frame).expect("encode viewer frame PNG");
    }
}
