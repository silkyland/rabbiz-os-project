//! Now-playing: MPRIS (or the mock track) -> status-bar pill texture and the
//! shade media card.
//!
//! A worker thread owns D-Bus; this controller only reads the latest snapshot
//! every tick, decodes artwork on change, and rasterizes the media surface
//! independently of the Home frame.

use std::any::Any;
use std::time::{Duration, Instant};

use de_api::{Pixels, Point, Size};
use de_system::mpris::{MediaCommand, MediaSnapshot, Mpris};

use crate::state::De;

/// Equalizer raster cadence while playing.
const PILL_STEP_MS: u64 = 100;

pub(crate) struct MediaFx {
    mpris: Mpris,
    pub(crate) snap: Option<MediaSnapshot>,
    /// Bumped whenever `snap` or the decoded art changes.
    pub(crate) generation: u64,
    art: Option<(String, Pixels)>,
    pub(crate) surface_px: Option<Pixels>,
    pub(crate) surface_tex: Option<Box<dyn Any>>,
    pub(crate) surface_generation: u64,
    pub(crate) surface_tex_generation: u64,
    surface_key: Option<(u64, u64, Size, u32)>,
    expanded: bool,
    open_t: f32,
    open_anim: Option<OpenAnimation>,
    /// Instant the current position baseline was sampled (extrapolation).
    pos_since: Instant,
}

impl MediaFx {
    pub(crate) fn new() -> Self {
        Self {
            mpris: Mpris::start(),
            snap: None,
            generation: 0,
            art: None,
            surface_px: None,
            surface_tex: None,
            surface_generation: 0,
            surface_tex_generation: u64::MAX,
            surface_key: None,
            expanded: false,
            open_t: 0.0,
            open_anim: None,
            pos_since: Instant::now(),
        }
    }

    pub(crate) fn art(&self) -> Option<&Pixels> {
        self.art.as_ref().map(|(_, p)| p)
    }

    /// Current source: the mock track (emulator) wins over the session bus.
    fn source(&self) -> Option<MediaSnapshot> {
        let m = de_system::mock::snapshot();
        if let Some(t) = m.media {
            return Some(MediaSnapshot {
                bus_name: "mock".into(),
                player_name: "Rabbiz Music".into(),
                title: t.title,
                artist: t.artist,
                playing: t.playing,
                position_ms: 19_000,
                duration_ms: 252_000,
                can_go_next: true,
                can_go_previous: true,
                art_path: None,
                desktop_entry: Some("rabbiz-music".into()),
            });
        }
        self.mpris.active()
    }

    pub(crate) fn request(&self, cmd: MediaCommand) {
        let Some(snap) = &self.snap else { return };
        if snap.bus_name == "mock" {
            if cmd == MediaCommand::PlayPause {
                de_system::mock::media_toggle_playing();
            }
        } else {
            self.mpris.request_for(&snap.bus_name, cmd);
        }
    }

    /// Output size changed: the pill raster depends on it.
    pub(crate) fn on_resize(&mut self) {
        self.surface_px = None;
        self.surface_tex = None;
        self.surface_key = None;
    }

    fn load_art(path: &str) -> Option<Pixels> {
        let bytes = std::fs::read(path).ok()?;
        let art = de_paint::image::decode_album_art(&bytes)?;
        // Crop and downscale once, so each morph frame samples a small square.
        Some(de_ui::media::rounded_art(&art, 128, 0.0))
    }
}

const OPEN_MS: f32 = 240.0;

struct OpenAnimation {
    start: Instant,
    from: f32,
    last_frame: Instant,
}

impl MediaFx {
    pub(crate) fn wake_at(&self, interval: Duration) -> Option<Instant> {
        self.open_anim
            .as_ref()
            .map(|anim| anim.last_frame + interval)
    }

    pub(crate) fn rect(&self, size: Size) -> de_api::Rect {
        de_ui::media::morph_rect(size, self.open_t)
    }

    fn set_expanded(&mut self, expanded: bool, now: Instant, animate: bool) {
        self.expanded = expanded;
        if animate {
            self.open_anim = Some(OpenAnimation {
                start: now,
                from: self.open_t,
                last_frame: now,
            });
        } else {
            self.open_t = if expanded { 1.0 } else { 0.0 };
            self.open_anim = None;
        }
    }

    fn advance(&mut self, now: Instant) {
        if let Some(anim) = &mut self.open_anim {
            anim.last_frame = now;
            let target: f32 = if self.expanded { 1.0 } else { 0.0 };
            let step = now.saturating_duration_since(anim.start).as_secs_f32() * 1000.0 / OPEN_MS;
            self.open_t = target.clamp(anim.from - step, anim.from + step);
            if self.open_t == target {
                self.open_anim = None;
            }
        }
    }
}

impl De {
    /// Media belongs to Home's status band, outside all application surfaces.
    pub(crate) fn media_on_home(&self) -> bool {
        self.foreground == crate::state::Foreground::Shell
            && self.nav.current == de_ui::nav::Screen::Home
            && self.morph.is_none()
            && !self.lock.shown()
            && !self.sleeping()
    }

