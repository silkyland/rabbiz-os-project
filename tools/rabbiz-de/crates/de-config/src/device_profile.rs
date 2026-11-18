//! Load the installed device's display profile for layout and motion.

use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Default, Deserialize)]
#[serde(default)]
pub struct DeviceProfile {
    pub display: DisplayProfile,
}

#[derive(Default, Deserialize)]
#[serde(default)]
pub struct DisplayProfile {
    pub form_factor: String,
}

impl DeviceProfile {
    /// Load the explicit profile, or the installed system profile when present.
    /// Existing installs without a profile keep the handset defaults.
    pub fn load() -> Result<Self, String> {
        if let Some(path) = std::env::var_os("RABBIZ_DE_DEVICE_PROFILE") {
            return Self::from_path(&PathBuf::from(path));
        }
        let path = Path::new("/etc/rabbiz-de/device.toml");
        if path.exists() {
            Self::from_path(path)
        } else {
            Ok(Self::default())
        }
    }

    fn from_path(path: &Path) -> Result<Self, String> {
        let contents =
            std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        toml::from_str(&contents).map_err(|e| format!("{}: {e}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_form_factor_from_checked_in_profiles() {
        for (device, form_factor) in [("samsung/a21s", "handset"), ("xiaomi/muyu", "tablet")] {
            let path = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../../../devices")
                .join(device)
                .join("device.toml");
            let profile = DeviceProfile::from_path(&path).expect("checked-in device profile");
            assert_eq!(profile.display.form_factor, form_factor);
        }
    }
}
