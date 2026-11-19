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
use rabbiz_app_kit::{ui, AppEvent, AppFrame, AppState, Canvas, Rect, TextInputCtl, Theme};

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
    bt: Option<bool>,
    last_volume_write: Instant,
    wallpaper_px: Vec<(String, Pixels)>,
    radio_tx: Sender<(system::Radio, Option<bool>)>,
    radio_rx: Receiver<(system::Radio, Option<bool>)>,
}

impl SettingsApp {
    fn new() -> Self {
        let prefs = Prefs::load();
        let keyboards = discover_keyboards(prefs.allow_third_party_keyboards);
        // Hardware reads once at startup (~0.5 s worst case for bluetoothctl);
        // radio toggles afterwards report back over the channel.
        let (wifi, bt) = system::poll_radios();
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
            bt,
            last_volume_write: Instant::now() - VOLUME_WRITE_INTERVAL,
            wallpaper_px,
            radio_tx,
            radio_rx,
        }
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
            bt: self.bt,
            wallpaper: self.prefs.wallpaper_id.clone(),
            timeout_sec: self.prefs.screen_timeout_sec,
            keyboard_name: self
                .keyboards
                .iter()
                .find(|k| k.id == self.prefs.keyboard_id)
                .map(|k| k.name.clone())
                .unwrap_or_else(|| self.prefs.keyboard_id.clone()),
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
        model::layout(self.size, self.top, self.scroll, &self.open, &self.facts())
    }

    fn hit_at(&self, pos: Point) -> Hit {
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

    fn activate(&mut self, hit: Hit) {
        match hit {
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
            Hit::KeyboardToggle => self.open.keyboard = !self.open.keyboard,
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

    fn handle_event(&mut self, event: AppEvent, _ti: &TextInputCtl) -> AppFrame {
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
                match t.drag {
                    Drag::Pending => {
                        if was_pressed == Some(t.hit) && t.hit != Hit::None {
                            self.activate(t.hit);
                        }
                    }
                    Drag::Slider => self.commit_slider(t.hit),
                    Drag::Scroll => {}
                }
                AppFrame {
                    dirty: true,
                    ..Default::default()
                }
            }
            _ => AppFrame::default(),
        }
    }

    fn render(&mut self, canvas: &mut Canvas, theme: &Theme) {
        self.drain_radio();
        let size = canvas.size();
        self.size = size;
        let top = ui::draw_header(canvas, theme, "Settings", &self.status);
        self.top = top;
        let (rows, content_h) = self.placed();
        self.content_h = content_h;
        for p in &rows {
            self.draw_row(canvas, theme, p);
        }
        self.draw_scrollbar(canvas, theme, size, top, content_h);
    }
}

impl SettingsApp {
    fn draw_scrollbar(
        &self,
        canvas: &mut Canvas,
        theme: &Theme,
        size: Size,
        top: i32,
        content_h: i32,
    ) {
        let vh = self.viewport_h();
        if content_h <= vh || vh <= 0 {
            return;
        }
        let thumb_h = ((vh * vh) / content_h.max(1)).clamp(48, vh);
        let travel = (vh - thumb_h).max(1);
        let max_scroll = (content_h - vh).max(1);
        let thumb_y = top + (self.scroll * travel / max_scroll).clamp(0, travel);
        canvas.fill_rounded_rect(
            Rect::new(size.w - ui::MARGIN + 8, thumb_y, 6, thumb_h),
            3.0,
            theme.surface3,
        );
    }

