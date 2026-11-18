//! Lock overlay state + raster (M4-T4): the whole surface slides up 1:1
//! with the finger and fades as `slide/UNLOCK_PX` grows — the QML
//! `unlockProgress` contract, now in Rust. Release past the threshold (or
//! with a strong upward flick) slides the surface off and drops the lock;
//! anything else springs it back.

use std::any::Any;
use std::time::Instant;

use de_api::{Pixels, Point, Size};
use de_design::motion::{MotionKind, SpringPreset};
use de_paint::animation::{AnimationClock, SpringValue};
use de_ui::lock::{LockModel, QuickAction};
use de_ui::nav::Screen;

use crate::state::De;

/// Drag distance that counts as a full unlock (matches the old QML value).
const UNLOCK_PX: f32 = 280.0;
/// Upward release velocity that unlocks regardless of distance (px/s).
const FLICK_VY: f32 = -600.0;
/// Fingerprint-fail shake duration.
const SHAKE_MS: f32 = 600.0;

pub(crate) struct LockUi {
    /// The lock owns the screen (covering, mid-drag or mid-unlock).
    locked: bool,
    /// Px the surface has slid up; 0 = fully covering.
    slide: SpringValue,
    dragging: bool,
    drag_origin: f32,
    pub px: Option<Pixels>,
    pub tex: Option<Box<dyn Any>>,
    /// Bumped on every re-raster — the frame loop uploads on change.
    pub generation: u64,
    pub tex_generation: u64,
    /// Last frame the slide spring advanced (dt source).
    last_anim: Option<Instant>,
    pub(crate) stale: bool,
    /// Clock string baked into the current raster — re-raster on change.
    baked_clock: String,
    /// fp-fail shake window.
    shake_since: Option<Instant>,
    /// Blurred wallpaper backdrop: `(wallpaper_id, size, pixels)`.
    backdrop: Option<(String, Size, Pixels)>,
    /// desktop-id behind the bottom-right action (camera or quick note).
    pub quick_id: Option<String>,
}

impl Default for LockUi {
    fn default() -> Self {
        Self {
            locked: false,
            slide: SpringValue::new(0.0, SpringPreset::Smooth, MotionKind::Spatial),
            dragging: false,
            drag_origin: 0.0,
            px: None,
            tex: None,
            generation: 0,
            tex_generation: u64::MAX,
            last_anim: None,
            stale: true,
            baked_clock: String::new(),
            shake_since: None,
            backdrop: None,
            quick_id: None,
        }
    }
}

impl LockUi {
    #[must_use]
    pub fn shown(&self) -> bool {
        self.locked
    }

    #[must_use]
    pub fn animating(&self) -> bool {
        self.dragging || !self.slide.settled() || self.shake_since.is_some()
    }

    #[must_use]
    pub fn slide_px(&self) -> f32 {
        self.slide.value.max(0.0)
    }

    /// 0..1 fade progress for the unlock drag.
    #[must_use]
    pub fn progress(&self) -> f32 {
        (self.slide_px() / UNLOCK_PX).clamp(0.0, 1.0)
    }

    /// Show the lock (fresh raster; slide starts covered).
    pub fn open(&mut self) {
        self.locked = true;
        self.dragging = false;
        self.drag_origin = 0.0;
        self.slide.retarget(0.0);
        self.slide.snap();
        self.stale = true;
    }

    /// Drop the lock immediately (fingerprint unlock while covered).
    pub fn unlock_now(&mut self) {
        self.locked = false;
        self.slide.retarget(0.0);
        self.slide.snap();
    }

    pub fn begin_drag(&mut self) {
        self.dragging = true;
        self.drag_origin = self.slide.value.max(0.0);
        self.slide.retarget(self.drag_origin);
    }

    /// `dy` is the gesture delta in px (negative = up).
    pub fn drag(&mut self, dy: f32, h: f32) {
        if !self.dragging {
            self.begin_drag();
        }
        self.slide.retarget((self.drag_origin - dy).clamp(0.0, h));
    }

    /// Finger up: `vy` is the release velocity in px/s (negative = up).
    /// Judge the drag by `target` — during a pull only the target tracks
    /// the finger, so a fast release between frames still counts.
    pub fn release(&mut self, vy: f32, h: f32) {
        self.dragging = false;
        let pos = self.slide.value.max(self.slide.target);
        if pos > UNLOCK_PX || vy < FLICK_VY {
            self.slide.retarget_with_velocity(h * 1.1, (-vy).max(0.0));
        } else {
            self.slide.retarget(0.0);
        }
    }

    pub fn cancel_drag(&mut self) {
        self.dragging = false;
        self.slide.retarget(0.0);
    }

    /// fp-fail: jiggle the surface for a moment.
    pub fn fail_shake(&mut self, now: Instant) {
        self.shake_since = Some(now);
    }

    /// Horizontal shake offset during the fail window.
    #[must_use]
    pub fn shake_x(&self, now: Instant) -> f64 {
        let Some(since) = self.shake_since else { return 0.0 };
        let t = now.saturating_duration_since(since).as_secs_f32() * 1000.0;
        if t >= SHAKE_MS {
            return 0.0;
        }
        let k = t / SHAKE_MS;
        (24.0 * (k * std::f32::consts::TAU * 3.0).sin() * (1.0 - k)) as f64
    }

