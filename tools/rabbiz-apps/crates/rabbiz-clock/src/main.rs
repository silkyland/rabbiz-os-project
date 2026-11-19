//! RabbizOS Clock — M3 clock / alarm / timer / stopwatch.
//!
//! Alarm persistence shares the QML app's contract
//! (`~/.local/share/rabbiz-clock/alarms.json`, a JSON array of
//! `{id, hour, minute, enabled, firedDay}`) so both implementations read and
//! write the same file. Writes are atomic (tmp + rename). Alarms ring only
//! while the app is open — the same honest limitation as the QML app — and
//! notify through `notify-send`/`gdbus` → `org.freedesktop.Notifications`.

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use de_design::motion::{bezier_eval, BEZIER_STANDARD};
use rabbiz_app_kit::{AppEvent, AppFrame, AppState, Canvas, TextInputCtl, Theme};

mod cli;
mod mcp;
mod model;
mod ui;

use model::{
    alarm_due, clamp_int, load_alarms, local_now, save_alarm_list, step_wrapped, timer_remaining,
    Alarm, Alert, Now, Tab, TabAnim, ALERT_SHOW, TAB_ANIM_S,
};

pub(crate) struct ClockApp {
    pub(crate) tab: Tab,
    pub(crate) view: (i32, i32),
    pub(crate) pressed: Option<u32>,
    pub(crate) tab_anim: Option<TabAnim>,

    // stopwatch
    pub(crate) sw_running: bool,
    sw_elapsed: Duration,
    sw_started: Option<Instant>,
    pub(crate) laps: Vec<Duration>, // cumulative, oldest → newest

    // timer
    pub(crate) tm_running: bool,
    pub(crate) tm_total: Duration,
    pub(crate) tm_bank: Duration, // remaining while paused / base while running
    tm_started: Option<Instant>,
    tm_done_notified: bool,

    // alarms
    pub(crate) alarms: Vec<Alarm>,
    pub(crate) adding: bool,
    pub(crate) draft_hour: i32,
    pub(crate) draft_minute: i32,
    pub(crate) alert: Option<Alert>,
    last_min_key: i64,
}

impl ClockApp {
    pub(crate) fn new() -> Self {
        let now = local_now();
        Self {
            tab: Tab::Clock,
            view: (720, 1600),
            pressed: None,
            tab_anim: None,
            sw_running: false,
            sw_elapsed: Duration::ZERO,
            sw_started: None,
            laps: Vec::new(),
            tm_running: false,
            tm_total: Duration::from_secs(300),
            tm_bank: Duration::from_secs(300),
            tm_started: None,
            tm_done_notified: false,
            alarms: load_alarms(),
            adding: false,
            draft_hour: (now.hour + 1).rem_euclid(24),
            draft_minute: 0,
            alert: None,
            last_min_key: -1,
        }
    }

    pub(crate) fn current_sw(&self) -> Duration {
        self.sw_elapsed + self.sw_started.map_or(Duration::ZERO, |t| t.elapsed())
    }

    pub(crate) fn current_tm(&self) -> Duration {
        timer_remaining(self.tm_bank, self.tm_started.filter(|_| self.tm_running))
    }

    pub(crate) fn tm_finished(&self) -> bool {
        self.tm_running && self.current_tm().is_zero()
    }

    fn save_alarms(&self) {
        let _ = save_alarm_list(&self.alarms);
    }

    // ---------- notifications ----------