    pub(crate) fn media_tap(&mut self, pos: Point) -> bool {
        let Some(snap) = &self.media.snap else {
            return false;
        };
        if !self.media_on_home() || self.media.surface_px.is_none() {
            return false;
        }
        let size = Size::new(self.output.width as i32, self.output.height as i32);
        let inside = self.media.rect(size).contains(pos);
        let now = Instant::now();
        let animate = !self.motion_clock.motion_restricted();
        if self.media.open_t == 1.0 && self.media.expanded && inside {
            let card = de_ui::media::card_rect(size);
            match de_ui::media::hit_card(card, pos) {
                Some(de_ui::media::CardHit::Previous) if snap.can_go_previous => {
                    self.media.request(MediaCommand::Previous);
                }
                Some(de_ui::media::CardHit::Next) if snap.can_go_next => {
                    self.media.request(MediaCommand::Next);
                }
                Some(de_ui::media::CardHit::PlayPause) => {
                    self.media.request(MediaCommand::PlayPause);
                }
                Some(_) => {}
                None => {
                    if let Some(id) = self.media_app_id() {
                        self.media.set_expanded(false, now, false);
                        self.try_launch(&id, animate.then_some(card));
                    } else {
                        self.media.request(MediaCommand::Raise);
                        self.media.set_expanded(false, now, animate);
                    }
                }
            }
        } else if self.media.expanded {
            self.media.set_expanded(false, now, animate);
        } else if inside {
            self.media.set_expanded(true, now, animate);
        } else {
            return false;
        }
        self.needs_present = true;
        true
    }

    fn media_app_id(&self) -> Option<String> {
        let snap = self.media.snap.as_ref()?;
        let from_bus = snap.bus_name.strip_prefix("org.mpris.MediaPlayer2.");
        let candidates = [
            snap.desktop_entry.as_deref(),
            from_bus,
            from_bus.and_then(|name| name.split_once('.').map(|(stem, _)| stem)),
        ];
        let windows = self.recent_windows();
        for candidate in candidates.into_iter().flatten() {
            let matches = |id: &str| {
                candidate
                    .trim_end_matches(".desktop")
                    .eq_ignore_ascii_case(id.trim_end_matches(".desktop"))
            };
            // A window can exist even when its desktop entry is absent.
            if let Some(id) = windows
                .iter()
                .filter_map(|win| win.toplevel().map(crate::snapshots::window_app_id))
                .find(|id| matches(id))
            {
                return Some(id);
            }
            if let Some(entry) = self.catalog.all().iter().find(|entry| matches(&entry.id)) {
                return Some(entry.id.clone());
            }
        }
        None
    }

    /// Refresh now-playing state at the polling cadence, independent of morph frames.
    pub(crate) fn media_tick(&mut self, now: Instant) {
        let src = self.media.source();
        if src != self.media.snap {
            let changed_player =
                src.as_ref().map(|s| &s.bus_name) != self.media.snap.as_ref().map(|s| &s.bus_name);
            if changed_player {
                self.media.set_expanded(false, now, false);
            }
            let art_path = src.as_ref().and_then(|s| s.art_path.clone());
            if art_path.as_ref() != self.media.art.as_ref().map(|(path, _)| path) {
                self.media.art = art_path.and_then(|p| MediaFx::load_art(&p).map(|px| (p, px)));
            }
            self.media.snap = src;
            self.media.generation += 1;
            self.media.pos_since = now;
            self.panels.mark_content_stale();
            self.needs_present = true;
        }
        self.needs_present |= self.media_frame(now);
    }

    pub(crate) fn media_frame(&mut self, now: Instant) -> bool {
        if !self.media_on_home() || self.media.snap.is_none() {
            self.media.set_expanded(false, now, false);
            if self.media.surface_px.take().is_some() {
                self.media.surface_tex = None;
                self.media.surface_key = None;
                return true;
            }
            return false;
        }
        self.media.advance(now);
        let Some(snap) = self.media.snap.as_ref() else {
            return false;
        };
        let size = Size::new(self.output.width as i32, self.output.height as i32);
        let open = self.media.open_t;
        let pos_ms = if snap.playing {
            snap.position_ms.saturating_add(
                now.saturating_duration_since(self.media.pos_since)
                    .as_millis() as u64,
            )
        } else {
            snap.position_ms
        };
        let position = if snap.duration_ms == 0 {
            pos_ms
        } else {
            pos_ms.min(snap.duration_ms)
        };
        let phase = if open == 1.0 {
            position / 500
        } else if snap.playing {
            self.uptime_ms(now) / PILL_STEP_MS
        } else {
            0
        };
        let key = (self.media.generation, phase, size, open.to_bits());
        if self.media.surface_key == Some(key) {
            return false;
        }
        let art = self.media.art();
        let rect = self.media.rect(size);
        let px = if open == 0.0 {
            let model = de_ui::media::PillModel {
                playing: snap.playing,
                art,
            };
            de_ui::media::render_pill(&self.theme, size, &model, phase * PILL_STEP_MS)
        } else {
            let model = de_ui::media::CardModel {
                title: &snap.title,
                artist: &snap.artist,
                playing: snap.playing,
                position_ms: position,
                duration_ms: snap.duration_ms,
                can_go_previous: snap.can_go_previous,
                can_go_next: snap.can_go_next,
            };
            if open == 1.0 {
                de_ui::media::render_card(&self.theme, rect, &model, art)
            } else {
                de_ui::media::render_morph(
                    &self.theme,
                    rect,
                    &model,
                    art,
                    open,
                    phase * PILL_STEP_MS,
                )
            }
        };
        self.media.surface_px = Some(px);
        self.media.surface_generation += 1;
        self.media.surface_key = Some(key);
        true
    }
}
