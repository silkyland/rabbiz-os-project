//! RabbizOS Settings — a small, honest settings app over the real backends:
//! `de-config::Prefs` (`~/.config/rabbiz-de/prefs.json`, atomic writes) for
//! persisted desktop prefs, sysfs backlight / `wpctl` / `nmcli` /
//! `bluetoothctl` for controls that apply immediately.
//!
//! Honesty contract: prefs are loaded by the compositor once at session
//! start, so wallpaper/theme/DND/timeout/keyboard changes are saved but
//! only take effect on the next desktop session — the footer says so.

mod cli;
mod model;
mod system;

use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::{Duration, Instant};

use de_api::{Pixels, Point, Size};
use de_config::keyboards::{discover_keyboards, KeyboardInfo};
use de_config::Prefs;
use de_paint::text;
use rabbiz_app_kit::{
    icons, ui, widgets, AppEvent, AppFrame, AppState, Canvas, Rect, TextBuffer, TextInputCtl, Theme,
};
use wayland_protocols::wp::text_input::zv3::client::zwp_text_input_v3::ContentPurpose;

use model::{clamp_scroll, Facts, Hit, KbFact, Open, Placed, View};

/// Throttle for `wpctl` writes during a volume drag (process spawn per move
/// would swamp the frame thread).
const VOLUME_WRITE_INTERVAL: Duration = Duration::from_millis(120);

enum Drag {
    /// Finger down, not yet classified (waiting to see if it becomes a scroll).
    Pending,
    /// Vertical scroll in progress.
    Scroll,
    /// Slider drag in progress.
    Slider,
}

struct Touch {
    id: i32,
    start: Point,
    last: Point,
    hit: Hit,
    drag: Drag,
}

struct SettingsApp {
    prefs: Prefs,
    keyboards: Vec<KeyboardInfo>,
    open: Open,
    scroll: i32,
    content_h: i32,
    size: Size,
    top: i32,
    status: String,
    pressed: Option<Hit>,
    touch: Option<Touch>,
    brightness: Option<f32>,
    volume: Option<(f32, bool)>,
    wifi: Option<bool>,
    wifi_ssid: Option<String>,
    wifi_nets: Vec<system::WifiNet>,
    wifi_scanning: bool,
    wifi_pass_ssid: Option<String>,
    wifi_pass: TextBuffer,
    wifi_pass_focused: bool,
    bt: Option<bool>,
    bt_device: Option<String>,
    bt_devs: Vec<system::BtDev>,
    bt_scanning: bool,
    last_volume_write: Instant,
    wallpaper_px: Vec<(String, Pixels)>,
    radio_tx: Sender<(system::Radio, Option<bool>)>,
    radio_rx: Receiver<(system::Radio, Option<bool>)>,
    net_tx: Sender<system::NetEvent>,
    net_rx: Receiver<system::NetEvent>,
    /// "Search settings" query — filters rows by title (see model::layout).
    query: TextBuffer,
    search_focused: bool,
    /// Last rendered search-pill rect (hit-tested ahead of placed rows).
    search_rect: Rect,
}

impl SettingsApp {
    fn new() -> Self {
        let prefs = Prefs::load();
        let keyboards = discover_keyboards(prefs.allow_third_party_keyboards);
        // Hardware reads once at startup (~0.5 s worst case for bluetoothctl);
        // radio toggles afterwards report back over the channel.
        let (wifi, bt) = system::poll_radios();
        let wifi_ssid = if wifi == Some(true) {
            system::active_wifi_ssid()
        } else {
            None
        };
        let bt_device = if bt == Some(true) {
            system::active_bt_device()
        } else {
            None
        };
        let brightness = system::backlight_level()
            .map(|(raw, max)| (raw as f32 / max.max(1) as f32).clamp(0.0, 1.0));
        let volume = system::volume();
        let wallpaper_px = de_paint::wallpaper::IDS
            .iter()
            .map(|id| {
                (
                    (*id).to_string(),
                    de_paint::wallpaper::wallpaper(None, id, Size::new(112, 84)),
                )
            })
            .collect();
        let (radio_tx, radio_rx) = channel();
        let (net_tx, net_rx) = channel();
        Self {
            prefs,
            keyboards,
            open: Open::default(),
            scroll: 0,
            content_h: 0,
            size: Size::ZERO,
            top: ui::TOP_SAFE + 96,
            status: String::new(),
            pressed: None,
            touch: None,
            brightness,
            volume,
            wifi,
            wifi_ssid,
            wifi_nets: Vec::new(),
            wifi_scanning: false,
            wifi_pass_ssid: None,
            wifi_pass: TextBuffer::new(),
            wifi_pass_focused: false,
            bt,
            bt_device,
            bt_devs: Vec::new(),
            bt_scanning: false,
            last_volume_write: Instant::now() - VOLUME_WRITE_INTERVAL,
            wallpaper_px,
            radio_tx,
            radio_rx,
            net_tx,
            net_rx,
            query: TextBuffer::new(),
            search_focused: false,
            search_rect: Rect::new(0, 0, 0, 0),
        }
    }