    /// Fire-and-forget desktop notification via the session bus
    /// (`org.freedesktop.Notifications`, served by de-system). Tries
    /// `notify-send` first (what the QML app uses), falls back to `gdbus`.
    fn notify(title: &str, body: &str) {
        let ok = std::process::Command::new("notify-send")
            .args(["-a", "Clock", title, body])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if ok {
            return;
        }
        let _ = std::process::Command::new("gdbus")
            .args([
                "call",
                "--session",
                "-d",
                "org.freedesktop.Notifications",
                "-o",
                "/org/freedesktop/Notifications",
                "-m",
                "org.freedesktop.Notifications.Notify",
                "rabbiz-clock",
                "0",
                "",
                title,
                body,
                "[]",
                "{'urgency': <2>}",
                "0",
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }

    fn raise_alert(&mut self, title: &str, body: &str) {
        self.alert = Some(Alert {
            title: title.to_string(),
            body: body.to_string(),
            since: Instant::now(),
        });
    }

    fn check_alarms(&mut self, now: &Now) {
        let minute_key = now.year as i64 * 525_600
            + now.wday as i64 * 1440
            + now.hour as i64 * 60
            + now.minute as i64;
        if minute_key == self.last_min_key {
            return;
        }
        self.last_min_key = minute_key;
        // Indices of alarms due right now (immutable pass first).
        let due: Vec<usize> = self
            .alarms
            .iter()
            .enumerate()
            .filter(|(_, a)| alarm_due(a, now))
            .map(|(i, _)| i)
            .collect();
        if due.is_empty() {
            return;
        }
        let times: Vec<String> = due
            .iter()
            .map(|&i| {
                let a = &self.alarms[i];
                format!("{:02}:{:02}", a.hour, a.minute)
            })
            .collect();
        // `fired_day` needs &mut — apply after collecting the display strings.
        for &i in &due {
            self.alarms[i].fired_day = now.day_key.clone();
        }
        for t in &times {
            Self::notify("Alarm", &format!("{t} — ringing now"));
            self.raise_alert("Alarm", &format!("{t} is ringing"));
        }
        self.save_alarms();
    }

    /// Alarms sorted by (hour, minute); `controls` row indices refer to this.
    pub(crate) fn sorted_alarms(&self) -> Vec<&Alarm> {
        let mut v: Vec<&Alarm> = self.alarms.iter().collect();
        v.sort_by_key(|a| (a.hour, a.minute));
        v
    }

    pub(crate) fn tab_frac(&self) -> f32 {
        match &self.tab_anim {
            Some(a) => {
                let t = (a.start.elapsed().as_secs_f32() / TAB_ANIM_S).min(1.0);
                let e = bezier_eval(BEZIER_STANDARD, t);
                a.from + (a.to - a.from) * e
            }
            None => self.tab.index() as f32,
        }
    }

    pub(crate) fn tab_anim_active(&self) -> bool {
        self.tab_anim
            .as_ref()
            .is_some_and(|a| a.start.elapsed().as_secs_f32() < TAB_ANIM_S)
    }

    fn set_tab(&mut self, idx: usize) {
        let next = Tab::from_index(idx);
        if next == self.tab {
            return;
        }
        let cur = self.tab_frac();
        self.tab_anim = Some(TabAnim {
            from: cur,
            to: idx as f32,
            start: Instant::now(),
        });
        self.tab = next;
    }

    fn dispatch(&mut self, id: u32) {
        match id {
            0..=3 => self.set_tab(id as usize),
            // stopwatch
            10 => {
                if self.sw_running {
                    self.sw_elapsed = self.current_sw();
                    self.sw_started = None;
                    self.sw_running = false;
                } else {
                    self.sw_started = Some(Instant::now());
                    self.sw_running = true;
                }
            }
            11 => {
                if self.sw_running {
                    self.laps.push(self.current_sw());
                    if self.laps.len() > 999 {
                        self.laps.remove(0);
                    }
                } else {
                    self.sw_elapsed = Duration::ZERO;
                    self.sw_started = None;
                    self.laps.clear();
                }
            }
            // timer presets / reset / start
            20..=22 => {
                let add = match id {
                    20 => 60,
                    21 => 300,
                    _ => 600,
                };
                let d = Duration::from_secs(add);
                self.tm_total += d;
                self.tm_bank += d;
                self.tm_done_notified = false;
            }
            23 => {
                self.tm_running = false;
                self.tm_started = None;
                self.tm_bank = self.tm_total;
                self.tm_done_notified = false;
            }
            24 => {
                if self.tm_finished() {
                    self.dispatch(23);
                } else if self.tm_running {
                    self.tm_bank = self.current_tm();
                    self.tm_started = None;
                    self.tm_running = false;
                } else if self.tm_bank > Duration::ZERO {
                    self.tm_started = Some(Instant::now());
                    self.tm_running = true;
                    self.tm_done_notified = false;
                }
            }
            // alarm add panel
            199 => self.adding = !self.adding,
            200..=203 => {
                let (dh, dm) = match id {
                    200 => (-1, 0),
                    201 => (1, 0),
                    202 => (0, -1),
                    _ => (0, 1),
                };
                if dh != 0 {
                    self.draft_hour = step_wrapped(self.draft_hour, dh, 0, 23);
                }
                if dm != 0 {
                    self.draft_minute = step_wrapped(self.draft_minute, dm, 0, 59);
                }
            }
            210 => {
                let now = local_now();
                let mut fired_day = String::new();
                // Don't ring instantly for the minute that is already running.
                if self.draft_hour == now.hour && self.draft_minute == now.minute {
                    fired_day = now.day_key.clone();
                }
                let id = format!(
                    "alarm-{}",
                    SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map(|d| d.as_millis())
                        .unwrap_or(0)
                );
                self.alarms.push(Alarm {
                    id,
                    hour: clamp_int(self.draft_hour, 0, 23),
                    minute: clamp_int(self.draft_minute, 0, 59),
                    enabled: true,
                    fired_day,
                });
                self.alarms.sort_by_key(|a| (a.hour, a.minute));
                self.save_alarms();
                self.adding = false;
            }
            211 => self.adding = false,
            // alarm rows: even id = switch toggle, odd id = delete
            300..=599 => {
                let i = ((id - 300) / 2) as usize;
                let target = self.sorted_alarms().get(i).map(|a| a.id.clone());
                let Some(target) = target else { return };
                if id.is_multiple_of(2) {
                    if let Some(a) = self.alarms.iter_mut().find(|a| a.id == target) {
                        a.enabled = !a.enabled;
                    }
                } else if let Some(pos) = self.alarms.iter().position(|a| a.id == target) {
                    self.alarms.remove(pos);
                }
                self.save_alarms();
            }
            _ => {}
        }
    }
}

impl AppState for ClockApp {
    fn app_id(&self) -> &'static str {
        "rabbiz-clock"
    }
    fn title(&self) -> &'static str {
        "Clock"
    }

