//! Power state machine (M4-T4): `Awake → Dim → FadeOut → Asleep → WakeIn`.
//!
//! Sources:
//! * idle clock — `deScreenTimeoutSec` (default 600 s) of no input dims the
//!   panel to 20 %; `DIM_MS` later it fades and sleeps.
//! * `$XDG_RUNTIME_DIR/a21s-power.cmd` — `a21s-power-key` writes
//!   `<action>:<nonce>` lines (`toggle|sleep|wake|lock|menu|fp-unlock|fp-fail`);
//!   the 300 ms screenshot-combo guard already ran in the script.
//! * `/sys/kernel/a21s_fp/irq_count` — an increase while locked/asleep
//!   unlocks (rear sensor = convenience unlock, per P1).
//! * touch while asleep — 600 ms hold or double-tap wakes.
//!
//! Suspend-to-RAM is out of scope: "sleep" here is panel-off + schedutil,
//! the compositor just stops presenting.

use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

use de_api::Size;
use de_ui::nav::Screen;
use de_ui::power::{PowerAction, PowerModel, DESTRUCTIVE, HOLD_MS};

use crate::state::De;

/// Panel dim level while waiting out the last stretch of the idle timer.
const DIM_FRAC: f32 = 0.2;
/// How long the dim holds before the fade → sleep.
const DIM_MS: u64 = 10_000;
/// Locked-but-awake screens sleep on their own shorter idle budget.
const LOCKED_IDLE_MS: u64 = 30_000;
/// Fade-out / wake-in durations (ms) — the fade element lives in frame.rs.
const FADE_OUT_MS: f32 = 300.0;
const WAKE_IN_MS: f32 = 220.0;
/// Finger on a sleeping screen this long wakes (hold-to-peek).
const SLEEP_TOUCH_WAKE_MS: u64 = 600;
/// Two taps inside this window on a sleeping screen also wake.
const DOUBLE_TAP_MS: u64 = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Phase {
    Awake,
    /// Idle timer fired — panel at 20 %, one input undoes it.
    Dim,
    /// Playing the black fade; `Instant` = when it started.
    FadeOut(Instant),
    /// Panel off, compositor idle — wake sources still polled.
    Asleep,
    /// Panel back on under the lock screen, fading in.
    WakeIn(Instant),
}

pub(crate) struct Power {
    phase: Phase,
    last_activity: Instant,
    dim_until: Option<Instant>,

    cmd_path: PathBuf,
    cmd_nonce: String,
    cmd_primed: bool,

    shot_path: PathBuf,
    shot_nonce: String,
    shot_primed: bool,

    fp_present: bool,
    fp_irq: Option<u64>,

    backlight: Option<de_system::power::Backlight>,
    saved: Option<u32>,
    saved_governor: Option<String>,

    /// Finger held on the sleeping screen.
    sleep_touch: Option<Instant>,
    last_sleep_tap: Option<Instant>,

    /// Armed destructive row + when the finger went down.
    pub menu_hold: Option<(PowerAction, Instant)>,
    /// Power menu drawn above the lock (long-press combo while locked).
    pub menu_over_lock: bool,
    /// Rasterized locked-menu texture cache.
    pub menu_px: Option<de_api::Pixels>,
    pub menu_tex: Option<Box<dyn std::any::Any>>,
    pub menu_stale: bool,
}

impl Default for Power {
    fn default() -> Self {
        let rt = std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/run/user/1001"));
        let fp_present = de_system::fingerprint::read().is_some_and(|r| r.supported());
        Self {
            phase: Phase::Awake,
            last_activity: Instant::now(),
            dim_until: None,
            cmd_path: rt.join("a21s-power.cmd"),
            cmd_nonce: String::new(),
            cmd_primed: false,
            shot_path: rt.join("a21s-screenshot.state"),
            shot_nonce: String::new(),
            shot_primed: false,
            fp_present,
            fp_irq: None,
            backlight: de_system::power::backlight(),
            saved: None,
            saved_governor: None,
            sleep_touch: None,
            last_sleep_tap: None,
            menu_hold: None,
            menu_over_lock: false,
            menu_px: None,
            menu_tex: None,
            menu_stale: true,
        }
    }
}

