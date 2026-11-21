//! Now-playing: MPRIS (or the mock track) -> status-bar pill texture and the
//! shade media card.
//!
//! A worker thread owns D-Bus; this controller only reads the latest snapshot
//! every tick, decodes artwork on change, and re-rasters the small pill
//! texture while it animates (never the whole shell frame).

use std::any::Any;
use std::time::Instant;

use de_api::{Pixels, Point, Size};
use de_system::mpris::{MediaCommand, MediaSnapshot, Mpris};

use crate::state::De;

/// Pill raster cadence while playing (marquee + equalizer).
const PILL_STEP_MS: u64 = 100;

pub(crate) struct MediaFx {
    mpris: Mpris,
    pub(crate) snap: Option<MediaSnapshot>,
    /// Bumped whenever `snap` or the decoded art changes.
    pub(crate) generation: u64,
    art: Option<(String, Pixels)>,
    pub(crate) pill_px: Option<Pixels>,
    pub(crate) pill_tex: Option<Box<dyn Any>>,
    pub(crate) pill_generation: u64,
    pub(crate) pill_tex_generation: u64,
    pill_key: Option<(u64, u64, Size)>,
    /// Expanded-island target (tap on the pill opens the player card).
    pub(crate) expanded: bool,
    /// Morph progress 0..=1 eased from `open_anim_start`.
    pub(crate) open_t: f32,
    open_anim_start: Option<Instant>,
    /// Instant the current position baseline was sampled (extrapolation).
    pos_since: Instant,
    pub(crate) card_px: Option<Pixels>,
    pub(crate) card_tex: Option<Box<dyn Any>>,
    pub(crate) card_generation: u64,
    pub(crate) card_tex_generation: u64,
    card_key: Option<(u64, u64)>,
}

impl MediaFx {
    pub(crate) fn new() -> Self {
        Self {
            mpris: Mpris::start(),
            snap: None,
            generation: 0,
            art: None,
            pill_px: None,
            pill_tex: None,
            pill_generation: 0,
            pill_tex_generation: u64::MAX,
            pill_key: None,
            expanded: false,
            open_t: 0.0,
            open_anim_start: None,
            pos_since: Instant::now(),
            card_px: None,
            card_tex: None,
            card_generation: 0,
            card_tex_generation: u64::MAX,
            card_key: None,
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
                desktop_entry: None,
            });
        }
        self.mpris.active()
    }

    pub(crate) fn request(&self, cmd: MediaCommand) {
        if de_system::mock::snapshot().media.is_some() {
            if cmd == MediaCommand::PlayPause {
                de_system::mock::media_toggle_playing();
            }
        } else {
            self.mpris.request(cmd);
        }
    }

    /// Output size changed: the pill raster depends on it.
    pub(crate) fn on_resize(&mut self) {
        self.pill_px = None;
        self.pill_tex = None;
        self.pill_key = None;
        self.card_px = None;
        self.card_tex = None;
        self.card_key = None;
    }

    fn load_art(path: &str) -> Option<Pixels> {
        let bytes = std::fs::read(path).ok()?;
        de_paint::image::decode_png(&bytes).ok()
    }
}

impl MediaFx {
}/// Island morph duration (ms).
const OPEN_MS: u64 = 240;

impl De {
    /// Advance the pill→card morph; `open_t` drives both raster + draw.
    fn anim_open(&mut self, now: Instant) {
        if let Some(start) = self.media.open_anim_start {
            let t = (now.duration_since(start).as_millis() as u64).min(OPEN_MS) as f32
                / OPEN_MS as f32;
            self.media.open_t = if self.media.expanded { t } else { 1.0 - t };
            if t >= 1.0 {
                self.media.open_anim_start = None;
            }
            self.needs_present = true;
        }
    }

    /// Tap on the island: expand the pill, hit transport, or collapse on an
    /// outside tap. Returns true when consumed (shell surfaces only).
    pub(crate) fn media_tap(&mut self, pos: Point) -> bool {
        let size = Size::new(self.output.width as i32, self.output.height as i32);
        let now = Instant::now();
        if self.media.expanded || self.media.open_anim_start.is_some() {
            let card = de_ui::media::card_rect(size);
            if card.contains(pos) {
                match de_ui::media::hit_card(card, pos) {
                    Some(de_ui::media::CardHit::Previous) => {
                        self.media.request(MediaCommand::Previous);
                        self.media.card_key = None;
                    }
                    Some(de_ui::media::CardHit::PlayPause) => {
                        self.media.request(MediaCommand::PlayPause);
                        self.media.card_key = None;
                    }
                    Some(de_ui::media::CardHit::Next) => {
                        self.media.request(MediaCommand::Next);
                        self.media.card_key = None;
                    }
                    None => {
                        // Card body: hand off to the player app (collapse when
                        // the player can't be mapped to a desktop entry).
                        if let Some(id) = self.media_app_id() {
                            self.media.expanded = false;
                            self.media.open_anim_start = None;
                            self.media.open_t = 0.0;
                            self.needs_present = true;
                            self.try_launch(&id, Some(card));
                            return true;
                        }
                        self.media.expanded = false;
                        self.media.open_anim_start = Some(now);
                    }
                }
            } else {
                self.media.expanded = false;
                self.media.open_anim_start = Some(now);
            }
            self.needs_present = true;
            return true;
        }
        let pw = self
            .media
            .pill_px
            .as_ref()
            .map(|p| p.size.w)
            .unwrap_or(de_ui::media::PILL_W);
        if self.media.snap.is_some() && de_ui::media::pill_rect(size, pw).contains(pos) {
            self.media.expanded = true;
            self.media.open_anim_start = Some(now);
            self.media.card_key = None;
            self.media.pos_since = now;
            self.needs_present = true;
            return true;
        }
        false
    }

