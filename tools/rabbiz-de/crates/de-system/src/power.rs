//! Panel backlight + CPU governor power control (P17, M4-T4).
//!
//! The DE owns the panel when it runs: `a21s-power-key` only writes
//! `a21s-power.cmd` and leaves the backlight alone. Sysfs nodes are
//! `video`-group writable on the A21s (`90-a21s-backlight.rules`), so the
//! session user can write them directly — no helper, no fork.
//!
//! The saved-brightness contract file (`$XDG_RUNTIME_DIR/a21s-saved-brightness`)
//! is shared with `a21s-power-key` and the old QML shell: write it before
//! blanking so a later session restores the user's level, never the dim or
//! screen-off value.

use std::path::{Path, PathBuf};

/// One writable backlight device.
#[derive(Debug, Clone)]
pub struct Backlight {
    dir: PathBuf,
    pub max: u32,
}

/// The first usable `/sys/class/backlight/*` device, if any.
#[must_use]
pub fn backlight() -> Option<Backlight> {
    let mut dirs: Vec<PathBuf> = std::fs::read_dir("/sys/class/backlight")
        .ok()?
        .flatten()
        .map(|e| e.path())
        .collect();
    dirs.sort();
    dirs.into_iter().find_map(|dir| {
        let max = read_u32(&dir.join("max_brightness"))?;
        (max > 0).then(|| Backlight { dir, max })
    })
}

impl Backlight {
    /// Raw driver level (0..=max).
    #[must_use]
    pub fn get(&self) -> Option<u32> {
        read_u32(&self.dir.join("brightness"))
    }

    /// Write a raw level, clamped into the device range. `0` also unblanks
    /// the panel so a dim restore never leaves `bl_power` asserted.
    pub fn set(&self, raw: u32) {
        let raw = raw.min(self.max);
        let _ = std::fs::write(self.dir.join("brightness"), raw.to_string());
        if raw > 0 {
            self.power(true);
        }
    }

    /// `bl_power`: 0 = unblank (FB_BLANK_UNBLANK), 4 = power down.
    pub fn power(&self, on: bool) {
        let v = if on { "0" } else { "4" };
        let _ = std::fs::write(self.dir.join("bl_power"), v);
    }

    /// Panel currently powered (readable `bl_power`, default on).
    #[must_use]
    pub fn powered(&self) -> bool {
        read_u32(&self.dir.join("bl_power")).map_or(true, |v| v == 0)
    }
}

/// Runtime file shared with `a21s-power-key` (cross-version contract — the
/// name stays `a21s-*` per the renaming policy).
#[must_use]
pub fn saved_brightness_path() -> PathBuf {
    let rt = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/run/user/1001"));
    rt.join("a21s-saved-brightness")
}

/// Persist a raw level for the next wake (best effort).
pub fn save_brightness(raw: u32) {
    let path = saved_brightness_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, format!("{raw}\n"));
}

/// The level `a21s-power-key` last saved, when still sane.
#[must_use]
pub fn saved_brightness() -> Option<u32> {
    read_u32(&saved_brightness_path()).filter(|v| *v > 0)
}

/// Every cpufreq policy's `scaling_governor`, first one wins the read-back.
#[must_use]
pub fn governor() -> Option<String> {
    policies().into_iter().find_map(|p| {
        std::fs::read_to_string(p.join("scaling_governor"))
            .ok()
            .map(|s| s.trim().to_string())
    })
}

/// Set the same governor on every policy, best effort. `video`-group
/// writable on the A21s.
pub fn set_governor(name: &str) {
    for p in policies() {
        let _ = std::fs::write(p.join("scaling_governor"), name);
    }
}

fn policies() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::fs::read_dir("/sys/devices/system/cpu/cpufreq")
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .collect();
    dirs.sort();
    dirs
}

fn read_u32(path: &Path) -> Option<u32> {
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_path_uses_runtime_dir() {
        std::env::set_var("XDG_RUNTIME_DIR", "/run/user/4242");
        assert_eq!(
            saved_brightness_path(),
            PathBuf::from("/run/user/4242/a21s-saved-brightness")
        );
    }
}
