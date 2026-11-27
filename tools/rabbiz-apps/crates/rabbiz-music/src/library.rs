//! Library model: scan `~/Music` + `~/Downloads` for audio files and the
//! pure helpers (titles, captions, time formatting, transport indices,
//! scope filtering) shared by the UI and the unit tests.
//!
//! Mirrors the QML predecessor (`apps/music/MusicLogic.js`): same
//! extension set, same "newest first" ordering, same `m:ss` clock format.

use std::cmp::Ordering;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

/// Audio extensions the scanner accepts (lowercase, no dot).
pub const AUDIO_EXTS: [&str; 6] = ["mp3", "flac", "ogg", "m4a", "wav", "opus"];

/// Recursion limit below each root — the QML version used `find -maxdepth 4`.
pub const SCAN_MAX_DEPTH: usize = 4;

/// Which library root a track came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Folder {
    Music,
    Downloads,
    Other,
}

impl Folder {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Music => "Music",
            Self::Downloads => "Downloads",
            Self::Other => "Other",
        }
    }
}

/// One playable file.
#[derive(Debug, Clone)]
pub struct Track {
    pub path: PathBuf,
    /// Display title derived from the file name (extension stripped,
    /// underscores collapsed). Tag titles, when mpv reports them, override
    /// this at display time — the file name is the honest fallback.
    pub title: String,
    /// Lowercase extension without the dot ("flac", "mp3", …).
    pub ext: String,
    pub folder: Folder,
    /// Directory below the library root ("" for files directly inside).
    pub subfolder: String,
    /// Seconds since the Unix epoch; 0 when metadata is unavailable.
    pub mtime: i64,
}

impl Track {
    /// Second line of a list row: "Music · Albums/Blue" style.
    #[must_use]
    pub fn caption(&self) -> String {
        if self.subfolder.is_empty() {
            self.folder.label().to_string()
        } else {
            format!("{} · {}", self.folder.label(), self.subfolder)
        }
    }
}

/// Library scope chips: every track, or one folder only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    All,
    Music,
    Downloads,
}

impl Scope {
    pub const ALL: [Scope; 3] = [Scope::All, Scope::Music, Scope::Downloads];

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Music => "Music",
            Self::Downloads => "Downloads",
        }
    }

    #[must_use]
    pub fn folder(self) -> Option<Folder> {
        match self {
            Self::All => None,
            Self::Music => Some(Folder::Music),
            Self::Downloads => Some(Folder::Downloads),
        }
    }
}

/// Result of a library scan.
#[derive(Debug, Default)]
pub struct Scan {
    pub tracks: Vec<Track>,
    /// Human-readable problems (unreadable dirs) for the UI banner.
    pub errors: Vec<String>,
}

/// Scan `home/Music` and `home/Downloads` (depth ≤ [`SCAN_MAX_DEPTH`]).
#[must_use]
pub fn scan(home: &Path) -> Scan {
    let mut out = Scan::default();
    for name in ["Music", "Downloads"] {
        let root = home.join(name);
        match std::fs::read_dir(&root) {
            Ok(_) => walk(&root, 0, home, &mut out.tracks),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => out.errors.push(format!("{}: {e}", root.display())),
        }
    }
    // Newest first, path as the stable tiebreak (matches the QML app).
    out.tracks
        .sort_by(|a, b| b.mtime.cmp(&a.mtime).then_with(|| a.path.cmp(&b.path)));
    out
}

fn walk(dir: &Path, depth: usize, home: &Path, out: &mut Vec<Track>) {
    if depth > SCAN_MAX_DEPTH {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in rd.flatten() {
        let p = entry.path();
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_dir() {
            // Never descend into hidden directories (.thumbnails, .git, …).
            if entry
                .file_name()
                .to_str()
                .is_some_and(|n| n.starts_with('.'))
            {
                continue;
            }
            walk(&p, depth + 1, home, out);
        } else if ft.is_file() && is_audio(&p) {
            out.push(make_track(&p, home));
        }
    }
}

#[must_use]
pub fn is_audio(path: &Path) -> bool {
    ext_of(path).is_some_and(|e| AUDIO_EXTS.contains(&e.as_str()))
}

/// Lowercase extension without the dot; `None` when absent.
#[must_use]
pub fn ext_of(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_lowercase())
        .filter(|s| !s.is_empty())
}

