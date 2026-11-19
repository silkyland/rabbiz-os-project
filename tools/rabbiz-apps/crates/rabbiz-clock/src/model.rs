//! Pure clock/alarm/timer/stopwatch model, formatting, and alarm persistence.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

pub(crate) const TAB_COUNT: usize = 4;
pub(crate) const TAB_LABELS: [&str; TAB_COUNT] = ["Clock", "Alarm", "Timer", "Stopwatch"];
pub(crate) const TAB_ANIM_S: f32 = 0.24;
pub(crate) const ALERT_SHOW: Duration = Duration::from_secs(8);

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tab {
    Clock,
    Alarm,
    Timer,
    Stopwatch,
}

impl Tab {
    pub(crate) fn index(self) -> usize {
        match self {
            Self::Clock => 0,
            Self::Alarm => 1,
            Self::Timer => 2,
            Self::Stopwatch => 3,
        }
    }
    pub(crate) fn from_index(i: usize) -> Self {
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
pub(crate) struct Alarm {
    pub(crate) id: String,
    pub(crate) hour: i32,
    pub(crate) minute: i32,
    pub(crate) enabled: bool,
    #[serde(rename = "firedDay", default)]
    pub(crate) fired_day: String,
}

/// Everything the UI needs from the wall clock, captured once per tick/render.
#[derive(Clone)]
pub(crate) struct Now {
    pub(crate) hour: i32,
    pub(crate) minute: i32,
    pub(crate) wday: i32,
    pub(crate) mon: i32,
    pub(crate) mday: i32,
    pub(crate) year: i32,
    pub(crate) day_key: String,
}

pub(crate) struct TabAnim {
    pub(crate) from: f32,
    pub(crate) to: f32,
    pub(crate) start: Instant,
}

pub(crate) struct Alert {
    pub(crate) title: String,
    pub(crate) body: String,
    pub(crate) since: Instant,
}

pub(crate) fn local_now() -> Now {
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

pub(crate) fn format_date(n: &Now) -> String {
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
pub(crate) fn format_stopwatch(d: Duration) -> String {
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
pub(crate) fn format_timer(d: Duration) -> String {
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

pub(crate) fn step_wrapped(v: i32, delta: i32, min: i32, max: i32) -> i32 {
    let span = max - min + 1;
    if span <= 0 {
        return min;
    }
    (v - min + delta).rem_euclid(span) + min
}

pub(crate) fn clamp_int(v: i32, min: i32, max: i32) -> i32 {
    v.clamp(min, max)
}

/// Should `alarm` ring at `now`? True during the matching minute of a day it
/// has not rung on yet (mirrors `ClockLogic.alarmDue`).
pub(crate) fn alarm_due(alarm: &Alarm, now: &Now) -> bool {
    alarm.enabled
        && alarm.hour == now.hour
        && alarm.minute == now.minute
        && alarm.fired_day != now.day_key
}

/// Index (into the sorted list) and minutes-ahead of the next enabled alarm.
pub(crate) fn next_alarm(alarms: &[&Alarm], now: &Now) -> Option<(usize, i32)> {
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

pub(crate) fn next_alarm_text(alarms: &[&Alarm], now: &Now) -> Option<String> {
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
pub(crate) fn lap_delta(laps: &[Duration], row: usize) -> Duration {
    let n = laps.len();
    if n == 0 {
        return Duration::ZERO;
    }
    let i = n - 1 - row.min(n - 1);
    laps[i].saturating_sub(if i == 0 { Duration::ZERO } else { laps[i - 1] })
}

/// Remaining countdown: banked remainder minus the run still in progress.
pub(crate) fn timer_remaining(banked: Duration, started: Option<Instant>) -> Duration {
    match started {
        Some(t) => banked.saturating_sub(t.elapsed()),
        None => banked,
    }
}

// ---------- alarms persistence ----------

pub(crate) fn data_dir() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("rabbiz-clock")
}

pub(crate) fn alarms_path() -> PathBuf {
    data_dir().join("alarms.json")
}

pub(crate) fn load_alarms() -> Vec<Alarm> {
    let Ok(text) = std::fs::read_to_string(alarms_path()) else {
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

/// Atomic tmp+rename write — the single persistence path shared by the GUI,
/// the `--cli` headless mode and the `--mcp` server, so none of them can
/// corrupt `alarms.json` for the others.
pub(crate) fn save_alarm_list(alarms: &[Alarm]) -> Result<(), String> {
    let dir = data_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    let json = serde_json::to_string_pretty(alarms).map_err(|e| format!("cannot encode: {e}"))?;
    let path = alarms_path();
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json).map_err(|e| format!("cannot write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("cannot rename {}: {e}", path.display()))?;
    Ok(())
}

/// Load → mutate → sort → atomic save, exactly the GUI's write path.
pub(crate) fn update_alarms(f: impl FnOnce(&mut Vec<Alarm>)) -> Result<(), String> {
    let mut alarms = load_alarms();
    f(&mut alarms);
    alarms.sort_by_key(|a| (a.hour, a.minute));
    save_alarm_list(&alarms)
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