    fn open_section(&mut self, section: &str) {
        match section {
            "wifi" => {
                self.open.wifi = true;
                if self.wifi == Some(true) {
                    self.wifi_scanning = true;
                    system::scan_wifi_async(true, self.net_tx.clone());
                }
            }
            "bluetooth" | "bt" => {
                self.open.bluetooth = true;
                if self.bt == Some(true) {
                    self.bt_scanning = true;
                    system::scan_bt_async(true, self.net_tx.clone());
                }
            }
            "keyboard" => self.open.keyboard = true,
            _ => {}
        }
    }

    fn drain_net(&mut self) -> bool {
        let mut changed = false;
        while let Ok(ev) = self.net_rx.try_recv() {
            changed = true;
            match ev {
                system::NetEvent::WifiScanned(nets, active) => {
                    self.wifi_nets = nets;
                    self.wifi_ssid = active;
                    self.wifi_scanning = false;
                }
                system::NetEvent::WifiAction(msg) => {
                    self.status = msg;
                }
                system::NetEvent::BtScanned(devs, active) => {
                    self.bt_devs = devs;
                    self.bt_device = active;
                    self.bt_scanning = false;
                }
                system::NetEvent::BtAction(msg) => {
                    self.status = msg;
                }
            }
        }
        changed
    }

    fn reload_kbs(&mut self) {
        self.keyboards = discover_keyboards(self.prefs.allow_third_party_keyboards);
    }

    fn facts(&self) -> Facts {
        Facts {
            dark: self.prefs.theme_mode.trim() == "dark",
            brightness: self.brightness,
            volume: self.volume.map(|v| v.0),
            muted: self.volume.map(|v| v.1).unwrap_or(false),
            dnd: self.prefs.do_not_disturb,
            wifi: self.wifi,
            wifi_ssid: self.wifi_ssid.clone(),
            wifi_nets: self.wifi_nets.clone(),
            wifi_scanning: self.wifi_scanning,
            wifi_pass_ssid: self.wifi_pass_ssid.clone(),
            wifi_pass_text: self.wifi_pass.as_str().to_string(),
            wifi_pass_focused: self.wifi_pass_focused,
            bt: self.bt,
            bt_device: self.bt_device.clone(),
            bt_devs: self.bt_devs.clone(),
            bt_scanning: self.bt_scanning,
            wallpaper: self.prefs.wallpaper_id.clone(),
            timeout_sec: self.prefs.screen_timeout_sec,
            search_engine: self.prefs.search_engine.clone(),
            keyboard_name: self
                .keyboards
                .iter()
                .find(|k| k.id == self.prefs.keyboard_id)
                .map(|k| k.name.clone())
                .unwrap_or_else(|| self.prefs.keyboard_id.clone()),
            osk_haptic: self.prefs.osk_haptic,
            osk_sound: self.prefs.osk_sound,
            third_party: self.prefs.allow_third_party_keyboards,
            keyboards: self
                .keyboards
                .iter()
                .map(|k| KbFact {
                    name: k.name.clone(),
                    installed: k.installed,
                    active: k.id == self.prefs.keyboard_id,
                })
                .collect(),
        }
    }

    /// Layout + hit-test share one code path — never hardcode canvas size.
    fn placed(&self) -> (Vec<Placed>, i32) {
        model::layout(
            self.size,
            self.top,
            self.scroll,
            &self.open,
            &self.facts(),
            self.query.as_str(),
        )
    }

    fn hit_at(&self, pos: Point) -> Hit {
        // The search pill is chrome above the placed rows — check it first.
        if self.search_rect.contains(pos) {
            return match widgets::SearchPill::hit(self.search_rect, pos, !self.query.is_empty()) {
                Some(widgets::SearchHit::Clear) => Hit::SearchClear,
                _ => Hit::SearchField,
            };
        }
        let (rows, _) = self.placed();
        rows.iter()
            .rev()
            .find(|p| p.rect.contains(pos))
            .map_or(Hit::None, |p| p.hit)
    }

    fn viewport_h(&self) -> i32 {
        (self.size.h - self.top - ui::BOTTOM_SAFE).max(0)
    }

    /// Apply radio-toggle results reported by worker threads.
    fn drain_radio(&mut self) {
        while let Ok((r, state)) = self.radio_rx.try_recv() {
            match r {
                system::Radio::Wifi => self.wifi = state,
                system::Radio::Bt => self.bt = state,
            }
        }
    }

    /// Persist prefs atomically (de-config's `Prefs::save` → tmp+rename+.bak)
    /// and surface the outcome honestly.
    fn save_prefs(&mut self, msg: String) {
        self.status = match self.prefs.save() {
            Ok(()) => msg,
            Err(e) => format!("Couldn't save settings: {e}"),
        };
    }

    /// Push the current query back to the IME/OSK after an edit.
    fn after_search_edit(&mut self, ti: &TextInputCtl) {
        self.scroll = 0;
        ti.set_surrounding(self.query.as_str(), self.query.cursor());
    }

    fn unfocus_search(&mut self, ti: &TextInputCtl) {
        if self.search_focused {
            self.search_focused = false;
            ti.deactivate();
        }
    }

