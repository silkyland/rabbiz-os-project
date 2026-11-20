//! MPRIS media-player state tracking and secure album-art URL validation.
//!
//! Pure `std` data model & tracker for `org.mpris.MediaPlayer2.Player`
//! snapshots (`docs/DE-NOTIFICATIONS-SERVICES.md` §2.5). The async D-Bus
//! watcher pushes [`MediaSnapshot`] updates into [`MprisTracker`] and
//! receives [`MediaCommand`] requests over a channel so the compositor loop
//! never blocks on D-Bus or remote HTTP fetches.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

/// Maximum accepted size for local album-art files (2 MiB).
pub const MAX_ART_BYTES: u64 = 2 * 1024 * 1024;

/// Point-in-time playback snapshot for one MPRIS media player.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MediaSnapshot {
    pub bus_name: String,
    pub player_name: String,
    pub title: String,
    pub artist: String,
    pub playing: bool,
    pub can_go_next: bool,
    pub can_go_previous: bool,
    /// Validated local `file://` path to album artwork; never a remote URL.
    pub art_path: Option<String>,
}

/// Transport command sent asynchronously to the active MPRIS player.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaCommand {
    PlayPause,
    Next,
    Previous,
}

/// Validate an `mpris:artUrl` value against `allowed_roots`.
///
/// Security rules:
/// - Accepts **only** `file://` URLs (`http://`, `https://`, `data:`, and
///   relative paths are rejected to prevent SSRF and compositor stalls).
/// - Percent-decodes `%XX` sequences safely (rejects invalid escapes and NUL).
/// - Rejects any `..` path traversal component.
/// - Requires the canonicalized path (or canonicalized parent, or lexical path
///   when neither exists on disk) to reside inside one of `allowed_roots`.
/// - When the target file exists on disk, rejects files larger than
///   [`MAX_ART_BYTES`].
#[must_use]
pub fn validate_art_url(url: &str, allowed_roots: &[&Path]) -> Option<PathBuf> {
    if allowed_roots.is_empty() {
        return None;
    }

    let rest = url.strip_prefix("file://")?;
    let raw_path = rest.strip_prefix("localhost").unwrap_or(rest);
    if !raw_path.starts_with('/') {
        return None;
    }

    let decoded = percent_decode(raw_path)?;
    if decoded.is_empty() || decoded.contains('\0') {
        return None;
    }

    let path = Path::new(&decoded);
    if !path.is_absolute() || has_parent_dir(path) {
        return None;
    }

    let normalized = normalize_lexical(path)?;
    let file_name = normalized.file_name()?;

    for root in allowed_roots {
        if !root.is_absolute() || has_parent_dir(root) {
            continue;
        }

        // Case 1: both target and root exist on disk -> full canonical check.
        if let (Ok(canon_path), Ok(canon_root)) = (
            std::fs::canonicalize(&normalized),
            std::fs::canonicalize(root),
        ) {
            if canon_path.starts_with(&canon_root) && canon_path != canon_root {
                if let Ok(meta) = std::fs::metadata(&canon_path) {
                    if meta.is_file() && meta.len() > MAX_ART_BYTES {
                        return None;
                    }
                }
                return Some(canon_path);
            }
            continue;
        }

        // Case 2: parent directory and root exist -> canonicalize parent to
        // catch symlink escapes even when the artwork file has not been written.
        if let Some(parent) = normalized.parent() {
            if let (Ok(canon_parent), Ok(canon_root)) =
                (std::fs::canonicalize(parent), std::fs::canonicalize(root))
            {
                if canon_parent.starts_with(&canon_root) {
                    return Some(canon_parent.join(file_name));
                }
                continue;
            }
        }

        // Case 3: synthetic/non-existent paths -> lexical containment check.
        if let Some(norm_root) = normalize_lexical(root) {
            if normalized.starts_with(&norm_root) && normalized != norm_root {
                return Some(normalized);
            }
        }
    }

    None
}