    fn handle_event(&mut self, event: AppEvent, _ti: &TextInputCtl) -> AppFrame {
        let (w, h) = self.view;
        let controls = ui::controls(self, w, h);
        let animating = self.sw_running
            || self.tm_running
            || self.tab_anim_active()
            || self.alert.is_some()
            || self.tab == Tab::Clock;
        match event {
            AppEvent::TouchDown { pos, .. } => {
                self.pressed = controls.iter().find(|c| c.r.contains(pos)).map(|c| c.id);
                AppFrame {
                    dirty: true,
                    animating,
                    quit: false,
                }
            }
            AppEvent::TouchUp { pos, .. } => {
                let hit = controls
                    .iter()
                    .find(|c| c.r.contains(pos) && Some(c.id) == self.pressed)
                    .map(|c| c.id);
                self.pressed = None;
                if let Some(id) = hit {
                    self.dispatch(id);
                }
                AppFrame {
                    dirty: true,
                    animating,
                    quit: false,
                }
            }
            AppEvent::Tick(_dt) => {
                let now = local_now();
                self.check_alarms(&now);
                if self.tm_running && self.current_tm().is_zero() {
                    self.tm_bank = Duration::ZERO;
                    self.tm_started = None;
                    self.tm_running = false;
                    if !self.tm_done_notified {
                        self.tm_done_notified = true;
                        Self::notify("Timer", "Time is up");
                        self.raise_alert("Timer", "Time is up");
                    }
                }
                if let Some(a) = &self.alert {
                    if a.since.elapsed() >= ALERT_SHOW {
                        self.alert = None;
                    }
                }
                // The big clock only needs a repaint when the minute rolls over.
                let minute_key = now.year as i64 * 525_600
                    + now.wday as i64 * 1440
                    + now.hour as i64 * 60
                    + now.minute as i64;
                let dirty = self.tab != Tab::Clock || minute_key != self.last_min_key;
                self.last_min_key = minute_key;
                AppFrame {
                    dirty,
                    animating,
                    quit: false,
                }
            }
            _ => AppFrame {
                dirty: false,
                animating,
                quit: false,
            },
        }
    }

    fn render(&mut self, canvas: &mut Canvas, theme: &Theme) {
        ui::render(self, canvas, theme);
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|a| a == "--mcp") {
        mcp::serve();
    }
    if args.first().is_some_and(|a| a == "--cli") {
        match cli::run(&args[1..]) {
            Ok(out) => print!("{out}"),
            Err(e) => {
                eprintln!("rabbiz-clock: {e}");
                std::process::exit(2);
            }
        }
        return;
    }
    rabbiz_app_kit::run_app(ClockApp::new());
}