    fn activate(&mut self, hit: Hit, ti: &TextInputCtl) {
        match hit {
            Hit::SearchField => {
                if !self.search_focused {
                    self.search_focused = true;
                    ti.activate(ContentPurpose::Normal);
                }
                ti.set_surrounding(self.query.as_str(), self.query.cursor());
            }
            Hit::SearchClear => {
                self.query.clear();
                self.after_search_edit(ti);
            }
            Hit::DarkTheme => {
                let dark = self.prefs.theme_mode.trim() != "dark";
                self.prefs.theme_mode = if dark { "dark" } else { "light" }.into();
                let _ = write_theme_file(&self.prefs.theme_mode);
                self.save_prefs(format!(
                    "Theme: {} — applies on next launch",
                    if dark { "dark" } else { "light" }
                ));
            }
            Hit::Mute => {
                let now = !self.volume.map(|v| v.1).unwrap_or(false);
                if system::set_muted(now) {
                    self.volume = system::volume();
                    self.status = if now {
                        "Muted".into()
                    } else {
                        "Unmuted".into()
                    };
                } else {
                    self.status = "No audio control found".into();
                }
            }
            Hit::Dnd => {
                self.prefs.do_not_disturb = !self.prefs.do_not_disturb;
                self.save_prefs(format!(
                    "Do Not Disturb {} — applies on next launch",
                    if self.prefs.do_not_disturb {
                        "on"
                    } else {
                        "off"
                    }
                ));
            }
            Hit::Wifi | Hit::Bluetooth => {
                let (state, radio, name) = match hit {
                    Hit::Wifi => (self.wifi, system::Radio::Wifi, "Wi-Fi"),
                    _ => (self.bt, system::Radio::Bt, "Bluetooth"),
                };
                match state {
                    Some(cur) => {
                        let on = !cur;
                        // Optimistic flip; the worker re-polls and reports the
                        // truth, correcting us if the command failed.
                        match radio {
                            system::Radio::Wifi => self.wifi = Some(on),
                            system::Radio::Bt => self.bt = Some(on),
                        }
                        system::toggle_radio(radio, on, self.radio_tx.clone());
                        self.status = format!("Turning {name} {}…", if on { "on" } else { "off" });
                    }
                    None => self.status = format!("{name} state unknown"),
                }
            }
            Hit::WifiNetworksToggle => {
                self.open.wifi = !self.open.wifi;
                if self.open.wifi && self.wifi == Some(true) {
                    self.wifi_scanning = true;
                    system::scan_wifi_async(true, self.net_tx.clone());
                } else {
                    self.wifi_pass_ssid = None;
                    self.wifi_pass_focused = false;
                }
            }
            Hit::WifiRescan => {
                self.wifi_scanning = true;
                self.status = "Scanning Wi-Fi…".into();
                system::scan_wifi_async(true, self.net_tx.clone());
            }
            Hit::WifiForget => {
                if let Some(ssid) = self.wifi_ssid.clone() {
                    self.status = format!("Forgetting {ssid}…");
                    system::forget_wifi_async(ssid, self.net_tx.clone());
                }
            }
            Hit::WifiPick(i) => {
                if let Some(net) = self.wifi_nets.get(i).cloned() {
                    if net.active {
                        self.status = format!("Already connected to {}", net.ssid);
                    } else if net.saved || !net.secured {
                        self.wifi_pass_ssid = None;
                        self.wifi_pass_focused = false;
                        self.status = format!("Connecting to {}…", net.ssid);
                        system::connect_wifi_async(net.ssid, None, self.net_tx.clone());
                    } else {
                        self.wifi_pass_ssid = Some(net.ssid.clone());
                        self.wifi_pass.clear();
                        self.wifi_pass_focused = true;
                        self.search_focused = false;
                        self.status = format!("Enter password for {}", net.ssid);
                    }
                }
            }
            Hit::WifiPassField => {
                self.wifi_pass_focused = true;
                self.search_focused = false;
            }
            Hit::WifiPassConnect => {
                if let Some(ssid) = self.wifi_pass_ssid.take() {
                    let pw = self.wifi_pass.as_str().to_string();
                    self.wifi_pass.clear();
                    self.wifi_pass_focused = false;
                    self.status = format!("Connecting to {ssid}…");
                    system::connect_wifi_async(ssid, Some(pw), self.net_tx.clone());
                }
            }
            Hit::WifiPassCancel => {
                self.wifi_pass_ssid = None;
                self.wifi_pass.clear();
                self.wifi_pass_focused = false;
            }
            Hit::BtDevicesToggle => {
                self.open.bluetooth = !self.open.bluetooth;
                if self.open.bluetooth && self.bt == Some(true) {
                    self.bt_scanning = true;
                    system::scan_bt_async(true, self.net_tx.clone());
                }
            }
            Hit::BtRescan => {
                self.bt_scanning = true;
                self.status = "Scanning Bluetooth…".into();
                system::scan_bt_async(true, self.net_tx.clone());
            }
            Hit::BtPick(i) => {
                if let Some(dev) = self.bt_devs.get(i).cloned() {
                    self.status = if dev.connected {
                        format!("Disconnecting {}…", dev.name)
                    } else {
                        format!("Connecting to {}…", dev.name)
                    };
                    system::toggle_bt_device_async(
                        dev.mac,
                        dev.name,
                        dev.connected,
                        self.net_tx.clone(),
                    );
                }
            }
            Hit::WallpaperToggle => self.open.wallpaper = !self.open.wallpaper,
            Hit::WallpaperPick(i) => {
                if let Some(id) = de_paint::wallpaper::IDS.get(i) {
                    self.prefs.wallpaper_id = (*id).to_string();
                    self.save_prefs(format!(
                        "Wallpaper: {} — applies on next launch",
                        model::wallpaper_name(id)
                    ));
                }
            }
            Hit::TimeoutToggle => self.open.timeout = !self.open.timeout,
            Hit::TimeoutPick(sec) => {
                self.prefs.screen_timeout_sec = sec;
                self.save_prefs(format!(
                    "Screen timeout: {} — applies on next launch",
                    model::timeout_label(sec)
                ));
            }
            Hit::SearchEngineToggle => self.open.search_engine = !self.open.search_engine,
            Hit::SearchEnginePick(idx) => {
                if let Some((id, label, _)) = de_config::prefs::SEARCH_ENGINES.get(idx) {
                    self.prefs.search_engine = (*id).to_string();
                    self.save_prefs(format!("Search engine: {label}"));
                }
            }
            Hit::KeyboardToggle => self.open.keyboard = !self.open.keyboard,
            Hit::OskHaptic => {
                self.prefs.osk_haptic = !self.prefs.osk_haptic;
                self.save_prefs(format!(
                    "Keypress vibration: {}",
                    if self.prefs.osk_haptic { "on" } else { "off" }
                ));
            }
            Hit::OskSound => {
                self.prefs.osk_sound = !self.prefs.osk_sound;
                self.save_prefs(format!(
                    "Keypress sound: {}",
                    if self.prefs.osk_sound { "on" } else { "off" }
                ));
            }
            Hit::ThirdParty => {
                self.prefs.allow_third_party_keyboards = !self.prefs.allow_third_party_keyboards;
                if !self.prefs.allow_third_party_keyboards {
                    self.prefs.keyboard_id = "rabbiz-osk".into();
                }
                self.save_prefs(format!(
                    "3rd-party keyboards: {} — applies on next launch",
                    if self.prefs.allow_third_party_keyboards {
                        "allowed"
                    } else {
                        "blocked"
                    }
                ));
                self.reload_kbs();
            }
            Hit::KeyboardPick(i) => {
                if let Some(kb) = self.keyboards.get(i) {
                    if kb.installed {
                        self.prefs.keyboard_id = kb.id.clone();
                        self.save_prefs(format!("Keyboard: {} — applies on next launch", kb.name));
                    } else {
                        self.status = format!("{} is not installed", kb.name);
                    }
                }
            }
            _ => {}
        }
    }