fn percent_decode(input: &str) -> Option<String> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if i + 2 >= bytes.len() {
                return None;
            }
            let hi = hex_nibble(bytes[i + 1])?;
            let lo = hex_nibble(bytes[i + 2])?;
            let byte = (hi << 4) | lo;
            if byte == 0 {
                return None;
            }
            out.push(byte);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

fn hex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn has_parent_dir(path: &Path) -> bool {
    path.components().any(|c| matches!(c, Component::ParentDir))
}

fn normalize_lexical(path: &Path) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    for comp in path.components() {
        match comp {
            Component::RootDir => out.push(comp.as_os_str()),
            Component::Normal(seg) => out.push(seg),
            Component::CurDir => {}
            Component::ParentDir | Component::Prefix(_) => return None,
        }
    }
    Some(out)
}

#[derive(Clone, Debug)]
struct TrackedPlayer {
    snapshot: MediaSnapshot,
    updated_ms: u64,
    seq: u64,
}

/// Tracks active MPRIS players and selects the primary one for the shade card.
#[derive(Debug, Default)]
pub struct MprisTracker {
    players: HashMap<String, TrackedPlayer>,
    next_seq: u64,
}

impl MprisTracker {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert or update a player snapshot at timestamp `now_ms`.
    pub fn upsert(&mut self, snapshot: MediaSnapshot, now_ms: u64) {
        self.next_seq = self.next_seq.wrapping_add(1);
        let key = snapshot.bus_name.clone();
        self.players.insert(
            key,
            TrackedPlayer {
                snapshot,
                updated_ms: now_ms,
                seq: self.next_seq,
            },
        );
    }

    /// Remove a player that vanished from the session bus.
    pub fn remove(&mut self, bus_name: &str) {
        self.players.remove(bus_name);
    }

    /// Select the active player: prefers the most recently updated player with
    /// `playing == true`; if none is playing, falls back to the most recently
    /// updated player overall.
    #[must_use]
    pub fn active(&self) -> Option<&MediaSnapshot> {
        self.players
            .values()
            .filter(|p| p.snapshot.playing)
            .max_by_key(|p| (p.updated_ms, p.seq))
            .or_else(|| self.players.values().max_by_key(|p| (p.updated_ms, p.seq)))
            .map(|p| &p.snapshot)
    }

