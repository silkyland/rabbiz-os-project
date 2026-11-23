//! Lock overlay state + raster (M4-T4): the whole surface slides up 1:1
//! with the finger and fades as `slide/UNLOCK_PX` grows — the QML
//! `unlockProgress` contract, now in Rust. Release past the threshold (or
//! with a strong upward flick) slides the surface off and drops the lock;
//! anything else springs it back.

use std::any::Any;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use de_api::{Pixels, Point, Size};
use de_design::motion::{MotionKind, SpringPreset};
use de_paint::animation::{AnimationClock, SpringValue};
use de_system::auth::{
    keyguard_state_path, load_lockout_policy, save_lockout_state_atomic,
    save_pin_credential_atomic, zeroize_secret, AuthOutcome, LockoutPolicy, LockoutState,
    PinCredential,
};
use de_ui::lock::{LockModel, QuickAction};
use de_ui::lock_bouncer::{BouncerHit, BouncerModel};
use de_ui::nav::Screen;
use de_ui::notification_groups::{redact_for_lockscreen, LockscreenPrivacy};

use crate::state::De;

/// Drag distance that counts as a full unlock (matches the old QML value).
const UNLOCK_PX: f32 = 280.0;
/// Upward release velocity that unlocks regardless of distance (px/s).
const FLICK_VY: f32 = -600.0;
/// Fingerprint-fail shake duration.
const SHAKE_MS: f32 = 600.0;
/// Maximum numeric PIN length accepted by the bouncer keypad.
const MAX_PIN_LEN: usize = 12;
/// Whether the bouncer displays and hit-tests the Emergency Call pill.
const EMERGENCY_AVAILABLE: bool = true;

/// Result returned from the off-thread PIN verification worker.
pub(crate) struct AuthVerifyResult {
    pub ok: bool,
    pub upgraded_cred: Option<PinCredential>,
}

/// Result of dispatching a tap on the PIN bouncer surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BouncerTapOutcome {
    Consumed,
    Unlocked,
    Emergency,
    Cancelled,
}

fn now_epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn keyguard_path() -> Option<PathBuf> {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        if !xdg.is_empty() {
            return Some(
                PathBuf::from(xdg)
                    .join("rabbiz-shell")
                    .join("keyguard.json"),
            );
        }
    }
    let home = std::env::var("HOME").ok()?;
    if home.is_empty() {
        return None;
    }
    Some(
        PathBuf::from(home)
            .join(".config")
            .join("rabbiz-shell")
            .join("keyguard.json"),
    )
}

fn load_pin_credential() -> Option<PinCredential> {
    let path = keyguard_path()?;
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str::<PinCredential>(&text).ok()
}

pub(crate) struct LockUi {
    /// The lock owns the screen (covering, mid-drag or mid-unlock).
    locked: bool,
    /// Unlock committed — sliding off-screen; touches pass through and
    /// dragging/cancelling can never pull the lock back down.
    unlocking: bool,
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
    /// Cooldown seconds baked into the current bouncer raster — re-raster on change.
    baked_cooldown: Option<u32>,
    /// fp-fail shake window.
    shake_since: Option<Instant>,
    /// Blurred wallpaper backdrop: `(wallpaper_id, size, pixels)`.
    backdrop: Option<(String, Size, Pixels)>,
    /// desktop-id behind the bottom-right action (camera or quick note).
    pub quick_id: Option<String>,
    /// Whether the PIN credential bouncer is currently open over the lock screen.
    pub bouncer_open: bool,
    /// Entered ASCII PIN digits — always zeroized via `zeroize_secret` on submit,
    /// cancel, lock, unlock, and drop.
    pin_buf: Vec<u8>,
    /// Transient error line shown above the PIN dot row.
    bouncer_error: Option<String>,
    /// Deterministic backoff/lockout policy for credential attempts.
    pub lockout: LockoutPolicy,
    /// True while an off-thread Argon2id verification is in flight.
    pub verifying: bool,
    /// Channel sender to deliver verification results back to the calloop event loop.
    pub(crate) auth_tx: Option<smithay::reexports::calloop::channel::Sender<AuthVerifyResult>>,
    /// Off-thread writer channel for persisting `keyguard.state`.
    state_tx: Option<std::sync::mpsc::Sender<(PathBuf, LockoutState)>>,
    /// Optional salted PIN credential loaded from `keyguard.json`.
    pin_cred: Option<PinCredential>,
}

