use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Persistent user preferences for `rabbiz-de` (`~/.config/rabbiz-de/prefs.json`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    #[serde(rename = "deAccent")]
    pub de_accent: String,
    #[serde(rename = "deWallpaperId")]
    pub de_wallpaper_id: String,
    #[serde(rename = "darkMode")]
    pub dark_mode: bool,
    #[serde(rename = "textScale")]
    pub text_scale: f32,
    #[serde(rename = "screenTimeoutSec")]
    pub screen_timeout_sec: u32,
    #[serde(rename = "edgeBackSensitivity")]
    pub edge_back_sensitivity: f32,
    #[serde(rename = "reducedMotion")]
    pub reduced_motion: bool,
    #[serde(rename = "brightness")]
    pub brightness: u8,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            de_accent: "copper".to_string(),
            de_wallpaper_id: "ember".to_string(),
            dark_mode: true,
            text_scale: 1.0,
            screen_timeout_sec: 120,
            edge_back_sensitivity: 1.0,
            reduced_motion: false,
            brightness: 75,
        }
    }
}

/// Errors returned when loading or saving `Prefs`.
#[derive(Debug)]
pub enum ConfigError {
    Io(io::Error),
    Json(serde_json::Error),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => write!(f, "config I/O error: {err}"),
            Self::Json(err) => write!(f, "config JSON error: {err}"),
        }
    }
}

impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            Self::Json(err) => Some(err),
        }
    }
}

impl From<io::Error> for ConfigError {
    fn from(err: io::Error) -> Self {
        Self::Io(err)
    }
}

impl From<serde_json::Error> for ConfigError {
    fn from(err: serde_json::Error) -> Self {
        Self::Json(err)
    }
}

/// Subset of `~/.config/a21s-shell/prefs.json` keys read once during migration.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct LegacyShellPrefs {
    #[serde(rename = "brightnessLevel")]
    brightness_level: Option<i64>,
    #[serde(rename = "lightTheme")]
    light_theme: Option<bool>,
    #[serde(rename = "sleepTimeoutSec")]
    sleep_timeout_sec: Option<i64>,
}

/// Default path for `rabbiz-de` preferences (`~/.config/rabbiz-de/prefs.json`).
pub fn default_de_prefs_path() -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/home/gamer"));
    home.join(".config").join("rabbiz-de").join("prefs.json")
}

/// Default path for legacy `a21s-shell` preferences (`~/.config/a21s-shell/prefs.json`).
pub fn default_legacy_prefs_path() -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/home/gamer"));
    home.join(".config").join("a21s-shell").join("prefs.json")
}

/// Loads `de_prefs_path` if present; otherwise migrates hints from `legacy_prefs_path`
/// (read-only, never writing to the legacy file) onto `Prefs::default()`.
pub fn load_or_migrate(
    de_prefs_path: &Path,
    legacy_prefs_path: &Path,
) -> Result<Prefs, ConfigError> {
    match fs::read_to_string(de_prefs_path) {
        Ok(raw) => {
            if raw.trim().is_empty() {
                return Ok(Prefs::default());
            }
            let prefs: Prefs = serde_json::from_str(&raw)?;
            return Ok(prefs);
        }
        Err(err) if err.kind() == io::ErrorKind::NotFound => {}
        Err(err) => return Err(ConfigError::Io(err)),
    }

    let mut prefs = Prefs::default();

    match fs::read_to_string(legacy_prefs_path) {
        Ok(raw) if !raw.trim().is_empty() => {
            if let Ok(legacy) = serde_json::from_str::<LegacyShellPrefs>(&raw) {
                if let Some(light) = legacy.light_theme {
                    prefs.dark_mode = !light;
                }
                if let Some(timeout) = legacy.sleep_timeout_sec {
                    if timeout >= 0 {
                        prefs.screen_timeout_sec = timeout as u32;
                    }
                }
                if let Some(level) = legacy.brightness_level {
                    if level >= 0 {
                        prefs.brightness = level.clamp(0, 100) as u8;
                    }
                }
            }
        }
        Ok(_) => {}
        Err(err) if err.kind() == io::ErrorKind::NotFound => {}
        Err(err) => return Err(ConfigError::Io(err)),
    }

    Ok(prefs)
}

/// Atomically writes `prefs` to `path` via temp file + rename.
/// Creates a `.bak` backup of the existing file before the first mutation.
pub fn save_atomic(path: &Path, prefs: &Prefs) -> Result<(), ConfigError> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }

    if path.exists() {
        let bak_path = backup_path(path);
        if !bak_path.exists() {
            fs::copy(path, &bak_path)?;
        }
    }

    let mut payload = serde_json::to_string_pretty(prefs)?;
    payload.push('\n');

    let tmp_path = temp_path(path);
    {
        let mut file = File::create(&tmp_path)?;
        file.write_all(payload.as_bytes())?;
        file.sync_all()?;
    }

    fs::rename(&tmp_path, path)?;
    Ok(())
}

