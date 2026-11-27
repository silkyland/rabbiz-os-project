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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WifiNet {
    pub ssid: String,
    pub signal: u8,
    pub secured: bool,
    pub active: bool,
    pub saved: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BtDev {
    pub mac: String,
    pub name: String,
    pub connected: bool,
    pub paired: bool,
}

#[derive(Debug, Clone)]
pub enum NetEvent {
    WifiScanned(Vec<WifiNet>, Option<String>),
    WifiAction(String),
    BtScanned(Vec<BtDev>, Option<String>),
    BtAction(String),
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

// ------------------------------------------------------------ Wi-Fi & BT lists

/// Active connected Wi-Fi SSID (fast single nmcli query).
#[must_use]
pub fn active_wifi_ssid() -> Option<String> {
    let out = run(&["nmcli", "-t", "-f", "active,ssid", "dev", "wifi"])?;
    out.lines()
        .find_map(|l| l.strip_prefix("yes:"))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// First connected Bluetooth device name.
#[must_use]
pub fn active_bt_device() -> Option<String> {
    let out = run(&["bluetoothctl", "devices", "Connected"])?;
    out.lines().find_map(|l| {
        let rest = l.trim().strip_prefix("Device ")?;
        let (_, name) = rest.split_once(' ')?;
        let name = name.trim();
        (!name.is_empty()).then(|| name.to_string())
    })
}

/// Parse `nmcli -t -f IN-USE,SSID,SIGNAL,SECURITY dev wifi list` lines.
pub fn parse_wifi_list(raw: &str, saved_names: &std::collections::HashSet<String>) -> Vec<WifiNet> {
    let mut by_ssid: std::collections::BTreeMap<String, WifiNet> = std::collections::BTreeMap::new();
    for line in raw.lines() {
        let mut parts = line.splitn(4, ':');
        let in_use = parts.next().unwrap_or_default().trim() == "*";
        let ssid = parts.next().unwrap_or_default().trim().replace("\\:", ":");
        if ssid.is_empty() {
            continue;
        }
        let signal: u8 = parts.next().unwrap_or("0").trim().parse().unwrap_or(0);
        let sec = parts.next().unwrap_or_default().trim();
        let secured = !sec.is_empty() && sec != "--";
        let saved = saved_names.contains(&ssid);
        by_ssid
            .entry(ssid.clone())
            .and_modify(|e| {
                if in_use {
                    e.active = true;
                }
                if signal > e.signal {
                    e.signal = signal;
                }
                e.secured |= secured;
                e.saved |= saved;
            })
            .or_insert(WifiNet {
                ssid,
                signal,
                secured,
                active: in_use,
                saved,
            });
    }
    let mut nets: Vec<WifiNet> = by_ssid.into_values().collect();
    nets.sort_by(|a, b| {
        b.active
            .cmp(&a.active)
            .then_with(|| b.saved.cmp(&a.saved))
            .then_with(|| b.signal.cmp(&a.signal))
            .then_with(|| a.ssid.to_lowercase().cmp(&b.ssid.to_lowercase()))
    });
    nets.truncate(14);
    nets
}

/// Async Wi-Fi scan (runs on a background thread, posts `NetEvent::WifiScanned`).
pub fn scan_wifi_async(rescan: bool, tx: Sender<NetEvent>) {
    std::thread::spawn(move || {
        let mut saved = std::collections::HashSet::new();
        if let Some(conns) = run(&["nmcli", "-t", "-f", "NAME,TYPE", "connection", "show"]) {
            for line in conns.lines() {
                if let Some((name, kind)) = line.rsplit_once(':') {
                    if kind.contains("wireless") || kind.contains("wifi") {
                        saved.insert(name.replace("\\:", ":"));
                    }
                }
            }
        }
        let mode = if rescan { "yes" } else { "auto" };
        let raw = run(&[
            "nmcli",
            "-t",
            "-f",
            "IN-USE,SSID,SIGNAL,SECURITY",
            "dev",
            "wifi",
            "list",
            "--rescan",
            mode,
        ])
        .unwrap_or_default();
        let nets = parse_wifi_list(&raw, &saved);
        let active = nets.iter().find(|n| n.active).map(|n| n.ssid.clone());
        let _ = tx.send(NetEvent::WifiScanned(nets, active));
    });
}

/// Connect to a Wi-Fi network; NetworkManager persists the password profile.
///
/// When a passphrase is supplied, it is piped over stdin to `rabbiz-wifi-connect`
/// so the PSK never appears in `/proc/<pid>/cmdline`.
pub fn connect_wifi_async(ssid: String, password: Option<String>, tx: Sender<NetEvent>) {
    std::thread::spawn(move || {
        if ssid.is_empty() || ssid.chars().any(char::is_control) {
            let _ = tx.send(NetEvent::WifiAction("Invalid Wi-Fi SSID".into()));
            return;
        }
        let ok = if let Some(pw) = password {
            let mut pw_bytes = pw.into_bytes();
            let res = Command::new("rabbiz-wifi-connect")
                .arg(&ssid)
                .env("LC_ALL", "C")
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .ok()
                .is_some_and(|mut c| {
                    if let Some(mut stdin) = c.stdin.take() {
                        use std::io::Write;
                        let _ = stdin.write_all(&pw_bytes);
                        let _ = stdin.write_all(b"\n");
                        let _ = stdin.flush();
                    }
                    pw_bytes.fill(0);
                    std::hint::black_box(&pw_bytes);
                    status_bounded(&mut c, 45)
                });
            pw_bytes.fill(0);
            std::hint::black_box(&pw_bytes);
            res
        } else {
            let up = Command::new("nmcli")
                .args(["connection", "up", "id", "--", &ssid])
                .env("LC_ALL", "C")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .ok()
                .is_some_and(|mut c| status_bounded(&mut c, 12));
            up || Command::new("nmcli")
                .args(["dev", "wifi", "connect", "--", &ssid])
                .env("LC_ALL", "C")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .ok()
                .is_some_and(|mut c| status_bounded(&mut c, 12))
        };
        let msg = if ok {
            format!("Connected to {ssid}")
        } else {
            format!("Failed to connect to {ssid}")
        };
        let _ = tx.send(NetEvent::WifiAction(msg));
        scan_wifi_async(false, tx);
    });
}

/// Forget a saved Wi-Fi connection profile.
pub fn forget_wifi_async(ssid: String, tx: Sender<NetEvent>) {
    std::thread::spawn(move || {
        if ssid.is_empty() || ssid.chars().any(char::is_control) {
            let _ = tx.send(NetEvent::WifiAction("Invalid Wi-Fi SSID".into()));
            return;
        }
        let ok = run(&["nmcli", "connection", "delete", "id", "--", &ssid]).is_some();
        let msg = if ok {
            format!("Forgot {ssid}")
        } else {
            format!("Could not forget {ssid}")
        };
        let _ = tx.send(NetEvent::WifiAction(msg));
        scan_wifi_async(false, tx);
    });
}

/// Async Bluetooth device discovery + paired/connected list.
pub fn scan_bt_async(discover: bool, tx: Sender<NetEvent>) {
    std::thread::spawn(move || {
        if discover {
            if let Ok(mut c) = Command::new("bluetoothctl")
                .args(["--timeout", "4", "scan", "on"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
            {
                let _ = status_bounded(&mut c, 5);
            }
        }
        let parse_macs = |out: Option<String>| -> std::collections::HashSet<String> {
            out.unwrap_or_default()
                .lines()
                .filter_map(|l| l.trim().strip_prefix("Device "))
                .filter_map(|r| r.split_whitespace().next())
                .map(str::to_string)
                .collect()
        };
        let connected_set = parse_macs(run(&["bluetoothctl", "devices", "Connected"]));
        let paired_set = parse_macs(run(&["bluetoothctl", "devices", "Paired"]));
        let all_raw = run(&["bluetoothctl", "devices"]).unwrap_or_default();
        let mut devs: Vec<BtDev> = all_raw
            .lines()
            .filter_map(|l| {
                let rest = l.trim().strip_prefix("Device ")?;
                let (mac, name) = rest.split_once(' ')?;
                let mac = mac.trim().to_string();
                let name = name.trim().to_string();
                if mac.is_empty() || name.is_empty() {
                    return None;
                }
                Some(BtDev {
                    connected: connected_set.contains(&mac),
                    paired: paired_set.contains(&mac),
                    mac,
                    name,
                })
            })
            .collect();
        devs.sort_by(|a, b| {
            b.connected
                .cmp(&a.connected)
                .then_with(|| b.paired.cmp(&a.paired))
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        devs.truncate(12);
        let active = devs.iter().find(|d| d.connected).map(|d| d.name.clone());
        let _ = tx.send(NetEvent::BtScanned(devs, active));
    });
}

/// Connect/pair or disconnect a Bluetooth device off the UI thread.
pub fn toggle_bt_device_async(
    mac: String,
    name: String,
    currently_connected: bool,
    tx: Sender<NetEvent>,
) {
    std::thread::spawn(move || {
        let msg = if currently_connected {
            let ok = run(&["bluetoothctl", "disconnect", &mac]).is_some();
            if ok {
                format!("Disconnected {name}")
            } else {
                format!("Failed to disconnect {name}")
            }
        } else {
            let _ = run(&["bluetoothctl", "pair", &mac]);
            let _ = run(&["bluetoothctl", "trust", &mac]);
            let ok = Command::new("bluetoothctl")
                .args(["connect", &mac])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .ok()
                .is_some_and(|mut c| status_bounded(&mut c, 10));
            if ok {
                format!("Connected to {name}")
            } else {
                format!("Failed to connect {name}")
            }
        };
        let _ = tx.send(NetEvent::BtAction(msg));
        scan_bt_async(false, tx);
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