impl Power {
    #[must_use]
    pub fn sleeping(&self) -> bool {
        self.phase == Phase::Asleep
    }

    #[must_use]
    pub fn fp_present(&self) -> bool {
        self.fp_present
    }

    /// Any input: restarts the idle clock and undims.
    /// Returns true when the compositor should repaint (undim happened).
    pub fn note_activity(&mut self, now: Instant) -> bool {
        self.last_activity = now;
        if self.phase == Phase::Dim {
            self.restore();
            self.phase = Phase::Awake;
            self.dim_until = None;
            return true;
        }
        false
    }

    fn save(&mut self) {
        // First save wins: Dim→FadeOut would otherwise persist the dim
        // level as the "user" brightness.
        if self.saved.is_none() {
            if let Some(bl) = &self.backlight {
                if let Some(raw) = bl.get().filter(|v| *v > 0) {
                    self.saved = Some(raw);
                    de_system::power::save_brightness(raw);
                }
            }
        }
        if self.saved_governor.is_none() {
            self.saved_governor = de_system::power::governor();
        }
        de_system::power::set_governor("schedutil");
    }

    fn restore(&mut self) {
        if let Some(bl) = &self.backlight {
            bl.power(true);
            bl.set(self.saved.take().unwrap_or(bl.max));
        }
        if let Some(g) = self.saved_governor.take() {
            de_system::power::set_governor(&g);
        }
    }

    /// Black-overlay alpha for the fade phases (0 = none).
    #[must_use]
    pub fn fade_alpha(&self, now: Instant) -> f32 {
        match self.phase {
            Phase::FadeOut(since) => {
                (now.saturating_duration_since(since).as_secs_f32() * 1000.0 / FADE_OUT_MS).min(1.0)
            }
            Phase::WakeIn(since) => 1.0
                - (now.saturating_duration_since(since).as_secs_f32() * 1000.0 / WAKE_IN_MS)
                    .min(1.0),
            _ => 0.0,
        }
    }

    /// Transition FadeOut→Asleep and WakeIn→Awake once their fades land.
    /// Called at the top of every frame.
    pub fn advance(&mut self, now: Instant) {
        match self.phase {
            Phase::FadeOut(since)
                if now.saturating_duration_since(since).as_secs_f32() * 1000.0 >= FADE_OUT_MS =>
            {
                if let Some(bl) = &self.backlight {
                    bl.power(false);
                    bl.set(0);
                }
                self.phase = Phase::Asleep;
            }
            Phase::WakeIn(since)
                if now.saturating_duration_since(since).as_secs_f32() * 1000.0 >= WAKE_IN_MS =>
            {
                self.phase = Phase::Awake;
            }
            _ => {}
        }
    }

    /// Next deadline the frame loop should honour, if any.
    #[must_use]
    pub fn wake_at(&self, now: Instant) -> Option<Instant> {
        let mut out: Option<Instant> = None;
        let mut add = |i: Instant| {
            if out.is_none_or(|o| i < o) {
                out = Some(i);
            }
        };
        if let Some(d) = self.dim_until {
            add(d);
        }
        match self.phase {
            Phase::FadeOut(s) => add(s + Duration::from_millis(FADE_OUT_MS as u64)),
            Phase::WakeIn(s) => add(s + Duration::from_millis(WAKE_IN_MS as u64)),
            Phase::Asleep => {
                if let Some(t) = self.sleep_touch {
                    add(t + Duration::from_millis(SLEEP_TOUCH_WAKE_MS));
                }
            }
            _ => {}
        }
        if let Some((_, s)) = self.menu_hold {
            add(s + Duration::from_millis(HOLD_MS));
        }
        let _ = now;
        out
    }

    /// Last non-empty `action:nonce` line, deduped; first read just primes.
    fn cmd_poll(&mut self) -> Option<String> {
        let body = std::fs::read_to_string(&self.cmd_path).ok()?;
        let line = body.lines().rev().find(|l| !l.trim().is_empty())?;
        let (action, nonce) = line.split_once(':')?;
        if !self.cmd_primed || nonce == self.cmd_nonce {
            self.cmd_primed = true;
            return None;
        }
        self.cmd_nonce = nonce.to_string();
        Some(action.to_string())
    }