    /// Advance the spring; returns true while anything is still moving.
    pub fn advance(&mut self, dt_s: f32, clock: &AnimationClock, size: Size) -> bool {
        self.slide.advance(dt_s, clock);
        // Px-scale epsilon: on a ~1700 px travel the spring's 0.001 epsilon
        // can park in a fixed point just short of `settled()` and burn
        // frames forever — snap once the residual is sub-pixel.
        if !self.dragging
            && (self.slide.target - self.slide.value).abs() < 1.0
            && self.slide.velocity.abs() < 2.0
        {
            self.slide.snap();
        }
        if self.shake_since.is_some_and(|s| s.elapsed().as_secs_f32() * 1000.0 >= SHAKE_MS) {
            self.shake_since = None;
        }
        if !self.dragging && self.slide.target > size.h as f32 && self.slide.settled() {
            self.locked = false;
            self.slide.retarget(0.0);
            self.slide.snap();
        }
        !self.slide.settled() || self.shake_since.is_some()
    }
}

impl De {
    /// Build the model + rasterize when stale. Returns the lock pixels.
    pub(crate) fn lock_prepare(&mut self, size: Size) -> Option<&Pixels> {
        if !self.lock.shown() {
            return None;
        }
        let (status, date) = self.shell_meta();
        let wp_id = self.app_prefs.prefs().wallpaper_id.clone();
        let backdrop_fresh =
            matches!(&self.lock.backdrop, Some((id, s, _)) if *id == wp_id && *s == size);
        if !self.lock.stale
            && backdrop_fresh
            && self.lock.baked_clock == status.clock
            && self.lock.px.is_some()
        {
            return self.lock.px.as_ref();
        }
        let wp = self.wallpaper_for(&wp_id, size);
        let refresh = !backdrop_fresh;
        if refresh {
            self.lock.backdrop = Some((
                wp_id,
                size,
                de_paint::glass::blurred_backdrop(&wp),
            ));
        }
        let backdrop = self.lock.backdrop.as_ref().map(|(_, _, p)| p.clone())?;
        let charging = if status.battery_full {
            self.strings.get("lock.fully_charged")
        } else if status.charging {
            self.strings.get("lock.charging")
        } else {
            ""
        };
        let shown = self.panels.notifications.len().min(de_ui::lock::PREVIEW);
        let quick = self
            .catalog
            .all()
            .iter()
            .find(|a| a.id.contains("camera") || a.exec.as_deref().unwrap_or("").contains("camera"))
            .map(|a| (a.id.clone(), QuickAction::Camera))
            .or_else(|| {
                self.catalog
                    .all()
                    .iter()
                    .find(|a| a.id.contains("note") || a.id.contains("editor"))
                    .map(|a| (a.id.clone(), QuickAction::Note))
            });
        self.lock.quick_id = quick.as_ref().map(|(id, _)| id.clone());
        let model = LockModel {
            status: &status,
            clock: &status.clock,
            date: &date,
            charging,
            notifications: &self.panels.notifications[..shown],
            hidden_notifications: self.panels.notifications.len().saturating_sub(shown),
            fingerprint: self.power.fp_present(),
            flashlight: self.panels.flashlight(),
            quick: quick.map(|(_, q)| q),
            backdrop: &backdrop,
        };
        let px = de_ui::lock::render(size, &self.strings, &de_design::tokens::DEFAULT, &model);
        self.lock.baked_clock = status.clock;
        self.lock.px = Some(px);
        self.lock.generation += 1;
        self.lock.stale = false;
        self.lock.px.as_ref()
    }

    /// Advance the unlock spring / fail shake; requests frames while moving.
    pub(crate) fn advance_lock(&mut self, now: Instant) {
        if !self.lock.animating() {
            self.lock.last_anim = None;
            return;
        }
        let dt = self
            .lock
            .last_anim
            .map_or(1.0 / 60.0, |t| now.duration_since(t).as_secs_f32())
            .clamp(0.001, 0.05);
        self.lock.last_anim = Some(now);
        let size = Size::new(self.output.width as i32, self.output.height as i32);
        if !self.lock.advance(dt, &self.motion_clock, size) {
            self.lock.last_anim = None;
        }
        self.needs_present = true;
    }

    /// Frame deadline while the lock overlay moves (drag settle, shake).
    pub(crate) fn lock_wake_at(&self, interval: std::time::Duration) -> Option<Instant> {
        (self.lock.shown() && self.lock.animating())
            .then(|| Instant::now() + interval)
    }

    /// Lock-screen tap: bottom-row actions only; everything else is a drag.
    pub(crate) fn lock_tap(&mut self, pos: Point) {
        let size = Size::new(self.output.width as i32, self.output.height as i32);
        match de_ui::lock::lock_hit(size, pos) {
            Some(de_ui::lock::LockHit::Flashlight) => {
                let on = !self.panels.flashlight().unwrap_or(false);
                self.panels
                    .request_control(de_system::controls::Action::Flashlight(on));
                self.lock.stale = true;
            }
            Some(de_ui::lock::LockHit::Quick) => {
                if let Some(id) = self.lock.quick_id.clone() {
                    self.lock.unlock_now();
                    self.nav.current = Screen::Home;
                    self.shell_dirty = true;
                    self.try_launch(&id, Some(de_api::Rect::new(size.w / 2, size.h - 200, 1, 1)));
                }
            }
            None => {}
        }
        self.needs_present = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_drag_springs_back_long_drag_unlocks() {
        let mut l = LockUi::default();
        let clock = AnimationClock::new();
        let size = Size::new(720, 1600);
        l.open();
        l.begin_drag();
        l.drag(-100.0, 1600.0);
        l.release(0.0, 1600.0);
        for _ in 0..120 {
            l.advance(0.016, &clock, size);
        }
        assert!(l.shown() && l.slide_px() < 1.0);

        l.begin_drag();
        l.drag(-400.0, 1600.0);
        l.release(0.0, 1600.0);
        for _ in 0..240 {
            l.advance(0.016, &clock, size);
        }
        assert!(!l.shown());
    }
}
