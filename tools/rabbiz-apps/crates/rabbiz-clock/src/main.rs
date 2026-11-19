//! RabbizOS Clock — M3 clock / alarm / timer / stopwatch.
//!
//! Alarm persistence shares the QML app's contract
//! (`~/.local/share/rabbiz-clock/alarms.json`, a JSON array of
//! `{id, hour, minute, enabled, firedDay}`) so both implementations read and
//! write the same file. Writes are atomic (tmp + rename). Alarms ring only
//! while the app is open — the same honest limitation as the QML app — and
//! notify through `notify-send`/`gdbus` → `org.freedesktop.Notifications`.

use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use de_api::{Pixels, Size};
use de_design::motion::{bezier_eval, BEZIER_STANDARD};
use de_design::tokens;
use rabbiz_app_kit::{ui, AppEvent, AppFrame, AppState, Canvas, Rect, TextInputCtl, Theme};
use serde::{Deserialize, Serialize};

const TAB_COUNT: usize = 4;
const TAB_LABELS: [&str; TAB_COUNT] = ["Clock", "Alarm", "Timer", "Stopwatch"];
const TAB_ANIM_S: f32 = 0.24;
const ALERT_SHOW: Duration = Duration::from_secs(8);
const RING_STROKE: f32 = 16.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Clock,
    Alarm,
    Timer,
    Stopwatch,
}

impl Tab {
    fn index(self) -> usize {
        match self {
            Self::Clock => 0,
            Self::Alarm => 1,
            Self::Timer => 2,
            Self::Stopwatch => 3,
        }
    }
    fn from_index(i: usize) -> Self {
        match i {
            1 => Self::Alarm,
            2 => Self::Timer,
            3 => Self::Stopwatch,
            _ => Self::Clock,
        }
    }
}

/// Same on-disk shape as `apps/clock/ClockLogic.js` (`makeAlarm`/`normalizeAlarm`).
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Alarm {
    id: String,
    hour: i32,
    minute: i32,
    enabled: bool,
    #[serde(rename = "firedDay", default)]
    fired_day: String,
}

/// Everything the UI needs from the wall clock, captured once per tick/render.
#[derive(Clone)]
struct Now {
    hour: i32,
    minute: i32,
    wday: i32,
    mon: i32,
    mday: i32,
    year: i32,
    day_key: String,
}

fn local_now() -> Now {
    unsafe {
        let t = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&t, &mut tm);
        Now {
            hour: tm.tm_hour,
            minute: tm.tm_min,
            wday: tm.tm_wday,
            mon: tm.tm_mon,
            mday: tm.tm_mday,
            year: tm.tm_year + 1900,
            day_key: format!(
                "{:04}-{:02}-{:02}",
                tm.tm_year + 1900,
                tm.tm_mon + 1,
                tm.tm_mday
            ),
        }
    }
}

#[cfg(test)]
fn pad2(n: i32) -> String {
    let v = n.rem_euclid(100);
    if v < 10 {
        format!("0{v}")
    } else {
        v.to_string()
    }
}

const WEEKDAYS: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];
const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

fn format_date(n: &Now) -> String {
    let wd = WEEKDAYS
        .get(n.wday.rem_euclid(7) as usize)
        .copied()
        .unwrap_or("");
    let mo = MONTHS
        .get(n.mon.rem_euclid(12) as usize)
        .copied()
        .unwrap_or("");
    format!("{wd} {} {mo}", n.mday)
}

/// Stopwatch display: `MM:SS.cc`, `H:MM:SS.cc` past one hour.
fn format_stopwatch(d: Duration) -> String {
    let total_ms = d.as_millis() as u64;
    let cs = (total_ms / 10) % 100;
    let secs = total_ms / 1000;
    let s = secs % 60;
    let m = (secs / 60) % 60;
    let h = secs / 3600;
    let head = if h > 0 {
        format!("{h}:{:02}:{:02}", m, s)
    } else {
        format!("{:02}:{:02}", m, s)
    };
    format!("{head}.{cs:02}")
}

/// Timer countdown, rounded up so `00:00` only shows when finished.
fn format_timer(d: Duration) -> String {
    let secs = d.as_secs_f64().ceil().max(0.0) as u64;
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m:02}:{s:02}")
    }
}

fn step_wrapped(v: i32, delta: i32, min: i32, max: i32) -> i32 {
    let span = max - min + 1;
    if span <= 0 {
        return min;
    }
    (v - min + delta).rem_euclid(span) + min
}

fn clamp_int(v: i32, min: i32, max: i32) -> i32 {
    v.clamp(min, max)
}

/// Should `alarm` ring at `now`? True during the matching minute of a day it
/// has not rung on yet (mirrors `ClockLogic.alarmDue`).
fn alarm_due(alarm: &Alarm, now: &Now) -> bool {
    alarm.enabled
        && alarm.hour == now.hour
        && alarm.minute == now.minute
        && alarm.fired_day != now.day_key
}

/// Index (into the sorted list) and minutes-ahead of the next enabled alarm.
fn next_alarm(alarms: &[&Alarm], now: &Now) -> Option<(usize, i32)> {
    let now_min = now.hour * 60 + now.minute;
    let mut best: Option<(usize, i32)> = None;
    for (i, a) in alarms.iter().enumerate() {
        if !a.enabled {
            continue;
        }
        let mut ahead = a.hour * 60 + a.minute - now_min;
        if ahead < 0 {
            ahead += 24 * 60;
        }
        if best.is_none_or(|(_, b)| ahead < b) {
            best = Some((i, ahead));
        }
    }
    best
}

fn next_alarm_text(alarms: &[&Alarm], now: &Now) -> Option<String> {
    let (i, ahead) = next_alarm(alarms, now)?;
    let a = alarms[i];
    let when = if ahead == 0 {
        "now".to_string()
    } else if ahead < 60 {
        format!("in {ahead} min")
    } else {
        let h = ahead / 60;
        let m = ahead % 60;
        if m == 0 {
            format!("in {h} h")
        } else {
            format!("in {h} h {m} min")
        }
    };
    Some(format!("Next alarm {:02}:{:02} · {when}", a.hour, a.minute))
}

/// Lap delta for the display row `row` (0 = newest) given cumulative `laps`.
fn lap_delta(laps: &[Duration], row: usize) -> Duration {
    let n = laps.len();
    if n == 0 {
        return Duration::ZERO;
    }
    let i = n - 1 - row.min(n - 1);
    laps[i].saturating_sub(if i == 0 { Duration::ZERO } else { laps[i - 1] })
}