    /// Slider fraction under `x` for `hit`, using the live layout.
    fn slider_frac_at(&self, hit: Hit, x: i32) -> Option<f32> {
        let (rows, _) = self.placed();
        let p = rows.iter().find(|p| p.hit == hit)?;
        Some(model::slider_frac(x, model::slider_track(p.rect)))
    }

    /// Live slider apply (brightness writes sysfs directly; volume is
    /// throttled and committed on release).
    fn apply_slider(&mut self, hit: Hit, x: i32) {
        let Some(f) = self.slider_frac_at(hit, x) else {
            return;
        };
        match hit {
            Hit::Brightness => {
                if system::set_brightness_frac(f) {
                    self.brightness = Some(f);
                } else {
                    self.status = "No backlight control found".into();
                }
            }
            Hit::Volume => {
                // Optimistic display; real write throttled below.
                self.volume = Some((f, false));
                if self.last_volume_write.elapsed() >= VOLUME_WRITE_INTERVAL
                    && system::set_volume_frac(f)
                {
                    self.last_volume_write = Instant::now();
                }
            }
            _ => {}
        }
    }

    /// Final slider commit on release.
    fn commit_slider(&mut self, hit: Hit) {
        match hit {
            Hit::Brightness => {
                self.brightness = system::backlight_level()
                    .map(|(raw, max)| (raw as f32 / max.max(1) as f32).clamp(0.0, 1.0));
            }
            Hit::Volume => {
                if let Some((f, _)) = self.volume {
                    let _ = system::set_volume_frac(f);
                }
                self.volume = system::volume();
            }
            _ => {}
        }
    }
}

