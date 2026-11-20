//! Volume / brightness on-screen display pill (`ShowOsd` IPC, P10/P11):
//! transient left-edge overlay drawn in `DisplayArea::Overlay` for 1.8 s.

use std::any::Any;
use std::time::{Duration, Instant};

use de_api::{Pixels, Rect, Size};
use de_config::ipc::OsdKind;

use crate::state::De;

const OSD_MS: u64 = 1800;

pub(crate) struct OsdFx {
    until: Option<Instant>,
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
            until: None,
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
    pub fn show(&mut self, kind: OsdKind, percent: u8, muted: bool, now: Instant) {
        self.until = Some(now + Duration::from_millis(OSD_MS));
        self.kind = kind;
        self.percent = percent.min(100);
        self.muted = muted;
        self.stale = true;
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

    /// Next deadline worth waking the frame loop for.
    #[must_use]
    pub fn wake_at(&self, now: Instant) -> Option<Instant> {
        self.until.filter(|u| *u > now)
    }

    /// Expire the OSD pill once its deadline passes. Returns true when a
    /// repaint is needed.
    pub fn poll(&mut self, now: Instant) -> bool {
        if self.until.is_some_and(|u| now >= u) {
            self.until = None;
            self.px = None;
            self.tex = None;
            self.stale = false;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn show_clamps_percent_and_expires() {
        let mut osd = OsdFx::default();
        let t0 = Instant::now();
        assert!(!osd.active(t0));
        assert!(osd.wake_at(t0).is_none());

        osd.show(OsdKind::Volume, 150, false, t0);
        assert!(osd.active(t0));
        assert_eq!(osd.percent, 100);
        assert_eq!(osd.glyph(), "\u{e050}");
        assert!(osd.wake_at(t0).is_some());
        assert!(!osd.poll(t0 + Duration::from_millis(500)));

        let after = t0 + Duration::from_millis(OSD_MS);
        assert!(!osd.active(after));
        assert!(osd.poll(after));
        assert!(osd.wake_at(after).is_none());
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
