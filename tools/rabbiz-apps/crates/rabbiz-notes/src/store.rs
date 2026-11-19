//! Note persistence per docs/BASE-APPS.md: one JSON file per note at
//! `~/.local/share/rabbiz-notes/<id>.json` (honouring `XDG_DATA_HOME`),
//! written atomically via tmp-file + rename so a crash never leaves a
//! half-written note. Corrupt/unreadable files are skipped on load, never
//! fatal.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::model::{derive_title, normalize_body, now_unix, Note};

/// On-disk schema. `v` is a format version for future migrations; unknown
/// extra fields are ignored by serde so newer files still load.
#[derive(Debug, Serialize, Deserialize)]
struct NoteFile {
    #[serde(default = "default_v")]
    v: u32,
    id: u64,
    #[serde(default)]
    title: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    updated: i64,
}

fn default_v() -> u32 {
    1
}

const CURRENT_V: u32 = 1;

/// `~/.local/share/rabbiz-notes` (or `$XDG_DATA_HOME/rabbiz-notes`).
#[must_use]
pub fn data_dir() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("rabbiz-notes")
}

fn note_path(dir: &Path, id: u64) -> PathBuf {
    dir.join(format!("{id}.json"))
}

/// Load every readable note in `dir`, sorted by `updated` descending
/// (Google Keep order: most recently edited first). Corrupt files,
/// non-UTF-8 files and non-note files are skipped silently.
#[must_use]
pub fn load_all(dir: &Path) -> Vec<Note> {
    let mut notes = Vec::new();
    let Ok(rd) = fs::read_dir(dir) else {
        return notes;
    };
    for ent in rd.flatten() {
        let path = ent.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Ok(raw) = fs::read_to_string(&path) else {
            continue;
        };
        let Ok(f) = serde_json::from_str::<NoteFile>(&raw) else {
            continue;
        };
        if f.v != CURRENT_V {
            // Unknown future format: skip rather than guess.
            continue;
        }
        let body = normalize_body(&f.body);
        notes.push(Note {
            id: f.id,
            // Title is derived, not trusted: a stale/absent stored title
            // must never disagree with the body the user sees.
            title: derive_title(&body),
            body,
            updated: f.updated,
        });
    }
    notes.sort_by(|a, b| b.updated.cmp(&a.updated).then(a.id.cmp(&b.id)));
    notes
}

/// Load from the real data dir.
#[allow(dead_code)] // used by tests and future callers; app uses data_dir+load_all
#[must_use]
pub fn load_all_default() -> Vec<Note> {
    load_all(&data_dir())
}

