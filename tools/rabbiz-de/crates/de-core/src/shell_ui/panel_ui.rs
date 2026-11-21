//! The notification panel (pulled down from the top-left) and the quick
//! actions (top-right): frosted full-screen sheets drawn over whatever is on
//! screen, the home screen or an app, as their own texture. Dragging only
//! moves that texture; the panel image is rendered when its content changes.
//! Geometry and hit testing are `de_ui::panels`, the pull maths `de_ui::pull`.

use std::time::Instant;

use de_api::{Pixels, Point, Size};
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
    TileDef {
        id: "wifi",
        glyph: "\u{e63e}",
        label: "qs.wifi",
    },
    TileDef {
        id: "bluetooth",
        glyph: "\u{e1a7}",
        label: "qs.bluetooth",
    },
    TileDef {
        id: "airplane",
        glyph: "\u{e195}",
        label: "qs.airplane",
    },
    TileDef {
        id: "dnd",
        glyph: "\u{e644}",
        label: "qs.dnd",
    },
    TileDef {
        id: "flashlight",
        glyph: "\u{e3e7}",
        label: "qs.flashlight",
    },
    TileDef {
        id: "rotate",
        glyph: "\u{e1c1}",
        label: "qs.rotate",
    },
    TileDef {
        id: "battery_saver",
        glyph: "\u{efde}",
        label: "qs.battery_saver",
    },
    TileDef {
        id: "location",
        glyph: "\u{e0c8}",
        label: "qs.location",
    },
    TileDef {
        id: "hotspot",
        glyph: "\u{e1e2}",
        label: "qs.hotspot",
    },
    TileDef {
        id: "screenshot",
        glyph: "\u{e412}",
        label: "qs.screenshot",
    },
    TileDef {
        id: "record",
        glyph: "\u{e04b}",
        label: "qs.record",
    },
    TileDef {
        id: "nfc",
        glyph: "\u{e1bb}",
        label: "NFC",
    },
];

pub(crate) struct PanelUi {
    kind: PanelKind,
    pub(crate) pull: Pull,
    start_x: i32,
    start_y: i32,
    slider_drag: Option<usize>,
    /// Grace window after a slider drag so stale `Controls` poll snapshots
    /// never snap the knob back while `brightnessctl`/`wpctl` catches up.
    slider_lock_until: [Option<Instant>; 2],
    last_slider_req: Option<Instant>,
    /// Pre-rendered panel image per kind, so a pull never waits on a raster.
    pub(crate) pixels: [Option<Pixels>; 2],
    pub(crate) generation: u64,
    pub(crate) tex: Option<Box<dyn std::any::Any>>,
    pub(crate) tex_generation: u64,
    pub(crate) tex_kind: PanelKind,
    stale: [bool; 2],
    /// The blurred scene behind the glass, tagged by the wallpaper it was
    /// captured for. A pull defers recapture to `capture_pending`.
    blur: Option<(String, Pixels)>,
    capture_pending: bool,
    blur_rx: Option<(String, std::sync::mpsc::Receiver<Pixels>)>,
    pub(crate) use_gpu_blur: bool,
    pub(crate) gpu_blur_dirty: bool,
    pub(crate) gpu_blur: Option<de_render::blur::GpuBlur>,
    pub(crate) scrim_buf: Option<smithay::backend::renderer::element::solid::SolidColorBuffer>,
    /// Panel raster worker: kind index → pixels. Keeps the ~90 ms raster
    /// off the frame path entirely.
    render_rx: Option<(usize, u64, std::sync::mpsc::Receiver<Pixels>)>,
    content_gen: u64,
    /// Wall-clock minute the panel images were last rendered for: their
    /// header draws the clock, so a new minute makes them stale.
    clock_minute: u64,
    last_tick: Option<Instant>,
    on: [bool; 12],
    available: [bool; 12],
    controls: de_system::controls::Controls,
    pub(crate) control_state: de_system::controls::Snapshot,
    server: de_system::notifications::Server,
    notification_generation: u64,
    /// Last seen `media.generation` — marks the shade stale when
    /// the active player/track changes.
    media_generation: u64,
    pub(crate) brightness: f32,
    pub(crate) volume: f32,
    pub(crate) notifications: Vec<Notification>,
}