/// Remaining countdown: banked remainder minus the run still in progress.
fn timer_remaining(banked: Duration, started: Option<Instant>) -> Duration {
    match started {
        Some(t) => banked.saturating_sub(t.elapsed()),
        None => banked,
    }
}

/// What a control does — the single source for both hit-testing and drawing.
#[derive(Clone)]
struct Control {
    r: Rect,
    id: u32,
    kind: Kind,
}

#[derive(Clone)]
enum Kind {
    Tab(usize),
    Pill { label: String, primary: bool },
    Switch { on: bool },
    Stepper { dh: i32, dm: i32 },
}

struct TabAnim {
    from: f32,
    to: f32,
    start: Instant,
}

struct Alert {
    title: String,
    body: String,
    since: Instant,
}

struct ClockApp {
    tab: Tab,
    view: (i32, i32),
    pressed: Option<u32>,
    tab_anim: Option<TabAnim>,

    // stopwatch
    sw_running: bool,
    sw_elapsed: Duration,
    sw_started: Option<Instant>,
    laps: Vec<Duration>, // cumulative, oldest → newest

    // timer
    tm_running: bool,
    tm_total: Duration,
    tm_bank: Duration, // remaining while paused / base while running
    tm_started: Option<Instant>,
    tm_done_notified: bool,

    // alarms
    alarms: Vec<Alarm>,
    adding: bool,
    draft_hour: i32,
    draft_minute: i32,
    alert: Option<Alert>,
    last_min_key: i64,
}

