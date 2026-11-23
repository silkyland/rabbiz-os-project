//! Synthetic hardware and system-service state store for `--backend=virtual`
//! and host-side emulator control (`rabbiz-de ctl mock <key> <value>`).
//!
//! Works identically on macOS and Linux with zero sysfs or D-Bus dependencies.

#[cfg(any(feature = "emu", test))]
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};

/// One synthetic notification queued via `ctl mock notif "App|Title|Body"`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MockNotification {
    pub app: String,
    pub title: String,
    pub body: String,
}

/// Synthetic now-playing track (`ctl mock media "Title|Artist|playing"`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MockMedia {
    pub title: String,
    pub artist: String,
    pub playing: bool,
}

/// Complete snapshot of mocked hardware/service state.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MockState {
    pub enabled: bool,
    pub battery_pct: Option<u8>,
    pub charging: Option<bool>,
    pub wifi_enabled: Option<bool>,
    pub wifi_ssid: Option<String>,
    pub bluetooth_enabled: Option<bool>,
    pub airplane: Option<bool>,
    pub dnd: Option<bool>,
    pub brightness_pct: Option<u8>,
    pub volume_pct: Option<u8>,
    pub carrier_sim1: Option<String>,
    pub carrier_sim2: Option<String>,
    pub incoming_call: Option<String>,
    pub psi_pressure: bool,
    pub fingerprint_ok: Option<bool>,
    pub pending_notifications: Vec<MockNotification>,
    /// Injected accelerometer reading, m/s^2 (`ctl mock accel x,y,z`).
    pub accel: Option<(f32, f32, f32)>,
    /// Synthetic now-playing track; wins over the session-bus player.
    pub media: Option<MockMedia>,
}

static STORE: OnceLock<Mutex<MockState>> = OnceLock::new();
#[cfg(any(feature = "emu", test))]
static VIRTUAL_BACKEND_ENABLED: AtomicBool = AtomicBool::new(false);

fn store() -> &'static Mutex<MockState> {
    STORE.get_or_init(|| Mutex::new(MockState::default()))
}

/// Apply a transport command to the mock track (play/pause toggles).
pub fn media_toggle_playing() {
    if !is_enabled() {
        return;
    }
    let mut guard = store().lock().unwrap_or_else(|e| e.into_inner());
    if let Some(m) = guard.media.as_mut() {
        m.playing = !m.playing;
    }
}

/// Enable the mock store for `--backend=virtual`.
///
/// Compiled as a no-op when the `emu` feature is disabled in non-test builds.
pub fn enable_virtual_backend() {
    #[cfg(any(feature = "emu", test))]
    {
        VIRTUAL_BACKEND_ENABLED.store(true, Ordering::Release);
    }
}

/// Disable the mock store (used by tests).
#[cfg(test)]
pub fn disable_virtual_backend() {
    VIRTUAL_BACKEND_ENABLED.store(false, Ordering::Release);
    let mut guard = store().lock().unwrap_or_else(|e| e.into_inner());
    *guard = MockState::default();
}

/// Whether the virtual backend has enabled the mock store in this process.
#[must_use]
pub fn is_enabled() -> bool {
    #[cfg(any(feature = "emu", test))]
    {
        VIRTUAL_BACKEND_ENABLED.load(Ordering::Acquire)
    }
    #[cfg(not(any(feature = "emu", test)))]
    {
        false
    }
}