    /// Screenshot daemon wrote a new path → fire the flash/toast.
    fn shot_poll(&mut self) -> Option<String> {
        let body = std::fs::read_to_string(&self.shot_path).ok()?;
        let line = body.lines().rev().find(|l| !l.trim().is_empty())?;
        let (nonce, path) = line.split_once(':').map_or(("1", line), |(n, p)| (n, p));
        if !self.shot_primed || nonce == self.shot_nonce {
            self.shot_primed = true;
            return None;
        }
        self.shot_nonce = nonce.to_string();
        let p = path.trim();
        (!p.is_empty()).then(|| p.to_string())
    }

    /// A finger interrupt while locked/asleep counts as unlock.
    fn fp_poll(&mut self) -> Option<bool> {
        let r = de_system::fingerprint::read()?;
        let prev = self.fp_irq.replace(r.irq_count);
        match prev {
            None => None,
            Some(p) if r.irq_count > p => Some(true),
            Some(_) if r.status.contains("fail") => Some(false),
            _ => None,
        }
    }
}

impl De {
    /// Screen locked (overlay covering or mid-unlock-drag).
    #[must_use]
    pub(crate) fn locked(&self) -> bool {
        self.lock.shown()
    }

    /// Panel off, deep-idle.
    #[must_use]
    pub(crate) fn sleeping(&self) -> bool {
        self.power.sleeping()
    }

    /// Stamp user activity (every input event calls this first).
    pub(crate) fn power_note_activity(&mut self) {
        if self.power.note_activity(Instant::now()) {
            self.needs_present = true;
        }
    }

    /// Finger down/up/cancel while the panel is off.
    pub(crate) fn sleep_touch_down(&mut self) {
        self.power.sleep_touch = Some(Instant::now());
    }

    /// Returns true when this release completed a wake double-tap.
    pub(crate) fn sleep_touch_up(&mut self) -> bool {
        let Some(start) = self.power.sleep_touch.take() else {
            return false;
        };
        let now = Instant::now();
        if now.duration_since(start).as_millis() > 400 {
            self.power.last_sleep_tap = None;
            return false;
        }
        if self
            .power
            .last_sleep_tap
            .is_some_and(|t| now.duration_since(t).as_millis() <= DOUBLE_TAP_MS.into())
        {
            self.power.last_sleep_tap = None;
            self.power_wake();
            return true;
        }
        self.power.last_sleep_tap = Some(now);
        false
    }

    pub(crate) fn sleep_touch_cancel(&mut self) {
        self.power.sleep_touch = None;
    }

    /// Idle → dim → fade → sleep; cmd/fp/shot watchers; hold-to-confirm.
    /// Called on the 500 ms loop timer.
    pub(crate) fn poll_power(&mut self) {
        let now = Instant::now();
        if let Some(action) = self.power.cmd_poll() {
            self.power_cmd(&action);
        }
        if let Some(path) = self.power.shot_poll() {
            self.shot_fx.fire(&path, now);
        }
        match self.power.fp_poll() {
            Some(true) if self.locked() || self.sleeping() => {
                de_system::fingerprint::arm_trigger();
                self.power_wake();
                self.lock.unlock_now();
            }
            Some(false) if self.locked() => self.lock.fail_shake(now),
            _ => {}
        }

        // Idle chain — only while nothing covers the screen; 0 = never dim.
        let idle = now.saturating_duration_since(self.power.last_activity);
        let timeout_secs = self.app_prefs.prefs().screen_timeout_sec;
        let timeout = Duration::from_secs(u64::from(timeout_secs));
        match self.power.phase {
            Phase::Awake if self.locked() && idle.as_millis() as u64 > LOCKED_IDLE_MS => {
                self.power_fade(now);
            }
            Phase::Awake
                if timeout_secs > 0
                    && !self.locked()
                    && !self.panels.shown()
                    && idle >= timeout =>
            {
                self.power.save();
                if let Some(bl) = &self.power.backlight {
                    bl.set((bl.max as f32 * DIM_FRAC) as u32);
                }
                self.power.dim_until = Some(now + Duration::from_millis(DIM_MS));
                self.power.phase = Phase::Dim;
            }
            Phase::Dim if self.power.dim_until.is_some_and(|d| now >= d) => {
                self.power_fade(now);
            }
            Phase::Asleep => {
                if self
                    .power
                    .sleep_touch
                    .is_some_and(|t| now.duration_since(t).as_millis() as u64 > SLEEP_TOUCH_WAKE_MS)
                {
                    self.power.sleep_touch = None;
                    self.power_wake();
                }
            }
            _ => {}
        }

        // Hold-to-confirm on the armed destructive row.
        if let Some((action, since)) = self.power.menu_hold {
            if now.duration_since(since).as_millis() as u64 >= HOLD_MS {
                self.power.menu_hold = None;
                self.power_action(action);
            }
        }

        // Lock raster freshness (clock minute / notifications / torch) and
        // the armed-row progress bar while a destructive row is held.
        if self.locked() && !self.sleeping() {
            self.lock.stale = true;
        }
        if self.power.menu_hold.is_some() {
            self.power.menu_stale = true;
            self.shell_dirty = true;
        }
        self.needs_present = true;
    }

