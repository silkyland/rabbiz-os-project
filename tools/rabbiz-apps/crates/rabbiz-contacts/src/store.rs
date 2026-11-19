//! Contacts persistence — the shared store contract for RabbizOS.
//!
//! # Schema (v1)
//!
//! File: `$XDG_DATA_HOME/rabbiz-contacts/contacts.json` (or
//! `~/.local/share/rabbiz-contacts/contacts.json`). UTF-8 JSON, one object:
//!
//! ```json
//! {"version":1,"contacts":[{
//!   "id":"c-<unix-nanos>",
//!   "name":"…",
//!   "phones":[{"label":"mobile","number":"+66…"}],
//!   "emails":[{"label":"home","address":"…"}],
//!   "note":"…",
//!   "favorite":false,
//!   "created":<unix-seconds>,
//!   "updated":<unix-seconds>
//! }]}
//! ```
//!
//! Rules other readers (rabbiz-dialer, rabbiz-sms, the QML shell) can rely on:
//! - `version` is `1`; treat unknown versions as unreadable.
//! - `id` is `"c-"` + nanoseconds; unique per contact, stable forever.
//! - `phones`/`emails` are arrays of `{label, number|address}`; `label` is a
//!   free-form string ("mobile", "work", …) — readers must tolerate values
//!   they don't know. Numbers may be in any dialable form (`081-…`, `+66…`).
//! - `created`/`updated` are unix seconds.
//! - Sort order for display: `favorite` first, then `name` case-insensitive
//!   (`model::sort_contacts` is the reference implementation).
//! - Writes are atomic (tmp + rename); a crash never leaves a torn file.
//! - A corrupt file is quarantined to `contacts.json.bak` and treated as
//!   empty — the store self-heals on next launch.
//!
//! First run with no file seeds the same 8 demo contacts the dialer ships
//! (`model::demo_seed`) and writes them out, so Contacts/Dialer/SMS agree.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::model::{self, Contact};

const CURRENT_V: u32 = 1;
const FILE_NAME: &str = "contacts.json";
const BAK_NAME: &str = "contacts.json.bak";

/// On-disk envelope. `version` gates future migrations.
#[derive(Debug, Serialize, Deserialize)]
struct StoreFile {
    #[serde(default = "default_version")]
    version: u32,
    #[serde(default)]
    contacts: Vec<Contact>,
}

fn default_version() -> u32 {
    CURRENT_V
}

/// `~/.local/share/rabbiz-contacts` (or `$XDG_DATA_HOME/rabbiz-contacts`).
#[must_use]
pub fn data_dir() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("rabbiz-contacts")
}

fn store_path(dir: &Path) -> PathBuf {
    dir.join(FILE_NAME)
}

/// Load all contacts from `dir`, sorted for display (favorites first, then
/// name). Behaviours:
/// - missing file → seed demo contacts, persist them, return them;
/// - unreadable/corrupt/unknown-version file → renamed to `contacts.json.bak`
///   and treated as empty (then seeded like a missing file);
/// - valid file → returned as stored (still re-sorted for display).
#[must_use]
pub fn load(dir: &Path) -> Vec<Contact> {
    let path = store_path(dir);
    match fs::read_to_string(&path) {
        Ok(raw) => match serde_json::from_str::<StoreFile>(&raw) {
            Ok(f) if f.version == CURRENT_V => {
                let mut contacts = f.contacts;
                model::sort_contacts(&mut contacts);
                contacts
            }
            _ => {
                // Corrupt JSON or an unknown future version: keep the bytes
                // for debugging, start clean, seed below.
                let _ = fs::rename(&path, dir.join(BAK_NAME));
                seed_and_save(dir)
            }
        },
        Err(_) => seed_and_save(dir),
    }
}

/// Load from the real data dir.
#[allow(dead_code)] // convenience for future callers/tests; app uses data_dir+load
#[must_use]
pub fn load_default() -> Vec<Contact> {
    load(&data_dir())
}