/// Persist `note` atomically: write `.<id>.json.tmp` then rename over
/// `<id>.json`. The tmp name is dot-prefixed so `load_all` never picks it up
/// even if a crash leaves it behind.
pub fn save_note(dir: &Path, note: &Note) {
    let _ = fs::create_dir_all(dir);
    let file = NoteFile {
        v: CURRENT_V,
        id: note.id,
        title: note.title.clone(),
        body: note.body.clone(),
        updated: note.updated,
    };
    let Ok(json) = serde_json::to_string(&file) else {
        return;
    };
    let tmp = dir.join(format!(".{}.json.tmp", note.id));
    let dst = note_path(dir, note.id);
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

/// Delete `<id>.json` and any stale tmp for it. Missing files are fine.
pub fn delete_note(dir: &Path, id: u64) {
    let _ = fs::remove_file(note_path(dir, id));
    let _ = fs::remove_file(dir.join(format!(".{id}.json.tmp")));
}

/// One-time import of the pre-`BASE-APPS` format: a single `notes.json`
/// holding `[{"id":…,"text":"…"}]`. Each entry becomes `<id>.json`; the
/// legacy file is renamed to `notes.json.imported` (kept as a backup, and
/// ignored by `load_all` since it no longer ends in `.json`).
pub fn migrate_legacy(dir: &Path) {
    let legacy = dir.join("notes.json");
    if !legacy.exists() {
        return;
    }
    #[derive(Deserialize)]
    struct Legacy {
        id: u64,
        #[serde(default)]
        text: String,
    }
    let Ok(raw) = fs::read_to_string(&legacy) else {
        return;
    };
    // New format is also valid JSON but an object, not an array — the Vec
    // parse fails and we leave the file alone.
    if let Ok(items) = serde_json::from_str::<Vec<Legacy>>(&raw) {
        // Legacy stored newest-first (insert(0)); keep that order by giving
        // earlier entries a slightly later timestamp.
        let now = now_unix();
        for (i, it) in items.iter().enumerate() {
            let n = Note::new(it.id, &it.text, now - i as i64);
            save_note(dir, &n);
        }
        let _ = fs::rename(&legacy, dir.join("notes.json.imported"));
    }
}

/// Allocate an id that doesn't collide with `existing` or on-disk files.
/// Nanosecond timestamp + probe — collisions only under same-ns creation.
#[must_use]
pub fn fresh_id(dir: &Path, existing: &[Note]) -> u64 {
    let base = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or_else(|_| now_unix() as u64);
    let mut id = base.max(1);
    loop {
        if !existing.iter().any(|n| n.id == id) && !note_path(dir, id).exists() {
            return id;
        }
        id += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "rabbiz-notes-test-{}-{}-{}",
            tag,
            std::process::id(),
            now_unix()
        ));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn round_trip() {
        let dir = tmpdir("roundtrip");
        let n = Note::new(42, "สวัสดี\nhello world", 1_700_000_000);
        save_note(&dir, &n);
        let loaded = load_all(&dir);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, 42);
        assert_eq!(loaded[0].body, n.body);
        assert_eq!(loaded[0].title, "สวัสดี");
        assert_eq!(loaded[0].updated, n.updated);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn atomic_write_leaves_no_tmp() {
        let dir = tmpdir("atomic");
        save_note(&dir, &Note::new(7, "x", 1));
        let entries: Vec<_> = fs::read_dir(&dir).unwrap().flatten().collect();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].file_name(), "7.json");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn corrupt_files_are_skipped() {
        let dir = tmpdir("corrupt");
        fs::write(dir.join("1.json"), "{not json").unwrap();
        fs::write(dir.join("2.json"), "\u{fffd}invalid utf8 would fail read").unwrap();
        // Valid JSON but wrong shape still parses via serde defaults.
        fs::write(dir.join("3.json"), "{\"v\":1,\"id\":3,\"body\":\"ok\"}").unwrap();
        fs::write(dir.join("4.txt"), "ignored").unwrap();
        // Unknown future version skipped.
        fs::write(
            dir.join("5.json"),
            "{\"v\":99,\"id\":5,\"body\":\"future\"}",
        )
        .unwrap();
        let notes = load_all(&dir);
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].id, 3);
        assert_eq!(notes[0].body, "ok");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_dir_loads_empty() {
        let dir = tmpdir("missing").join("nope");
        assert!(load_all(&dir).is_empty());
    }

    #[test]
    fn delete_removes_file() {
        let dir = tmpdir("delete");
        save_note(&dir, &Note::new(9, "bye", 1));
        assert_eq!(load_all(&dir).len(), 1);
        delete_note(&dir, 9);
        assert!(load_all(&dir).is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn sorted_by_updated_desc() {
        let dir = tmpdir("sort");
        save_note(&dir, &Note::new(1, "old", 100));
        save_note(&dir, &Note::new(2, "new", 300));
        save_note(&dir, &Note::new(3, "mid", 200));
        let notes = load_all(&dir);
        let ids: Vec<u64> = notes.iter().map(|n| n.id).collect();
        assert_eq!(ids, vec![2, 3, 1]);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn fresh_id_unique() {
        let dir = tmpdir("fresh");
        let n = Note::new(5, "x", 1);
        let id = fresh_id(&dir, &[n]);
        assert_ne!(id, 5);
        assert!(!note_path(&dir, id).exists());
        let _ = fs::remove_dir_all(&dir);
    }
}