/// Build a [`Track`] for an arbitrary audio file — used by the headless
/// `play <path>` command for files outside the scanned roots.
#[must_use]
pub fn track_for_path(path: &Path, home: &Path) -> Track {
    make_track(path, home)
}

fn make_track(path: &Path, home: &Path) -> Track {
    let (folder, subfolder) = folder_and_sub(path, home);
    let mtime = std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs() as i64);
    Track {
        title: title_from_path(path),
        ext: ext_of(path).unwrap_or_default(),
        path: path.to_path_buf(),
        folder,
        subfolder,
        mtime,
    }
}

/// "01_Dark_Matter.flac" → "01 Dark Matter". Underscores become spaces,
/// whitespace runs collapse; non-ASCII (Thai) passes through untouched.
#[must_use]
pub fn title_from_path(path: &Path) -> String {
    let base = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("Untitled");
    let stem = match base.rfind('.') {
        Some(i) if i > 0 => &base[..i],
        _ => base,
    };
    let cleaned = stem
        .replace('_', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if cleaned.is_empty() {
        "Untitled".to_string()
    } else {
        cleaned
    }
}

/// Resolve the display folder + subfolder for `path`. Files can only appear
/// under Music/Downloads during a scan; anything else is `Other` (defensive).
fn folder_and_sub(path: &Path, home: &Path) -> (Folder, String) {
    for (name, folder) in [("Music", Folder::Music), ("Downloads", Folder::Downloads)] {
        let root = home.join(name);
        if let Ok(rel) = path.strip_prefix(&root) {
            let sub = rel
                .parent()
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default();
            return (folder, sub);
        }
    }
    (Folder::Other, String::new())
}

/// Indices of `tracks` visible under `scope`, in library order.
#[must_use]
pub fn visible_indices(tracks: &[Track], scope: Scope) -> Vec<usize> {
    match scope.folder() {
        None => (0..tracks.len()).collect(),
        Some(f) => tracks
            .iter()
            .enumerate()
            .filter(|(_, t)| t.folder == f)
            .map(|(i, _)| i)
            .collect(),
    }
}

#[must_use]
pub fn count_folder(tracks: &[Track], folder: Folder) -> usize {
    tracks.iter().filter(|t| t.folder == folder).count()
}

// ---------------- transport helpers (port of MusicLogic.js) ----------------

/// Explicit next/previous wrap around the whole library.
#[must_use]
pub fn next_index(index: Option<usize>, count: usize) -> Option<usize> {
    if count == 0 {
        return None;
    }
    Some(match index {
        None => 0,
        Some(i) => (i + 1) % count,
    })
}

#[must_use]
pub fn prev_index(index: Option<usize>, count: usize) -> Option<usize> {
    if count == 0 {
        return None;
    }
    Some(match index {
        None => 0,
        Some(i) => (i + count - 1) % count,
    })
}

/// End-of-track advance: stop at the end of the list instead of looping.
#[must_use]
pub fn auto_next_index(index: Option<usize>, count: usize) -> Option<usize> {
    match index {
        None => (count > 0).then_some(0),
        Some(i) => (i + 1 < count).then_some(i + 1),
    }
}

/// Repeat mode for the GUI transport (headless playback leaves it at `Off`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Repeat {
    /// Stop at the end of the library.
    #[default]
    Off,
    /// Wrap to the first track at the end of the library.
    All,
    /// Replay the current track.
    One,
}

impl Repeat {
    /// Transport-button cycle: Off → All → One → Off (Material convention).
    #[must_use]
    pub fn cycle(self) -> Self {
        match self {
            Self::Off => Self::All,
            Self::All => Self::One,
            Self::One => Self::Off,
        }
    }
}

/// Random-ish index in `0..count` different from `current` (when `count > 1`),
/// driven by a caller-supplied seed — deterministic per (seed, current, count)
/// so tests can assert it; callers mix in wall-clock nanos for real entropy.
/// xorshift64* — enough for "not the same song twice in a row".
#[must_use]
pub fn shuffle_next_index(current: Option<usize>, count: usize, seed: u64) -> Option<usize> {
    if count == 0 {
        return None;
    }
    if count == 1 {
        return Some(0);
    }
    let mut x = seed.max(1);
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    let mut i = (x % count as u64) as usize;
    if Some(i) == current {
        i = (i + 1) % count;
    }
    Some(i)
}

