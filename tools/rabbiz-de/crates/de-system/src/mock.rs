//! Synthetic hardware and system-service state store for `--backend=virtual`
//! and host-side emulator control (`rabbiz-de ctl mock <key> <value>`).
//!
//! Works identically on macOS and Linux with zero sysfs or D-Bus dependencies.

use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};

/// One synthetic notification queued via `ctl mock notif "App|Title|Body"`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MockNotification {
    pub app: String,
    pub title: String,
    pub body: String,
}

/// Complete snapshot of mocked hardware/service state.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
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
}

static STORE: OnceLock<Mutex<MockState>> = OnceLock::new();

fn store() -> &'static Mutex<MockState> {
    STORE.get_or_init(|| Mutex::new(MockState::default()))
}

/// Read the current mock state snapshot.
#[must_use]
pub fn snapshot() -> MockState {
    store().lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// Drain and return any pending synthetic notifications queued since the last drain.
#[must_use]
pub fn drain_notifications() -> Vec<MockNotification> {
    let mut guard = store().lock().unwrap_or_else(|e| e.into_inner());
    std::mem::take(&mut guard.pending_notifications)
}

/// Consume a pending one-shot fingerprint result (`Some(true)` = pass, `Some(false)` = fail).
#[must_use]
pub fn take_fingerprint() -> Option<bool> {
    let mut guard = store().lock().unwrap_or_else(|e| e.into_inner());
    guard.fingerprint_ok.take()
}

/// Apply a `ctl mock <key> <value>` mutation and return the serialized state.
pub fn apply(key: &str, value: &str) -> Result<serde_json::Value, String> {
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
    fn apply_all_keys_and_reset() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
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
        assert!(apply("battery", "105").is_err());
        assert!(apply("brightness", "abc").is_err());
        assert!(apply("charging", "maybe").is_err());
        assert!(apply("fingerprint", "unknown").is_err());
        assert!(apply("notif", "").is_err());
        assert!(apply("unknown_key", "1").is_err());
        apply("reset", "").unwrap();
    }
}
