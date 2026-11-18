//! Battery snapshot (M3-T1): sysfs `power_supply` poll.
//!
//! Reads `/sys/class/power_supply/<Battery>/capacity` + `status` directly —
//! no `UPower` D-Bus round-trip on the compositor path. The `de-system`
//! worker polls this ~1/s; misses (desktop host, no battery) degrade to
//! `percent: None` which the status bar renders as "no battery".

use de_api::services::BatteryState;
use std::path::Path;

const SYSFS: &str = "/sys/class/power_supply";

/// Poll the first `type=Battery` supply under `root` (root is injectable
/// for tests — real callers pass [`SYSFS`]).
#[must_use]
pub fn poll_battery_in(root: &Path) -> BatteryState {
    let Ok(entries) = std::fs::read_dir(root) else {
        return BatteryState::default();
    };
    for e in entries.flatten() {
        let dir = e.path();
        let ty = std::fs::read_to_string(dir.join("type")).unwrap_or_default();
        if ty.trim() != "Battery" {
            continue;
        }
        let percent = std::fs::read_to_string(dir.join("capacity"))
            .ok()
            .and_then(|s| s.trim().parse::<u8>().ok());
        let status = std::fs::read_to_string(dir.join("status")).unwrap_or_default();
        let status = status.trim();
        return BatteryState {
            percent,
            charging: status == "Charging",
            full: status == "Full" || percent == Some(100),
        };
    }
    BatteryState::default()
}

/// Poll the real sysfs tree.
#[must_use]
pub fn poll_battery() -> BatteryState {
    poll_battery_in(Path::new(SYSFS))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_sysfs_no_battery() {
        let s = poll_battery_in(Path::new("/nonexistent/power_supply"));
        assert_eq!(s.percent, None);
        assert!(!s.charging && !s.full);
    }

    #[test]
    fn parses_capacity_and_status() {
        let dir = tempfile::tempdir().unwrap();
        let bat = dir.path().join("s2mu106-battery");
        std::fs::create_dir_all(&bat).unwrap();
        std::fs::write(bat.join("type"), "Battery").unwrap();
        std::fs::write(bat.join("capacity"), "86").unwrap();
        std::fs::write(bat.join("status"), "Charging").unwrap();
        let s = poll_battery_in(dir.path());
        assert_eq!(s.percent, Some(86));
        assert!(s.charging);
        assert!(!s.full);
    }
}