/// Which track plays next at end-of-file under `repeat`/`shuffle`.
/// Repeat-one replays first; shuffle keeps picking random tracks (any mode);
/// repeat-all wraps; off stops at the end of the list.
#[must_use]
pub fn finish_next_index(
    current: Option<usize>,
    count: usize,
    repeat: Repeat,
    shuffle: bool,
    seed: u64,
) -> Option<usize> {
    match repeat {
        Repeat::One => (count > 0).then_some(current.unwrap_or(0)),
        _ if shuffle => shuffle_next_index(current, count, seed),
        Repeat::All => next_index(current, count),
        Repeat::Off => auto_next_index(current, count),
    }
}

/// "0:05", "3:07", "1:02:03"; unknown lengths print "--:--" (never a fake 0).
#[must_use]
pub fn format_time(seconds: f64) -> String {
    if !seconds.is_finite() || seconds < 0.0 {
        return "--:--".to_string();
    }
    let total = seconds.floor() as u64;
    let (h, m, s) = (total / 3600, (total % 3600) / 60, total % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// 0..=1 progress; 0 when the duration is unknown.
#[must_use]
pub fn progress_fraction(elapsed: f64, duration: f64) -> f32 {
    if duration.partial_cmp(&0.0) != Some(Ordering::Greater) || !elapsed.is_finite() {
        return 0.0;
    }
    (elapsed / duration).clamp(0.0, 1.0) as f32
}

/// Seconds to seek to for a 0..=1 bar fraction.
#[must_use]
pub fn seek_seconds(fraction: f32, duration: f64) -> f64 {
    if duration.partial_cmp(&0.0) != Some(Ordering::Greater) {
        return 0.0;
    }
    (fraction.clamp(0.0, 1.0) as f64) * duration
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_from_path_cleans_names() {
        assert_eq!(
            title_from_path(Path::new("/home/r/Downloads/01_Dark_Matter.flac")),
            "01 Dark Matter"
        );
        assert_eq!(title_from_path(Path::new("/m/เพลง_ไทย.mp3")), "เพลง ไทย");
        assert_eq!(title_from_path(Path::new("/m/.hidden.mp3")), ".hidden");
        assert_eq!(title_from_path(Path::new("/m/noext")), "noext");
    }

    #[test]
    fn format_time_matches_spec() {
        assert_eq!(format_time(0.0), "0:00");
        assert_eq!(format_time(65.0), "1:05");
        assert_eq!(format_time(3725.0), "1:02:05");
        assert_eq!(format_time(3599.9), "59:59");
        assert_eq!(format_time(-1.0), "--:--");
        assert_eq!(format_time(f64::NAN), "--:--");
    }

    #[test]
    fn transport_indices_wrap_and_stop() {
        assert_eq!(next_index(None, 3), Some(0));
        assert_eq!(next_index(Some(2), 3), Some(0));
        assert_eq!(next_index(Some(0), 0), None);
        assert_eq!(prev_index(Some(0), 3), Some(2));
        assert_eq!(prev_index(None, 3), Some(0));
        assert_eq!(auto_next_index(Some(1), 3), Some(2));
        assert_eq!(auto_next_index(Some(2), 3), None);
        assert_eq!(auto_next_index(None, 0), None);
    }

    #[test]
    fn shuffle_never_repeats_current_and_covers_range() {
        for count in [2usize, 5, 17] {
            for current in [None, Some(0), Some(count - 1)] {
                for seed in 0..40u64 {
                    let i = shuffle_next_index(current, count, seed).unwrap();
                    assert!(i < count);
                    assert_ne!(Some(i), current, "shuffle repeated current");
                }
            }
        }
        assert_eq!(shuffle_next_index(Some(0), 1, 7), Some(0));
        assert_eq!(shuffle_next_index(Some(0), 0, 7), None);
        // Deterministic per seed.
        assert_eq!(
            shuffle_next_index(Some(2), 9, 1234),
            shuffle_next_index(Some(2), 9, 1234)
        );
        // Different seeds reach different indices (not a fixed rotation).
        let hits: std::collections::BTreeSet<_> = (0..32u64)
            .map(|s| shuffle_next_index(Some(0), 4, s).unwrap())
            .collect();
        assert!(hits.len() > 1);
    }

    #[test]
    fn finish_next_honors_repeat_and_shuffle() {
        assert_eq!(
            finish_next_index(Some(1), 3, Repeat::One, false, 0),
            Some(1)
        );
        assert_eq!(
            finish_next_index(Some(2), 3, Repeat::All, false, 0),
            Some(0)
        );
        assert_eq!(finish_next_index(Some(2), 3, Repeat::Off, false, 0), None);
        // Shuffle + repeat-off still keeps playing (random ≠ current).
        let i = finish_next_index(Some(0), 4, Repeat::Off, true, 9).unwrap();
        assert_ne!(i, 0);
        assert_eq!(finish_next_index(None, 0, Repeat::One, true, 0), None);
        // Cycle order.
        assert_eq!(Repeat::Off.cycle(), Repeat::All);
        assert_eq!(Repeat::All.cycle(), Repeat::One);
        assert_eq!(Repeat::One.cycle(), Repeat::Off);
    }

    #[test]
    fn progress_and_seek_clamp() {
        assert_eq!(progress_fraction(30.0, 60.0), 0.5);
        assert_eq!(progress_fraction(90.0, 60.0), 1.0);
        assert_eq!(progress_fraction(10.0, 0.0), 0.0);
        assert_eq!(seek_seconds(0.5, 200.0), 100.0);
        assert_eq!(seek_seconds(2.0, 200.0), 200.0);
        assert_eq!(seek_seconds(0.5, 0.0), 0.0);
    }

    #[test]
    fn scan_finds_audio_recursively_and_sorts_newest_first() {
        let tmp = std::env::temp_dir().join(format!("rabbiz-music-test-{}", std::process::id()));
        let music = tmp.join("Music");
        let dl = tmp.join("Downloads");
        std::fs::create_dir_all(music.join("Albums/Blue")).unwrap();
        std::fs::create_dir_all(&dl).unwrap();
        std::fs::write(music.join("Albums/Blue/01_Song.flac"), b"x").unwrap();
        std::fs::write(music.join("notes.txt"), b"x").unwrap();
        std::fs::write(dl.join("podcast.OPUS"), b"x").unwrap();
        std::fs::create_dir_all(music.join(".hidden")).unwrap();
        std::fs::write(music.join(".hidden/secret.mp3"), b"x").unwrap();

        let scan = scan(&tmp);
        assert_eq!(scan.tracks.len(), 2, "errors: {:?}", scan.errors);
        assert!(scan.errors.is_empty());
        let flac = scan
            .tracks
            .iter()
            .find(|t| t.ext == "flac")
            .expect("flac track");
        assert_eq!(flac.folder, Folder::Music);
        assert_eq!(flac.subfolder, "Albums/Blue");
        assert_eq!(flac.caption(), "Music · Albums/Blue");
        let opus = scan.tracks.iter().find(|t| t.ext == "opus").expect("opus");
        assert_eq!(opus.folder, Folder::Downloads);

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn visible_indices_and_counts_respect_scope() {
        let tracks = vec![
            Track {
                path: PathBuf::from("/m/a.mp3"),
                title: "a".into(),
                ext: "mp3".into(),
                folder: Folder::Music,
                subfolder: String::new(),
                mtime: 0,
            },
            Track {
                path: PathBuf::from("/d/b.mp3"),
                title: "b".into(),
                ext: "mp3".into(),
                folder: Folder::Downloads,
                subfolder: String::new(),
                mtime: 0,
            },
        ];
        assert_eq!(visible_indices(&tracks, Scope::All), vec![0, 1]);
        assert_eq!(visible_indices(&tracks, Scope::Music), vec![0]);
        assert_eq!(visible_indices(&tracks, Scope::Downloads), vec![1]);
        assert_eq!(count_folder(&tracks, Folder::Music), 1);
    }
}
