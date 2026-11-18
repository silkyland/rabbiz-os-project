//! Quick-control worker. Slow CLI/D-Bus work never runs on the frame thread.
//! State is read back after every action; a failed write never becomes a fake
//! enabled tile. USB networking is left to networkd (only Wi-Fi radio is set).
use std::{path::{Path, PathBuf}, process::{Command, Stdio}, sync::{mpsc, Arc, Mutex}, time::{Duration, Instant}};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snapshot {
    pub wifi: Option<bool>,
    pub bluetooth: Option<bool>,
    pub airplane: Option<bool>,
    pub flashlight: Option<bool>,
    pub battery_saver: Option<bool>,
    pub brightness: Option<f32>,
    pub volume: Option<f32>,
    /// Stable control ID; never includes command output or personal data.
    pub error: Option<String>,
    pub pending: bool,
}
#[derive(Clone, Debug)]
pub enum Action { Wifi(bool), Bluetooth(bool), Airplane(bool), Flashlight(bool), BatterySaver(bool), Brightness(f32), Volume(f32) }
impl Action { fn id(&self) -> &'static str { match self {
    Self::Wifi(_) => "wifi", Self::Bluetooth(_) => "bluetooth", Self::Airplane(_) => "airplane",
    Self::Flashlight(_) => "flashlight", Self::BatterySaver(_) => "battery_saver",
    Self::Brightness(_) => "brightness", Self::Volume(_) => "volume",
} } }