impl AppState for SettingsApp {
    fn app_id(&self) -> &'static str {
        "rabbiz-settings"
    }
    fn title(&self) -> &'static str {
        "Settings"
    }

    fn handle_event(&mut self, event: AppEvent, ti: &TextInputCtl) -> AppFrame {
        self.drain_radio();
        match event {
            AppEvent::TouchDown { id, pos } => {
                if self.touch.is_some() {
                    return AppFrame {
                        dirty: true,
                        ..Default::default()
                    };
                }
                let hit = self.hit_at(pos);
                // Tap outside the search pill drops search focus (query kept).
                if !matches!(hit, Hit::SearchField | Hit::SearchClear) {
                    self.unfocus_search(ti);
                }
                self.pressed = (hit != Hit::None).then_some(hit);
                let mut drag = Drag::Pending;
                if matches!(hit, Hit::Brightness | Hit::Volume) {
                    self.apply_slider(hit, pos.x);
                    drag = Drag::Slider;
                }
                self.touch = Some(Touch {
                    id,
                    start: pos,
                    last: pos,
                    hit,
                    drag,
                });
                AppFrame {
                    dirty: true,
                    ..Default::default()
                }
            }
            AppEvent::TouchMove { id, pos } => {
                // Classify under a short borrow, then act on `self`.
                enum Act {
                    Scroll(i32),
                    Slider(Hit, i32),
                    None,
                }
                let act = {
                    let Some(t) = self.touch.as_mut() else {
                        return AppFrame::default();
                    };
                    if t.id != id {
                        return AppFrame::default();
                    }
                    match t.drag {
                        // Decide scroll vs tap once the finger has moved enough.
                        Drag::Pending => {
                            if (pos.y - t.start.y).abs() > 10 {
                                t.drag = Drag::Scroll;
                                let dy = pos.y - t.last.y;
                                t.last = pos;
                                Act::Scroll(dy)
                            } else {
                                Act::None
                            }
                        }
                        Drag::Scroll => {
                            let dy = pos.y - t.last.y;
                            t.last = pos;
                            Act::Scroll(dy)
                        }
                        Drag::Slider => Act::Slider(t.hit, pos.x),
                    }
                };
                match act {
                    Act::Scroll(dy) => {
                        self.scroll =
                            clamp_scroll(self.scroll - dy, self.content_h, self.viewport_h());
                    }
                    Act::Slider(hit, x) => self.apply_slider(hit, x),
                    Act::None => {}
                }
                AppFrame {
                    dirty: true,
                    ..Default::default()
                }
            }
            AppEvent::TouchUp { id, pos: _ } => {
                let Some(t) = self.touch.take() else {
                    return AppFrame::default();
                };
                if t.id != id {
                    // Put it back — a different finger lifted.
                    self.touch = Some(t);
                    return AppFrame::default();
                }
                let was_pressed = self.pressed.take();
                let was_pass_focused = self.wifi_pass_focused;
                match t.drag {
                    Drag::Pending => {
                        if was_pressed == Some(t.hit) && t.hit != Hit::None {
                            self.activate(t.hit, ti);
                        }
                    }
                    Drag::Slider => self.commit_slider(t.hit),
                    Drag::Scroll => {}
                }
                if self.wifi_pass_focused && !was_pass_focused {
                    ti.activate(ContentPurpose::Password);
                    ti.set_surrounding(self.wifi_pass.as_str(), self.wifi_pass.cursor());
                } else if !self.wifi_pass_focused && was_pass_focused && !self.search_focused {
                    ti.deactivate();
                }
                AppFrame {
                    dirty: true,
                    animating: self.wifi_scanning || self.bt_scanning,
                    ..Default::default()
                }
            }
            AppEvent::Key { keysym, utf8 } => {
                if self.wifi_pass_focused {
                    match keysym {
                        0xff1b => {
                            self.wifi_pass_ssid = None;
                            self.wifi_pass.clear();
                            self.wifi_pass_focused = false;
                            ti.deactivate();
                        }
                        0xff0d => {
                            if let Some(ssid) = self.wifi_pass_ssid.take() {
                                let pw = self.wifi_pass.as_str().to_string();
                                self.wifi_pass.clear();
                                self.wifi_pass_focused = false;
                                ti.deactivate();
                                self.status = format!("Connecting to {ssid}…");
                                system::connect_wifi_async(ssid, Some(pw), self.net_tx.clone());
                            }
                        }
                        0xff08 => {
                            self.wifi_pass.backspace();
                            ti.set_surrounding(self.wifi_pass.as_str(), self.wifi_pass.cursor());
                        }
                        _ => {
                            if let Some(s) = utf8.filter(|s| !s.is_empty()) {
                                self.wifi_pass.insert(&s);
                                ti.set_surrounding(self.wifi_pass.as_str(), self.wifi_pass.cursor());
                            }
                        }
                    }
                    return AppFrame {
                        dirty: true,
                        animating: self.wifi_scanning || self.bt_scanning,
                        ..Default::default()
                    };
                }
                if self.search_focused {
                    match keysym {
                        // Esc / Enter drop focus (the query stays put).
                        0xff1b | 0xff0d => self.unfocus_search(ti),
                        0xff08 => {
                            self.query.backspace();
                            self.after_search_edit(ti);
                        }
                        0xff51 => {
                            self.query.move_left();
                            ti.set_surrounding(self.query.as_str(), self.query.cursor());
                        }
                        0xff53 => {
                            self.query.move_right();
                            ti.set_surrounding(self.query.as_str(), self.query.cursor());
                        }
                        _ => {
                            if let Some(s) = utf8 {
                                if !s.is_empty() {
                                    self.query.insert(&s);
                                    self.after_search_edit(ti);
                                }
                            }
                        }
                    }
                    return AppFrame {
                        dirty: true,
                        ..Default::default()
                    };
                } else if keysym == 0xff1b {
                    if !self.query.is_empty() {
                        self.query.clear();
                        self.scroll = 0;
                        return AppFrame {
                            dirty: true,
                            ..Default::default()
                        };
                    }
                    if self.open.wifi
                        || self.open.bluetooth
                        || self.open.wallpaper
                        || self.open.timeout
                        || self.open.search_engine
                        || self.open.keyboard
                    {
                        self.open = Open::default();
                        self.wifi_pass_ssid = None;
                        self.wifi_pass_focused = false;
                        return AppFrame {
                            dirty: true,
                            ..Default::default()
                        };
                    }
                    return AppFrame {
                        quit: true,
                        ..Default::default()
                    };
                }
                AppFrame::default()
            }
            AppEvent::TextCommit(s) => {
                if self.wifi_pass_focused {
                    self.wifi_pass.insert(&s);
                    ti.set_surrounding(self.wifi_pass.as_str(), self.wifi_pass.cursor());
                } else if self.search_focused {
                    self.query.insert(&s);
                    self.after_search_edit(ti);
                }
                AppFrame {
                    dirty: true,
                    ..Default::default()
                }
            }
            AppEvent::TextBackspace(n) => {
                if self.wifi_pass_focused {
                    for _ in 0..n {
                        self.wifi_pass.backspace();
                    }
                    ti.set_surrounding(self.wifi_pass.as_str(), self.wifi_pass.cursor());
                } else if self.search_focused {
                    for _ in 0..n {
                        self.query.backspace();
                    }
                    self.after_search_edit(ti);
                }
                AppFrame {
                    dirty: true,
                    ..Default::default()
                }
            }
            AppEvent::Tick(_) => {
                let changed = self.drain_net();
                AppFrame {
                    dirty: changed,
                    animating: self.wifi_scanning || self.bt_scanning,
                    ..Default::default()
                }
            }
            _ => AppFrame::default(),
        }
    }

    fn needs_tick(&self) -> bool {
        self.wifi_scanning || self.bt_scanning
    }

    fn render(&mut self, canvas: &mut Canvas, theme: &Theme) {
        self.drain_radio();
        self.drain_net();
        let size = canvas.size();
        self.size = size;
        let head_end = ui::draw_header(canvas, theme, "Settings", "");
        let pill = Rect::new(
            ui::MARGIN,
            head_end - 4,
            (size.w - ui::MARGIN * 2).max(0),
            widgets::SearchPill::HEIGHT,
        );
        widgets::SearchPill::render(
            canvas,
            pill,
            theme,
            self.query.as_str(),
            "Search settings",
            self.search_focused,
        );
        self.search_rect = pill;
        self.top = pill.bottom() + 12;
        let (rows, content_h) = self.placed();
        self.content_h = content_h;
        self.scroll = clamp_scroll(self.scroll, content_h, self.viewport_h());
        for p in &rows {
            self.draw_row(canvas, theme, p);
        }
        // AOSP search: a friendly empty state when nothing matches.
        let filtering = !self.query.as_str().trim().is_empty();
        let has_pref = rows.iter().any(|p| {
            matches!(
                p.view,
                View::Switch { .. } | View::Slider { .. } | View::Nav { .. } | View::KeyRow { .. }
            )
        });
        if filtering && !has_pref {
            widgets::EmptyState::render(
                canvas,
                Rect::new(0, self.top, size.w.max(0), self.viewport_h()),
                theme,
                icons::SEARCH,
                "No results",
                &format!("No settings match “{}”", self.query.as_str()),
            );
        }
        let track = Rect::new(size.w - 8, self.top, 6, self.viewport_h());
        ui::scrollbar(canvas, theme, track, self.scroll, content_h);
        widgets::Toast::render(canvas, size, ui::BOTTOM_SAFE, theme, &self.status);
    }
}