    /// Rasterize the locked-screen power menu (fresh while a row is held).
    pub(crate) fn menu_prepare(&mut self, size: Size) -> Option<&de_api::Pixels> {
        if !self.power.menu_over_lock {
            return None;
        }
        if self.power.menu_stale || self.power.menu_px.is_none() {
            let m = self.power_menu_model();
            let px = de_ui::power::render(size, &self.strings, &de_design::tokens::DEFAULT, &m);
            self.power.menu_px = Some(px);
            self.power.menu_tex = None;
            self.power.menu_stale = false;
        }
        self.power.menu_px.as_ref()
    }

    /// `<action>` from `a21s-power.cmd`.
    fn power_cmd(&mut self, action: &str) {
        match action {
            "toggle" => {
                if self.sleeping() || matches!(self.power.phase, Phase::FadeOut(_)) {
                    self.power_wake();
                } else {
                    self.power_sleep();
                }
            }
            "sleep" => self.power_sleep(),
            "wake" => self.power_wake(),
            "lock" => self.power_lock(),
            "menu" => {
                if self.sleeping() || matches!(self.power.phase, Phase::FadeOut(_)) {
                    self.power_wake();
                }
                if self.locked() {
                    // Long-press combo while locked: menu floats above the lock.
                    self.power.menu_over_lock = true;
                    self.power.menu_stale = true;
                } else {
                    self.nav.current = Screen::Power;
                    self.foreground = crate::state::Foreground::Shell;
                    self.shell_dirty = true;
                }
            }
            "fp-unlock" => {
                self.power_wake();
                self.lock.unlock_now();
            }
            "fp-fail" => {
                if self.locked() {
                    self.lock.fail_shake(Instant::now());
                }
            }
            _ => {}
        }
        self.needs_present = true;
    }

    /// Locked + panel on → start the fade to black, then Asleep.
    fn power_fade(&mut self, now: Instant) {
        if matches!(self.power.phase, Phase::FadeOut(_) | Phase::Asleep) {
            return;
        }
        if self.panels.flashlight() == Some(true) {
            self.panels
                .request_control(de_system::controls::Action::Flashlight(false));
        }
        self.power.save();
        self.lock.open();
        self.power.phase = Phase::FadeOut(now);
        self.needs_present = true;
    }

    /// Single Power press: lock immediately and play the fade → sleep.
    fn power_sleep(&mut self) {
        self.power.menu_over_lock = false;
        self.power_fade(Instant::now());
    }

    /// Lock the screen without sleeping (`lock` action).
    fn power_lock(&mut self) {
        self.lock.open();
        self.needs_present = true;
    }

    /// Panel back on under the lock screen.
    pub(crate) fn power_wake(&mut self) {
        match self.power.phase {
            Phase::Asleep | Phase::FadeOut(_) => {
                self.power.restore();
                self.power.phase = Phase::WakeIn(Instant::now());
                self.lock.open(); // always land on the lock, never on home
                self.needs_present = true;
            }
            Phase::Dim => {
                self.power_note_activity();
            }
            _ => {}
        }
    }

