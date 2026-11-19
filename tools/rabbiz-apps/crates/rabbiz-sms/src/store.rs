//! Local message persistence: one JSON file under the user data dir,
//! written atomically (tmp + rename) per the base-app persistence rule.

use std::path::{Path, PathBuf};

use crate::model::Message;

/// Cap on stored messages — oldest are dropped first. Generous for SMS.
pub const MAX_MESSAGES: usize = 5000;

#[must_use]
pub fn data_dir() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("rabbiz-sms")
}

#[must_use]
pub fn store_path() -> PathBuf {
    data_dir().join("messages.json")
}

/// Load all messages (oldest→newest as stored). Missing/corrupt file → empty.
#[must_use]
pub fn load(path: &Path) -> Vec<Message> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// Persist `msgs` atomically: serialize → write `.tmp` → rename over target.
/// Creates the parent dir. Errors are swallowed by callers via `Result` here
/// so tests can assert; the app logs nothing (status line covers UX).
pub fn save(path: &Path, msgs: &[Message]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let trimmed: Vec<&Message> = msgs.iter().rev().take(MAX_MESSAGES).rev().collect();
    let json = serde_json::to_string(&trimmed).map_err(std::io::Error::other)?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<Message> {
        vec![
            Message::new(1, "0812345678", "สวัสดีครับ", false, 1_700_000_000),
            Message::new(2, "0812345678", "hello", true, 1_700_000_060),
        ]
    }

    #[test]
    fn round_trip_preserves_thai_and_state() {
        let dir = std::env::temp_dir().join(format!("rabbiz-sms-test-{}", std::process::id()));
        let path = dir.join("messages.json");
        let msgs = sample();
        save(&path, &msgs).unwrap();
        let back = load(&path);
        assert_eq!(back, msgs);
        assert_eq!(back[0].body, "สวัสดีครับ");
        assert_eq!(back[1].state, crate::model::MsgState::Queued);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_and_corrupt_load_empty() {
        let dir = std::env::temp_dir().join(format!("rabbiz-sms-test2-{}", std::process::id()));
        let path = dir.join("nope.json");
        assert!(load(&path).is_empty());
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&path, "{not json").unwrap();
        assert!(load(&path).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_is_atomic_no_tmp_left() {
        let dir = std::env::temp_dir().join(format!("rabbiz-sms-test3-{}", std::process::id()));
        let path = dir.join("messages.json");
        save(&path, &sample()).unwrap();
        assert!(path.exists());
        assert!(!path.with_extension("json.tmp").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
