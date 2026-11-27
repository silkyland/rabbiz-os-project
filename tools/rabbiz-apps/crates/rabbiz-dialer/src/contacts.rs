//! Read-only reader for the shared contacts store written by
//! `rabbiz-contacts`: `$XDG_DATA_HOME/rabbiz-contacts/contacts.json`
//! (fallback `~/.local/share/rabbiz-contacts/contacts.json`).
//!
//! Schema contract (owned by the contacts crate's `store.rs`, version 1):
//! `{ "version":1, "contacts":[{ "id","name","phones":[{"label","number"}],
//! "emails":[…], "note","favorite","created","updated" }] }`. Unknown
//! versions and unparseable files are treated as "no store" — this crate
//! never writes or quarantines the file; the caller falls back to the
//! built-in demo contacts (`model::builtin_contacts`).

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::model::{self, Contact};

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
    #[serde(default)]
    favorite: bool,
}

#[derive(Debug, Deserialize)]
struct StorePhone {
    #[serde(default)]
    label: String,
    #[serde(default)]
    number: String,
}

/// `$XDG_DATA_HOME/rabbiz-contacts/contacts.json` (or the
/// `~/.local/share` fallback) — the contacts crate's data dir, NOT this
/// app's `rabbiz-dialer` dir.
#[must_use]
pub fn store_path() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("rabbiz-contacts")
        .join("contacts.json")
}

/// Load contacts from an explicit file. `None` when the file is missing,
/// unparseable, or carries an unknown schema version. Each `phones[]`
/// entry becomes one [`Contact`] row so every number of a contact matches
/// and dials independently.
#[must_use]
pub fn load_from(path: &Path) -> Option<Vec<Contact>> {
    let raw = std::fs::read_to_string(path).ok()?;
    let f: StoreFile = serde_json::from_str(&raw).ok()?;
    if f.version != 1 {
        return None;
    }
    let mut out = Vec::new();
    for c in &f.contacts {
        for p in &c.phones {
            // Normalize to dialable form (digits, `*`, `#`, leading `+`);
            // empty results (label-only rows) are skipped.
            let number = model::digits_only(&p.number);
            if number.is_empty() {
                continue;
            }
            out.push(Contact {
                name: c.name.clone(),
                number,
                label: label_display(&p.label),
                color: avatar_tint(&c.name, &p.number),
                favorite: c.favorite,
            });
        }
    }
    Some(out)
}

/// Load from the shared store; a valid empty store stays empty.
fn resolve_from(path: &Path) -> Vec<Contact> {
    load_from(path).unwrap_or_else(model::builtin_contacts)
}

#[must_use]
pub fn resolved() -> Vec<Contact> {
    resolve_from(&store_path())
}

/// Look up a dialed/received number in the resolved list.
#[must_use]
pub fn lookup(number: &str) -> Option<Contact> {
    model::lookup_in(&resolved(), number).cloned()
}

/// Free-form store label → display casing ("mobile" → "Mobile",
/// "home fax" → "Home fax"); empty → "Phone".
fn label_display(raw: &str) -> String {
    let t = raw.trim();
    if t.is_empty() {
        return "Phone".to_string();
    }
    let mut chars = t.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => "Phone".to_string(),
    }
}

/// Avatar tint — the shared app-kit palette (`ui::avatar_tint`), so a
/// contact looks identical in the dialer, contacts and sms apps.
fn avatar_tint(name: &str, number: &str) -> [u8; 4] {
    let seed = if name.trim().is_empty() { number } else { name };
    rabbiz_app_kit::ui::avatar_tint(seed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "rabbiz-dialer-contacts-test-{}-{}-{}",
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
        {"id":"c-1","name":"Store Person","phones":[
            {"label":"mobile","number":"0811112222"},
            {"label":"work","number":"+66 2 555 0199"}],
         "emails":[],"note":"","favorite":true,"created":1,"updated":1},
        {"id":"c-2","name":"No Phones","phones":[],"emails":[],
         "note":"","favorite":false,"created":1,"updated":1}
    ]}"#;

    #[test]
    fn load_from_parses_each_phone() {
        let dir = tmpdir("parse");
        let path = dir.join("contacts.json");
        std::fs::write(&path, FIXTURE).unwrap();
        let contacts = load_from(&path).unwrap();
        // c-1 contributes two rows (one per phone); c-2 contributes none.
        assert_eq!(contacts.len(), 2);
        assert!(contacts.iter().all(|c| c.name == "Store Person"));
        assert!(contacts.iter().all(|c| c.favorite));
        assert_eq!(contacts[0].label, "Mobile");
        assert_eq!(contacts[1].label, "Work");
        // "+66 2 …" normalized to dialable digits.
        assert_eq!(contacts[1].number, "+6625550199");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn lookup_matches_any_phone_with_country_fold() {
        let dir = tmpdir("lookup");
        let path = dir.join("contacts.json");
        std::fs::write(&path, FIXTURE).unwrap();
        let contacts = load_from(&path).unwrap();
        // First phone, local form.
        assert_eq!(
            model::lookup_in(&contacts, "0811112222").map(|c| c.label.as_str()),
            Some("Mobile")
        );
        // Second phone entered in +66 international form folds to 0-trunk.
        assert_eq!(
            model::lookup_in(&contacts, "+66 2 555 0199").map(|c| c.label.as_str()),
            Some("Work")
        );
        assert_eq!(
            model::lookup_in(&contacts, "025550199").map(|c| c.label.as_str()),
            Some("Work")
        );
        assert!(model::lookup_in(&contacts, "999999999").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_corrupt_and_unknown_version_fall_back() {
        let dir = tmpdir("fallback");
        let missing = dir.join("nope.json");
        assert!(load_from(&missing).is_none());

        let corrupt = dir.join("corrupt.json");
        std::fs::write(&corrupt, "{not json").unwrap();
        assert!(load_from(&corrupt).is_none());

        let future = dir.join("future.json");
        std::fs::write(&future, "{\"version\":99,\"contacts\":[]}").unwrap();
        assert!(load_from(&future).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn resolved_prefers_store_over_builtin() {
        let dir = tmpdir("xdg");
        let path = dir.join("contacts.json");
        std::fs::write(&path, FIXTURE).unwrap();

        let contacts = resolve_from(&path);
        assert_eq!(contacts.len(), 2);
        assert!(contacts.iter().all(|c| c.name == "Store Person"));

        std::fs::write(&path, r#"{"version":1,"contacts":[]}"#).unwrap();
        assert!(resolve_from(&path).is_empty());

        std::fs::write(&path, "{oops").unwrap();
        let contacts = resolve_from(&path);
        assert_eq!(contacts.len(), model::builtin_contacts().len());
        assert!(contacts.iter().any(|c| c.name == "Mom"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