    /// Power-menu row (nav screen or locked overlay), non-destructive rows.
    /// Destructive rows arrive here after the 800 ms hold completes.
    pub(crate) fn power_action(&mut self, action: PowerAction) {
        use std::os::unix::process::CommandExt as _;
        match action {
            PowerAction::Lock => {
                self.power.menu_over_lock = false;
                self.power_lock();
            }
            PowerAction::Screenshot => {
                let _ = Command::new("rabbiz-screenshot")
                    .process_group(0)
                    .spawn();
                self.power.menu_over_lock = false;
                if !self.locked() {
                    self.nav.current = Screen::Home;
                    self.shell_dirty = true;
                }
            }
            PowerAction::SafeMode => {
                let flag = dirs_config().join("safe-mode");
                let _ = std::fs::write(flag, "1\n");
                let _ = Command::new("systemctl").arg("reboot").process_group(0).spawn();
            }
            PowerAction::Restart => {
                let _ = Command::new("systemctl").arg("reboot").process_group(0).spawn();
            }
            PowerAction::PowerOff => {
                let _ = Command::new("systemctl").arg("poweroff").process_group(0).spawn();
            }
        }
        self.power.menu_stale = true;
        self.needs_present = true;
    }

    /// Render state for `de_ui::power` (armed row + 0..1 hold progress).
    pub(crate) fn power_menu_model(&self) -> PowerModel {
        PowerModel {
            armed: self.power.menu_hold.map(|(a, s)| {
                (
                    a,
                    (s.elapsed().as_secs_f32() * 1000.0 / HOLD_MS as f32).min(1.0),
                )
            }),
        }
    }

    /// Touch went down on the (unlocked) power screen — arm a destructive
    /// row if the finger is on one.
    pub(crate) fn power_menu_press(&mut self, pos: de_api::Point) {
        let size = Size::new(self.output.width as i32, self.output.height as i32);
        self.power.menu_hold = de_ui::power::power_hit(size, pos)
            .filter(|a| DESTRUCTIVE.contains(a))
            .map(|a| (a, Instant::now()));
        if self.power.menu_hold.is_some() {
            self.shell_dirty = true;
            self.needs_present = true;
        }
    }

    /// Finger came up (tap or cancel) before the hold completed.
    pub(crate) fn power_menu_release(&mut self) {
        if self.power.menu_hold.is_some() {
            self.power.menu_hold = None;
            self.shell_dirty = true;
            self.needs_present = true;
        }
    }

    /// Tap on the power screen / locked menu overlay: `None` = scrim → close.
    pub(crate) fn power_menu_tap(&mut self, pos: de_api::Point, over_lock: bool) {
        let size = Size::new(self.output.width as i32, self.output.height as i32);
        match de_ui::power::power_hit(size, pos) {
            Some(a) if !DESTRUCTIVE.contains(&a) => {
                if !over_lock {
                    self.nav.back();
                }
                self.power_action(a);
            }
            Some(_) => {} // destructive rows only fire via the hold
            None => {
                if over_lock {
                    self.power.menu_over_lock = false;
                } else {
                    self.nav.back();
                    self.shell_dirty = true;
                }
            }
        }
        self.needs_present = true;
    }
}

fn dirs_config() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("rabbiz-de")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fade_alphas_bookend() {
        let mut p = Power::default();
        let t0 = Instant::now();
        p.phase = Phase::FadeOut(t0);
        assert_eq!(p.fade_alpha(t0), 0.0);
        p.phase = Phase::WakeIn(t0);
        assert_eq!(p.fade_alpha(t0), 1.0);
        p.phase = Phase::Asleep;
        assert_eq!(p.fade_alpha(t0), 0.0);
    }

    #[test]
    fn dim_is_undone_by_activity() {
        let mut p = Power::default();
        p.phase = Phase::Dim;
        assert!(p.note_activity(Instant::now()));
        assert_eq!(p.phase, Phase::Awake);
    }
}
