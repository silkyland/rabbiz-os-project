//! Now-playing: MPRIS (or the mock track) -> status-bar pill texture and the
//! shade media card.
//!
//! A worker thread owns D-Bus; this controller only reads the latest snapshot
//! every tick, decodes artwork on change, and re-rasters the small pill
//! texture while it animates (never the whole shell frame).

use std::any::Any;
use std::time::Instant;

use de_api::{Pixels, Size};
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
                can_go_next: true,
                can_go_previous: true,
                art_path: None,
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
    }

    fn load_art(path: &str) -> Option<Pixels> {
        let bytes = std::fs::read(path).ok()?;
        de_paint::image::decode_png(&bytes).ok()
    }
}

impl De {
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
            // The shade bakes the card into its raster.
            self.panels.mark_content_stale();
            self.needs_present = true;
        }
        let Some(snap) = self.media.snap.as_ref().filter(|_| !self.sleeping()) else {
            if self.media.pill_px.take().is_some() {
                self.media.pill_tex = None;
                self.media.pill_key = None;
                self.needs_present = true;
            }
            return;
        };
        let size = Size::new(self.output.width as i32, self.output.height as i32);
        let t = self.uptime_ms(now);
        let phase = if snap.playing { t / PILL_STEP_MS } else { 0 };
        let key = (self.media.generation, phase, size);
        if self.media.pill_key == Some(key) {
            return;
        }
        let model = de_ui::media::PillModel {
            title: &snap.title,
            artist: &snap.artist,
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