impl ClockApp {
    fn new() -> Self {
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
            alarms: Self::load_alarms(),
            adding: false,
            draft_hour: (now.hour + 1).rem_euclid(24),
            draft_minute: 0,
            alert: None,
            last_min_key: -1,
        }
    }

    fn current_sw(&self) -> Duration {
        self.sw_elapsed + self.sw_started.map_or(Duration::ZERO, |t| t.elapsed())
    }

    fn current_tm(&self) -> Duration {
        timer_remaining(self.tm_bank, self.tm_started.filter(|_| self.tm_running))
    }

    fn tm_finished(&self) -> bool {
        self.tm_running && self.current_tm().is_zero()
    }

    // ---------- alarms persistence ----------

    fn data_dir() -> PathBuf {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("rabbiz-clock")
    }

    fn alarms_path() -> PathBuf {
        Self::data_dir().join("alarms.json")
    }

    fn load_alarms() -> Vec<Alarm> {
        let Ok(text) = std::fs::read_to_string(Self::alarms_path()) else {
            return Vec::new();
        };
        let Ok(list) = serde_json::from_str::<Vec<Alarm>>(&text) else {
            return Vec::new();
        };
        // Same normalization rules as ClockLogic.normalizeAlarm.
        let mut out: Vec<Alarm> = list
            .into_iter()
            .filter(|a| (0..=23).contains(&a.hour) && (0..=59).contains(&a.minute))
            .map(|mut a| {
                if a.id.is_empty() {
                    a.id = format!("alarm-{}-{}", a.hour, a.minute);
                }
                a
            })
            .collect();
        out.sort_by_key(|a| (a.hour, a.minute));
        out
    }

    fn save_alarms(&self) {
        let _ = Self::save_alarm_list(&self.alarms);
    }

    /// Atomic tmp+rename write — the single persistence path shared by the GUI,
    /// the `--cli` headless mode and the `--mcp` server, so none of them can
    /// corrupt `alarms.json` for the others.
    fn save_alarm_list(alarms: &[Alarm]) -> Result<(), String> {
        let dir = Self::data_dir();
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
        let json =
            serde_json::to_string_pretty(alarms).map_err(|e| format!("cannot encode: {e}"))?;
        let path = Self::alarms_path();
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, json).map_err(|e| format!("cannot write {}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, &path)
            .map_err(|e| format!("cannot rename {}: {e}", path.display()))?;
        Ok(())
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

    // ---------- layout / controls ----------

    fn tab_bar(w: i32) -> Rect {
        Rect::new(ui::MARGIN, ui::TOP_SAFE + 8, w - ui::MARGIN * 2, 56)
    }

    fn content_top(w: i32) -> i32 {
        Self::tab_bar(w).bottom() + 20
    }

    fn content_bottom(h: i32) -> i32 {
        h - ui::BOTTOM_SAFE - 16
    }

    /// Every tappable control for the current state at size (w,h).
    /// `render` draws from this same list, so hit-testing can never drift
    /// from what's on screen (no hardcoded 720x1600 anywhere).
    fn controls(&self, w: i32, h: i32) -> Vec<Control> {
        let mut out = Vec::new();
        let bar = Self::tab_bar(w);
        let seg = bar.w / TAB_COUNT as i32;
        for i in 0..TAB_COUNT {
            out.push(Control {
                r: Rect::new(bar.x + seg * i as i32, bar.y, seg, bar.h),
                id: i as u32,
                kind: Kind::Tab(i),
            });
        }
        // The alert banner pushes tab content down; controls must match.
        let top = Self::content_top(w) + if self.alert.is_some() { 92 } else { 0 };
        let bottom = Self::content_bottom(h);
        match self.tab {
            Tab::Clock => {}
            Tab::Alarm => {
                // "New alarm" / "Close" pill at the section's top-right.
                let bw = 180;
                out.push(Control {
                    r: Rect::new(w - ui::MARGIN - bw, top, bw, 52),
                    id: 199,
                    kind: Kind::Pill {
                        label: if self.adding {
                            "Close".into()
                        } else {
                            "New alarm".into()
                        },
                        primary: !self.adding,
                    },
                });
                let mut y = top + 68;
                if self.adding {
                    // Add panel card: two stepper columns + save/cancel.
                    let panel = Rect::new(ui::MARGIN, y, w - ui::MARGIN * 2, 360);
                    let col_w = (panel.w - 72) / 2;
                    for col in 0..2 {
                        let cx = panel.x + 24 + col * (col_w + 24);
                        let is_hour = col == 0;
                        // − on the left, + on the right of each column.
                        out.push(Control {
                            r: Rect::new(cx, panel.y + 170, 88, 56),
                            id: 200 + col as u32 * 2,
                            kind: Kind::Stepper {
                                dh: if is_hour { -1 } else { 0 },
                                dm: if is_hour { 0 } else { -1 },
                            },
                        });
                        out.push(Control {
                            r: Rect::new(cx + col_w - 88, panel.y + 170, 88, 56),
                            id: 201 + col as u32 * 2,
                            kind: Kind::Stepper {
                                dh: if is_hour { 1 } else { 0 },
                                dm: if is_hour { 0 } else { 1 },
                            },
                        });
                    }
                    let bw2 = (panel.w - 48 - 16) / 2;
                    out.push(Control {
                        r: Rect::new(panel.x + 24, panel.y + panel.h - 72, bw2, 56),
                        id: 210,
                        kind: Kind::Pill {
                            label: "Save".into(),
                            primary: true,
                        },
                    });
                    out.push(Control {
                        r: Rect::new(panel.x + 24 + bw2 + 16, panel.y + panel.h - 72, bw2, 56),
                        id: 211,
                        kind: Kind::Pill {
                            label: "Cancel".into(),
                            primary: false,
                        },
                    });
                    y += panel.h + 16;
                }
                // Alarm rows: switch toggles, × deletes.
                let sorted = self.sorted_alarms();
                for (i, a) in sorted.iter().enumerate() {
                    if y + 76 > bottom {
                        break;
                    }
                    let r = Rect::new(ui::MARGIN, y, w - ui::MARGIN * 2, 76);
                    out.push(Control {
                        r: Rect::new(r.right() - ui::MARGIN - 52, r.y + 12, 52, 52),
                        id: 300 + i as u32 * 2 + 1,
                        kind: Kind::Pill {
                            label: "×".into(),
                            primary: false,
                        },
                    });
                    out.push(Control {
                        r: Rect::new(r.right() - ui::MARGIN - 52 - 84, r.y + 22, 56, 32),
                        id: 300 + i as u32 * 2,
                        kind: Kind::Switch { on: a.enabled },
                    });
                    y += 88;
                }
            }
            Tab::Timer => {
                let bw = (w - ui::MARGIN * 2 - 48) / 4;
                let cy = bottom - 140;
                for (i, label) in ["+1m", "+5m", "+10m", "Reset"].iter().enumerate() {
                    out.push(Control {
                        r: Rect::new(ui::MARGIN + i as i32 * (bw + 16), cy, bw, 52),
                        id: 20 + i as u32,
                        kind: Kind::Pill {
                            label: label.to_string(),
                            primary: false,
                        },
                    });
                }
                out.push(Control {
                    r: Rect::new(ui::MARGIN, bottom - 72, w - ui::MARGIN * 2, 64),
                    id: 24,
                    kind: Kind::Pill {
                        label: if self.tm_finished() {
                            "Reset".into()
                        } else if self.tm_running {
                            "Pause".into()
                        } else {
                            "Start".into()
                        },
                        primary: true,
                    },
                });
            }
            Tab::Stopwatch => {
                let bw = (w - ui::MARGIN * 2 - 16) / 2;
                out.push(Control {
                    r: Rect::new(ui::MARGIN, bottom - 72, bw, 64),
                    id: 10,
                    kind: Kind::Pill {
                        label: if self.sw_running {
                            "Pause".into()
                        } else {
                            "Start".into()
                        },
                        primary: true,
                    },
                });
                out.push(Control {
                    r: Rect::new(ui::MARGIN + bw + 16, bottom - 72, bw, 64),
                    id: 11,
                    kind: Kind::Pill {
                        label: if self.sw_running {
                            "Lap".into()
                        } else {
                            "Reset".into()
                        },
                        primary: false,
                    },
                });
            }
        }
        out
    }

    /// Alarms sorted by (hour, minute); `controls` row indices refer to this.
    fn sorted_alarms(&self) -> Vec<&Alarm> {
        let mut v: Vec<&Alarm> = self.alarms.iter().collect();
        v.sort_by_key(|a| (a.hour, a.minute));
        v
    }

    fn tab_frac(&self) -> f32 {
        match &self.tab_anim {
            Some(a) => {
                let t = (a.start.elapsed().as_secs_f32() / TAB_ANIM_S).min(1.0);
                let e = bezier_eval(BEZIER_STANDARD, t);
                a.from + (a.to - a.from) * e
            }
            None => self.tab.index() as f32,
        }
    }

    fn tab_anim_active(&self) -> bool {
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

    // ---------- drawing helpers ----------

    fn draw_control(&self, canvas: &mut Canvas, theme: &Theme, c: &Control) {
        let pressed = self.pressed == Some(c.id);
        match &c.kind {
            Kind::Pill { label, primary } => {
                ui::draw_pill_button(canvas, theme, c.r, label, *primary, pressed);
            }
            Kind::Switch { on } => {
                let r = if pressed {
                    Rect::new(c.r.x - 4, c.r.y - 4, c.r.w + 8, c.r.h + 8)
                } else {
                    c.r
                };
                canvas.fill_rounded_rect(r, 16.0, if *on { theme.accent } else { theme.surface2 });
                let kx = if *on { r.x + r.w - 4 - 24 } else { r.x + 4 };
                canvas.fill_rounded_rect(
                    Rect::new(kx, r.y + 4, 24, 24),
                    12.0,
                    if *on {
                        theme.text_on_accent
                    } else {
                        theme.text3
                    },
                );
            }
            Kind::Stepper { dh, dm } => {
                let label = if *dh != 0 {
                    if *dh > 0 {
                        "+1h"
                    } else {
                        "−1h"
                    }
                } else if *dm > 0 {
                    "+1m"
                } else {
                    "−1m"
                };
                ui::draw_pill_button(canvas, theme, c.r, label, false, pressed);
            }
            Kind::Tab(_) => {}
        }
    }

    fn render_clock(&self, canvas: &mut Canvas, theme: &Theme, w: i32, top: i32, bottom: i32) {
        let now = local_now();
        // Big time, in the upper part of the content area.
        let time_rect = Rect::new(
            ui::MARGIN,
            top + (bottom - top) / 5,
            w - ui::MARGIN * 2,
            150,
        );
        ui::draw_centered(
            canvas,
            &format!("{:02}:{:02}", now.hour, now.minute),
            &ui::styled(104.0, 600, theme.text),
            time_rect,
        );
        let date_rect = Rect::new(ui::MARGIN, time_rect.bottom() + 8, w - ui::MARGIN * 2, 44);
        ui::draw_centered(
            canvas,
            &format_date(&now),
            &ui::body(theme.text2),
            date_rect,
        );

        // Next-alarm card pinned above the bottom edge.
        let sorted: Vec<&Alarm> = {
            let mut v: Vec<&Alarm> = self.alarms.iter().collect();
            v.sort_by_key(|a| (a.hour, a.minute));
            v
        };
        let card_h = 88;
        let card = Rect::new(ui::MARGIN, bottom - card_h, w - ui::MARGIN * 2, card_h);
        canvas.fill_rounded_rect(card, tokens::RADIUS_CARD, theme.surface1);
        if let Some(text) = next_alarm_text(&sorted, &now) {
            ui::draw_text(
                canvas,
                &text,
                &ui::label(theme.text),
                card.x + 24,
                card.y + 18,
                Some(card.w - 48),
            );
            ui::draw_text(
                canvas,
                "Alarms ring while Clock is open",
                &ui::caption(theme.text3),
                card.x + 24,
                card.y + 50,
                Some(card.w - 48),
            );
        } else {
            ui::draw_centered(canvas, "No alarms set", &ui::body(theme.text3), card);
        }
    }

    fn render_alarm(
        &self,
        canvas: &mut Canvas,
        theme: &Theme,
        w: i32,
        top: i32,
        bottom: i32,
        controls: &[Control],
    ) {
        // Section label.
        ui::draw_text(
            canvas,
            "ALARMS",
            &ui::styled(18.0, 600, theme.text3),
            ui::MARGIN,
            top,
            Some(w - ui::MARGIN * 2),
        );
        let mut y = top + 68;

        // Add panel.
        if self.adding {
            let panel = Rect::new(ui::MARGIN, y, w - ui::MARGIN * 2, 360);
            canvas.fill_rounded_rect(panel, tokens::RADIUS_CARD, theme.surface1);
            ui::draw_centered(
                canvas,
                "NEW ALARM",
                &ui::styled(18.0, 600, theme.text3),
                Rect::new(panel.x, panel.y + 20, panel.w, 28),
            );
            let col_w = (panel.w - 72) / 2;
            for (col, (label, val)) in [("HOUR", self.draft_hour), ("MINUTE", self.draft_minute)]
                .iter()
                .enumerate()
            {
                let cx = panel.x + 24 + col as i32 * (col_w + 24);
                ui::draw_centered(
                    canvas,
                    label,
                    &ui::styled(18.0, 600, theme.text3),
                    Rect::new(cx, panel.y + 60, col_w, 26),
                );
                ui::draw_centered(
                    canvas,
                    &format!("{:02}", val),
                    &ui::styled(56.0, 600, theme.text),
                    Rect::new(cx, panel.y + 92, col_w, 72),
                );
            }
            y += panel.h + 16;
        }

        // Rows.
        let sorted = self.sorted_alarms();
        if sorted.is_empty() && !self.adding {
            let empty = Rect::new(ui::MARGIN, y + 40, w - ui::MARGIN * 2, 220);
            canvas.fill_rounded_rect(empty, tokens::RADIUS_CARD, theme.surface1);
            ui::draw_centered(
                canvas,
                "No alarms yet",
                &ui::title(theme.text),
                Rect::new(empty.x, empty.y + 56, empty.w, 48),
            );
            ui::draw_centered(
                canvas,
                "Tap New alarm and set a time with the steppers.",
                &ui::body(theme.text3),
                Rect::new(empty.x + 32, empty.y + 116, empty.w - 64, 80),
            );
        }
        for a in &sorted {
            if y + 76 > bottom {
                break;
            }
            let r = Rect::new(ui::MARGIN, y, w - ui::MARGIN * 2, 76);
            canvas.fill_rounded_rect(r, tokens::RADIUS_ROW, theme.surface1);
            let dim = if a.enabled { theme.text } else { theme.text3 };
            ui::draw_text(
                canvas,
                &format!("{:02}:{:02}", a.hour, a.minute),
                &ui::styled(36.0, 600, dim),
                r.x + 24,
                r.y + 16,
                Some(r.w - 220),
            );
            ui::draw_text(
                canvas,
                if a.enabled { "Every day" } else { "Off" },
                &ui::caption(theme.text3),
                r.x + 26,
                r.y + 56,
                Some(r.w - 220),
            );
            y += 88;
        }
        // Controls (switches, ×, panel buttons, New alarm) drawn on top.
        for c in controls {
            self.draw_control(canvas, theme, c);
        }
    }

    fn render_timer(
        &self,
        canvas: &mut Canvas,
        theme: &Theme,
        w: i32,
        top: i32,
        bottom: i32,
        controls: &[Control],
    ) {
        let remaining = self.current_tm();
        let finished = self.tm_finished() || (!self.tm_running && self.tm_bank.is_zero());
        let total = self.tm_total.max(Duration::from_secs(1));
        let frac = if finished {
            0.0
        } else {
            (remaining.as_secs_f64() / total.as_secs_f64()).clamp(0.0, 1.0)
        };

        // Ring centered in the space above the preset row.
        let ring_r = ((w - ui::MARGIN * 2) / 2 - 40).min(((bottom - top - 220) / 2).max(80));
        let cx = w / 2;
        let cy = top + (bottom - top - 220) / 2 + ring_r + 10;
        let ring_mid = ring_r as f32 + RING_STROKE / 2.0;

        // Track + progress arc, generated as premultiplied pixels.
        let track = ring_tile(ring_mid, RING_STROKE, None, theme.surface3);
        let ox = cx - track.size.w / 2;
        let oy = cy - track.size.h / 2;
        canvas.blit(&track, de_api::Point::new(ox, oy));
        let prog_color = if finished { theme.danger } else { theme.accent };
        let prog = ring_tile(ring_mid, RING_STROKE, Some(frac as f32), prog_color);
        canvas.blit(&prog, de_api::Point::new(ox, oy));

        // Time inside the ring.
        let inner = Rect::new(cx - ring_r, cy - ring_r, ring_r * 2, ring_r * 2);
        ui::draw_centered(
            canvas,
            &format_timer(remaining),
            &ui::styled(72.0, 600, if finished { theme.danger } else { theme.text }),
            Rect::new(inner.x, inner.y + inner.h / 2 - 60, inner.w, 120),
        );
        ui::draw_centered(
            canvas,
            &format!("of {}", format_timer(total)),
            &ui::caption(theme.text3),
            Rect::new(inner.x, inner.y + inner.h / 2 + 40, inner.w, 40),
        );
        if finished {
            ui::draw_centered(
                canvas,
                "Time's up",
                &ui::label(theme.danger),
                Rect::new(inner.x, inner.y + inner.h / 2 - 130, inner.w, 40),
            );
        }

        for c in controls {
            self.draw_control(canvas, theme, c);
        }
    }

    fn render_stopwatch(
        &self,
        canvas: &mut Canvas,
        theme: &Theme,
        w: i32,
        top: i32,
        bottom: i32,
        controls: &[Control],
    ) {
        let elapsed = self.current_sw();
        let time_rect = Rect::new(ui::MARGIN, top + 24, w - ui::MARGIN * 2, 140);
        ui::draw_centered(
            canvas,
            &format_stopwatch(elapsed),
            &ui::styled(88.0, 600, theme.text),
            time_rect,
        );

        // Lap rows, newest first, directly under the time.
        let mut y = time_rect.bottom() + 24;
        let row_h = 72;
        let max_rows = ((bottom - 72 - y) / (row_h + 16)).max(0) as usize;
        for row in 0..self.laps.len().min(max_rows) {
            let r = Rect::new(ui::MARGIN, y, w - ui::MARGIN * 2, row_h);
            canvas.fill_rounded_rect(r, tokens::RADIUS_ROW, theme.surface1);
            let lap_no = self.laps.len() - row;
            ui::draw_text(
                canvas,
                &format!("Lap {lap_no}"),
                &ui::label(theme.text2),
                r.x + 24,
                r.y + 14,
                Some(160),
            );
            let delta = lap_delta(&self.laps, row);
            ui::draw_text(
                canvas,
                &format!("+{}", format_stopwatch(delta)),
                &ui::caption(theme.text3),
                r.x + 24,
                r.y + 44,
                Some(220),
            );
            // Total, right-aligned via measured width approximation.
            let total_s = format_stopwatch(self.laps[self.laps.len() - 1 - row]);
            ui::draw_text(
                canvas,
                &total_s,
                &ui::styled(30.0, 600, theme.text),
                r.right() - ui::MARGIN - 260,
                r.y + 20,
                Some(240),
            );
            y += row_h + 16;
        }

        for c in controls {
            self.draw_control(canvas, theme, c);
        }
    }
}

/// Build a square premultiplied-RGBA tile of a ring (track or arc).
/// `frac` = fraction of the circle covered, measured clockwise from 12 o'clock;
/// `None` = full track.
fn ring_tile(radius: f32, stroke: f32, frac: Option<f32>, color: [u8; 4]) -> Pixels {
    let half = (radius + stroke) as i32 + 2;
    let n = half * 2;
    let mut data = vec![0u8; (n * n * 4) as usize];
    let c = half as f32 - 0.5;
    let inner = radius - stroke / 2.0;
    let outer = radius + stroke / 2.0;
    let arc_end = frac.map(|f| f.clamp(0.0, 1.0) * std::f32::consts::TAU);
    for py in 0..n {
        for px in 0..n {
            let dx = px as f32 - c;
            let dy = py as f32 - c;
            let dist = (dx * dx + dy * dy).sqrt();
            // Coverage fades over ~1px at both edges for cheap AA.
            let cov = (outer + 0.6 - dist)
                .min(dist - (inner - 0.6))
                .clamp(0.0, 1.0);
            if cov <= 0.0 {
                continue;
            }
            if let Some(end) = arc_end {
                // Angle clockwise from top: 0 at 12 o'clock, TAU/4 at 3 o'clock.
                let ang = dy.atan2(dx) + std::f32::consts::FRAC_PI_2;
                let pos = ang.rem_euclid(std::f32::consts::TAU);
                if pos > end + 0.01 {
                    continue;
                }
            }
            // Premultiplied: rgb scaled by coverage × source alpha.
            let a = cov * f32::from(color[3]) / 255.0;
            let i = ((py * n + px) * 4) as usize;
            data[i] = (f32::from(color[0]) * a) as u8;
            data[i + 1] = (f32::from(color[1]) * a) as u8;
            data[i + 2] = (f32::from(color[2]) * a) as u8;
            data[i + 3] = (a * 255.0) as u8;
        }
    }
    Pixels::new(Size::new(n, n), data)
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
        let controls = self.controls(w, h);
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
        let size = canvas.size();
        let (w, h) = (size.w, size.h);
        self.view = (w, h);
        let top = Self::content_top(w);
        let bottom = Self::content_bottom(h);
        let controls = self.controls(w, h);

        // ---- segmented tab bar with sliding accent indicator ----
        let bar = Self::tab_bar(w);
        canvas.fill_rounded_rect(bar, 28.0, theme.surface1);
        let seg = bar.w / TAB_COUNT as i32;
        let frac = self.tab_frac();
        if !self.tab_anim_active() {
            self.tab_anim = None;
        }
        let ix = bar.x + (frac * seg as f32) as i32;
        canvas.fill_rounded_rect(
            Rect::new(ix + 6, bar.y + 6, seg - 12, bar.h - 12),
            22.0,
            theme.accent,
        );
        for c in &controls {
            if let Kind::Tab(i) = c.kind {
                let active = i == self.tab.index();
                let fg = if active {
                    theme.text_on_accent
                } else {
                    theme.text2
                };
                let r = if self.pressed == Some(c.id) {
                    Rect::new(c.r.x + 3, c.r.y + 3, c.r.w - 6, c.r.h - 6)
                } else {
                    c.r
                };
                ui::draw_centered(
                    canvas,
                    TAB_LABELS[i],
                    &ui::styled(20.0, if active { 600 } else { 500 }, fg),
                    r,
                );
            }
        }

        // ---- in-app alert banner ----
        let mut content_top = top;
        if let Some(alert) = &self.alert {
            let br = Rect::new(ui::MARGIN, top, w - ui::MARGIN * 2, 76);
            canvas.fill_rounded_rect(br, tokens::RADIUS_ROW, theme.surface2);
            canvas.fill_rounded_rect(Rect::new(br.x, br.y, 6, br.h), 3.0, theme.accent);
            ui::draw_text(
                canvas,
                &alert.title,
                &ui::label(theme.text),
                br.x + 24,
                br.y + 10,
                Some(br.w - 48),
            );
            ui::draw_text(
                canvas,
                &alert.body,
                &ui::caption(theme.text2),
                br.x + 24,
                br.y + 42,
                Some(br.w - 48),
            );
            content_top = br.bottom() + 16;
        }

        match self.tab {
            Tab::Clock => self.render_clock(canvas, theme, w, content_top, bottom),
            Tab::Alarm => self.render_alarm(canvas, theme, w, content_top, bottom, &controls),
            Tab::Timer => self.render_timer(canvas, theme, w, content_top, bottom, &controls),
            Tab::Stopwatch => {
                self.render_stopwatch(canvas, theme, w, content_top, bottom, &controls)
            }
        }
    }
}

// ---------- headless modes (`--cli`, `--mcp`) ----------
//
// Both modes operate on the same `alarms.json` through `ClockApp::load_alarms`
// / `ClockApp::save_alarm_list` — the identical load/normalize and atomic
// tmp+rename write the GUI uses — so GUI and headless clients never corrupt
// each other's file. `firedDay` semantics are untouched: the field is carried
// through as-is and only set to "today" when an alarm is created for the
// minute already in progress (same rule as the GUI's add panel).

/// Load → mutate → sort → atomic save, exactly the GUI's write path.
fn update_alarms(f: impl FnOnce(&mut Vec<Alarm>)) -> Result<(), String> {
    let mut alarms = ClockApp::load_alarms();
    f(&mut alarms);
    alarms.sort_by_key(|a| (a.hour, a.minute));
    ClockApp::save_alarm_list(&alarms)
}

fn parse_hhmm(s: &str) -> Result<(i32, i32), String> {
    let bad = || format!("bad time '{s}' (want HH:MM)");
    let (h, m) = s.split_once(':').ok_or_else(bad)?;
    if m.contains(':') {
        return Err(bad());
    }
    let hour: i32 = h.trim().parse().map_err(|_| bad())?;
    let minute: i32 = m.trim().parse().map_err(|_| bad())?;
    if !(0..=23).contains(&hour) || !(0..=59).contains(&minute) {
        return Err(bad());
    }
    Ok((hour, minute))
}

fn parse_on_off(s: &str) -> Result<bool, String> {
    match s.to_ascii_lowercase().as_str() {
        "on" | "true" | "1" | "enable" | "enabled" => Ok(true),
        "off" | "false" | "0" | "disable" | "disabled" => Ok(false),
        _ => Err(format!("bad state '{s}' (want on|off)")),
    }
}

fn alarm_line(a: &Alarm) -> String {
    let fired = if a.fired_day.is_empty() {
        "-".to_string()
    } else {
        a.fired_day.clone()
    };
    format!(
        "{}\t{:02}:{:02}\t{}\tfiredDay={}",
        a.id,
        a.hour,
        a.minute,
        if a.enabled { "on" } else { "off" },
        fired
    )
}

fn alarms_text() -> Result<String, String> {
    let alarms = ClockApp::load_alarms();
    if alarms.is_empty() {
        return Ok("No alarms.\n".into());
    }
    Ok(alarms.iter().map(alarm_line).collect::<Vec<_>>().join("\n") + "\n")
}

/// Same rules as the GUI add panel: enabled, id `alarm-<millis>`, and
/// `firedDay` preset to today when the time equals the current minute so the
/// new alarm doesn't ring instantly.
fn alarm_add_text(time: &str) -> Result<String, String> {
    let (hour, minute) = parse_hhmm(time)?;
    let now = local_now();
    let mut fired_day = String::new();
    if hour == now.hour && minute == now.minute {
        fired_day = now.day_key.clone();
    }
    let id = format!(
        "alarm-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    );
    update_alarms(|list| {
        list.push(Alarm {
            id: id.clone(),
            hour,
            minute,
            enabled: true,
            fired_day,
        })
    })?;
    Ok(format!("Added alarm {id} at {hour:02}:{minute:02}\n"))
}

fn alarm_remove_text(id: &str) -> Result<String, String> {
    let mut found = false;
    update_alarms(|list| {
        if let Some(pos) = list.iter().position(|a| a.id == id) {
            list.remove(pos);
            found = true;
        }
    })?;
    if found {
        Ok(format!("Removed alarm {id}\n"))
    } else {
        Err(format!("no alarm with id '{id}'"))
    }
}

fn alarm_set_enabled(id: &str, enabled: bool) -> Result<String, String> {
    let mut found = false;
    update_alarms(|list| {
        if let Some(a) = list.iter_mut().find(|a| a.id == id) {
            a.enabled = enabled;
            found = true;
        }
    })?;
    if found {
        Ok(format!(
            "Alarm {id} {}\n",
            if enabled { "enabled" } else { "disabled" }
        ))
    } else {
        Err(format!("no alarm with id '{id}'"))
    }
}

fn alarm_enable_text(id: &str, state: &str) -> Result<String, String> {
    alarm_set_enabled(id, parse_on_off(state)?)
}

fn now_text() -> String {
    let now = local_now();
    let alarms = ClockApp::load_alarms();
    let refs: Vec<&Alarm> = alarms.iter().collect();
    let next = next_alarm_text(&refs, &now).unwrap_or_else(|| "No alarms set".into());
    format!(
        "{:02}:{:02} · {} · {}\n",
        now.hour,
        now.minute,
        format_date(&now),
        next
    )
}

const CLI_USAGE: &str = "usage: rabbiz-clock --cli <command> [args]
commands:
  alarms                       list alarms (id, time, on/off, firedDay)
  alarm-add <HH:MM>            add a daily alarm, enabled
  alarm-remove <id>            delete an alarm
  alarm-enable <id> <on|off>   toggle an alarm
  now                          current time and next alarm
  help                         this text";

/// Headless command dispatch. Returns the text to print on stdout; errors go
/// to stderr with exit code 2. Pure w.r.t. args — file access only through the
/// shared alarm store.
fn cli(args: &[String]) -> Result<String, String> {
    match args.first().map(String::as_str) {
        Some("alarms") | Some("alarm-list") => alarms_text(),
        Some("alarm-add") => {
            let t = args.get(1).ok_or("alarm-add needs <HH:MM>")?;
            alarm_add_text(t)
        }
        Some("alarm-remove") => {
            let id = args.get(1).ok_or("alarm-remove needs <id>")?;
            alarm_remove_text(id)
        }
        Some("alarm-enable") => {
            let id = args.get(1).ok_or("alarm-enable needs <id> <on|off>")?;
            let state = args.get(2).ok_or("alarm-enable needs <id> <on|off>")?;
            alarm_enable_text(id, state)
        }
        Some("now") => Ok(now_text()),
        Some("help") | Some("--help") | Some("-h") => Ok(format!("{CLI_USAGE}\n")),
        Some(other) => Err(format!("unknown command '{other}'\n{CLI_USAGE}")),
        None => Err(CLI_USAGE.to_string()),
    }
}

// ---------- MCP tools ----------

fn mcp_str<'a>(args: &'a serde_json::Value, key: &str) -> Result<&'a str, String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("missing or invalid '{key}'"))
}