/// Persist the whole contact list atomically: write `contacts.json.tmp`
/// then rename over `contacts.json`. The tmp name doesn't match `*.json`
/// readers and a crash leaves at most a stale tmp, never a torn store.
pub fn save(dir: &Path, contacts: &[Contact]) {
    let _ = fs::create_dir_all(dir);
    let file = StoreFile {
        version: CURRENT_V,
        contacts: contacts.to_vec(),
    };
    let Ok(json) = serde_json::to_string(&file) else {
        return;
    };
    let tmp = dir.join("contacts.json.tmp");
    let dst = store_path(dir);
    let ok = (|| -> std::io::Result<()> {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(json.as_bytes())?;
        f.sync_all()?;
        drop(f);
        fs::rename(&tmp, &dst)?;
        Ok(())
    })();
    if ok.is_err() {
        let _ = fs::remove_file(&tmp);
    }
}

fn seed_and_save(dir: &Path) -> Vec<Contact> {
    let mut contacts = model::demo_seed(model::now_unix());
    model::sort_contacts(&mut contacts);
    save(dir, &contacts);
    contacts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "rabbiz-contacts-test-{}-{}-{}",
            tag,
            std::process::id(),
            model::now_unix()
        ));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn missing_file_seeds_demo_contacts() {
        let dir = tmpdir("seed").join("nope");
        let contacts = load(&dir);
        assert_eq!(contacts.len(), 8);
        assert!(store_path(&dir).exists());
        let _ = fs::remove_dir_all(dir.parent().unwrap());
    }

    #[test]
    fn round_trip_preserves_schema() {
        let dir = tmpdir("roundtrip");
        let mut c = Contact::new("c-123".into(), "สมชาย ใจดี", 1_700_000_000);
        c.phones.push(model::Phone {
            label: "mobile".into(),
            number: "+66 81 234 5678".into(),
        });
        c.emails.push(model::Email {
            label: "home".into(),
            address: "s@example.com".into(),
        });
        c.note = "met at coffee shop".into();
        c.favorite = true;
        save(&dir, &[c.clone()]);

        let raw = fs::read_to_string(store_path(&dir)).unwrap();
        // Exact wire shape the contract promises.
        assert!(raw.contains("\"version\":1"));
        assert!(raw.contains("\"phones\":[{\"label\":\"mobile\""));
        assert!(raw.contains("สมชาย"));

        let loaded = load(&dir);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0], c);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn corrupt_file_quarantined_and_reseeded() {
        let dir = tmpdir("corrupt");
        fs::create_dir_all(&dir).unwrap();
        fs::write(store_path(&dir), "{not json").unwrap();
        let contacts = load(&dir);
        assert_eq!(contacts.len(), 8); // reseeded
        assert!(dir.join(BAK_NAME).exists());
        assert!(store_path(&dir).exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn unknown_version_quarantined() {
        let dir = tmpdir("version");
        fs::create_dir_all(&dir).unwrap();
        fs::write(store_path(&dir), "{\"version\":99,\"contacts\":[]}").unwrap();
        let contacts = load(&dir);
        assert_eq!(contacts.len(), 8);
        assert!(dir.join(BAK_NAME).exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn atomic_write_leaves_no_tmp() {
        let dir = tmpdir("atomic");
        save(&dir, &model::demo_seed(1));
        let entries: Vec<_> = fs::read_dir(&dir).unwrap().flatten().collect();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].file_name(), FILE_NAME);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn empty_contacts_array_loads_empty_not_seeded() {
        // A user who deleted every contact must not get the demo seed back.
        let dir = tmpdir("empty");
        fs::create_dir_all(&dir).unwrap();
        fs::write(store_path(&dir), "{\"version\":1,\"contacts\":[]}").unwrap();
        assert!(load(&dir).is_empty());
        let _ = fs::remove_dir_all(&dir);
    }
}
