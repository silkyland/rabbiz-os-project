//! OS-facing helpers for the Settings app: sysfs backlight, `wpctl`
//! volume, `nmcli`/`bluetoothctl` radios. Everything is best-effort —
//! `None`/`false` means "unavailable", never a panic. No new crates:
//! plain `std::process::Command` + sysfs, mirroring `de-system`'s approach.

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use crate::model::parse_volume;

/// Radio toggle kinds reported back over the channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Radio {
    Wifi,
    Bt,
}

/// Run a command, capture stdout, bounded to 3 s (stderr suppressed — it can
/// contain device identities). Returns `None` on spawn failure, non-zero
/// exit, timeout, or non-UTF8 output.
fn run(args: &[&str]) -> Option<String> {
    let mut child = Command::new(args.first()?)
        .args(&args[1..])
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    return None;
                }
                return child
                    .wait_with_output()
                    .ok()
                    .and_then(|o| String::from_utf8(o.stdout).ok());
            }
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(10));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

/// `Child::try_wait` polling bounded to `secs` — for fire-and-forget toggles.
fn status_bounded(child: &mut std::process::Child, secs: u64) -> bool {
    let deadline = Instant::now() + Duration::from_secs(secs);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status.success(),
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(10));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return false;
            }
        }
    }
}

fn read_u32(path: &std::path::Path) -> Option<u32> {
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

// ---------------------------------------------------------------- backlight

/// First usable `/sys/class/backlight/*` dir (sorted, `max > 0`).
fn backlight_dir() -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::fs::read_dir("/sys/class/backlight")
        .ok()?
        .flatten()
        .map(|e| e.path())
        .collect();
    dirs.sort();
    dirs.into_iter()
        .find(|d| read_u32(&d.join("max_brightness")).is_some_and(|m| m > 0))
}

/// `(raw, max)` of the panel backlight, when present.
#[must_use]
pub fn backlight_level() -> Option<(u32, u32)> {
    let d = backlight_dir()?;
    let max = read_u32(&d.join("max_brightness"))?;
    let raw = read_u32(&d.join("brightness")).unwrap_or(max);
    Some((raw, max))
}

/// `$XDG_RUNTIME_DIR/a21s-saved-brightness` — cross-version contract shared
/// with `a21s-power-key` and the DE's power module (name stays `a21s-*`).
fn saved_brightness_path() -> PathBuf {
    let rt = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/run/user/1001"));
    rt.join("a21s-saved-brightness")
}

fn save_brightness(raw: u32) {
    let path = saved_brightness_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, format!("{raw}\n"));
}

/// Write a 0..=1 brightness fraction to the panel (never 0 — the panel
/// would look dead; the power key handles real off). Also persists the raw
/// level for the next wake, per the shared contract.
pub fn set_brightness_frac(f: f32) -> bool {
    let Some(d) = backlight_dir() else {
        return false;
    };
    let Some(max) = read_u32(&d.join("max_brightness")) else {
        return false;
    };
    if max == 0 {
        return false;
    }
    let raw = ((f.clamp(0.0, 1.0) * max as f32).round() as u32).clamp(1, max);
    if std::fs::write(d.join("brightness"), raw.to_string()).is_err() {
        return false;
    }
    save_brightness(raw);
    true
}

// ------------------------------------------------------------------ volume

/// `(fraction, muted)` of the default audio sink via `wpctl`.
#[must_use]
pub fn volume() -> Option<(f32, bool)> {
    parse_volume(&run(&["wpctl", "get-volume", "@DEFAULT_AUDIO_SINK@"])?).map(|(v, m)| {
        // PipeWire reports >1.0 for boosted volumes; clamp to the slider range.
        (v.clamp(0.0, 1.0), m)
    })
}

/// Set absolute volume 0..=1 and unmute (same contract as `de-system`'s
/// `Action::Volume`).
pub fn set_volume_frac(f: f32) -> bool {
    let v = f.clamp(0.0, 1.0);
    let ok = run(&[
        "wpctl",
        "set-volume",
        "@DEFAULT_AUDIO_SINK@",
        &format!("{v:.3}"),
    ])
    .is_some();
    if ok {
        let _ = run(&["wpctl", "set-mute", "@DEFAULT_AUDIO_SINK@", "0"]);
    }
    ok
}

/// Mute/unmute the default audio sink.
pub fn set_muted(muted: bool) -> bool {
    run(&[
        "wpctl",
        "set-mute",
        "@DEFAULT_AUDIO_SINK@",
        if muted { "1" } else { "0" },
    ])
    .is_some()
}

// ------------------------------------------------------------------ radios

/// Wi-Fi radio state via `nmcli radio wifi`.
#[must_use]
pub fn wifi_state() -> Option<bool> {
    match run(&["nmcli", "radio", "wifi"]).as_deref().map(str::trim) {
        Some("enabled") => Some(true),
        Some("disabled") => Some(false),
        _ => None,
    }
}

/// Bluetooth power state via `bluetoothctl show`.
#[must_use]
pub fn bt_state() -> Option<bool> {
    let out = run(&["bluetoothctl", "show"])?;
    out.lines()
        .find_map(|l| l.trim().strip_prefix("Powered:"))
        .map(|v| v.trim() == "yes")
}

/// Poll both radios (blocking; call once at startup or from a thread).
#[must_use]
pub fn poll_radios() -> (Option<bool>, Option<bool>) {
    (wifi_state(), bt_state())
}

/// Toggle a radio off the UI thread, then re-poll and report the *actual*
/// resulting state on `tx` — the switch never shows a state we didn't read
/// back (same rule as `de-system::controls`).
pub fn toggle_radio(r: Radio, on: bool, tx: Sender<(Radio, Option<bool>)>) {
    std::thread::spawn(move || {
        let spawn = |prog: &str, args: &[&str]| {
            Command::new(prog)
                .args(args)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
        };
        match r {
            Radio::Wifi => {
                if let Ok(mut c) = spawn("nmcli", &["radio", "wifi", if on { "on" } else { "off" }])
                {
                    let _ = status_bounded(&mut c, 6);
                }
            }
            Radio::Bt => {
                if let Ok(mut c) = spawn("bluetoothctl", &["power", if on { "on" } else { "off" }])
                {
                    let _ = status_bounded(&mut c, 6);
                }
            }
        }
        // Let NetworkManager / bluetoothd settle, then read the truth.
        std::thread::sleep(Duration::from_millis(500));
        let st = match r {
            Radio::Wifi => wifi_state(),
            Radio::Bt => bt_state(),
        };
        let _ = tx.send((r, st));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn radio_enum_is_sendable_across_threads() {
        let (tx, rx) = std::sync::mpsc::channel::<(Radio, Option<bool>)>();
        tx.send((Radio::Wifi, Some(true))).unwrap();
        assert_eq!(rx.recv().unwrap(), (Radio::Wifi, Some(true)));
    }
}