fn tool_alarms_list(_args: &serde_json::Value) -> Result<String, String> {
    alarms_text()
}

fn tool_alarm_add(args: &serde_json::Value) -> Result<String, String> {
    alarm_add_text(mcp_str(args, "time")?)
}

fn tool_alarm_remove(args: &serde_json::Value) -> Result<String, String> {
    alarm_remove_text(mcp_str(args, "id")?)
}

fn tool_alarm_enable(args: &serde_json::Value) -> Result<String, String> {
    let id = mcp_str(args, "id")?;
    let enabled = args
        .get("enabled")
        .and_then(|v| v.as_bool())
        .ok_or("missing or invalid 'enabled' (want boolean)")?;
    alarm_set_enabled(id, enabled)
}

fn tool_now(_args: &serde_json::Value) -> Result<String, String> {
    Ok(now_text())
}

static TOOLS: &[rabbiz_app_kit::mcp::Tool] = &[
    rabbiz_app_kit::mcp::Tool {
        name: "alarms_list",
        description: "List all alarms (id, HH:MM, on/off, firedDay).",
        input_schema: r#"{"type":"object","properties":{}}"#,
        handler: tool_alarms_list,
    },
    rabbiz_app_kit::mcp::Tool {
        name: "alarm_add",
        description: "Add a daily alarm at HH:MM (24h), enabled. Same store as the Clock GUI.",
        input_schema: r#"{"type":"object","properties":{"time":{"type":"string","description":"HH:MM, e.g. \"07:30\""}},"required":["time"]}"#,
        handler: tool_alarm_add,
    },
    rabbiz_app_kit::mcp::Tool {
        name: "alarm_remove",
        description: "Delete an alarm by id.",
        input_schema: r#"{"type":"object","properties":{"id":{"type":"string"}},"required":["id"]}"#,
        handler: tool_alarm_remove,
    },
    rabbiz_app_kit::mcp::Tool {
        name: "alarm_enable",
        description: "Enable or disable an alarm by id.",
        input_schema: r#"{"type":"object","properties":{"id":{"type":"string"},"enabled":{"type":"boolean"}},"required":["id","enabled"]}"#,
        handler: tool_alarm_enable,
    },
    rabbiz_app_kit::mcp::Tool {
        name: "now",
        description: "Current local time and the next enabled alarm.",
        input_schema: r#"{"type":"object","properties":{}}"#,
        handler: tool_now,
    },
];

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|a| a == "--mcp") {
        rabbiz_app_kit::mcp::serve(TOOLS, "rabbiz-clock", "0.1.0");
    }
    if args.first().is_some_and(|a| a == "--cli") {
        match cli(&args[1..]) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn now(hour: i32, minute: i32, wday: i32, mon: i32, mday: i32) -> Now {
        Now {
            hour,
            minute,
            wday,
            mon,
            mday,
            year: 2026,
            day_key: format!("2026-{:02}-{:02}", mon + 1, mday),
        }
    }

    fn alarm(hour: i32, minute: i32, enabled: bool, fired: &str) -> Alarm {
        Alarm {
            id: format!("a{hour}-{minute}"),
            hour,
            minute,
            enabled,
            fired_day: fired.to_string(),
        }
    }

    #[test]
    fn pad2_and_dates() {
        assert_eq!(pad2(7), "07");
        assert_eq!(pad2(23), "23");
        assert_eq!(pad2(-3), "97"); // rem_euclid wraps, never negative
        let n = now(14, 41, 5, 10, 21); // Friday 21 November
        assert_eq!(format_date(&n), "Friday 21 November");
    }

    #[test]
    fn stopwatch_format() {
        assert_eq!(format_stopwatch(Duration::from_millis(12_340)), "00:12.34");
        assert_eq!(
            format_stopwatch(Duration::from_secs(3723) + Duration::from_millis(456)),
            "1:02:03.45"
        );
        assert_eq!(format_stopwatch(Duration::ZERO), "00:00.00");
    }

    #[test]
    fn timer_format_rounds_up() {
        assert_eq!(format_timer(Duration::from_millis(999)), "00:01");
        assert_eq!(format_timer(Duration::from_secs(300)), "05:00");
        assert_eq!(format_timer(Duration::from_secs(3661)), "1:01:01");
        assert_eq!(format_timer(Duration::ZERO), "00:00");
    }

    #[test]
    fn steppers_wrap() {
        assert_eq!(step_wrapped(0, -1, 0, 23), 23);
        assert_eq!(step_wrapped(23, 1, 0, 23), 0);
        assert_eq!(step_wrapped(59, 1, 0, 59), 0);
        assert_eq!(step_wrapped(30, 5, 0, 59), 35);
        assert_eq!(clamp_int(99, 0, 23), 23);
    }

    #[test]
    fn alarm_due_rules() {
        let n = now(7, 30, 1, 0, 15);
        assert!(alarm_due(&alarm(7, 30, true, ""), &n));
        assert!(!alarm_due(&alarm(7, 30, true, "2026-01-15"), &n)); // already rang today
        assert!(!alarm_due(&alarm(7, 30, false, ""), &n)); // disabled
        assert!(!alarm_due(&alarm(7, 31, true, ""), &n)); // wrong minute
        assert!(!alarm_due(&alarm(8, 30, true, ""), &n)); // wrong hour
    }

    #[test]
    fn next_alarm_picks_soonest() {
        let n = now(10, 0, 3, 5, 1);
        let alarms = [
            alarm(18, 0, true, ""),
            alarm(6, 30, true, ""),  // tomorrow → 1230 min ahead
            alarm(12, 30, true, ""), // today → 150 min
            alarm(9, 0, false, ""),  // disabled, ignored
        ];
        let refs: Vec<&Alarm> = alarms.iter().collect();
        let (i, ahead) = next_alarm(&refs, &n).unwrap();
        assert_eq!(alarms[i].hour, 12);
        assert_eq!(ahead, 150);
        assert_eq!(
            next_alarm_text(&refs, &n).unwrap(),
            "Next alarm 12:30 · in 2 h 30 min"
        );
        let off = alarm(9, 0, false, "");
        let disabled: Vec<&Alarm> = vec![&off];
        assert!(next_alarm(&disabled, &n).is_none());
    }

    #[test]
    fn lap_deltas() {
        let laps = vec![
            Duration::from_secs(10),
            Duration::from_secs(25),
            Duration::from_secs(30),
        ];
        // row 0 = newest (30s cumulative → +5s), row 2 = oldest (+10s)
        assert_eq!(lap_delta(&laps, 0), Duration::from_secs(5));
        assert_eq!(lap_delta(&laps, 1), Duration::from_secs(15));
        assert_eq!(lap_delta(&laps, 2), Duration::from_secs(10));
        assert_eq!(lap_delta(&[], 0), Duration::ZERO);
    }

    #[test]
    fn timer_remaining_saturates() {
        let bank = Duration::from_secs(60);
        assert_eq!(timer_remaining(bank, None), bank);
        let started = Instant::now() - Duration::from_secs(30);
        assert!(timer_remaining(bank, Some(started)) <= bank);
        let long_ago = Instant::now() - Duration::from_secs(120);
        assert_eq!(timer_remaining(bank, Some(long_ago)), Duration::ZERO);
    }

    #[test]
    fn controls_use_canvas_size_not_hardcoded() {
        let app = ClockApp::new();
        for (w, h) in [(720, 1600), (400, 800), (1080, 2400)] {
            for c in app.controls(w, h) {
                assert!(c.r.w > 0 && c.r.h > 0, "degenerate rect at {w}x{h}");
                assert!(c.r.x >= 0 && c.r.y >= 0);
                assert!(c.r.right() <= w + ui::MARGIN, "control escapes width");
                assert!(c.r.bottom() <= h, "control escapes height at {w}x{h}");
            }
        }
    }

    #[test]
    fn ring_tile_coverage() {
        // Full track: center of a cardinal point is covered, far corner is not.
        let px = ring_tile(50.0, 10.0, None, [255, 0, 0, 255]);
        let at = |x: i32, y: i32| {
            let i = ((y * px.size.w + x) * 4) as usize;
            px.data[i + 3]
        };
        // Tile is 124x124, ring center ≈ (61.5, 61.5), band radius 45..55.
        assert!(at(62, 12) > 200, "top of ring should be covered");
        assert!(at(111, 62) > 200, "right of ring should be covered");
        assert_eq!(at(0, 0), 0, "corner is outside the ring");
        // Quarter arc covers only the first quadrant (top → right, clockwise).
        let q = ring_tile(50.0, 10.0, Some(0.25), [0, 255, 0, 255]);
        let atq = |x: i32, y: i32| {
            let i = ((y * q.size.w + x) * 4) as usize;
            q.data[i + 3]
        };
        assert!(atq(62, 12) > 200, "arc start (top) covered");
        assert!(atq(110, 60) > 200, "arc end (right) covered");
        assert_eq!(atq(12, 62), 0, "left side outside 25% arc");
        assert_eq!(atq(62, 111), 0, "bottom outside 25% arc");
    }

    // ---------- headless CLI ----------

    fn cli_args(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parse_hhmm_accepts_valid_times() {
        assert_eq!(parse_hhmm("07:30"), Ok((7, 30)));
        assert_eq!(parse_hhmm("7:05"), Ok((7, 5)));
        assert_eq!(parse_hhmm("23:59"), Ok((23, 59)));
        assert_eq!(parse_hhmm("00:00"), Ok((0, 0)));
    }

    #[test]
    fn parse_hhmm_rejects_invalid_times() {
        for bad in [
            "", "7", "7:", ":30", "24:00", "12:60", "-1:30", "7:30:00", "aa:bb", "  ",
        ] {
            assert!(parse_hhmm(bad).is_err(), "'{bad}' should fail");
        }
    }

    #[test]
    fn parse_on_off_states() {
        assert_eq!(parse_on_off("on"), Ok(true));
        assert_eq!(parse_on_off("OFF"), Ok(false));
        assert_eq!(parse_on_off("1"), Ok(true));
        assert_eq!(parse_on_off("0"), Ok(false));
        assert!(parse_on_off("maybe").is_err());
    }

    #[test]
    fn cli_help_and_errors() {
        assert!(cli(&cli_args(&["help"])).unwrap().contains("alarm-add"));
        assert!(cli(&[]).is_err());
        assert!(cli(&cli_args(&["bogus"]))
            .unwrap_err()
            .contains("unknown command"));
        assert!(cli(&cli_args(&["alarm-add"])).is_err());
        assert!(cli(&cli_args(&["alarm-enable", "x"])).is_err());
        assert!(cli(&cli_args(&["now"])).is_ok());
    }

    /// End-to-end: CLI writes must land in the same alarms.json the GUI reads,
    /// in the shared schema, via the atomic save path.
    #[test]
    fn cli_alarm_roundtrip() {
        let dir =
            std::env::temp_dir().join(format!("rabbiz-clock-cli-test-{}", std::process::id()));
        std::env::set_var("XDG_DATA_HOME", &dir);
        let _guard = scopeguard(&dir);

        // add → list → toggle → remove, all through `cli()`
        let out = cli(&cli_args(&["alarm-add", "07:30"])).unwrap();
        assert!(out.contains("07:30"));
        let list = cli(&cli_args(&["alarms"])).unwrap();
        assert!(list.contains("07:30") && list.contains("\ton\t"));
        let id = ClockApp::load_alarms()[0].id.clone();

        cli(&cli_args(&["alarm-enable", &id, "off"])).unwrap();
        let a = &ClockApp::load_alarms()[0];
        assert!(!a.enabled, "toggle must persist through save/load");

        cli(&cli_args(&["alarm-remove", &id])).unwrap();
        assert!(ClockApp::load_alarms().is_empty());
        assert!(cli(&cli_args(&["alarm-remove", &id])).is_err());

        // File on disk is the shared schema: array of {id,hour,minute,enabled,firedDay}.
        cli(&cli_args(&["alarm-add", "23:45"])).unwrap();
        let raw = std::fs::read_to_string(dir.join("rabbiz-clock/alarms.json")).unwrap();
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&raw).unwrap();
        assert_eq!(parsed.len(), 1);
        let a = &parsed[0];
        for key in ["id", "hour", "minute", "enabled", "firedDay"] {
            assert!(a.get(key).is_some(), "missing key {key} in {raw}");
        }
        assert_eq!(a["hour"], 23);
        assert_eq!(a["minute"], 45);
        assert_eq!(a["enabled"], true);

        // `firedDay` preset rule: adding an alarm for the current minute marks
        // it fired today (same as the GUI add panel) so it doesn't ring
        // instantly. NOTE: this test owns XDG_DATA_HOME for the whole binary —
        // keep it a single test so parallel tests can't swap the env var.
        let now = local_now();
        let time = format!("{:02}:{:02}", now.hour, now.minute);
        cli(&cli_args(&["alarm-add", &time])).unwrap();
        let fired = ClockApp::load_alarms()
            .into_iter()
            .find(|a| a.hour == now.hour && a.minute == now.minute)
            .unwrap();
        assert_eq!(fired.fired_day, now.day_key);
        assert!(
            !alarm_due(&fired, &now),
            "must not ring the minute it was added"
        );
    }

    struct Cleanup(PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn scopeguard(dir: &Path) -> Cleanup {
        Cleanup(dir.to_path_buf())
    }
}