    fn draw_row(&self, canvas: &mut Canvas, theme: &Theme, p: &Placed) {
        let pressed = self.pressed == Some(p.hit);
        match &p.view {
            View::Section(name) => {
                ui::draw_text(
                    canvas,
                    name,
                    &ui::caption(theme.text3),
                    p.rect.x + 8,
                    p.rect.y + 10,
                    Some(p.rect.w - 16),
                );
            }
            View::Note(text) => {
                ui::draw_text(
                    canvas,
                    text,
                    &ui::caption(theme.text3),
                    p.rect.x,
                    p.rect.y + 2,
                    Some(p.rect.w),
                );
            }
            View::Switch {
                title,
                sub,
                on,
                unknown,
                ..
            } => {
                ui::draw_card(canvas, theme, p.rect, pressed);
                ui::draw_text(
                    canvas,
                    title,
                    &ui::label(theme.text),
                    p.rect.x + 24,
                    p.rect.y + (if sub.is_some() { 18 } else { 30 }),
                    Some(p.rect.w - 140),
                );
                if let Some(s) = sub {
                    ui::draw_text(
                        canvas,
                        s,
                        &ui::caption(theme.text3),
                        p.rect.x + 24,
                        p.rect.y + 48,
                        Some(p.rect.w - 140),
                    );
                }
                let _ = unknown; // visual state comes from `on` + sub text
                ui::draw_switch(
                    canvas,
                    theme,
                    p.rect.x + p.rect.w - 24 - 56,
                    p.rect.y + (p.rect.h - 32) / 2,
                    *on,
                );
            }
            View::Slider {
                title, frac, value, ..
            } => {
                ui::draw_card(canvas, theme, p.rect, pressed);
                ui::draw_text(
                    canvas,
                    title,
                    &ui::label(theme.text),
                    p.rect.x + 24,
                    p.rect.y + 18,
                    Some(p.rect.w - 160),
                );
                let vcol = if frac.is_some() {
                    theme.text2
                } else {
                    theme.text3
                };
                ui::draw_text(
                    canvas,
                    value,
                    &ui::caption(vcol),
                    p.rect.x + p.rect.w - 24 - 90,
                    p.rect.y + 22,
                    Some(90),
                );
                let track = model::slider_track(p.rect);
                canvas.fill_rounded_rect(track, 6.0, theme.surface2);
                if let Some(f) = frac {
                    let fill_w = (track.w as f32 * f.clamp(0.0, 1.0)) as i32;
                    if fill_w > 0 {
                        canvas.fill_rounded_rect(
                            Rect::new(track.x, track.y, fill_w, track.h),
                            6.0,
                            theme.accent,
                        );
                    }
                    let kx = track.x + ((track.w as f32 * f.clamp(0.0, 1.0)) as i32) - 11;
                    canvas.fill_rounded_rect(
                        Rect::new(
                            kx.clamp(track.x - 5, track.x + track.w - 11),
                            track.y - 5,
                            22,
                            22,
                        ),
                        11.0,
                        theme.accent_hi,
                    );
                } else {
                    canvas.fill_rounded_rect(
                        Rect::new(track.x, track.y - 5, 22, 22),
                        11.0,
                        theme.text3,
                    );
                }
            }
            View::Nav {
                title, value, open, ..
            } => {
                ui::draw_card(canvas, theme, p.rect, pressed);
                ui::draw_text(
                    canvas,
                    title,
                    &ui::label(theme.text),
                    p.rect.x + 24,
                    p.rect.y + 30,
                    Some(p.rect.w - 200),
                );
                ui::draw_text(
                    canvas,
                    value,
                    &ui::caption(theme.text3),
                    p.rect.x + p.rect.w - 24 - 150,
                    p.rect.y + 34,
                    Some(130),
                );
                draw_icon(
                    canvas,
                    if *open { "\u{e5d8}" } else { "\u{e5cf}" },
                    22.0,
                    theme.text3,
                    p.rect.x + p.rect.w - 24 - 22,
                    p.rect.y + (p.rect.h - 22) / 2,
                );
            }
            View::Swatch {
                index,
                name,
                selected,
                ..
            } => {
                ui::draw_card(canvas, theme, p.rect, pressed);
                let id = de_paint::wallpaper::IDS.get(*index).copied().unwrap_or("");
                if let Some((_, px)) = self.wallpaper_px.iter().find(|(k, _)| k == id) {
                    // Aspect-correct (previews are 112x84 = 4:3), centered in
                    // the space above the name strip.
                    let avail_w = p.rect.w - 20;
                    let avail_h = p.rect.h - 54;
                    let pw = avail_w.min(avail_h * 4 / 3);
                    let ph = pw * 3 / 4;
                    let preview = Rect::new(
                        p.rect.x + 10 + (avail_w - pw) / 2,
                        p.rect.y + 10 + (avail_h - ph) / 2,
                        pw,
                        ph,
                    );
                    canvas.blit_scaled(px, preview, 1.0);
                }
                let name_col = if *selected { theme.accent } else { theme.text };
                ui::draw_centered(
                    canvas,
                    name,
                    &ui::caption(name_col),
                    Rect::new(p.rect.x + 4, p.rect.y + p.rect.h - 34, p.rect.w - 8, 24),
                );
                if *selected {
                    canvas.fill_rounded_rect(
                        Rect::new(p.rect.x + 16, p.rect.y + p.rect.h - 8, p.rect.w - 32, 4),
                        2.0,
                        theme.accent,
                    );
                }
            }
            View::Pill {
                label, selected, ..
            } => {
                ui::draw_pill_button(canvas, theme, p.rect, label, *selected, pressed);
            }
            View::KeyRow {
                name,
                badge,
                enabled,
                ..
            } => {
                ui::draw_card(canvas, theme, p.rect, pressed);
                let ncol = if *enabled { theme.text } else { theme.text3 };
                ui::draw_text(
                    canvas,
                    name,
                    &ui::label(ncol),
                    p.rect.x + 24,
                    p.rect.y + (p.rect.h - 26) / 2,
                    Some(p.rect.w - 180),
                );
                let bcol = if *badge == "Active" {
                    theme.accent
                } else {
                    theme.text3
                };
                ui::draw_text(
                    canvas,
                    if *enabled { badge } else { "Not installed" },
                    &ui::caption(bcol),
                    p.rect.x + p.rect.w - 24 - 140,
                    p.rect.y + (p.rect.h - 22) / 2,
                    Some(140),
                );
            }
        }
    }
}

/// Material Symbols glyph → canvas (same conversion as `ui::draw_text`).
fn draw_icon(canvas: &mut Canvas, codepoint: &str, size: f32, color: [u8; 4], x: i32, y: i32) {
    if let Some(px) = text::icon(codepoint, size, color) {
        let w = px.width() as i32;
        let h = px.height() as i32;
        canvas.blit(
            &Pixels::new(Size::new(w, h), px.data().to_vec()),
            Point::new(x, y),
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
    rabbiz_app_kit::run_app(SettingsApp::new());
}