/// Read the current mock state snapshot.
#[must_use]
pub fn snapshot() -> MockState {
    if !is_enabled() {
        return MockState::default();
    }
    store().lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// Drain and return any pending synthetic notifications queued since the last drain.
#[must_use]
pub fn drain_notifications() -> Vec<MockNotification> {
    if !is_enabled() {
        return Vec::new();
    }
    let mut guard = store().lock().unwrap_or_else(|e| e.into_inner());
    std::mem::take(&mut guard.pending_notifications)
}

/// Consume a pending one-shot fingerprint result (`Some(true)` = pass, `Some(false)` = fail).
#[must_use]
pub fn take_fingerprint() -> Option<bool> {
    if !is_enabled() {
        return None;
    }
    let mut guard = store().lock().unwrap_or_else(|e| e.into_inner());
    guard.fingerprint_ok.take()
}

/// Apply a `ctl mock <key> <value>` mutation and return the serialized state.
pub fn apply(key: &str, value: &str) -> Result<serde_json::Value, String> {
    if !is_enabled() {
        return Err("mock disabled: requires virtual backend with emu feature".to_string());
    }
    let mut guard = store().lock().unwrap_or_else(|e| e.into_inner());
    apply_in(&mut guard, key, value)?;
    serde_json::to_value(&*guard).map_err(|e| format!("serialize mock state: {e}"))
}

fn apply_in(state: &mut MockState, key: &str, value: &str) -> Result<(), String> {
    let k = key.trim();
    let v = value.trim();
    match k {
        "reset" => {
            *state = MockState::default();
            return Ok(());
        }
        "battery" => {
            state.battery_pct = Some(parse_percent(k, v)?);
        }
        "charging" => {
            state.charging = Some(parse_bool(k, v)?);
        }
        "wifi" => {
            if let Ok(b) = parse_bool(k, v) {
                state.wifi_enabled = Some(b);
                if !b {
                    state.wifi_ssid = None;
                }
            } else if v.is_empty() || v.eq_ignore_ascii_case("none") {
                state.wifi_enabled = Some(false);
                state.wifi_ssid = None;
            } else {
                state.wifi_enabled = Some(true);
                state.wifi_ssid = Some(v.to_string());
            }
        }
        "bluetooth" | "bt" => {
            state.bluetooth_enabled = Some(parse_bool(k, v)?);
        }
        "airplane" => {
            state.airplane = Some(parse_bool(k, v)?);
        }
        "dnd" => {
            state.dnd = Some(parse_bool(k, v)?);
        }
        "brightness" => {
            state.brightness_pct = Some(parse_percent(k, v)?);
        }
        "volume" => {
            state.volume_pct = Some(parse_percent(k, v)?);
        }
        "sim1" => {
            state.carrier_sim1 = parse_opt_str(v);
        }
        "sim2" => {
            state.carrier_sim2 = parse_opt_str(v);
        }
        "call" => {
            state.incoming_call = parse_opt_str(v);
        }
        "psi" => {
            state.psi_pressure = parse_bool(k, v)?;
        }
        "fingerprint" => {
            state.fingerprint_ok = match v.to_ascii_lowercase().as_str() {
                "pass" | "true" | "ok" | "1" => Some(true),
                "fail" | "false" | "0" => Some(false),
                "none" | "" => None,
                _ => {
                    return Err(format!(
                        "mock fingerprint expects pass|fail|none, got '{v}'"
                    ))
                }
            };
        }
        "notif" | "notification" => {
            if v.is_empty() {
                return Err("mock notif cannot be empty".to_string());
            }
            let parts: Vec<&str> = v.splitn(3, '|').collect();
            let notif = match parts.as_slice() {
                [app, title, body] => MockNotification {
                    app: (*app).trim().to_string(),
                    title: (*title).trim().to_string(),
                    body: (*body).trim().to_string(),
                },
                [title, body] => MockNotification {
                    app: "RabbizOS".to_string(),
                    title: (*title).trim().to_string(),
                    body: (*body).trim().to_string(),
                },
                [title] => MockNotification {
                    app: "RabbizOS".to_string(),
                    title: (*title).trim().to_string(),
                    body: String::new(),
                },
                _ => return Err("invalid mock notification format".to_string()),
            };
            state.pending_notifications.push(notif);
        }
        "accel" => {
            state.accel = if v.is_empty() || v.eq_ignore_ascii_case("none") {
                None
            } else {
                let p: Vec<f32> = v
                    .split(',')
                    .map(|t| t.trim().parse::<f32>())
                    .collect::<Result<_, _>>()
                    .map_err(|_| format!("mock accel expects x,y,z in m/s^2, got '{v}'"))?;
                let [x, y, z] = p[..] else {
                    return Err(format!("mock accel expects x,y,z in m/s^2, got '{v}'"));
                };
                Some((x, y, z))
            };
        }
        "media" => {
            state.media = if v.is_empty() || v.eq_ignore_ascii_case("none") {
                None
            } else {
                let mut it = v.splitn(3, '|').map(str::trim);
                let title = it.next().unwrap_or_default().to_string();
                let artist = it.next().unwrap_or_default().to_string();
                let playing = !matches!(it.next(), Some("paused" | "pause" | "false" | "0"));
                Some(MockMedia {
                    title,
                    artist,
                    playing,
                })
            };
        }
        "orientation" | "rotate" => {
            state.accel = if v.is_empty() || v.eq_ignore_ascii_case("none") {
                None
            } else {
                let o = de_api::Orientation::parse(v).ok_or_else(|| {
                    format!("mock orientation expects portrait|landscape_ccw|portrait_flipped|landscape_cw|none, got '{v}'")
                })?;
                let s = de_api::AccelSample::at_rest(o);
                Some((s.x, s.y, s.z))
            };
        }
        other => return Err(format!("unknown mock key '{other}'")),
    }
    state.enabled = true;
    Ok(())
}

fn parse_percent(key: &str, value: &str) -> Result<u8, String> {
    let pct: u8 = value
        .parse()
        .map_err(|_| format!("mock {key} expects 0..=100, got '{value}'"))?;
    if pct > 100 {
        return Err(format!("mock {key} must be 0..=100, got {pct}"));
    }
    Ok(pct)
}

fn parse_bool(key: &str, value: &str) -> Result<bool, String> {
    match value.to_ascii_lowercase().as_str() {
        "true" | "1" | "on" | "yes" | "enabled" => Ok(true),
        "false" | "0" | "off" | "no" | "disabled" => Ok(false),
        _ => Err(format!("mock {key} expects true|false, got '{value}'")),
    }
}

fn parse_opt_str(value: &str) -> Option<String> {
    if value.is_empty() || value.eq_ignore_ascii_case("none") {
        None
    } else {
        Some(value.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn sensor_and_media_keys() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        enable_virtual_backend();
        apply("reset", "").unwrap();
        apply("orientation", "landscape_cw").unwrap();
        let (x, y, _) = snapshot().accel.unwrap();
        assert!(x < -9.0 && y.abs() < 1e-3);
        apply("accel", "0, 9.8, 0").unwrap();
        assert_eq!(snapshot().accel, Some((0.0, 9.8, 0.0)));
        assert!(apply("accel", "1,2").is_err());
        assert!(apply("orientation", "sideways").is_err());
        apply("orientation", "none").unwrap();
        assert_eq!(snapshot().accel, None);

        apply("media", "Midnight Signal|Devded|paused").unwrap();
        let m = snapshot().media.unwrap();
        assert_eq!((m.title.as_str(), m.artist.as_str(), m.playing), ("Midnight Signal", "Devded", false));
        media_toggle_playing();
        assert!(snapshot().media.unwrap().playing);
        apply("media", "none").unwrap();
        assert!(snapshot().media.is_none());
        apply("reset", "").unwrap();
    }

    #[test]
    fn inert_unless_virtual_backend_enabled() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        disable_virtual_backend();
        assert!(!is_enabled());
        assert!(apply("fingerprint", "pass").is_err());
        assert_eq!(take_fingerprint(), None);
        assert_eq!(snapshot(), MockState::default());
        assert!(drain_notifications().is_empty());
    }

    #[test]
    fn apply_all_keys_and_reset() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        enable_virtual_backend();
        apply("reset", "").unwrap();
        assert!(!snapshot().enabled);

        apply("battery", "42").unwrap();
        apply("charging", "true").unwrap();
        apply("wifi", "RabbizLab-5G").unwrap();
        apply("bt", "true").unwrap();
        apply("airplane", "false").unwrap();
        apply("dnd", "true").unwrap();
        apply("brightness", "80").unwrap();
        apply("volume", "65").unwrap();
        apply("sim1", "AIS").unwrap();
        apply("sim2", "TRUE-H").unwrap();
        apply("call", "+66812345678").unwrap();
        apply("psi", "true").unwrap();
        apply("fingerprint", "pass").unwrap();

        let s = snapshot();
        assert!(s.enabled);
        assert_eq!(s.battery_pct, Some(42));
        assert_eq!(s.charging, Some(true));
        assert_eq!(s.wifi_enabled, Some(true));
        assert_eq!(s.wifi_ssid.as_deref(), Some("RabbizLab-5G"));
        assert_eq!(s.bluetooth_enabled, Some(true));
        assert_eq!(s.airplane, Some(false));
        assert_eq!(s.dnd, Some(true));
        assert_eq!(s.brightness_pct, Some(80));
        assert_eq!(s.volume_pct, Some(65));
        assert_eq!(s.carrier_sim1.as_deref(), Some("AIS"));
        assert_eq!(s.carrier_sim2.as_deref(), Some("TRUE-H"));
        assert_eq!(s.incoming_call.as_deref(), Some("+66812345678"));
        assert!(s.psi_pressure);
        assert_eq!(s.fingerprint_ok, Some(true));
        assert_eq!(take_fingerprint(), Some(true));
        assert_eq!(take_fingerprint(), None);

        apply("wifi", "false").unwrap();
        assert_eq!(snapshot().wifi_enabled, Some(false));
        assert_eq!(snapshot().wifi_ssid, None);

        apply("call", "none").unwrap();
        assert_eq!(snapshot().incoming_call, None);

        apply("reset", "").unwrap();
        assert_eq!(snapshot(), MockState::default());
    }

    #[test]
    fn notifications_enqueue_and_drain() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        enable_virtual_backend();
        apply("reset", "").unwrap();

        apply("notif", "Chats|Sarah|Are we still on for 7?").unwrap();
        apply("notification", "Battery Low|15% remaining").unwrap();

        let drained = drain_notifications();
        assert_eq!(
            drained,
            vec![
                MockNotification {
                    app: "Chats".into(),
                    title: "Sarah".into(),
                    body: "Are we still on for 7?".into(),
                },
                MockNotification {
                    app: "RabbizOS".into(),
                    title: "Battery Low".into(),
                    body: "15% remaining".into(),
                },
            ]
        );
        assert!(drain_notifications().is_empty());
        apply("reset", "").unwrap();
    }

    #[test]
    fn rejects_invalid_mock_keys_and_values() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        enable_virtual_backend();
        assert!(apply("battery", "105").is_err());
        assert!(apply("brightness", "abc").is_err());
        assert!(apply("charging", "maybe").is_err());
        assert!(apply("fingerprint", "unknown").is_err());
        assert!(apply("notif", "").is_err());
        assert!(apply("unknown_key", "1").is_err());
        apply("reset", "").unwrap();
    }
}