    /// Number of tracked players currently on the bus.
    #[must_use]
    pub fn player_count(&self) -> usize {
        self.players.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn player(bus: &str, name: &str, title: &str, playing: bool) -> MediaSnapshot {
        MediaSnapshot {
            bus_name: bus.into(),
            player_name: name.into(),
            title: title.into(),
            artist: "Artist".into(),
            playing,
            can_go_next: true,
            can_go_previous: true,
            art_path: None,
        }
    }

    #[test]
    fn validate_art_url_accepts_valid_cache_path() {
        let root = Path::new("/home/rabbiz/.cache/mpris");
        let got = validate_art_url(
            "file:///home/rabbiz/.cache/mpris/album%20cover.png",
            &[root],
        );
        assert_eq!(
            got,
            Some(PathBuf::from("/home/rabbiz/.cache/mpris/album cover.png"))
        );
    }

    #[test]
    fn validate_art_url_accepts_localhost_file_url() {
        let root = Path::new("/run/user/1000/rabbiz");
        let got = validate_art_url("file://localhost/run/user/1000/rabbiz/art.jpg", &[root]);
        assert_eq!(got, Some(PathBuf::from("/run/user/1000/rabbiz/art.jpg")));
    }

    #[test]
    fn validate_art_url_rejects_remote_and_non_file_schemes() {
        let root = Path::new("/tmp");
        assert_eq!(
            validate_art_url("http://example.com/art.jpg", &[root]),
            None
        );
        assert_eq!(
            validate_art_url("https://cdn.example.com/art.jpg", &[root]),
            None
        );
        assert_eq!(
            validate_art_url("data:image/png;base64,AAAA", &[root]),
            None
        );
        assert_eq!(validate_art_url("relative/cover.png", &[root]), None);
        assert_eq!(validate_art_url("/tmp/cover.png", &[root]), None);
        assert_eq!(
            validate_art_url("file://evil.host/tmp/cover.png", &[root]),
            None
        );
    }

    #[test]
    fn validate_art_url_rejects_traversal_and_outside_root() {
        let root = Path::new("/home/rabbiz/.cache");
        assert_eq!(
            validate_art_url("file:///home/rabbiz/.cache/../.ssh/id_rsa", &[root]),
            None
        );
        assert_eq!(
            validate_art_url("file:///home/rabbiz/.cache/%2e%2e/secret.png", &[root]),
            None
        );
        assert_eq!(validate_art_url("file:///etc/passwd", &[root]), None);
        // Sibling prefix with shared string prefix must not match.
        assert_eq!(
            validate_art_url("file:///home/rabbiz/.cache2/art.png", &[root]),
            None
        );
        // Bad percent escape and NUL byte.
        assert_eq!(
            validate_art_url("file:///home/rabbiz/.cache/bad%2.png", &[root]),
            None
        );
        assert_eq!(
            validate_art_url("file:///home/rabbiz/.cache/nul%00.png", &[root]),
            None
        );
    }

    #[test]
    fn tracker_prefers_playing_over_newer_paused() {
        let mut tracker = MprisTracker::new();
        assert_eq!(tracker.player_count(), 0);
        assert_eq!(tracker.active(), None);

        tracker.upsert(
            player("org.mpris.MediaPlayer2.music", "Music", "Track A", true),
            100,
        );
        tracker.upsert(
            player(
                "org.mpris.MediaPlayer2.browser",
                "Browser",
                "Video B",
                false,
            ),
            200,
        );

        assert_eq!(tracker.player_count(), 2);
        let active = tracker.active().expect("active player");
        assert_eq!(active.bus_name, "org.mpris.MediaPlayer2.music");
        assert!(active.playing);
    }

    #[test]
    fn tracker_falls_back_to_most_recent_when_all_paused() {
        let mut tracker = MprisTracker::new();
        tracker.upsert(
            player("org.mpris.MediaPlayer2.a", "Player A", "Song 1", false),
            100,
        );
        tracker.upsert(
            player("org.mpris.MediaPlayer2.b", "Player B", "Song 2", false),
            250,
        );
        assert_eq!(
            tracker.active().map(|p| p.bus_name.as_str()),
            Some("org.mpris.MediaPlayer2.b")
        );

        // Updating A later makes A the fallback.
        tracker.upsert(
            player("org.mpris.MediaPlayer2.a", "Player A", "Song 1b", false),
            300,
        );
        assert_eq!(
            tracker.active().map(|p| p.bus_name.as_str()),
            Some("org.mpris.MediaPlayer2.a")
        );
    }

    #[test]
    fn tracker_removal_promotes_next_player() {
        let mut tracker = MprisTracker::new();
        tracker.upsert(
            player("org.mpris.MediaPlayer2.a", "Player A", "Song 1", true),
            100,
        );
        tracker.upsert(
            player("org.mpris.MediaPlayer2.b", "Player B", "Song 2", true),
            200,
        );
        assert_eq!(
            tracker.active().map(|p| p.bus_name.as_str()),
            Some("org.mpris.MediaPlayer2.b")
        );

        tracker.remove("org.mpris.MediaPlayer2.b");
        assert_eq!(tracker.player_count(), 1);
        assert_eq!(
            tracker.active().map(|p| p.bus_name.as_str()),
            Some("org.mpris.MediaPlayer2.a")
        );

        tracker.remove("org.mpris.MediaPlayer2.a");
        assert_eq!(tracker.player_count(), 0);
        assert_eq!(tracker.active(), None);
    }
}
