//! The notification panel (pulled down from the top-left) and the quick
//! actions (top-right): frosted full-screen sheets drawn over whatever is on
//! screen, the home screen or an app, as their own texture. Dragging only
//! moves that texture; the panel image is rendered when its content changes.
//! Geometry and hit testing are `de_ui::panels`, the pull maths `de_ui::pull`.

use std::time::Instant;

use de_api::{Pixels, Point, Size};
use de_design::tokens;
use de_ui::nav::Screen;
use de_ui::panels::{self, NotifHit, Notification, QuickHit, QuickModel, QuickTile};
use de_ui::pull::Pull;

use crate::state::{De, Foreground};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum PanelKind {
    Notifications,
    Quick,
}

/// One quick tile's identity; the first `WIDE_TILES` are the wide pills.
struct TileDef {
    id: &'static str,
    glyph: &'static str,
    label: &'static str,
}

const TILES: [TileDef; 12] = [
    TileDef { id: "wifi", glyph: "\u{e63e}", label: "qs.wifi" },
    TileDef { id: "bluetooth", glyph: "\u{e1a7}", label: "qs.bluetooth" },
    TileDef { id: "airplane", glyph: "\u{e195}", label: "qs.airplane" },
    TileDef { id: "dnd", glyph: "\u{e644}", label: "qs.dnd" },
    TileDef { id: "flashlight", glyph: "\u{e3e7}", label: "qs.flashlight" },
    TileDef { id: "rotate", glyph: "\u{e1c1}", label: "qs.rotate" },
    TileDef { id: "battery_saver", glyph: "\u{efde}", label: "qs.battery_saver" },
    TileDef { id: "location", glyph: "\u{e0c8}", label: "qs.location" },
    TileDef { id: "hotspot", glyph: "\u{e1e2}", label: "qs.hotspot" },
    TileDef { id: "screenshot", glyph: "\u{e412}", label: "qs.screenshot" },
    TileDef { id: "record", glyph: "\u{e04b}", label: "qs.record" },
    TileDef { id: "nfc", glyph: "\u{e1bb}", label: "NFC" },
];

pub(crate) struct PanelUi {
    kind: PanelKind,
    pub(crate) pull: Pull,
    start_x: i32,
    /// The panel image for `kind` and a counter that changes with it.
    pub(crate) pixels: Option<Pixels>,
    pub(crate) generation: u64,
    pub(crate) tex: Option<Box<dyn std::any::Any>>,
    pub(crate) tex_generation: u64,
    /// The frosted sheet surface beneath the controls (`panels::render_veil`),
    /// rebuilt whenever the panel image is.
    pub(crate) veil: Option<Pixels>,
    pub(crate) veil_tex: Option<Box<dyn std::any::Any>>,
    pub(crate) veil_tex_generation: u64,
    stale: bool,
    blur: Option<(String, Pixels)>,
    last_tick: Option<Instant>,
    on: [bool; 12],
    available: [bool; 12],
    controls: de_system::controls::Controls,
    control_state: de_system::controls::Snapshot,
    server: de_system::notifications::Server,
    notification_generation: u64,
    brightness: f32,
    volume: f32,
    pub(crate) notifications: Vec<Notification>,
}

impl PanelUi {
    pub(crate) fn new() -> Self {
        Self {
            kind: PanelKind::Quick,
            pull: Pull::new(),
            start_x: 0,
            pixels: None,
            generation: 0,
            tex: None,
            tex_generation: u64::MAX,
            veil: None,
            veil_tex: None,
            veil_tex_generation: u64::MAX,
            stale: true,
            blur: None,
            last_tick: None,
            on: [false; 12],
            available: [false; 12],
            controls: de_system::controls::Controls::start(),
            control_state: de_system::controls::Snapshot::default(),
            server: de_system::notifications::Server::start(),
            notification_generation: u64::MAX,
            brightness: 0.6,
            volume: 0.5,
            notifications: Vec::new(),
        }
    }

    pub(crate) fn shown(&self) -> bool {
        self.pull.shown()
    }

    /// Torch state for the lock-screen flashlight button.
    pub(crate) fn flashlight(&self) -> Option<bool> {
        self.control_state.flashlight
    }

    /// Forward a control action (lock screen toggles the torch too).
    pub(crate) fn request_control(&mut self, action: de_system::controls::Action) {
        self.controls.request(action);
        self.stale = true;
    }
}