impl SettingsApp {
    /// Unified right-edge value gutter inside a card row (value text ends
    /// `GUTTER` px before the row's right edge; chevron/switch share it).
    const GUTTER: i32 = 150;

    fn draw_row(&self, canvas: &mut Canvas, theme: &Theme, p: &Placed) {
        let pressed = self.pressed == Some(p.hit);
        let cy = p.rect.y + p.rect.h / 2;
        let text_x = p.rect.x + model::TEXT_X;
        let icon_cx = p.rect.x + model::ICON_X + model::ICON_W / 2;
        let radius = if p.round_top || p.round_bot {
            de_design::tokens::RADIUS_CARD
        } else {
            0.0
        };
        let press = |c: &mut Canvas| {
            if pressed {
                ui::press_overlay(c, theme, p.rect, radius, 0.10);
            }
        };
        match &p.view {
            View::CardBg => ui::draw_card(canvas, theme, p.rect, false),
            View::Hairline => {
                ui::divider_inset(
                    canvas,
                    theme,
                    text_x,
                    p.rect.right() - model::TRAIL_PAD,
                    p.rect.y,
                );
            }
            View::Section(name) => {
                ui::draw_text(
                    canvas,
                    name,
                    &ui::styled(17.0, 600, theme.text2),
                    p.rect.x + 16,
                    p.rect.y + 12,
                    Some(p.rect.w - 32),
                );
            }
            View::TextField {
                placeholder,
                value,
                focused,
            } => {
                let box_r = p.rect.inset(12);
                canvas.fill_rounded_rect(box_r, 14.0, theme.surface2);
                if *focused {
                    ui::stroke_rounded(canvas, theme, box_r, 14.0, theme.accent, 2);
                }
                let shown = if value.is_empty() {
                    placeholder.as_str()
                } else {
                    value.as_str()
                };
                let col = if value.is_empty() {
                    theme.text3
                } else {
                    theme.text
                };
                ui::draw_text(
                    canvas,
                    shown,
                    &ui::body(col),
                    box_r.x + 16,
                    box_r.y + (box_r.h - 28) / 2,
                    Some((box_r.w - 32).max(16)),
                );
            }
            View::Note(text) => {
                ui::draw_centered(canvas, text, &ui::caption(theme.text2), p.rect);
            }
            View::Switch {
                icon,
                title,
                sub,
                on,
                unknown,
            } => {
                let ghost = *unknown;
                let tcol = if ghost { theme.text3 } else { theme.text };
                icon_centered(
                    canvas,
                    icon,
                    24.0,
                    if ghost { theme.text3 } else { theme.text2 },
                    icon_cx,
                    cy,
                );
                let gutter = Self::GUTTER.max(96);
                let text_w = (p.rect.right() - gutter - text_x).max(8);
                if let Some(s) = sub {
                    ui::draw_text(
                        canvas,
                        title,
                        &ui::body(tcol),
                        text_x,
                        cy - 28,
                        Some(text_w),
                    );
                    ui::draw_text(
                        canvas,
                        s,
                        &ui::caption(theme.text3),
                        text_x,
                        cy + 4,
                        Some(text_w),
                    );
                } else {
                    ui::draw_text(
                        canvas,
                        title,
                        &ui::body(tcol),
                        text_x,
                        cy - 14,
                        Some(text_w),
                    );
                }
                // Trailing switch — ghosted styling when the state is unknown
                // (AOSP disables the control instead of faking "off").
                let sw = Rect::new(
                    p.rect.right() - model::TRAIL_PAD - widgets::ToggleSwitch::WIDTH,
                    cy - widgets::ToggleSwitch::HEIGHT / 2,
                    widgets::ToggleSwitch::WIDTH,
                    widgets::ToggleSwitch::HEIGHT,
                );
                if ghost {
                    canvas.fill_rounded_rect(sw, 16.0, ui::with_alpha(theme.surface2, 0x99));
                    canvas.fill_rounded_rect(
                        Rect::new(sw.x + 4, sw.y + 4, 24, 24),
                        12.0,
                        theme.surface3,
                    );
                } else {
                    widgets::ToggleSwitch::render(canvas, sw, theme, *on);
                }
                press(canvas);
            }
            View::Slider {
                icon,
                title,
                frac,
                value,
            } => {
                let known = frac.is_some();
                let tcol = if known { theme.text } else { theme.text3 };
                icon_centered(
                    canvas,
                    icon,
                    24.0,
                    if known { theme.text2 } else { theme.text3 },
                    icon_cx,
                    p.rect.y + 40,
                );
                let gutter = 96;
                ui::draw_text(
                    canvas,
                    title,
                    &ui::body(tcol),
                    text_x,
                    p.rect.y + 24,
                    Some(p.rect.right() - gutter - text_x),
                );
                // Value in the shared right gutter, right-aligned.
                let vr = Rect::new(
                    p.rect.right() - model::TRAIL_PAD - gutter,
                    p.rect.y,
                    gutter,
                    56,
                );
                ui::draw_text_right(
                    canvas,
                    value,
                    &ui::label(if known { theme.text2 } else { theme.text3 }),
                    vr,
                    0,
                );
                let track = model::slider_track(p.rect);
                if let Some(f) = frac {
                    let active = matches!(
                        &self.touch,
                        Some(t) if matches!(t.drag, Drag::Slider) && t.hit == p.hit
                    );
                    widgets::slider(canvas, theme, track, *f, active);
                } else {
                    // Unavailable control: groove only, no thumb.
                    let cy2 = track.y + track.h / 2;
                    canvas.fill_rounded_rect(
                        Rect::new(track.x, cy2 - 4, track.w, 8),
                        4.0,
                        theme.surface2,
                    );
                }
                press(canvas);
            }
            View::Nav {
                icon,
                title,
                value,
                open,
            } => {
                icon_centered(canvas, icon, 24.0, theme.text2, icon_cx, cy);
                let trail = p.rect.right() - model::TRAIL_PAD;
                // Chevron shares the gutter; value text is right-aligned
                // just before it (same edge as the slider/switch gutter).
                let chev_icon = if *open {
                    icons::EXPAND_LESS
                } else {
                    icons::CHEVRON_RIGHT
                };
                icon_centered(canvas, chev_icon, 24.0, theme.text3, trail - 14, cy);
                let value_w = Self::GUTTER;
                let vr = Rect::new(trail - 30 - value_w, p.rect.y, value_w, p.rect.h);
                ui::draw_text_right(canvas, value, &ui::caption(theme.text3), vr, 0);
                ui::draw_text(
                    canvas,
                    title,
                    &ui::body(theme.text),
                    text_x,
                    cy - 14,
                    Some((vr.x - 12 - text_x).max(8)),
                );
                press(canvas);
            }
            View::Swatch {
                index,
                name,
                selected,
            } => {
                let id = de_paint::wallpaper::IDS.get(*index).copied().unwrap_or("");
                if let Some((_, px)) = self.wallpaper_px.iter().find(|(k, _)| k == id) {
                    // Aspect-correct (previews are 112x84 = 4:3), centered in
                    // the space above the name strip.
                    let avail_w = p.rect.w - 16;
                    let avail_h = p.rect.h - 50;
                    let pw = avail_w.min(avail_h * 4 / 3);
                    let ph = pw * 3 / 4;
                    let preview = Rect::new(
                        p.rect.x + 8 + (avail_w - pw) / 2,
                        p.rect.y + 6 + (avail_h - ph) / 2,
                        pw,
                        ph,
                    );
                    canvas.blit_scaled(px, preview, 1.0);
                    if *selected {
                        // M3 accent outline around the chosen swatch.
                        ui::stroke_rounded(canvas, theme, preview.inset(-3), 14.0, theme.accent, 3);
                    }
                }
                let name_col = if *selected { theme.accent } else { theme.text };
                ui::draw_centered(
                    canvas,
                    name,
                    &ui::caption(name_col),
                    Rect::new(p.rect.x + 4, p.rect.y + p.rect.h - 32, p.rect.w - 8, 24),
                );
                if pressed {
                    ui::press_overlay(canvas, theme, p.rect, 16.0, 0.10);
                }
            }
            View::Pill { label, selected } => {
                widgets::chip(canvas, theme, p.rect, label, *selected, pressed);
            }
            View::KeyRow {
                name,
                installed,
                active,
            } => {
                let enabled = *installed;
                let ncol = if enabled { theme.text } else { theme.text3 };
                let check_cx = p.rect.right() - model::TRAIL_PAD - 12;
                let text_w = (check_cx - 20 - text_x).max(8);
                if enabled || *active {
                    ui::draw_text(canvas, name, &ui::body(ncol), text_x, cy - 14, Some(text_w));
                } else {
                    ui::draw_text(canvas, name, &ui::body(ncol), text_x, cy - 28, Some(text_w));
                    ui::draw_text(
                        canvas,
                        "Not installed",
                        &ui::caption(theme.text3),
                        text_x,
                        cy + 4,
                        Some(text_w),
                    );
                }
                // Radio-style trailing: filled check for the active keyboard.
                icon_centered(
                    canvas,
                    if *active {
                        icons::RADIO_BUTTON_CHECKED
                    } else {
                        icons::RADIO_BUTTON_UNCHECKED
                    },
                    24.0,
                    if *active { theme.accent } else { theme.text3 },
                    check_cx,
                    cy,
                );
                press(canvas);
            }
        }
    }
}