fn backup_path(path: &Path) -> PathBuf {
    let mut os = path.as_os_str().to_os_string();
    os.push(".bak");
    PathBuf::from(os)
}

fn temp_path(path: &Path) -> PathBuf {
    let mut os = path.as_os_str().to_os_string();
    os.push(format!(".tmp.{}", std::process::id()));
    PathBuf::from(os)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "de-config-test-{}-{}-{}",
            name,
            std::process::id(),
            nanos
        ));
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn defaults_match_brand_contract() {
        let prefs = Prefs::default();
        assert_eq!(prefs.de_accent, "copper");
        assert_eq!(prefs.de_wallpaper_id, "ember");
        assert!(prefs.dark_mode);
        assert_eq!(prefs.text_scale, 1.0);
        assert_eq!(prefs.screen_timeout_sec, 120);
        assert_eq!(prefs.edge_back_sensitivity, 1.0);
        assert!(!prefs.reduced_motion);
        assert_eq!(prefs.brightness, 75);
    }

    #[test]
    fn load_or_migrate_returns_defaults_when_no_files_exist() {
        let dir = temp_dir("missing");
        let de_path = dir.join("rabbiz-de/prefs.json");
        let legacy_path = dir.join("a21s-shell/prefs.json");

        let loaded = load_or_migrate(&de_path, &legacy_path).expect("load defaults");
        assert_eq!(loaded, Prefs::default());
        assert!(!de_path.exists());
        assert!(!legacy_path.exists());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_or_migrate_reads_legacy_hints_without_writing_legacy() {
        let dir = temp_dir("migrate");
        let de_path = dir.join("rabbiz-de/prefs.json");
        let legacy_dir = dir.join("a21s-shell");
        fs::create_dir_all(&legacy_dir).expect("legacy dir");
        let legacy_path = legacy_dir.join("prefs.json");

        let legacy_json = r#"{
  "brightnessLevel": 42,
  "wallpaperId": "aurora",
  "accentId": "blue",
  "lightTheme": true,
  "sleepTimeoutSec": 300
}
"#;
        fs::write(&legacy_path, legacy_json).expect("write legacy");

        let loaded = load_or_migrate(&de_path, &legacy_path).expect("migrate");
        assert_eq!(loaded.de_accent, "copper");
        assert_eq!(loaded.de_wallpaper_id, "ember");
        assert!(!loaded.dark_mode);
        assert_eq!(loaded.screen_timeout_sec, 300);
        assert_eq!(loaded.brightness, 42);

        // Legacy file must remain completely untouched.
        let after = fs::read_to_string(&legacy_path).expect("read legacy after");
        assert_eq!(after, legacy_json);
        assert!(!de_path.exists());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_or_migrate_prefers_de_prefs_when_present() {
        let dir = temp_dir("existing-de");
        let de_dir = dir.join("rabbiz-de");
        let legacy_dir = dir.join("a21s-shell");
        fs::create_dir_all(&de_dir).expect("de dir");
        fs::create_dir_all(&legacy_dir).expect("legacy dir");

        let de_path = de_dir.join("prefs.json");
        let legacy_path = legacy_dir.join("prefs.json");

        let custom = Prefs {
            de_accent: "sage".to_string(),
            de_wallpaper_id: "plum".to_string(),
            dark_mode: false,
            text_scale: 1.25,
            screen_timeout_sec: 60,
            edge_back_sensitivity: 1.5,
            reduced_motion: true,
            brightness: 88,
        };
        save_atomic(&de_path, &custom).expect("save custom");
        fs::write(&legacy_path, r#"{"brightnessLevel": 10, "lightTheme": false}"#)
            .expect("write legacy");

        let loaded = load_or_migrate(&de_path, &legacy_path).expect("load de");
        assert_eq!(loaded, custom);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_atomic_round_trips_and_creates_backup_on_first_mutation() {
        let dir = temp_dir("atomic");
        let de_path = dir.join("rabbiz-de/prefs.json");
        let bak_path = dir.join("rabbiz-de/prefs.json.bak");

        let first = Prefs::default();
        save_atomic(&de_path, &first).expect("first save");
        assert!(de_path.exists());
        assert!(!bak_path.exists());

        let second = Prefs {
            brightness: 55,
            reduced_motion: true,
            ..Prefs::default()
        };
        save_atomic(&de_path, &second).expect("second save");
        assert!(bak_path.exists());

        let third = Prefs {
            brightness: 90,
            ..Prefs::default()
        };
        save_atomic(&de_path, &third).expect("third save");

        // Backup should still hold the state before the first mutation (`first`).
        let bak_raw = fs::read_to_string(&bak_path).expect("read backup");
        let bak_prefs: Prefs = serde_json::from_str(&bak_raw).expect("parse backup");
        assert_eq!(bak_prefs, first);

        let reloaded = load_or_migrate(&de_path, &dir.join("none.json")).expect("reload");
        assert_eq!(reloaded, third);

        let _ = fs::remove_dir_all(&dir);
    }
}