impl Drop for LockUi {
    fn drop(&mut self) {
        zeroize_secret(&mut self.pin_buf);
    }
}

impl Default for LockUi {
    fn default() -> Self {
        Self {
            locked: false,
            unlocking: false,
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
            baked_cooldown: None,
            shake_since: None,
            backdrop: None,
            quick_id: None,
            bouncer_open: false,
            pin_buf: Vec::new(),
            bouncer_error: None,
            lockout: keyguard_state_path().map_or_else(LockoutPolicy::new, |p| {
                load_lockout_policy(&p, now_epoch_secs())
            }),
            verifying: false,
            auth_tx: None,
            state_tx: None,
            pin_cred: load_pin_credential(),
        }
    }
}

impl LockUi {
    fn persist_lockout_state(&mut self) {
        let Some(path) = keyguard_state_path() else {
            return;
        };
        if self.state_tx.is_none() {
            let (tx, rx) = std::sync::mpsc::channel::<(PathBuf, LockoutState)>();
            if let Ok(_) = std::thread::Builder::new()
                .name("rabbiz-keyguard-state".into())
                .spawn(move || {
                    while let Ok((p, s)) = rx.recv() {
                        if let Err(e) = save_lockout_state_atomic(&p, &s) {
                            eprintln!("rabbiz-de: save keyguard state {} failed: {e}", p.display());
                        }
                    }
                })
            {
                self.state_tx = Some(tx);
            }
        }
        if let Some(tx) = &self.state_tx {
            let _ = tx.send((path, self.lockout.state().clone()));
        }
    }
    #[must_use]
    pub fn shown(&self) -> bool {
        self.locked
    }

