//! Screenshot feedback (P16): a 120 ms white flash (a solid element drawn
//! *after* the capture, so it never self-photographs) and a thumbnail toast
//! at the lower right for 3 s; tapping it opens the PNG.

use std::any::Any;
use std::process::Command;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use de_api::{Pixels, Point, Rect, Size};

use crate::state::De;

const FLASH_MS: u64 = 120;
const TOAST_MS: u64 = 3000;
const THUMB_W: u32 = 144;
const THUMB_H: u32 = 96;

pub(crate) struct ShotFx {
    flash_until: Option<Instant>,
    toast_until: Option<Instant>,
    path: String,
    name: String,
    thumb: Option<Pixels>,
    thumb_rx: Option<mpsc::Receiver<Pixels>>,
    pub px: Option<Pixels>,
    pub tex: Option<Box<dyn Any>>,
    /// Bumped on every re-raster — the frame loop uploads on change.
    pub generation: u64,
    pub tex_generation: u64,
    stale: bool,
}

impl Default for ShotFx {
    fn default() -> Self {
        Self {
            flash_until: None,
            toast_until: None,
            path: String::new(),
            name: String::new(),
            thumb: None,
            thumb_rx: None,
            px: None,
            tex: None,
            generation: 0,
            tex_generation: u64::MAX,
            stale: false,
        }
    }
}

impl ShotFx {
    /// A new screenshot landed at `path` (from `a21s-screenshot.state`).
    pub fn fire(&mut self, path: &str, now: Instant) {
        self.flash_until = Some(now + Duration::from_millis(FLASH_MS));
        self.toast_until = Some(now + Duration::from_millis(TOAST_MS));
        self.path = path.to_string();
        self.name = path.rsplit('/').next().unwrap_or(path).to_string();
        self.thumb = None;
        self.stale = true;
        let (tx, rx) = mpsc::channel();
        self.thumb_rx = Some(rx);
        let p = path.to_string();
        thread::spawn(move || {
            if let Ok(bytes) = std::fs::read(&p) {
                if let Ok(img) = de_paint::image::decode_png(&bytes) {
                    let thumb = de_paint::wallpaper::cover_scale(
                        &img,
                        de_api::Size::new(THUMB_W as i32, THUMB_H as i32),
                    );
                    let _ = tx.send(thumb);
                }
            }
        });
    }

    #[must_use]
    pub fn flash_active(&self, now: Instant) -> bool {
        self.flash_until.is_some_and(|u| now < u)
    }

    #[must_use]
    pub fn toast_active(&self, now: Instant) -> bool {
        self.toast_until.is_some_and(|u| now < u)
    }

    /// Next deadline worth waking the frame loop for.
    #[must_use]
    pub fn wake_at(&self, now: Instant) -> Option<Instant> {
        [self.flash_until, self.toast_until]
            .into_iter()
            .flatten()
            .filter(|u| *u > now)
            .min()
    }

    /// Pick up a decoded thumbnail / expire the toast. Returns true when a
    /// repaint is needed.
    pub fn poll(&mut self, now: Instant) -> bool {
        let mut dirty = false;
        if let Some(rx) = &self.thumb_rx {
            if let Ok(thumb) = rx.try_recv() {
                self.thumb = Some(thumb);
                self.thumb_rx = None;
                self.stale = true;
                dirty = dirty || self.toast_active(now);
            }
        }
        if self.flash_until.is_some_and(|u| now >= u) {
            self.flash_until = None;
            dirty = true;
        }
        if self.toast_until.is_some_and(|u| now >= u) {
            self.toast_until = None;
            self.px = None;
            self.tex = None;
            dirty = true;
        }
        dirty
    }

    /// Toast tap → open the PNG in the default viewer.
    pub fn tap(&mut self, pos: Point, size: Size, now: Instant) -> bool {
        if !self.toast_active(now) || !de_ui::screenshot::toast_rect(size).contains(pos) {
            return false;
        }
        let _ = Command::new("setsid")
            .args(["-f", "xdg-open", &self.path])
            .spawn();
        self.toast_until = Some(now); // dismiss
        true
    }

    /// Toast rect helper for the frame loop.
    #[must_use]
    pub fn toast_rect(size: Size) -> Rect {
        de_ui::screenshot::toast_rect(size)
    }
}

impl De {
    /// Rasterize the toast card while it is up (deferred — the thumbnail
    /// may arrive a frame after the flash).
    pub(crate) fn shot_prepare(&mut self, now: Instant) {
        if !self.shot_fx.toast_active(now) || (!self.shot_fx.stale && self.shot_fx.px.is_some()) {
            return;
        }
        let px = de_ui::screenshot::toast(
            &self.theme,
            self.shot_fx.thumb.as_ref(),
            &self.shot_fx.name,
            &self.strings,
        );
        self.shot_fx.px = Some(px);
        self.shot_fx.generation += 1;
        self.shot_fx.stale = false;
    }
}

impl ShotFx {
    /// Output size changed (rotation): drop the raster and re-render.
    pub(crate) fn on_resize(&mut self) {
        self.px = None;
        self.tex = None;
        self.stale = true;
        self.generation = self.generation.wrapping_add(1);
    }
}