impl PanelUi {
    pub(crate) fn new() -> Self {
        Self {
            kind: PanelKind::Notifications,
            pull: Pull::new(),
            start_x: 0,
            start_y: 0,
            slider_drag: None,
            slider_lock_until: [None, None],
            last_slider_req: None,
            pixels: [None, None],
            generation: 0,
            tex: None,
            tex_generation: u64::MAX,
            tex_kind: PanelKind::Quick,
            stale: [true; 2],
            blur: None,
            capture_pending: false,
            blur_rx: None,
            use_gpu_blur: false,
            gpu_blur_dirty: false,
            gpu_blur: None,
            scrim_buf: None,
            render_rx: None,
            content_gen: 0,
            clock_minute: u64::MAX,
            last_tick: None,
            on: [false; 12],
            available: [false; 12],
            controls: de_system::controls::Controls::start(),
            control_state: de_system::controls::Snapshot::default(),
            server: de_system::notifications::Server::start(),
            notification_generation: u64::MAX,
            media_generation: u64::MAX,
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
        self.stale = [true; 2];
        self.content_gen += 1;
    }

    /// The panel image for the kind a pull would show right now.
    pub(crate) fn active_pixels(&self) -> Option<&Pixels> {
        self.pixels[self.kind as usize].as_ref()
    }

    pub(crate) fn kind(&self) -> PanelKind {
        self.kind
    }
}

impl De {
    /// Called by the event-loop timer, never performs external I/O here.
    pub(crate) fn poll_panels(&mut self) {
        let now = Instant::now();
        if self.hun_fx.tick(self.uptime_ms(now)).is_some() {
            self.needs_present = true;
        }
        let state = self.panels.controls.snapshot();
        let (generation, entries) = self.panels.server.snapshot();
        let notifs_changed = generation != self.panels.notification_generation;
        let media_gen = self.media.generation;
        let media_changed = media_gen != self.panels.media_generation;
        let changed = self.panels.control_state != state || notifs_changed || media_changed;
        if !changed {
            return;
        }
        self.panels.media_generation = media_gen;
        self.panels.on[3] = self.app_prefs.prefs().do_not_disturb;
        self.panels.available[3] = true;
        self.panels.on[5] = self.rotation.policy.auto;
        self.panels.available[5] = true;
        for (i, value) in [
            (0, state.wifi),
            (1, state.bluetooth),
            (2, state.airplane),
            (4, state.flashlight),
            (6, state.battery_saver),
        ] {
            self.panels.available[i] = value.is_some();
            self.panels.on[i] = value.unwrap_or(false);
        }
        // GNSS/location, hotspot, recording and NFC stay
        // unavailable until their own providers expose a working contract.
        let lock_b = self.panels.slider_drag == Some(0)
            || self.panels.slider_lock_until[0].is_some_and(|t| now < t);
        let lock_v = self.panels.slider_drag == Some(1)
            || self.panels.slider_lock_until[1].is_some_and(|t| now < t);
        if !lock_b {
            self.panels.brightness = state.brightness.unwrap_or(0.0);
        }
        if !lock_v {
            self.panels.volume = state.volume.unwrap_or(0.0);
        }
        self.panels.control_state = state;
        self.panels.notification_generation = generation;
        if notifs_changed {
            if let Some(top) = entries.first() {
                if top.id != self.last_hun_notif_id {
                    self.last_hun_notif_id = top.id;
                    self.hun_enqueue_entry(top, now);
                }
            }
            self.lock.stale = true;
        }
        self.panels.notifications = entries
            .into_iter()
            .map(|e| Notification {
                id: e.id,
                app: e.app,
                title: e.title,
                body: e.body,
                when: format!("{} min", e.created.elapsed().as_secs() / 60),
                icon: None,
            })
            .collect();
        self.panels.stale = [true; 2];
        self.panels.content_gen += 1;
        self.meta_cache = None;
        self.shell_dirty = true;
        self.needs_present = true;
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

    /// Dismiss a single notification by id from the D-Bus notification store
    /// and cached shade list (used by HUN horizontal swipe dismiss).
    pub(crate) fn dismiss_notification(&mut self, id: u32) {
        if id == 0 {
            return;
        }
        self.panels.server.dismiss(id);
        self.panels.notifications.retain(|n| n.id != id);
        self.panels.stale = [true; 2];
        self.panels.content_gen += 1;
        self.lock.stale = true;
        self.meta_cache = None;
        self.shell_dirty = true;
    }

    pub(crate) fn panel_touch_started(&mut self, pos: Point) {
        self.panels.start_x = pos.x;
        self.panels.start_y = pos.y;
        self.panels.slider_drag = None;
        if self.panels.pull.shown() && self.panels.kind == PanelKind::Quick {
            let size = Size::new(self.output.width as i32, self.output.height as i32);
            match panels::quick_hit(size, pos) {
                Some(QuickHit::Brightness(_)) => self.panels.slider_drag = Some(0),
                Some(QuickHit::Volume(_)) => self.panels.slider_drag = Some(1),
                _ => {}
            }
        }
    }

    /// Finger moved. `from_top` is true for a pull that started in the top
    /// strip; `dy` is relative to touch-down. Returns whether the panel took
    /// the movement (the caller then skips home paging and the rest).
    pub(crate) fn panel_progress(&mut self, from_top: bool, dx: f32, dy: f32) -> bool {
        if let Some(idx) = self.panels.slider_drag {
            let size = Size::new(self.output.width as i32, self.output.height as i32);
            let r = panels::quick_layout(size).sliders[idx];
            let x = (self.panels.start_x + dx.round() as i32).clamp(r.x, r.x + r.w);
            let f = ((x - r.x) as f32 / r.w.max(1) as f32).clamp(0.0, 1.0);
            let now = Instant::now();
            let send_now = self
                .panels
                .last_slider_req
                .is_none_or(|t| now.duration_since(t).as_millis() >= 60);
            match idx {
                0 if self.panels.control_state.brightness.is_some() => {
                    if (self.panels.brightness - f).abs() >= 0.005 {
                        self.panels.brightness = f;
                        self.panels.stale = [true; 2];
                        self.panels.content_gen += 1;
                    }
                    self.panels.slider_lock_until[0] =
                        Some(now + std::time::Duration::from_millis(600));
                    if send_now {
                        self.panels.last_slider_req = Some(now);
                        self.panels
                            .controls
                            .request(de_system::controls::Action::Brightness(f));
                    }
                }
                1 if self.panels.control_state.volume.is_some() => {
                    if (self.panels.volume - f).abs() >= 0.005 {
                        self.panels.volume = f;
                        self.panels.stale = [true; 2];
                        self.panels.content_gen += 1;
                    }
                    self.panels.slider_lock_until[1] =
                        Some(now + std::time::Duration::from_millis(600));
                    if send_now {
                        self.panels.last_slider_req = Some(now);
                        self.panels
                            .controls
                            .request(de_system::controls::Action::Volume(f));
                    }
                }
                _ => {}
            }
            self.panel_touch();
            return true;
        }
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
            // The capture is deferred to `panel_prepare` so the input path
            // never pays for it; the panel opens with the cached backdrop
            // and the live-scene blur swaps in a few frames later.
            self.panels.capture_pending = true;
            self.panels.pull.begin();
            self.panels.pull.drag(dy);
            self.panel_touch();
            return true;
        }
        false
    }