    #[must_use]
    pub fn is_unlocking(&self) -> bool {
        self.locked && self.unlocking
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

    /// Whether swipe-up must open the PIN bouncer instead of unlocking immediately.
    ///
    /// Fail closed: any mode other than `"swipe"` requires credential verification,
    /// even if `keyguard.json` is missing or unreadable.
    #[must_use]
    pub fn requires_pin(&self, lock_method: &str) -> bool {
        lock_method.trim() != "swipe"
    }

    /// Show the lock (fresh raster; slide starts covered).
    pub fn open(&mut self) {
        zeroize_secret(&mut self.pin_buf);
        self.bouncer_open = false;
        self.bouncer_error = None;
        self.locked = true;
        self.unlocking = false;
        self.dragging = false;
        self.drag_origin = 0.0;
        self.slide.retarget(0.0);
        self.slide.snap();
        self.stale = true;
    }

    /// Drop the lock immediately (fingerprint or verified PIN unlock).
    pub fn unlock_now(&mut self) {
        zeroize_secret(&mut self.pin_buf);
        self.bouncer_open = false;
        self.bouncer_error = None;
        self.locked = false;
        self.unlocking = false;
        self.slide.retarget(0.0);
        self.slide.snap();
    }

    pub fn begin_drag(&mut self) {
        if self.unlocking || self.bouncer_open {
            return;
        }
        self.dragging = true;
        self.drag_origin = self.slide.value.max(0.0);
        self.slide.retarget(self.drag_origin);
    }

    /// `dy` is the gesture delta in px (negative = up).
    pub fn drag(&mut self, dy: f32, h: f32) {
        if self.unlocking || self.bouncer_open {
            return;
        }
        if !self.dragging {
            self.begin_drag();
        }
        let pos = (self.drag_origin - dy).clamp(0.0, h);
        self.slide.value = pos;
        self.slide.target = pos;
    }

    /// Finger up: `vy` is the release velocity in px/s (negative = up).
    /// Judge the drag by `target` — during a pull only the target tracks
    /// the finger, so a fast release between frames still counts.
    ///
    /// When `requires_pin(lock_method)` is true, completing the swipe-up opens
    /// the PIN bouncer instead of unlocking immediately.
    pub fn release(&mut self, vy: f32, h: f32, lock_method: &str) {
        if self.unlocking || self.bouncer_open {
            return;
        }
        self.dragging = false;
        let pos = self.slide.value.max(self.slide.target);
        if pos > UNLOCK_PX || vy < FLICK_VY {
            if self.requires_pin(lock_method) {
                zeroize_secret(&mut self.pin_buf);
                self.bouncer_open = true;
                self.bouncer_error = if self.pin_cred.is_none() {
                    Some("Keyguard credential missing or unreadable — cannot unlock".to_string())
                } else {
                    None
                };
                self.slide.retarget(0.0);
                self.slide.snap();
                self.stale = true;
            } else {
                self.unlocking = true;
                self.slide.retarget_with_velocity(h * 1.1, (-vy).max(0.0));
            }
        } else {
            self.slide.retarget(0.0);
        }
    }

    pub fn cancel_drag(&mut self) {
        if self.unlocking || self.bouncer_open {
            return;
        }
        self.dragging = false;
        self.slide.retarget(0.0);
    }

    /// Handle a tap on the PIN bouncer overlay.
    pub(crate) fn handle_bouncer_tap(
        &mut self,
        pt: Point,
        output: Size,
        now_s: u64,
    ) -> BouncerTapOutcome {
        if !self.bouncer_open {
            return BouncerTapOutcome::Consumed;
        }
        let dummy = Pixels::empty(Size::new(0, 0));
        let model = BouncerModel {
            entered_len: self.pin_buf.len(),
            max_len: self.pin_buf.len().clamp(6, MAX_PIN_LEN),
            error: self.bouncer_error.as_deref(),
            cooldown_secs: self.lockout.cooldown_remaining(now_s),
            checking: self.verifying,
            emergency_available: EMERGENCY_AVAILABLE,
            backdrop: &dummy,
        };
        let Some(hit) = de_ui::lock_bouncer::hit(output, pt, &model) else {
            return BouncerTapOutcome::Consumed;
        };
        match hit {
            BouncerHit::Digit(d) => {
                if !self.verifying && self.pin_buf.len() < MAX_PIN_LEN && d <= 9 {
                    self.pin_buf.push(b'0' + d);
                    self.bouncer_error = None;
                    self.stale = true;
                }
                BouncerTapOutcome::Consumed
            }
            BouncerHit::Backspace => {
                if !self.verifying && self.pin_buf.pop().is_some() {
                    self.stale = true;
                }
                BouncerTapOutcome::Consumed
            }
            BouncerHit::Cancel => {
                zeroize_secret(&mut self.pin_buf);
                self.verifying = false;
                self.bouncer_open = false;
                self.bouncer_error = None;
                self.stale = true;
                BouncerTapOutcome::Cancelled
            }
            BouncerHit::Emergency => BouncerTapOutcome::Emergency,
            BouncerHit::Submit => {
                if self.verifying || self.pin_buf.is_empty() {
                    return BouncerTapOutcome::Consumed;
                }
                if self.lockout.cooldown_remaining(now_s).is_some() {
                    return BouncerTapOutcome::Consumed;
                }
                let Some(cred) = self.pin_cred.clone() else {
                    zeroize_secret(&mut self.pin_buf);
                    self.bouncer_error = Some(
                        "Keyguard credential missing or unreadable — cannot unlock".to_string(),
                    );
                    self.stale = true;
                    return BouncerTapOutcome::Consumed;
                };
                let mut pin_bytes = std::mem::take(&mut self.pin_buf);
                if let Some(tx) = self.auth_tx.clone() {
                    self.verifying = true;
                    self.bouncer_error = None;
                    self.stale = true;
                    let _ = std::thread::Builder::new()
                        .name("rabbiz-pin-verify".into())
                        .spawn(move || {
                            let (ok, upgraded_cred) = std::str::from_utf8(&pin_bytes)
                                .map_or((false, None), |pin| cred.verify_and_upgrade(pin));
                            zeroize_secret(&mut pin_bytes);
                            let _ = tx.send(AuthVerifyResult { ok, upgraded_cred });
                        });
                    BouncerTapOutcome::Consumed
                } else {
                    // Synchronous test fallback: still run on a named worker thread so
                    // verification never executes on the compositor main thread.
                    let res = std::thread::Builder::new()
                        .name("rabbiz-pin-verify".into())
                        .spawn(move || {
                            let (ok, upgraded_cred) = std::str::from_utf8(&pin_bytes)
                                .map_or((false, None), |pin| cred.verify_and_upgrade(pin));
                            zeroize_secret(&mut pin_bytes);
                            AuthVerifyResult { ok, upgraded_cred }
                        })
                        .ok()
                        .and_then(|h| h.join().ok())
                        .unwrap_or(AuthVerifyResult {
                            ok: false,
                            upgraded_cred: None,
                        });
                    self.apply_verify_result(res, now_s)
                }
            }
        }
    }

    pub(crate) fn apply_verify_result(
        &mut self,
        res: AuthVerifyResult,
        now_s: u64,
    ) -> BouncerTapOutcome {
        self.verifying = false;
        if res.ok {
            if let Some(new_cred) = res.upgraded_cred {
                self.pin_cred = Some(new_cred.clone());
                if let Some(path) = keyguard_path() {
                    let _ = std::thread::Builder::new()
                        .name("rabbiz-keyguard-migrate".into())
                        .spawn(move || {
                            if let Err(e) = save_pin_credential_atomic(&path, &new_cred) {
                                eprintln!(
                                    "rabbiz-de: migrate keyguard credential {} failed: {e}",
                                    path.display()
                                );
                            }
                        });
                }
            }
        }
        let outcome = self.lockout.record_attempt(res.ok, now_s);
        self.persist_lockout_state();
        match outcome {
            AuthOutcome::Accepted => {
                self.unlock_now();
                self.stale = true;
                BouncerTapOutcome::Unlocked
            }
            AuthOutcome::Rejected { attempts_remaining } => {
                self.bouncer_error = Some(format!(
                    "Incorrect PIN ({attempts_remaining} attempts left)"
                ));
                self.stale = true;
                BouncerTapOutcome::Consumed
            }
            AuthOutcome::Cooldown { retry_after_secs } => {
                self.bouncer_error = Some(format!("Try again in {retry_after_secs}s"));
                self.stale = true;
                BouncerTapOutcome::Consumed
            }
        }
    }

    /// fp-fail: jiggle the surface for a moment.
    pub fn fail_shake(&mut self, now: Instant) {
        self.shake_since = Some(now);
    }

    /// Horizontal shake offset during the fail window.
    #[must_use]
    pub fn shake_x(&self, now: Instant) -> f64 {
        let Some(since) = self.shake_since else {
            return 0.0;
        };
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
        if self
            .shake_since
            .is_some_and(|s| s.elapsed().as_secs_f32() * 1000.0 >= SHAKE_MS)
        {
            self.shake_since = None;
        }
        if (self.unlocking || (!self.dragging && self.slide.target > size.h as f32))
            && self.slide.settled()
        {
            self.unlock_now();
        }
        !self.slide.settled() || self.shake_since.is_some()
    }
}

/// Map `Prefs::lockscreen_notifications` string to [`LockscreenPrivacy`].
fn lockscreen_privacy_from_pref(value: &str) -> LockscreenPrivacy {
    match value {
        "hide_sensitive" => LockscreenPrivacy::HideSensitive,
        "hide_all" => LockscreenPrivacy::HideAll,
        _ => LockscreenPrivacy::ShowAll,
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
        let cooldown_secs = if self.lock.bouncer_open {
            self.lock.lockout.cooldown_remaining(now_epoch_secs())
        } else {
            None
        };
        if !self.lock.stale
            && backdrop_fresh
            && self.lock.baked_clock == status.clock
            && self.lock.baked_cooldown == cooldown_secs
            && self.lock.px.is_some()
        {
            return self.lock.px.as_ref();
        }
        let wp = self.wallpaper_for(&wp_id, size);
        let refresh = !backdrop_fresh;
        if refresh {
            self.lock.backdrop = Some((wp_id, size, de_paint::glass::blurred_backdrop(&wp)));
        }
        let backdrop = self.lock.backdrop.as_ref().map(|(_, _, p)| p.clone())?;
        let px = if self.lock.bouncer_open {
            let model = BouncerModel {
                entered_len: self.lock.pin_buf.len(),
                max_len: self.lock.pin_buf.len().clamp(6, MAX_PIN_LEN),
                error: self.lock.bouncer_error.as_deref(),
                cooldown_secs,
                checking: self.lock.verifying,
                emergency_available: EMERGENCY_AVAILABLE,
                backdrop: &backdrop,
            };
            de_ui::lock_bouncer::render(size, &self.theme, &model, &self.strings)
        } else {
            let charging = if status.battery_full {
                self.strings.get("lock.fully_charged")
            } else if status.charging {
                self.strings.get("lock.charging")
            } else {
                ""
            };
            let privacy = lockscreen_privacy_from_pref(
                self.app_prefs.prefs().lockscreen_notifications.as_str(),
            );
            let redacted =
                redact_for_lockscreen(&self.panels.notifications, privacy, "New notification");
            let shown = redacted.len().min(de_ui::lock::PREVIEW);
            let quick = self
                .catalog
                .all()
                .iter()
                .find(|a| {
                    a.id.contains("camera") || a.exec.as_deref().unwrap_or("").contains("camera")
                })
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
                notifications: &redacted[..shown],
                hidden_notifications: redacted.len().saturating_sub(shown),
                fingerprint: self.power.fp_present(),
                flashlight: self.panels.flashlight(),
                quick: quick.map(|(_, q)| q),
                backdrop: &backdrop,
            };
            de_ui::lock::render(size, &self.strings, &self.theme, &model)
        };
        self.lock.baked_clock = status.clock;
        self.lock.baked_cooldown = cooldown_secs;
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

    /// Frame deadline while the lock overlay moves (drag settle, shake) or a
    /// bouncer cooldown countdown is ticking.
    pub(crate) fn lock_wake_at(&self, interval: Duration) -> Option<Instant> {
        if !self.lock.shown() {
            return None;
        }
        if self.lock.animating() {
            return Some(Instant::now() + interval);
        }
        if self.lock.bouncer_open
            && self
                .lock
                .lockout
                .cooldown_remaining(now_epoch_secs())
                .is_some()
        {
            return Some(Instant::now() + Duration::from_secs(1));
        }
        None
    }

    /// Apply the result of an off-thread PIN verification on the compositor loop.
    pub(crate) fn finish_pin_verify(&mut self, res: AuthVerifyResult) {
        if matches!(
            self.lock.apply_verify_result(res, now_epoch_secs()),
            BouncerTapOutcome::Unlocked
        ) {
            self.shell_dirty = true;
        }
        self.needs_present = true;
    }

    /// Dispatch a tap on the PIN bouncer overlay.
    pub(crate) fn lock_bouncer_tap(&mut self, pos: Point) {
        let size = Size::new(self.output.width as i32, self.output.height as i32);
        match self.lock.handle_bouncer_tap(pos, size, now_epoch_secs()) {
            BouncerTapOutcome::Unlocked => {
                self.shell_dirty = true;
                self.needs_present = true;
            }
            BouncerTapOutcome::Consumed
            | BouncerTapOutcome::Cancelled
            | BouncerTapOutcome::Emergency => {
                self.needs_present = true;
            }
        }
    }

    /// Lock-screen tap: bouncer controls when open, otherwise bottom-row actions.
    pub(crate) fn lock_tap(&mut self, pos: Point) {
        if self.lock.bouncer_open {
            self.lock_bouncer_tap(pos);
            return;
        }
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

impl LockUi {
    /// Output size changed (rotation): drop raster + backdrop and re-render.
    pub(crate) fn on_resize(&mut self) {
        self.px = None;
        self.tex = None;
        self.backdrop = None;
        self.stale = true;
        self.generation = self.generation.wrapping_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn find_bouncer_target(size: Size, target: BouncerHit) -> Point {
        let dummy = Pixels::empty(Size::new(0, 0));
        let model = BouncerModel {
            entered_len: 0,
            max_len: 6,
            error: None,
            cooldown_secs: None,
            checking: false,
            emergency_available: EMERGENCY_AVAILABLE,
            backdrop: &dummy,
        };
        for y in (0..size.h).step_by(12) {
            for x in (0..size.w).step_by(12) {
                let pt = Point::new(x, y);
                if de_ui::lock_bouncer::hit(size, pt, &model) == Some(target) {
                    return pt;
                }
            }
        }
        panic!("bouncer hit target {target:?} not found on {size:?}");
    }

    #[test]
    fn short_drag_springs_back_long_drag_unlocks() {
        let mut l = LockUi::default();
        let clock = AnimationClock::new();
        let size = Size::new(720, 1600);
        l.open();
        l.begin_drag();
        l.drag(-100.0, 1600.0);
        l.release(0.0, 1600.0, "swipe");
        for _ in 0..120 {
            l.advance(0.016, &clock, size);
        }
        assert!(l.shown() && l.slide_px() < 1.0);

        l.begin_drag();
        l.drag(-400.0, 1600.0);
        l.release(0.0, 1600.0, "swipe");
        for _ in 0..240 {
            l.advance(0.016, &clock, size);
        }
        assert!(!l.shown());
    }

    #[test]
    fn committed_unlock_cannot_be_pulled_back_down() {
        let mut l = LockUi::default();
        let clock = AnimationClock::new();
        let size = Size::new(720, 1600);
        l.open();
        l.begin_drag();
        l.drag(-400.0, 1600.0);
        l.release(-800.0, 1600.0, "swipe");
        assert!(l.is_unlocking());
        // Swiping down or cancelling mid-unlock must not bring the lock back.
        l.begin_drag();
        l.drag(500.0, 1600.0);
        l.cancel_drag();
        for _ in 0..240 {
            l.advance(0.016, &clock, size);
        }
        assert!(!l.shown());
    }

    #[test]
    fn swipe_mode_unlocks_immediately_without_bouncer() {
        let mut l = LockUi::default();
        l.pin_cred = Some(
            PinCredential::create_with_params("1234", [0x22_u8; 16], 1024, 1, 1)
                .expect("create_with_params"),
        );
        let clock = AnimationClock::new();
        let size = Size::new(720, 1600);
        assert!(!l.requires_pin("swipe"));

        l.open();
        l.begin_drag();
        l.drag(-400.0, 1600.0);
        l.release(0.0, 1600.0, "swipe");
        assert!(!l.bouncer_open);
        assert!(l.is_unlocking());
        for _ in 0..240 {
            l.advance(0.016, &clock, size);
        }
        assert!(!l.shown());
    }

    #[test]
    fn pin_mode_requires_bouncer_wrong_right_and_cancel() {
        let size = Size::new(720, 1600);
        let mut l = LockUi::default();
        l.pin_cred = Some(
            PinCredential::create_with_params("1234", [0x42_u8; 16], 1024, 1, 1)
                .expect("create_with_params"),
        );
        assert!(l.requires_pin("pin"));

        // Swipe up opens bouncer instead of unlocking immediately.
        l.open();
        l.begin_drag();
        l.drag(-400.0, 1600.0);
        l.release(0.0, 1600.0, "pin");
        assert!(l.shown());
        assert!(l.bouncer_open);
        assert!(!l.is_unlocking());
        assert!(l.slide_px() < 1.0);

        let submit = find_bouncer_target(size, BouncerHit::Submit);
        let backspace = find_bouncer_target(size, BouncerHit::Backspace);
        let cancel = find_bouncer_target(size, BouncerHit::Cancel);
        let emergency = find_bouncer_target(size, BouncerHit::Emergency);
        let digit = |d: u8| find_bouncer_target(size, BouncerHit::Digit(d));

        // Wrong PIN increments lockout, sets error, and zeroizes pin_buf.
        for d in [9_u8, 9, 9, 9] {
            assert_eq!(
                l.handle_bouncer_tap(digit(d), size, 1_000),
                BouncerTapOutcome::Consumed
            );
        }
        assert_eq!(l.pin_buf.len(), 4);
        assert_eq!(
            l.handle_bouncer_tap(submit, size, 1_000),
            BouncerTapOutcome::Consumed
        );
        assert!(l.pin_buf.is_empty());
        assert!(l.shown() && l.bouncer_open);
        assert_eq!(l.lockout.state().consecutive_failures, 1);
        assert!(l.bouncer_error.is_some());

        // Emergency button returns Emergency without closing bouncer.
        assert_eq!(
            l.handle_bouncer_tap(emergency, size, 1_001),
            BouncerTapOutcome::Emergency
        );
        assert!(l.bouncer_open);

        // Entering digits + Backspace + Cancel closes bouncer and zeroizes buffer.
        assert_eq!(
            l.handle_bouncer_tap(digit(1), size, 1_001),
            BouncerTapOutcome::Consumed
        );
        assert_eq!(
            l.handle_bouncer_tap(digit(5), size, 1_001),
            BouncerTapOutcome::Consumed
        );
        assert_eq!(l.pin_buf.len(), 2);
        assert_eq!(
            l.handle_bouncer_tap(backspace, size, 1_001),
            BouncerTapOutcome::Consumed
        );
        assert_eq!(l.pin_buf.len(), 1);
        assert_eq!(
            l.handle_bouncer_tap(cancel, size, 1_002),
            BouncerTapOutcome::Cancelled
        );
        assert!(!l.bouncer_open);
        assert!(l.pin_buf.is_empty());
        assert!(l.shown());

        // Re-open bouncer via swipe-up, enter right PIN -> unlocks and resets lockout.
        l.begin_drag();
        l.drag(-400.0, 1600.0);
        l.release(0.0, 1600.0, "pin");
        assert!(l.bouncer_open);
        for d in [1_u8, 2, 3, 4] {
            assert_eq!(
                l.handle_bouncer_tap(digit(d), size, 1_005),
                BouncerTapOutcome::Consumed
            );
        }
        assert_eq!(
            l.handle_bouncer_tap(submit, size, 1_005),
            BouncerTapOutcome::Unlocked
        );
        assert!(!l.shown());
        assert!(!l.bouncer_open);
        assert!(l.pin_buf.is_empty());
        assert_eq!(l.lockout.state().consecutive_failures, 0);
        assert!(l.lockout.state().bfu_unlocked);
    }

    #[test]
    fn lockscreen_privacy_maps_pref_strings() {
        assert_eq!(
            lockscreen_privacy_from_pref("hide_sensitive"),
            LockscreenPrivacy::HideSensitive
        );
        assert_eq!(
            lockscreen_privacy_from_pref("hide_all"),
            LockscreenPrivacy::HideAll
        );
        assert_eq!(
            lockscreen_privacy_from_pref("show_all"),
            LockscreenPrivacy::ShowAll
        );
        assert_eq!(
            lockscreen_privacy_from_pref("unknown"),
            LockscreenPrivacy::ShowAll
        );
    }

    #[test]
    fn pin_mode_with_missing_credential_stays_locked_with_recovery_error() {
        let mut l = LockUi::default();
        l.pin_cred = None;
        assert!(l.requires_pin("pin"));
        assert!(l.requires_pin("passphrase"));

        l.open();
        l.begin_drag();
        l.drag_to(500.0);
        l.release(-800.0, 1600.0, "pin");
        assert!(l.shown());
        assert!(!l.is_unlocking());
        assert!(l.bouncer_open);
        assert!(l
            .bouncer_error
            .as_deref()
            .is_some_and(|e| e.contains("missing")));
    }

    #[test]
    fn pin_verify_runs_off_thread_and_ignores_taps_while_in_flight() {
        let size = Size::new(720, 1600);
        let (tx, _rx) = smithay::reexports::calloop::channel::channel();
        let mut l = LockUi::default();
        l.auth_tx = Some(tx);
        l.pin_cred = Some(
            PinCredential::create_with_params("1234", [0x33_u8; 16], 1024, 1, 1)
                .expect("create_with_params"),
        );
        l.open();
        l.bouncer_open = true;

        let d1 = find_bouncer_target(size, BouncerHit::Digit(1));
        let d2 = find_bouncer_target(size, BouncerHit::Digit(2));
        let d3 = find_bouncer_target(size, BouncerHit::Digit(3));
        let d4 = find_bouncer_target(size, BouncerHit::Digit(4));
        let submit = find_bouncer_target(size, BouncerHit::Submit);

        for pt in [d1, d2, d3, d4] {
            assert_eq!(
                l.handle_bouncer_tap(pt, size, 1_000),
                BouncerTapOutcome::Consumed
            );
        }
        assert_eq!(
            l.handle_bouncer_tap(submit, size, 1_000),
            BouncerTapOutcome::Consumed
        );
        assert!(l.verifying);
        assert!(l.pin_buf.is_empty());

        // Further digit taps while verification is in flight are ignored.
        assert_eq!(
            l.handle_bouncer_tap(d1, size, 1_000),
            BouncerTapOutcome::Consumed
        );
        assert!(l.pin_buf.is_empty());

        // Applying the async result unlocks cleanly.
        assert_eq!(
            l.apply_verify_result(
                AuthVerifyResult {
                    ok: true,
                    upgraded_cred: None,
                },
                1_000
            ),
            BouncerTapOutcome::Unlocked
        );
        assert!(!l.verifying);
        assert!(!l.shown());
    }
}
