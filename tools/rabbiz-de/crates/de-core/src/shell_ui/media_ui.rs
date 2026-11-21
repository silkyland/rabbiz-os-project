//! "Now playing" status pill compositor overlay: tracks the active MPRIS
//! player via `de_system::mpris::Mpris` and re-rasters the small
//! `de_ui::media` capsule for `DisplayArea::SystemBars` — visible centred in
//! the status-bar band on the shell and floating above apps.
//!
//! The marquee is a pure function of the compositor clock: while the active
//! player is playing the pill re-rasters per ~50 ms step; paused or stopped
//! it rasterizes once and stays still.

use std::any::Any;

use de_api::{Pixels, Rect};
use de_system::mpris::{MediaSnapshot, Mpris};

use crate::state::De;

/// Marquee re-raster cadence (20 fps — ~2 px steps at the 45 px/s speed).
const MARQUEE_STEP_MS: u64 = 50;

/// Compositor-side media pill state, mirroring [`super::osd_ui::OsdFx`].
pub(crate) struct MediaFx {
    /// Session-bus MPRIS watcher (worker thread; never polled with I/O here).
    pub(crate) mpris: Mpris,
    /// Active player snapshot mirrored from the tracker for shade/hit use.
    pub(crate) snap: Option<MediaSnapshot>,
    /// Rasterized album art for `snap.art_path` (PNG decode), else `None`.
    art: Option<Pixels>,
    pub px: Option<Pixels>,
    pub tex: Option<Box<dyn Any>>,
    /// Bumped on every re-raster — the frame loop uploads on change.
    pub generation: u64,
    pub tex_generation: u64,
    tracker_gen: u64,
    rastered: bool,
    marquee_ms: u64,
}

impl MediaFx {
    pub(crate) fn new() -> Self {
        Self {
            mpris: Mpris::start(),
            snap: None,
            art: None,
            px: None,
            tex: None,
            generation: 0,
            tex_generation: u64::MAX,
            tracker_gen: u64::MAX,
            rastered: false,
            marquee_ms: 0,
        }
    }

    /// Screen-space rect of the pill on an output `w` wide, if visible.
    #[must_use]
    pub fn rect(&self, output_w: i32) -> Option<Rect> {
        let px = self.px.as_ref()?;
        Some(Rect::new(
            (output_w - px.size.w) / 2,
            de_ui::media::PILL_Y,
            px.size.w,
            px.size.h,
        ))
    }
}

/// Read + decode the validated local art path (`de_paint` rasterizes PNG).
fn load_art(path: &str) -> Option<Pixels> {
    let bytes = std::fs::read(path).ok()?;
    de_paint::image::decode_png(&bytes).ok()
}

impl De {
    /// Refresh the pill raster: mirrors the tracker snapshot on generation
    /// change, then re-rasters when the marquee step advances. Returns
    /// whether `media_fx` produced new pixels (caller sets `needs_present`).
    pub(crate) fn media_prepare(&mut self, now_ms: u64, output_w: i32) -> bool {
        let fx = &mut self.media_fx;
        let gen = fx.mpris.generation();
        if gen != fx.tracker_gen {
            fx.tracker_gen = gen;
            fx.snap = fx.mpris.active();
            fx.art = fx
                .snap
                .as_ref()
                .and_then(|s| s.art_path.as_deref())
                .and_then(load_art);
            fx.rastered = false;
        }
        if fx.snap.is_none() {
            if fx.px.is_some() || fx.rastered {
                fx.px = None;
                fx.tex = None;
                fx.rastered = false;
                fx.generation = fx.generation.wrapping_add(1);
                return true;
            }
            return false;
        }
        let snap = fx.snap.as_ref().expect("snap checked above");
        let step = now_ms / MARQUEE_STEP_MS * MARQUEE_STEP_MS;
        if fx.rastered && !(snap.playing && step != fx.marquee_ms) {
            return false;
        }
        let label = if snap.artist.is_empty() {
            snap.title.clone()
        } else if snap.title.is_empty() {
            snap.artist.clone()
        } else {
            format!("{} — {}", snap.title, snap.artist)
        };
        if label.is_empty() {
            if fx.px.is_some() {
                fx.px = None;
                fx.tex = None;
                fx.rastered = false;
                fx.generation = fx.generation.wrapping_add(1);
                return true;
            }
            return false;
        }
        let model = de_ui::media::MediaPillModel {
            label: &label,
            playing: snap.playing,
            art: fx.art.as_ref(),
            scroll_ms: step,
        };
        let Some(px) = de_ui::media::render_pill(&model, output_w) else {
            return false;
        };
        fx.marquee_ms = step;
        fx.px = Some(px);
        fx.rastered = true;
        fx.generation = fx.generation.wrapping_add(1);
        true
    }
}
