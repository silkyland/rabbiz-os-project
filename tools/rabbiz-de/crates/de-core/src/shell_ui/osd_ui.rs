//! Volume / brightness on-screen display pill (`ShowOsd` IPC, P10/P11):
//! transient left-edge overlay drawn in `DisplayArea::Overlay` for 1.8 s.

use std::any::Any;
use std::time::{Duration, Instant};

use de_api::{Pixels, Rect, Size};
use de_config::ipc::OsdKind;

use crate::state::De;

const OSD_MS: u64 = 1800;
const ENTER_MS: u64 = 160;
const EXIT_MS: u64 = 180;
const SLIDE_IN_PX: f32 = -44.0;
const SLIDE_OUT_PX: f32 = -32.0;

pub(crate) struct OsdFx {
    shown_at: Option<Instant>,
    until: Option<Instant>,
    settled: bool,
    kind: OsdKind,
    percent: u8,
    muted: bool,
    pub px: Option<Pixels>,
    pub tex: Option<Box<dyn Any>>,
    /// Bumped on every re-raster — the frame loop uploads on change.
    pub generation: u64,
    pub tex_generation: u64,
    stale: bool,
}

impl Default for OsdFx {
    fn default() -> Self {
        Self {
            shown_at: None,
            until: None,
            settled: false,
            kind: OsdKind::Volume,
            percent: 0,
            muted: false,
            px: None,
            tex: None,
            generation: 0,
            tex_generation: u64::MAX,
            stale: false,
        }
    }
}

impl OsdFx {
    /// Display or update the OSD pill for `OSD_MS` milliseconds.
    ///
    /// Re-triggering while already visible extends the deadline without
    /// restarting the enter slide; re-triggering during exit smoothly reverses
    /// back in. The pill bitmap is re-rasterized only when content changes.
    pub fn show(&mut self, kind: OsdKind, percent: u8, muted: bool, now: Instant) {
        let clamped = percent.min(100);
        let was_active = self.active(now);
        if !was_active {
            self.shown_at = Some(now);
            self.settled = false;
        } else if let Some(exit_start) = self.exit_start() {
            if now >= exit_start {
                // Smoothly reverse from the current fade-out alpha back into enter.
                let (_, alpha) = self.transform(now);
                let elapsed_ms = ((1.0 - (1.0 - alpha).cbrt()) * ENTER_MS as f32)
                    .clamp(0.0, ENTER_MS as f32) as u64;
                self.shown_at = Some(now - Duration::from_millis(elapsed_ms));
                self.settled = false;
            }
        }
        let content_changed =
            !was_active || self.kind != kind || self.percent != clamped || self.muted != muted;
        self.until = Some(now + Duration::from_millis(OSD_MS));
        self.kind = kind;
        self.percent = clamped;
        self.muted = muted;
        if content_changed {
            self.stale = true;
        }
    }

    fn enter_end(&self) -> Option<Instant> {
        self.shown_at.map(|s| s + Duration::from_millis(ENTER_MS))
    }

    fn exit_start(&self) -> Option<Instant> {
        self.until.map(|u| u - Duration::from_millis(EXIT_MS))
    }

    /// True while the pill is actively sliding/fading in or out (GPU transform
    /// only — no CPU re-rasterization).
    #[must_use]
    pub fn animating(&self, now: Instant) -> bool {
        if !self.active(now) {
            return false;
        }
        let entering = self.enter_end().is_some_and(|e| now < e);
        let exiting = self.exit_start().is_some_and(|s| now >= s);
        entering || exiting
    }

    /// `(offset_x_px, alpha)` for GPU/compositor placement at `now`.
    #[must_use]
    pub fn transform(&self, now: Instant) -> (f32, f32) {
        if !self.active(now) {
            return (0.0, 0.0);
        }
        if let (Some(shown), Some(enter_end)) = (self.shown_at, self.enter_end()) {
            if now < enter_end {
                let p = (now.saturating_duration_since(shown).as_secs_f32() * 1000.0
                    / ENTER_MS as f32)
                    .clamp(0.0, 1.0);
                let eased = 1.0 - (1.0 - p).powi(3);
                return (SLIDE_IN_PX * (1.0 - eased), eased);
            }
        }
        if let Some(exit_start) = self.exit_start() {
            if now >= exit_start {
                let p = (now.saturating_duration_since(exit_start).as_secs_f32() * 1000.0
                    / EXIT_MS as f32)
                    .clamp(0.0, 1.0);
                let eased = 1.0 - p.powi(3);
                return (SLIDE_OUT_PX * (1.0 - eased), eased);
            }
        }
        (0.0, 1.0)
    }

    /// Force a re-raster on the next frame if currently visible (e.g. theme reload).
    pub fn mark_stale(&mut self) {
        if self.until.is_some() {
            self.stale = true;
        }
    }