/// Rasterize + blit a Material icon with its ink bounds centered on (cx, cy).
fn icon_centered(c: &mut Canvas, codepoint: &str, size: f32, color: [u8; 4], cx: i32, cy: i32) {
    if let Some(px) = text::icon(codepoint, size, color) {
        let (x0, y0, x1, y1) =
            text::ink_bounds(&px).unwrap_or((0, 0, px.width() as i32, px.height() as i32));
        c.blit(
            &Pixels::new(
                Size::new(px.width() as i32, px.height() as i32),
                px.data().to_vec(),
            ),
            Point::new(cx - (x0 + x1) / 2, cy - (y0 + y1) / 2),
        );
    }
}

/// Write the `dark|light` mode file the DE reads at startup
/// (`de-design::theme::mode_file`), atomically. Shared by the GUI toggle
/// and the `--cli set deTheme` / `settings_set` path.
pub(crate) fn write_theme_file(mode: &str) -> std::io::Result<()> {
    let path = de_design::theme::mode_file();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, format!("{mode}\n"))?;
    std::fs::rename(&tmp, &path)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(pos) = args.iter().position(|a| a == "--cli") {
        std::process::exit(cli::run(&args[pos + 1..]));
    }
    if args.iter().any(|a| a == "--mcp") {
        rabbiz_app_kit::mcp::serve(cli::TOOLS, "rabbiz-settings", "0.1.0");
    }
    let mut app = SettingsApp::new();
    if let Some(pos) = args.iter().position(|a| a == "--open") {
        if let Some(sec) = args.get(pos + 1) {
            app.open_section(sec);
        }
    }
    rabbiz_app_kit::run_app(app);
}

