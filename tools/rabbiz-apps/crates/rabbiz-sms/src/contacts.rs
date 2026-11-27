//! Read-only reader for the shared contacts store written by
//! `rabbiz-contacts`: `$XDG_DATA_HOME/rabbiz-contacts/contacts.json`
//! (fallback `~/.local/share/rabbiz-contacts/contacts.json`).
//!
//! Used only to resolve peer numbers to display names in the conversation
//! list and thread header — SMS never writes contacts. Schema contract
//! (version 1) is owned by the contacts crate's `store.rs`; unknown
//! versions and unparseable files resolve to "no contacts".

use std::path::{Path, PathBuf};

use serde::Deserialize;

/// `(thread_key(phone), contact name)` pairs — one per `phones[]` entry so
/// every number of a contact resolves to the same name.
#[derive(Debug, Default)]
pub struct Contacts {
    entries: Vec<(String, String)>,
}

#[derive(Debug, Deserialize)]
struct StoreFile {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    contacts: Vec<StoreContact>,
}

#[derive(Debug, Deserialize)]
struct StoreContact {
    #[serde(default)]
    name: String,
    #[serde(default)]
    phones: Vec<StorePhone>,
}

#[derive(Debug, Deserialize)]
struct StorePhone {
    #[serde(default)]
    number: String,
}

impl Contacts {
    /// Contact name for a peer number/address. Both sides are normalized
    /// with [`norm_key`] (digits-only + `66…`→`0…` fold, both directions of
    /// length covered — `model::thread_key` only folds 11-digit numbers,
    /// which misses `+66 2…` landlines). `None` for unknown peers and
    /// non-numeric sender ids.
    #[must_use]
    pub fn name_for(&self, peer: &str) -> Option<&str> {
        let key = norm_key(peer);
        if key.is_empty() {
            return None;
        }
        self.entries
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, name)| name.as_str())
    }

    /// Deterministic entries for render tests — the shared store is host
    /// state, and harness shots must not depend on it (or on env mutated
    /// by sibling tests running in parallel).
    #[cfg(test)]
    pub(crate) fn demo_entries() -> Self {
        Self {
            entries: vec![
                (norm_key("0812345678"), "Alex".to_string()),
                (norm_key("0865551234"), "Mom".to_string()),
            ],
        }
    }

    /// Load from an explicit file; missing/corrupt/unknown-version → empty.
    #[must_use]
    pub fn load_from(path: &Path) -> Self {
        let Ok(raw) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        let Ok(f) = serde_json::from_str::<StoreFile>(&raw) else {
            return Self::default();
        };
        if f.version != 1 {
            return Self::default();
        }
        let mut entries = Vec::new();
        for c in &f.contacts {
            let name = c.name.trim();
            if name.is_empty() {
                continue;
            }
            for p in &c.phones {
                let key = norm_key(&p.number);
                // A phone row with fewer than 3 digits can never match a
                // real peer anyway.
                if key.chars().any(|ch| ch.is_ascii_digit()) {
                    entries.push((key, name.to_string()));
                }
            }
        }
        Self { entries }
    }

    /// Load from the real data dir (`$XDG_DATA_HOME/rabbiz-contacts/`).
    #[must_use]
    pub fn load() -> Self {
        Self::load_from(&store_path())
    }
}

/// Normalization shared by peers and stored phone numbers: digits-only,
/// canonical national `0…` form — `66…` folds to `0…` (length ≥ 10 covers
/// both `+66 8…` mobiles and `+66 2…` landlines; `model::thread_key` only
/// folds 11-digit numbers and misses the landlines). Short codes like
/// `191` pass through; non-numeric peers yield an empty key.
fn norm_key(s: &str) -> String {
    let digits: String = s.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() < 3 {
        return String::new();
    }
    if digits.starts_with("66") && digits.len() >= 10 {
        format!("0{}", &digits[2..])
    } else {
        digits
    }
}

/// `$XDG_DATA_HOME/rabbiz-contacts/contacts.json` (or the
/// `~/.local/share` fallback) — the contacts crate's data dir, NOT this
/// app's `rabbiz-sms` dir.
#[must_use]
pub fn store_path() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("rabbiz-contacts")
        .join("contacts.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "rabbiz-sms-contacts-test-{}-{}-{}",
            tag,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        ));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    const FIXTURE: &str = r#"{"version":1,"contacts":[
        {"id":"c-1","name":"สมชาย ใจดี","phones":[
            {"label":"mobile","number":"081-234-5678"},
            {"label":"work","number":"+66 2 555 0199"}],
         "emails":[],"note":"","favorite":false,"created":1,"updated":1},
        {"id":"c-2","name":"   ","phones":[{"label":"","number":"0899999999"}],
         "emails":[],"note":"","favorite":false,"created":1,"updated":1}
    ]}"#;

    #[test]
    fn name_for_resolves_every_phone_with_thread_key_fold() {
        let dir = tmpdir("names");
        let path = dir.join("contacts.json");
        std::fs::write(&path, FIXTURE).unwrap();
        let contacts = Contacts::load_from(&path);

        // Local form, formatted form and +66 international form all hit.
        assert_eq!(contacts.name_for("0812345678"), Some("สมชาย ใจดี"));
        assert_eq!(contacts.name_for("081-234-5678"), Some("สมชาย ใจดี"));
        assert_eq!(contacts.name_for("+66 81 234 5678"), Some("สมชาย ใจดี"));
        assert_eq!(contacts.name_for("66812345678"), Some("สมชาย ใจดี"));
        // Second phone of the same contact resolves too.
        assert_eq!(contacts.name_for("02 555 0199"), Some("สมชาย ใจดี"));
        // Nameless contact is skipped; unknown numbers miss.
        assert_eq!(contacts.name_for("0899999999"), None);
        assert_eq!(contacts.name_for("0000000000"), None);
        assert_eq!(contacts.name_for(""), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_corrupt_and_unknown_version_load_empty() {
        let dir = tmpdir("empty");
        assert!(Contacts::load_from(&dir.join("nope.json"))
            .entries
            .is_empty());
        let corrupt = dir.join("bad.json");
        std::fs::write(&corrupt, "{not json").unwrap();
        assert!(Contacts::load_from(&corrupt).entries.is_empty());
        let future = dir.join("future.json");
        std::fs::write(&future, "{\"version\":2,\"contacts\":[]}").unwrap();
        assert!(Contacts::load_from(&future).entries.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
