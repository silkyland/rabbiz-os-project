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
    /// Whether anything on this device can actually change it yet.
    wired: bool,
}

const TILES: [TileDef; 12] = [
    TileDef { id: "wifi", glyph: "\u{e63e}", label: "qs.wifi", wired: false },
    TileDef { id: "bluetooth", glyph: "\u{e1a7}", label: "qs.bluetooth", wired: false },
    TileDef { id: "airplane", glyph: "\u{e195}", label: "qs.airplane", wired: false },
    TileDef { id: "dnd", glyph: "\u{e644}", label: "qs.dnd", wired: true },
    TileDef { id: "flashlight", glyph: "\u{e3e7}", label: "qs.flashlight", wired: false },
    TileDef { id: "rotate", glyph: "\u{e1c1}", label: "qs.rotate", wired: true },
    TileDef { id: "battery_saver", glyph: "\u{efde}", label: "qs.battery_saver", wired: false },
    TileDef { id: "location", glyph: "\u{e0c8}", label: "qs.location", wired: false },
    TileDef { id: "hotspot", glyph: "\u{e1e2}", label: "qs.hotspot", wired: false },
    TileDef { id: "screenshot", glyph: "\u{e412}", label: "qs.screenshot", wired: false },
    TileDef { id: "record", glyph: "\u{e04b}", label: "qs.record", wired: false },
    TileDef { id: "nfc", glyph: "\u{e1bb}", label: "NFC", wired: false },
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
    stale: bool,
    blur: Option<(String, Pixels)>,
    last_tick: Option<Instant>,
    on: [bool; 12],
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
            stale: true,
            blur: None,
            last_tick: None,
            on: [false; 12],
            brightness: 0.6,
            volume: 0.5,
            notifications: Vec::new(),
        }
    }

    pub(crate) fn shown(&self) -> bool {
        self.pull.shown()
    }
}

impl De {
    fn panel_allowed(&self) -> bool {
        !matches!(
            self.nav.current,
            Screen::Lock | Screen::Boot | Screen::Power | Screen::FirstRun
        ) && self.morph.is_none()
            && !self.menu_is_open()
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
                        self.panels.notifications.clear();
                        self.panels.stale = true;
                    }
                    Some(NotifHit::Card(i)) => {
                        self.panels.notifications.remove(i);
                        self.panels.stale = true;
                    }
                    None => self.panel_close(),
                }
            }
            PanelKind::Quick => match panels::quick_hit(size, pos) {
                Some(QuickHit::Tile(i)) => {
                    if TILES.get(i).is_some_and(|t| t.wired) {
                        self.panels.on[i] = !self.panels.on[i];
                        self.panels.stale = true;
                    }
                }
                Some(QuickHit::Brightness(f)) => {
                    self.panels.brightness = f;
                    self.panels.stale = true;
                }
                Some(QuickHit::Volume(f)) => {
                    self.panels.volume = f;
                    self.panels.stale = true;
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
                available: d.wired,
                detail: s
                    .get(if self.panels.on[i] { "qs.on" } else { "qs.off" })
                    .to_owned(),
            })
            .collect();
        QuickModel {
            tiles,
            brightness: self.panels.brightness,
            volume: self.panels.volume,
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
        self.panels.pixels = Some(px);
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