#[cfg(test)]
mod tests {
    use super::*;
    use rabbiz_app_kit::TestHarness;

    /// Render-evidence shots: run this test, keep the PNGs, then compare
    /// against the pre-restyle frames (`settings-before-*.png`).
    #[test]
    fn harness_renders_settings_and_saves_shots() {
        let mut h = TestHarness::new(SettingsApp::new());
        h.save_png(std::path::Path::new(
            "/tmp/rabbiz-shots/settings-after-main.png",
        ))
        .expect("settings-after-main.png");

        h.state_mut().open.wallpaper = true;
        h.state_mut().open.timeout = true;
        h.state_mut().open.keyboard = true;
        h.save_png(std::path::Path::new(
            "/tmp/rabbiz-shots/settings-after-expanded.png",
        ))
        .expect("settings-after-expanded.png");
        h.state_mut().open = Open::default();

        // Every placed row stays inside the panel bounds.
        let (rows, _h) = h.state().placed();
        for p in &rows {
            assert!(p.rect.y + p.rect.h > 0 && p.rect.y < h.size().h);
        }

        // Search pill focuses on tap, filters rows on IME commit.
        let pill = h.state().search_rect;
        h.tap(Point::new(pill.x + pill.w / 2, pill.y + pill.h / 2));
        assert!(h.state().search_focused);
        h.type_text("bright");
        h.save_png(std::path::Path::new(
            "/tmp/rabbiz-shots/settings-after-search.png",
        ))
        .expect("settings-after-search.png");
        let (rows, _) = h.state().placed();
        let titles: Vec<&str> = rows
            .iter()
            .filter_map(|p| match &p.view {
                View::Slider { title, .. }
                | View::Switch { title, .. }
                | View::Nav { title, .. } => Some(*title),
                _ => None,
            })
            .collect();
        assert_eq!(titles, ["Brightness"]);
    }
}