    pub(crate) fn panel_released(&mut self, vy: f32) {
        if let Some(idx) = self.panels.slider_drag.take() {
            let now = Instant::now();
            self.panels.slider_lock_until[idx] = Some(now + std::time::Duration::from_millis(600));
            self.panels.last_slider_req = Some(now);
            match idx {
                0 if self.panels.control_state.brightness.is_some() => {
                    self.panels
                        .controls
                        .request(de_system::controls::Action::Brightness(
                            self.panels.brightness,
                        ));
                }
                1 if self.panels.control_state.volume.is_some() => {
                    self.panels
                        .controls
                        .request(de_system::controls::Action::Volume(self.panels.volume));
                }
                _ => {}
            }
            self.panel_touch();
        }
        if self.panels.pull.dragging() {
            self.panels.pull.release(vy);
            self.panels.last_tick = None;
            self.panel_touch();
        }
    }

    pub(crate) fn panel_cancelled(&mut self) {
        self.panel_released(0.0);
    }

    pub(crate) fn panel_open(&mut self, kind: PanelKind) -> bool {
        if !self.panel_allowed() {
            return false;
        }
        self.panels.kind = kind;
        self.panels.capture_pending = true;
        self.panels.pull.open();
        self.panels.last_tick = None;
        self.panel_touch();
        true
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

    /// Route a tap on the shade media card to the player.
    fn media_card_tap(&mut self, size: Size, pos: Point) {
        let count = self.panels.notifications.len();
        let (Some(rect), Some(view)) = (
            panels::notif_layout(size, count, true).media,
            media_view(self.media.snap.as_ref(), None),
        ) else {
            return;
        };
        use de_system::mpris::MediaCommand;
        use de_ui::media_card::{hit_card, MediaCardHit};
        let cmd = match hit_card(rect, pos, &view.model) {
            Some(MediaCardHit::Previous) => MediaCommand::Previous,
            Some(MediaCardHit::PlayPause) => MediaCommand::PlayPause,
            Some(MediaCardHit::Next) => MediaCommand::Next,
            _ => return,
        };
        self.media.request(cmd);
    }

    /// A tap while a panel is up. Always consumed.
    pub(crate) fn panel_tap(&mut self, pos: Point) {
        let size = Size::new(self.output.width as i32, self.output.height as i32);
        if self.panels.pull.dragging() || !self.panels.pull.wants_open() {
            return;
        }
        match self.panels.kind {
            PanelKind::Notifications => {
                let has_media = self.media.snap.is_some();
                match panels::notif_hit(size, pos, self.panels.notifications.len(), has_media) {
                    Some(NotifHit::Media) => self.media_card_tap(size, pos),

                    Some(NotifHit::ClearAll) => {
                        self.panels.server.dismiss(0);
                        self.panels.notifications.clear();
                        self.panels.stale = [true; 2];
                        self.panels.content_gen += 1;
                    }
                    Some(NotifHit::Card(i)) => {
                        if let Some(n) = self.panels.notifications.get(i) {
                            self.panels.server.dismiss(n.id);
                        }
                        self.panels.notifications.remove(i);
                        self.panels.stale = [true; 2];
                        self.panels.content_gen += 1;
                    }
                    None => self.panel_close(),
                }
            }
            PanelKind::Quick => match panels::quick_hit(size, pos) {
                Some(QuickHit::Tile(i)) => {
                    use de_system::controls::Action;
                    if !self.panels.available.get(i).copied().unwrap_or(false) {
                        return;
                    }
                    let on = !self.panels.on[i];
                    let action = match i {
                        0 => Some(Action::Wifi(on)),
                        1 => Some(Action::Bluetooth(on)),
                        2 => Some(Action::Airplane(on)),
                        4 => Some(Action::Flashlight(on)),
                        6 => Some(Action::BatterySaver(on)),
                        5 => {
                            self.toggle_auto_rotate();
                            self.panels.on[i] = self.rotation.policy.auto;
                            self.panels.stale = [true; 2];
                            self.panels.content_gen += 1;
                            None
                        }
                        3 => {
                            if self.app_prefs.set_do_not_disturb(on) {
                                self.panels.on[i] = on;
                            }
                            self.panels.stale = [true; 2];
                            self.panels.content_gen += 1;
                            self.meta_cache = None;
                            self.shell_dirty = true;
                            None
                        }
                        _ => None,
                    };
                    if let Some(action) = action {
                        self.panels.controls.request(action);
                    }
                }
                Some(QuickHit::Brightness(f)) => {
                    if self.panels.control_state.brightness.is_some() {
                        self.panels
                            .controls
                            .request(de_system::controls::Action::Brightness(f));
                    }
                }
                Some(QuickHit::Volume(f)) => {
                    if self.panels.control_state.volume.is_some() {
                        self.panels
                            .controls
                            .request(de_system::controls::Action::Volume(f));
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

    /// Long-press on a Quick Settings tile (Wi-Fi / Bluetooth) opens Settings
    /// directly to that connectivity section.
    pub(crate) fn panel_long_press(&mut self, pos: de_api::Point) {
        if self.panels.kind != PanelKind::Quick {
            return;
        }
        let size = de_api::Size::new(self.output.width as i32, self.output.height as i32);
        let section = match de_ui::panels::quick_hit(size, pos) {
            Some(QuickHit::Tile(0)) => Some("wifi"),
            Some(QuickHit::Tile(1)) => Some("bluetooth"),
            _ => None,
        };
        if let Some(sec) = section {
            self.panel_close();
            let _ = std::process::Command::new("setsid")
                .args(["rabbiz-settings", "--open", sec])
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn();
            self.needs_present = true;
        }
    }

    fn quick_model(&self) -> QuickModel {
        let s = &self.strings;
        let tiles = TILES
            .iter()
            .enumerate()
            .map(|(i, d)| {
                let detail = if self.panels.on[i] {
                    match d.id {
                        "wifi" => self
                            .panels
                            .control_state
                            .wifi_ssid
                            .clone()
                            .unwrap_or_else(|| s.get("qs.on").to_owned()),
                        "bluetooth" => self
                            .panels
                            .control_state
                            .bt_device
                            .clone()
                            .unwrap_or_else(|| s.get("qs.on").to_owned()),
                        _ => s.get("qs.on").to_owned(),
                    }
                } else {
                    s.get("qs.off").to_owned()
                };
                QuickTile {
                    id: d.id,
                    label: s.get(d.label).to_owned(),
                    glyph: d.glyph,
                    on: self.panels.on[i],
                    available: self.panels.available[i],
                    detail,
                }
            })
            .collect();
        QuickModel {
            tiles,
            brightness: self.panels.brightness,
            volume: self.panels.volume,
            brightness_available: self.panels.control_state.brightness.is_some(),
            volume_available: self.panels.control_state.volume.is_some(),
            feedback: if self.panels.control_state.pending {
                Some(s.get("qs.working").to_owned())
            } else if self.panels.control_state.error.is_some() {
                Some(s.get("qs.failed").to_owned())
            } else {
                None
            },
        }
    }

    /// Render the panel image if the content changed (before the frame is built).

    /// Keep the panel images fresh. Renders happen here — before the frame
    /// is built — and are pre-rendered for both kinds at boot, so a pull
    /// only ever waits on the texture upload. The expensive part of a pull
    /// (scene capture + blur) is deferred off the input path: the capture
    /// runs in this call, the blur on a worker thread.
    pub(crate) fn panel_prepare(&mut self) {
        // The panel header shows the clock: re-render both kinds when the
        // minute rolls over (and drop the cached status so it is not reused).
        let minute = crate::frame::minute_of_day();
        if self.panels.clock_minute != minute {
            self.panels.clock_minute = minute;
            self.meta_cache = None;
            self.panels.stale = [true; 2];
            self.panels.content_gen += 1;
        }
        // The blur worker landed: swap in the live-scene backdrop.
        if let Some((id, rx)) = self.panels.blur_rx.take() {
            match rx.try_recv() {
                Ok(px) => {
                    self.panels.blur = Some((id, px));
                    self.panels.stale = [true; 2];
                    self.panels.content_gen += 1;
                    if self.panels.shown() {
                        self.needs_present = true;
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    self.panels.blur_rx = Some((id, rx));
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {}
            }
        }
        // Pull-begin asked for a fresh backdrop: on GL, blur on GPU in <1 ms
        // directly from the live scene; on Soft, composite + blur off-thread.
        if self.panels.capture_pending {
            self.panels.capture_pending = false;
            if self.panels.use_gpu_blur {
                self.panels.gpu_blur_dirty = true;
            } else if let Some(scene) = crate::composite::composite_output(self) {
                let wp_id = self.app_prefs.prefs().wallpaper_id.clone();
                let (tx, rx) = std::sync::mpsc::channel();
                std::thread::spawn(move || {
                    let _ = tx.send(de_paint::glass::blurred_backdrop(&scene));
                });
                self.panels.blur_rx = Some((wp_id, rx));
            }
        }
        // The raster worker landed: swap in the fresh panel image. If the
        // content moved on while it was rendering the kind stays stale and
        // re-queues below.
        if let Some((k, cgen, rx)) = self.panels.render_rx.take() {
            match rx.try_recv() {
                Ok(px) => {
                    self.panels.pixels[k] = Some(px);
                    self.panels.generation += 1;
                    if cgen == self.panels.content_gen {
                        self.panels.stale[k] = false;
                    }
                    if self.panels.shown() {
                        self.needs_present = true;
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    self.panels.render_rx = Some((k, cgen, rx));
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {}
            }
        }
        // One kind in flight at a time, the visible kind first — spreads
        // the boot pre-render and content updates across pumps.
        if self.panels.render_rx.is_some() {
            return;
        }
        let other = match self.panels.kind {
            PanelKind::Notifications => PanelKind::Quick,
            PanelKind::Quick => PanelKind::Notifications,
        };
        let kind = [self.panels.kind, other]
            .into_iter()
            .find(|k| self.panels.pixels[*k as usize].is_none() || self.panels.stale[*k as usize]);
        let Some(kind) = kind else {
            return;
        };
        let size = Size::new(self.output.width as i32, self.output.height as i32);
        let (status, date) = self.shell_meta();
        let strings = self.strings.clone();
        let notifications = self.panels.notifications.clone();
        let media = self.media.snap.clone();
        let media_art = self.media.art().cloned();
        let quick = matches!(kind, PanelKind::Quick).then(|| self.quick_model());
        let (tx, rx) = std::sync::mpsc::channel();
        let theme = self.theme;
        if self.panels.use_gpu_blur {
            std::thread::spawn(move || {
                let px = match quick {
                    Some(m) => panels::render_quick_controls(
                        size,
                        &strings,
                        &theme,
                        &status.clock,
                        &date,
                        &m,
                    ),
                    None => panels::render_notifications_controls(
                        size,
                        &strings,
                        &theme,
                        &status.clock,
                        &date,
                        &notifications,
                        media_view(media.as_ref(), media_art.as_ref()),

                    ),
                };
                let _ = tx.send(px);
            });
        } else {
            let wp_id = self.app_prefs.prefs().wallpaper_id.clone();
            if self.panels.blur.as_ref().map(|(id, _)| id.as_str()) != Some(wp_id.as_str()) {
                let wp = self.wallpaper_for(&wp_id, size);
                self.panels.blur = Some((wp_id, de_paint::glass::blurred_backdrop(&wp)));
            }
            let Some((_, backdrop)) = self.panels.blur.as_ref() else {
                return;
            };
            let backdrop = backdrop.clone();
            std::thread::spawn(move || {
                let px = match quick {
                    Some(m) => panels::render_quick(
                        size,
                        &strings,
                        &theme,
                        &backdrop,
                        &status.clock,
                        &date,
                        &m,
                    ),
                    None => panels::render_notifications(
                        size,
                        &strings,
                        &theme,
                        &backdrop,
                        &status.clock,
                        &date,
                        &notifications,
                        media_view(media.as_ref(), media_art.as_ref()),

                    ),
                };
                let _ = tx.send(px);
            });
        }
        self.panels.render_rx = Some((kind as usize, self.panels.content_gen, rx));
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

/// Panel media card data for a snapshot (borrowing it).
fn media_view<'a>(
    snap: Option<&'a de_system::mpris::MediaSnapshot>,
    art: Option<&'a de_api::Pixels>,
) -> Option<panels::MediaView<'a>> {
    snap.map(|s| panels::MediaView {
        model: de_ui::media_card::MediaCardModel {
            player_name: &s.player_name,
            title: &s.title,
            artist: &s.artist,
            playing: s.playing,
            can_go_previous: s.can_go_previous,
            can_go_next: s.can_go_next,
        },
        art,
    })
}

impl PanelUi {
    /// Panel content (notifications, media card) changed: re-raster.
    pub(crate) fn mark_content_stale(&mut self) {
        self.stale = [true; 2];
        self.content_gen += 1;
    }

    /// Output size changed (rotation): drop every cached raster/blur.
    pub(crate) fn on_resize(&mut self, output_h: i32) {
        self.pixels = [None, None];
        self.tex = None;
        self.tex_generation = u64::MAX;
        self.stale = [true; 2];
        self.blur = None;
        self.blur_rx = None;
        self.render_rx = None;
        self.capture_pending = false;
        self.gpu_blur = None;
        self.scrim_buf = None;
        self.content_gen += 1;
        self.generation += 1;
        self.pull.set_reach(de_ui::pull::reach_for(output_h));
    }
}