pub struct Controls { tx: mpsc::SyncSender<Action>, state: Arc<Mutex<Snapshot>> }
impl Controls {
    pub fn start() -> Self {
        let (tx, rx) = mpsc::sync_channel::<Action>(8);
        let state = Arc::new(Mutex::new(Snapshot::default()));
        let shared = state.clone();
        std::thread::spawn(move || {
            let mut saved_radios = None;
            let mut error = None;
            loop {
                let next = rx.recv_timeout(Duration::from_secs(2));
                match next {
                    Ok(action) => {
                        let id = action.id();
                        shared.lock().unwrap_or_else(|e| e.into_inner()).pending = true;
                        error = apply(&action, &mut saved_radios).err().map(|_| id.to_owned());
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {},
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
                let mut value = poll();
                value.error = error.clone();
                *shared.lock().unwrap_or_else(|e| e.into_inner()) = value;
            }
        });
        Self { tx, state }
    }
    pub fn snapshot(&self) -> Snapshot { self.state.lock().unwrap_or_else(|e| e.into_inner()).clone() }
    pub fn request(&self, action: Action) -> bool { self.tx.try_send(action).is_ok() }
}

/// Bound process duration, suppress stderr (may include a network identity).
fn output(program: &str, args: &[&str]) -> Result<String, ()> {
    let mut child = Command::new(program).args(args).env("LC_ALL", "C")
        .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().map_err(|_| ())?;
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() { return Err(()); }
                let out = child.wait_with_output().map_err(|_| ())?;
                return String::from_utf8(out.stdout).map_err(|_| ());
            },
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
            _ => { let _ = child.kill(); let _ = child.wait(); return Err(()); }
        }
    }
}
fn read_u32(path: &Path) -> Option<u32> { std::fs::read_to_string(path).ok()?.trim().parse().ok() }
fn devices(root: &str) -> Vec<PathBuf> {
    let mut paths: Vec<_> = std::fs::read_dir(root).into_iter().flatten().flatten().map(|e| e.path()).collect();
    paths.sort(); paths
}
fn backlight() -> Option<PathBuf> { devices("/sys/class/backlight").into_iter().find(|p| read_u32(&p.join("max_brightness")).is_some_and(|v| v > 0)) }
fn torch() -> Option<PathBuf> { devices("/sys/class/leds").into_iter().find(|p| p.file_name().is_some_and(|n| n.to_string_lossy().contains("torch"))) }
fn rfkill(kind: &str) -> Option<bool> {
    let values: Vec<_> = devices("/sys/class/rfkill").iter().filter(|p|
        std::fs::read_to_string(p.join("type")).is_ok_and(|s| s.trim() == kind))
        .filter_map(|p| Some(read_u32(&p.join("soft"))? != 0 || read_u32(&p.join("hard"))? != 0)).collect();
    (!values.is_empty()).then(|| values.iter().any(|v| *v))
}
fn poll() -> Snapshot {
    let wifi = output("nmcli", &["--wait", "2", "radio", "wifi"]).ok()
        .and_then(|s| match s.trim() { "enabled" => Some(true), "disabled" => Some(false), _ => None });
    let bluetooth = output("bluetoothctl", &["show"]).ok().and_then(|s|
        s.lines().find_map(|l| l.trim().strip_prefix("Powered:").map(|v| v.trim() == "yes")));
    let airplane = match (rfkill("wlan"), rfkill("bluetooth")) { (Some(w), Some(b)) => Some(w && b), _ => None };
    let flashlight = torch().and_then(|p| read_u32(&p.join("brightness"))).map(|v| v != 0);
    let brightness = backlight().and_then(|p| Some(read_u32(&p.join("brightness"))? as f32 / read_u32(&p.join("max_brightness"))? as f32));
    let volume = output("wpctl", &["get-volume", "@DEFAULT_AUDIO_SINK@"]).ok().and_then(|s|
        if s.contains("MUTED") { Some(0.0) } else { s.split_whitespace().nth(1)?.parse::<f32>().ok().map(|v| v.clamp(0.0, 1.0)) });
    let policies = devices("/sys/devices/system/cpu/cpufreq");
    let governor: Vec<_> = policies.iter().filter_map(|p| std::fs::read_to_string(p.join("scaling_governor")).ok()).collect();
    let battery_saver = (!governor.is_empty()).then(|| governor.iter().all(|s| s.trim() == "powersave"));
    Snapshot { wifi, bluetooth, airplane, flashlight, battery_saver, brightness, volume, ..Snapshot::default() }
}
fn fraction_value(f: f32, max: u32) -> Result<u32, ()> {
    if !f.is_finite() || max == 0 { return Err(()); }
    // Never black out the panel when the slider reaches zero.
    Ok(((f.clamp(0.0, 1.0) * max as f32).round() as u32).clamp(1, max))
}
fn apply(a: &Action, saved: &mut Option<(bool, bool)>) -> Result<(), ()> {
    match *a {
        Action::Wifi(on) => output("nmcli", &["--wait", "2", "radio", "wifi", if on { "on" } else { "off" }]).map(|_| ()),
        Action::Bluetooth(on) => output("bluetoothctl", &["power", if on { "on" } else { "off" }]).map(|_| ()),
        Action::Airplane(on) => {
            if on {
                let s = poll();
                if saved.is_none() { *saved = Some((s.wifi.ok_or(())?, s.bluetooth.ok_or(())?)); }
                output("nmcli", &["--wait", "2", "radio", "wifi", "off"])?;
                output("bluetoothctl", &["power", "off"])?;
                output("rfkill", &["block", "wlan"])?;
                output("rfkill", &["block", "bluetooth"])?;
            } else {
                output("rfkill", &["unblock", "wlan"])?;
                output("rfkill", &["unblock", "bluetooth"])?;
                // An airplane state created by another client has no saved
                // preference: preserve powered-off radios in that case.
                let (wifi, bt) = saved.unwrap_or((false, false));
                apply(&Action::Wifi(wifi), saved)?;
                apply(&Action::Bluetooth(bt), saved)?;
                *saved = None;
            }
            Ok(())
        },
        Action::Brightness(f) => {
            let p = backlight().ok_or(())?;
            let value = fraction_value(f, read_u32(&p.join("max_brightness")).ok_or(())?)?;
            std::fs::write(p.join("brightness"), value.to_string()).map_err(|_| ())
        },
        Action::Volume(f) => {
            if !f.is_finite() { return Err(()); }
            output("wpctl", &["set-volume", "@DEFAULT_AUDIO_SINK@", &format!("{:.3}", f.clamp(0.0, 1.0))])?;
            output("wpctl", &["set-mute", "@DEFAULT_AUDIO_SINK@", "0"]).map(|_| ())
        },
        Action::Flashlight(on) => {
            let p = torch().ok_or(())?;
            let max = read_u32(&p.join("max_brightness")).ok_or(())?;
            std::fs::write(p.join("brightness"), if on { max } else { 0 }.to_string()).map_err(|_| ())
        },
        Action::BatterySaver(on) => {
            let paths = devices("/sys/devices/system/cpu/cpufreq");
            if paths.is_empty() { return Err(()); }
            let governor = if on { "powersave" } else { "schedutil" };
            // Validate every policy before making any change.
            for p in &paths {
                let choices = std::fs::read_to_string(p.join("scaling_available_governors")).map_err(|_| ())?;
                if !choices.split_whitespace().any(|s| s == governor) { return Err(()); }
            }
            for p in paths { std::fs::write(p.join("scaling_governor"), governor).map_err(|_| ())?; }
            Ok(())
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn brightness_bounds_keep_panel_visible_and_reject_invalid_inputs() {
        assert_eq!(fraction_value(0.0, 255), Ok(1));
        assert_eq!(fraction_value(2.0, 255), Ok(255));
        assert_eq!(fraction_value(0.5, 100), Ok(50));
        assert!(fraction_value(f32::NAN, 255).is_err());
        assert!(fraction_value(1.0, 0).is_err());
    }
}