impl De {
    /// Called by the event-loop timer, never performs external I/O here.
    pub(crate) fn poll_panels(&mut self) {
        let state = self.panels.controls.snapshot();
        let (generation, entries) = self.panels.server.snapshot();
        let changed = self.panels.control_state != state
            || generation != self.panels.notification_generation;
        if !changed { return; }
        self.panels.on[3] = self.app_prefs.prefs().do_not_disturb;
        self.panels.available[3] = true;
        for (i, value) in [(0, state.wifi), (1, state.bluetooth), (2, state.airplane),
            (4, state.flashlight), (6, state.battery_saver)] {
            self.panels.available[i] = value.is_some();
            self.panels.on[i] = value.unwrap_or(false);
        }
        // Auto-rotation, GNSS/location, hotspot, recording and NFC stay
        // unavailable until their own providers expose a working contract.
        self.panels.brightness = state.brightness.unwrap_or(0.0);
        self.panels.volume = state.volume.unwrap_or(0.0);
        self.panels.control_state = state;
        self.panels.notification_generation = generation;
        self.panels.notifications = entries.into_iter().map(|e| Notification {
            id: e.id, app: e.app, title: e.title, body: e.body,
            when: format!("{} min", e.created.elapsed().as_secs() / 60), icon: None,
        }).collect();
        self.panels.stale = true;
        if self.panels.shown() { self.needs_present = true; }
    }

    fn panel_allowed(&self) -> bool {
        !matches!(
            self.nav.current,
            Screen::Lock | Screen::Boot | Screen::Power | Screen::FirstRun
        ) && self.morph.is_none()
            && !self.menu_is_open()
            && !self.locked()
            && !self.sleeping()
    }

    fn panel_touch(&mut self) {
        self.needs_present = true;
    }

    /// A panel is on screen: it owns every touch until it is gone.
    pub(crate) fn panel_wants_touch(&self) -> bool {
        self.panels.shown()
    }

    pub(crate) fn panel_touch_started(&mut self, pos: Point) {
        self.panels.start_x = pos.x;
    }

    /// Finger moved. `from_top` is true for a pull that started in the top
    /// strip; `dy` is relative to touch-down. Returns whether the panel took
    /// the movement (the caller then skips home paging and the rest).
    pub(crate) fn panel_progress(&mut self, from_top: bool, dy: f32) -> bool {
        if self.panels.pull.dragging() {
            self.panels.pull.drag(dy);
            self.panel_touch();
            return true;
        }
        if self.panels.pull.shown() {
            // Open: an upward drag grabs it to close; anything else is swallowed.
            if dy < 0.0 {
                self.panels.pull.begin();
                self.panels.pull.drag(dy);
                self.panel_touch();
            }
            return true;
        }
        if from_top && dy > 0.0 && self.panel_allowed() {
            let w = self.output.width as i32;
            self.panels.kind = if self.panels.start_x < w / 2 {
                PanelKind::Notifications
            } else {
                PanelKind::Quick
            };
            // Capture before the panel is visible, so the glass samples the
            // real Home/app and never captures its own previous frame.
            if let Some(scene) = crate::composite::composite_output(self) {
                let at = Instant::now();
                self.panels.blur = Some((self.app_prefs.prefs().wallpaper_id.clone(),
                    de_paint::glass::blurred_backdrop(&scene)));
                if std::env::var_os("DE_GESTURE_DEBUG").is_some() {
                    eprintln!("panel-backdrop blur_ms={:.2}", at.elapsed().as_secs_f64() * 1000.0);
                }
            }
            self.panels.stale = true;
            self.panels.pull.begin();
            self.panels.pull.drag(dy);
            self.panel_touch();
            return true;
        }
        false
    }

    pub(crate) fn panel_released(&mut self, vy: f32) {
        if self.panels.pull.dragging() {
            self.panels.pull.release(vy);
            self.panels.last_tick = None;
            self.panel_touch();
        }
    }

    pub(crate) fn panel_cancelled(&mut self) {
        self.panel_released(0.0);
    }

    pub(crate) fn panel_close(&mut self) {
        if self.panels.pull.shown() {
            self.panels.pull.close();
            self.panels.last_tick = None;
            self.panel_touch();
        }
    }

    /// Per-frame settle of the pull spring.
    pub(crate) fn advance_panel(&mut self, now: Instant) {
        let p = &mut self.panels;
        if !p.pull.animating() || p.pull.dragging() {
            return;
        }
        let dt = p
            .last_tick
            .map_or(1.0 / 60.0, |t| now.duration_since(t).as_secs_f32())
            .clamp(0.001, 0.05);
        p.last_tick = Some(now);
        if !p.pull.advance(dt, &self.motion_clock) {
            p.last_tick = None;
        }
        self.needs_present = true;
    }

    pub(crate) fn panel_wake_at(&self, interval: std::time::Duration) -> Option<Instant> {
        let p = &self.panels.pull;
        (p.animating() && !p.dragging()).then(|| Instant::now() + interval)
    }