    /// Best-effort map from the active MPRIS player to a catalog desktop id.
    fn media_app_id(&self) -> Option<String> {
        let snap = self.media.snap.as_ref()?;
        if let Some(id) = snap.desktop_entry.as_deref() {
            if self.catalog.find(id).is_some() {
                return Some(id.to_string());
            }
        }
        let suffix = snap
            .bus_name
            .trim_start_matches("org.mpris.MediaPlayer2.")
            .split('.')
            .next()?
            .to_lowercase();
        if suffix.is_empty() {
            return None;
        }
        if self.catalog.find(&suffix).is_some() {
            return Some(suffix);
        }
        self.catalog
            .all()
            .iter()
            .find(|e| e.id.to_lowercase().contains(&suffix))
            .map(|e| e.id.clone())
    }

    /// Refresh now-playing state and the pill raster (~10 Hz timer).
    pub(crate) fn media_tick(&mut self, now: Instant) {
        let src = self.media.source();
        if src != self.media.snap {
            let art_path = src.as_ref().and_then(|s| s.art_path.clone());
            let have = self.media.art.as_ref().map(|(p, _)| p.clone());
            if art_path != have {
                self.media.art = art_path
                    .and_then(|p| MediaFx::load_art(&p).map(|px| (p, px)));
            }
            self.media.snap = src;
            self.media.generation += 1;
            self.media.pos_since = Instant::now();
            // The shade bakes the card into its raster.
            self.panels.mark_content_stale();
            self.needs_present = true;
        }
        let Some(snap) = self.media.snap.clone().filter(|_| !self.sleeping()) else {
            self.media.expanded = false;
            if self.media.pill_px.take().is_some() || self.media.card_px.take().is_some() {
                self.media.pill_tex = None;
                self.media.pill_key = None;
                self.media.card_tex = None;
                self.media.card_key = None;
                self.needs_present = true;
            }
            self.anim_open(now);
            return;
        };
        self.anim_open(now);
        // Extrapolated track position between player polls.
        let pos_ms = if snap.playing {
            snap.position_ms
                .saturating_add(now.duration_since(self.media.pos_since).as_millis() as u64)
        } else {
            snap.position_ms
        };
        if self.media.expanded || self.media.open_t > 0.0 {
            let bucket = if snap.playing { pos_ms / 500 } else { u64::MAX };
            if self.media.card_key != Some((self.media.generation, bucket)) {
                let size = Size::new(self.output.width as i32, self.output.height as i32);
                let r = de_ui::media::card_rect(size);
                let model = de_ui::media::CardModel {
                    title: &snap.title,
                    artist: &snap.artist,
                    playing: snap.playing,
                    position_ms: pos_ms.min(snap.duration_ms.max(pos_ms)),
                    duration_ms: snap.duration_ms,
                    can_go_previous: snap.can_go_previous,
                    can_go_next: snap.can_go_next,
                };
                self.media.card_px =
                    Some(de_ui::media::render_card(&self.theme, r, &model, self.media.art()));
                self.media.card_generation += 1;
                self.media.card_key = Some((self.media.generation, bucket));
                self.needs_present = true;
            } else if snap.playing {
                // Progress bucket rolls every 500 ms — keep frames coming.
                self.needs_present = true;
            }
        }
        let size = Size::new(self.output.width as i32, self.output.height as i32);
        let t = self.uptime_ms(now);
        let phase = if snap.playing { t / PILL_STEP_MS } else { 0 };
        let key = (self.media.generation, phase, size);
        if self.media.pill_key == Some(key) {
            return;
        }
        let model = de_ui::media::PillModel {
            playing: snap.playing,
            art: self.media.art(),
        };
        let px = de_ui::media::render_pill(&self.theme, size, &model, phase * PILL_STEP_MS);
        self.media.pill_px = Some(px);
        self.media.pill_generation += 1;
        self.media.pill_key = Some(key);
        self.needs_present = true;
    }
}