    #[must_use]
    pub fn active(&self, now: Instant) -> bool {
        self.until.is_some_and(|u| now < u)
    }

    /// Next deadline worth waking the frame loop for: paced at `interval`
    /// during enter/exit, zero wakeups during steady hold until fade-out.
    #[must_use]
    pub fn wake_at(&self, now: Instant, interval: Duration) -> Option<Instant> {
        if !self.active(now) {
            return None;
        }
        if self.animating(now) || !self.settled {
            return Some(now + interval);
        }
        self.exit_start().filter(|s| *s > now)
    }

    /// Advance animation / expire the OSD pill once its deadline passes.
    /// Returns true when a compositor repaint is needed.
    pub fn poll(&mut self, now: Instant) -> bool {
        if self.until.is_some_and(|u| now >= u) {
            self.shown_at = None;
            self.until = None;
            self.settled = false;
            self.px = None;
            self.tex = None;
            self.stale = false;
            return true;
        }
        if !self.active(now) {
            return false;
        }
        if self.animating(now) {
            self.settled = false;
            return true;
        }
        if !self.settled {
            self.settled = true;
            return true;
        }
        false
    }

    #[must_use]
    pub fn glyph(&self) -> &'static str {
        match self.kind {
            OsdKind::Volume if self.muted || self.percent == 0 => "\u{e04f}",
            OsdKind::Volume => "\u{e050}",
            OsdKind::Brightness => "\u{e1ac}",
        }
    }

    #[must_use]
    pub fn rect(size: Size) -> Rect {
        de_ui::osd::pill_rect(size)
    }
}

impl De {
    /// Rasterize the OSD pill while it is up.
    pub(crate) fn osd_prepare(&mut self, now: Instant) {
        if !self.osd_fx.active(now) || (!self.osd_fx.stale && self.osd_fx.px.is_some()) {
            return;
        }
        let px = de_ui::osd::pill(
            &self.theme,
            f32::from(self.osd_fx.percent) / 100.0,
            self.osd_fx.glyph(),
            self.osd_fx.muted,
        );
        self.osd_fx.px = Some(px);
        self.osd_fx.generation += 1;
        self.osd_fx.stale = false;
    }
}

impl OsdFx {
    /// Output size changed (rotation): drop the raster and re-render.
    pub(crate) fn on_resize(&mut self) {
        self.px = None;
        self.tex = None;
        self.stale = true;
        self.generation = self.generation.wrapping_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn show_clamps_percent_and_expires() {
        let mut osd = OsdFx::default();
        let t0 = Instant::now();
        let frame = Duration::from_millis(16);
        assert!(!osd.active(t0));
        assert!(osd.wake_at(t0, frame).is_none());

        osd.show(OsdKind::Volume, 150, false, t0);
        assert!(osd.active(t0));
        assert_eq!(osd.percent, 100);
        assert_eq!(osd.glyph(), "\u{e050}");
        // Enter animation: slides in from left and paces at frame interval.
        assert!(osd.animating(t0));
        let (dx0, a0) = osd.transform(t0);
        assert!(dx0 < -30.0 && a0 < 0.05);
        assert_eq!(osd.wake_at(t0, frame), Some(t0 + frame));

        // Steady hold: settles once, then zero repaints until fade-out starts.
        let mid = t0 + Duration::from_millis(500);
        assert!(!osd.animating(mid));
        assert_eq!(osd.transform(mid), (0.0, 1.0));
        assert!(osd.poll(mid)); // final settled frame
        assert!(!osd.poll(mid + Duration::from_millis(100))); // idle during hold
        assert_eq!(
            osd.wake_at(mid, frame),
            Some(t0 + Duration::from_millis(OSD_MS - EXIT_MS))
        );

        // Exit animation: fades & slides out.
        let exiting = t0 + Duration::from_millis(OSD_MS - EXIT_MS / 2);
        assert!(osd.animating(exiting));
        let (dx_exit, a_exit) = osd.transform(exiting);
        assert!(dx_exit < 0.0 && a_exit > 0.0 && a_exit < 1.0);
        assert!(osd.poll(exiting));

        let after = t0 + Duration::from_millis(OSD_MS);
        assert!(!osd.active(after));
        assert!(osd.poll(after));
        assert!(osd.wake_at(after, frame).is_none());
        assert!(!osd.poll(after));
    }

    #[test]
    fn glyphs_reflect_kind_and_mute_state() {
        let mut osd = OsdFx::default();
        let t0 = Instant::now();

        osd.show(OsdKind::Volume, 60, true, t0);
        assert_eq!(osd.glyph(), "\u{e04f}");

        osd.show(OsdKind::Volume, 0, false, t0);
        assert_eq!(osd.glyph(), "\u{e04f}");

        osd.show(OsdKind::Brightness, 40, false, t0);
        assert_eq!(osd.glyph(), "\u{e1ac}");
    }
}