    /// A tap while a panel is up. Always consumed.
    pub(crate) fn panel_tap(&mut self, pos: Point) {
        let size = Size::new(self.output.width as i32, self.output.height as i32);
        if self.panels.pull.dragging() || !self.panels.pull.wants_open() {
            return;
        }
        match self.panels.kind {
            PanelKind::Notifications => {
                match panels::notif_hit(size, pos, self.panels.notifications.len()) {
                    Some(NotifHit::ClearAll) => {
                        self.panels.server.dismiss(0);
                        self.panels.notifications.clear();
                        self.panels.stale = true;
                    }
                    Some(NotifHit::Card(i)) => {
                        if let Some(n) = self.panels.notifications.get(i) {
                            self.panels.server.dismiss(n.id);
                        }
                        self.panels.notifications.remove(i);
                        self.panels.stale = true;
                    }
                    None => self.panel_close(),
                }
            }
            PanelKind::Quick => match panels::quick_hit(size, pos) {
                Some(QuickHit::Tile(i)) => {
                    use de_system::controls::Action;
                    if !self.panels.available.get(i).copied().unwrap_or(false) { return; }
                    let on = !self.panels.on[i];
                    let action = match i {
                        0 => Some(Action::Wifi(on)), 1 => Some(Action::Bluetooth(on)),
                        2 => Some(Action::Airplane(on)), 4 => Some(Action::Flashlight(on)),
                        6 => Some(Action::BatterySaver(on)),
                        3 => {
                            if self.app_prefs.set_do_not_disturb(on) {
                                self.panels.on[i] = on;
                            }
                            self.panels.stale = true;
                            None
                        },
                        _ => None,
                    };
                    if let Some(action) = action { self.panels.controls.request(action); }
                }
                Some(QuickHit::Brightness(f)) => {
                    if self.panels.control_state.brightness.is_some() {
                        self.panels.controls.request(de_system::controls::Action::Brightness(f));
                    }
                }
                Some(QuickHit::Volume(f)) => {
                    if self.panels.control_state.volume.is_some() {
                        self.panels.controls.request(de_system::controls::Action::Volume(f));
                    }
                }
                Some(QuickHit::Settings) => {
                    self.panel_close();
                    self.show_shell_screen(Screen::Settings);
                }
                Some(QuickHit::Power) => {
                    self.panel_close();
                    self.show_shell_screen(Screen::Power);
                }
                None => self.panel_close(),
            },
        }
        self.panel_touch();
    }

    fn quick_model(&self) -> QuickModel {
        let s = &self.strings;
        let tiles = TILES
            .iter()
            .enumerate()
            .map(|(i, d)| QuickTile {
                id: d.id,
                label: s.get(d.label).to_owned(),
                glyph: d.glyph,
                on: self.panels.on[i],
                available: self.panels.available[i],
                detail: s
                    .get(if self.panels.on[i] { "qs.on" } else { "qs.off" })
                    .to_owned(),
            })
            .collect();
        QuickModel {
            tiles,
            brightness: self.panels.brightness,
            volume: self.panels.volume,
            brightness_available: self.panels.control_state.brightness.is_some(),
            volume_available: self.panels.control_state.volume.is_some(),
            feedback: if self.panels.control_state.pending { Some(s.get("qs.working").to_owned()) }
                else if self.panels.control_state.error.is_some() { Some(s.get("qs.failed").to_owned()) }
                else { None },
        }
    }

    /// Render the panel image if the content changed (before the frame is built).
    pub(crate) fn panel_prepare(&mut self) {
        if !self.panels.pull.shown() || (self.panels.pixels.is_some() && !self.panels.stale) {
            return;
        }
        let size = Size::new(self.output.width as i32, self.output.height as i32);
        let wp_id = self.app_prefs.prefs().wallpaper_id.clone();
        if self.panels.blur.as_ref().map(|(id, _)| id.as_str()) != Some(wp_id.as_str()) {
            let wp = self.wallpaper_for(&wp_id, size);
            self.panels.blur = Some((wp_id, de_paint::glass::blurred_backdrop(&wp)));
        }
        let (status, date) = self.shell_meta();
        let strings = self.strings.clone();
        let t = tokens::DEFAULT;
        let Some((_, backdrop)) = self.panels.blur.as_ref() else {
            return;
        };
        let raster_at = Instant::now();
        let px = match self.panels.kind {
            PanelKind::Notifications => panels::render_notifications(
                size,
                &strings,
                &t,
                backdrop,
                &status.clock,
                &date,
                &self.panels.notifications,
            ),
            PanelKind::Quick => {
                let m = self.quick_model();
                let Some((_, backdrop)) = self.panels.blur.as_ref() else {
                    return;
                };
                panels::render_quick(size, &strings, &t, backdrop, &status.clock, &date, &m)
            }
        };
        let veil = panels::render_veil(backdrop, t.scrim);
        if std::env::var_os("RABBIZ_DE_MOTION_TRACE").is_some() {
            eprintln!("panel-raster cpu_ms={:.3}", raster_at.elapsed().as_secs_f64() * 1000.0);
        }
        self.panels.pixels = Some(px);
        self.panels.veil = Some(veil);
        self.panels.generation += 1;
        self.panels.stale = false;
    }

    /// Switch to a shell screen from a panel (the app keeps running).
    fn show_shell_screen(&mut self, screen: Screen) {
        if self.foreground == Foreground::App {
            self.capture_topmost();
        }
        self.morph = None;
        self.foreground = Foreground::Shell;
        self.focus_foreground();
        self.nav.current = screen;
        self.shell_dirty = true;
        self.needs_present = true;
    }
}
