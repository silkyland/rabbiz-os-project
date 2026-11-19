//! Headless CLI for `rabbiz-clock --cli <command> [args]`.
//!
//! Operates on the same `alarms.json` through `model::load_alarms` /
//! `model::update_alarms` — the identical load/normalize and atomic
//! tmp+rename write the GUI uses — so GUI and headless clients never corrupt
//! each other's file. `firedDay` semantics are untouched: the field is carried
//! through as-is and only set to "today" when an alarm is created for the
//! minute already in progress (same rule as the GUI's add panel).

use std::time::{SystemTime, UNIX_EPOCH};

use crate::model::{format_date, load_alarms, local_now, next_alarm_text, update_alarms, Alarm};

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

pub(crate) fn alarms_text() -> Result<String, String> {
    let alarms = load_alarms();
    if alarms.is_empty() {
        return Ok("No alarms.\n".into());
    }
    Ok(alarms.iter().map(alarm_line).collect::<Vec<_>>().join("\n") + "\n")
}

/// Same rules as the GUI add panel: enabled, id `alarm-<millis>`, and
/// `firedDay` preset to today when the time equals the current minute so the
/// new alarm doesn't ring instantly.
pub(crate) fn alarm_add_text(time: &str) -> Result<String, String> {
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

pub(crate) fn alarm_remove_text(id: &str) -> Result<String, String> {
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

pub(crate) fn alarm_set_enabled(id: &str, enabled: bool) -> Result<String, String> {
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

pub(crate) fn now_text() -> String {
    let now = local_now();
    let alarms = load_alarms();
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
pub(crate) fn run(args: &[String]) -> Result<String, String> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::alarm_due;
    use std::path::{Path, PathBuf};

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
        assert!(run(&cli_args(&["help"])).unwrap().contains("alarm-add"));
        assert!(run(&[]).is_err());
        assert!(run(&cli_args(&["bogus"]))
            .unwrap_err()
            .contains("unknown command"));
        assert!(run(&cli_args(&["alarm-add"])).is_err());
        assert!(run(&cli_args(&["alarm-enable", "x"])).is_err());
        assert!(run(&cli_args(&["now"])).is_ok());
    }

    /// End-to-end: CLI writes must land in the same alarms.json the GUI reads,
    /// in the shared schema, via the atomic save path.
    #[test]
    fn cli_alarm_roundtrip() {
        let dir =
            std::env::temp_dir().join(format!("rabbiz-clock-cli-test-{}", std::process::id()));
        std::env::set_var("XDG_DATA_HOME", &dir);
        let _guard = scopeguard(&dir);

        // add → list → toggle → remove, all through `run()`
        let out = run(&cli_args(&["alarm-add", "07:30"])).unwrap();
        assert!(out.contains("07:30"));
        let list = run(&cli_args(&["alarms"])).unwrap();
        assert!(list.contains("07:30") && list.contains("\ton\t"));
        let id = load_alarms()[0].id.clone();

        run(&cli_args(&["alarm-enable", &id, "off"])).unwrap();
        let a = &load_alarms()[0];
        assert!(!a.enabled, "toggle must persist through save/load");

        run(&cli_args(&["alarm-remove", &id])).unwrap();
        assert!(load_alarms().is_empty());
        assert!(run(&cli_args(&["alarm-remove", &id])).is_err());

        // File on disk is the shared schema: array of {id,hour,minute,enabled,firedDay}.
        run(&cli_args(&["alarm-add", "23:45"])).unwrap();
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
        run(&cli_args(&["alarm-add", &time])).unwrap();
        let fired = load_alarms()
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
