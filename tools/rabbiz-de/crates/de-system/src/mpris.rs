//! MPRIS media-player state tracking and secure album-art URL validation.
//!
//! Pure `std` data model & tracker for `org.mpris.MediaPlayer2.Player`
//! snapshots (`docs/DE-NOTIFICATIONS-SERVICES.md` §2.5). The async D-Bus
//! watcher pushes [`MediaSnapshot`] updates into [`MprisTracker`] and
//! receives [`MediaCommand`] requests over a channel so the compositor loop
//! never blocks on D-Bus or remote HTTP fetches.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use zbus::blocking;
use zbus::zvariant::OwnedValue;

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

// ---------------------------------------------------------------------------
// Live session-bus watcher (worker thread; the compositor only reads snapshots).
// ---------------------------------------------------------------------------

const POLL: Duration = Duration::from_millis(500);
const MPRIS_PREFIX: &str = "org.mpris.MediaPlayer2.";
const PLAYER_PATH: &str = "/org/mpris/MediaPlayer2";
const PLAYER_IFACE: &str = "org.mpris.MediaPlayer2.Player";
const ROOT_IFACE: &str = "org.mpris.MediaPlayer2";

/// Live MPRIS state: a polling thread keeps a [`MprisTracker`] fresh,
/// `active()` / `generation()` are cheap reads for the frame loop, and
/// `request()` queues transport commands so the compositor never blocks on D-Bus.
pub struct Mpris {
    tracker: Arc<Mutex<MprisTracker>>,
    generation: Arc<AtomicU64>,
    commands: mpsc::Sender<MediaCommand>,
}

impl Mpris {
    /// Spawn the watcher. Without a session bus it keeps retrying and
    /// [`active`](Self::active) stays `None`.
    #[must_use]
    pub fn start() -> Self {
        let tracker = Arc::new(Mutex::new(MprisTracker::new()));
        let generation = Arc::new(AtomicU64::new(0));
        let (tx, rx) = mpsc::channel();
        let (t, g) = (tracker.clone(), generation.clone());
        std::thread::spawn(move || worker(&t, &g, &rx));
        Self {
            tracker,
            generation,
            commands: tx,
        }
    }

    /// Bumped whenever the selected snapshot changes.
    #[must_use]
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Relaxed)
    }

    /// Selected player (prefers playing, else most recently updated).
    #[must_use]
    pub fn active(&self) -> Option<MediaSnapshot> {
        self.tracker
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .active()
            .cloned()
    }

    /// Forward a transport command to the active player, asynchronously.
    pub fn request(&self, command: MediaCommand) {
        let _ = self.commands.send(command);
    }
}

fn worker(
    tracker: &Mutex<MprisTracker>,
    generation: &AtomicU64,
    rx: &mpsc::Receiver<MediaCommand>,
) {
    let roots: Vec<PathBuf> = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .into_iter()
        .collect();
    let roots: Vec<&Path> = roots.iter().map(PathBuf::as_path).collect();
    let mut conn: Option<blocking::Connection> = None;
    let mut known: Vec<String> = Vec::new();
    let mut last: Option<MediaSnapshot> = None;
    let mut tick: u64 = 0;
    loop {
        while let Ok(cmd) = rx.try_recv() {
            if let Some(c) = conn.as_ref() {
                send_command(c, tracker, cmd);
            }
        }
        if conn.is_none() {
            conn = blocking::Connection::session().ok();
        }
        if let Some(c) = conn.as_ref() {
            tick = tick.wrapping_add(1);
            match poll_once(c, tracker, &roots, &mut known, tick) {
                Ok(active) if active != last => {
                    last = active;
                    generation.fetch_add(1, Ordering::Relaxed);
                }
                Ok(_) => {}
                Err(_) => conn = None,
            }
        }
        std::thread::sleep(POLL);
    }
}

fn poll_once(
    conn: &blocking::Connection,
    tracker: &Mutex<MprisTracker>,
    roots: &[&Path],
    known: &mut Vec<String>,
    tick: u64,
) -> zbus::Result<Option<MediaSnapshot>> {
    let dbus = blocking::Proxy::new(
        conn,
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
    )?;
    let names: Vec<String> = dbus
        .call::<_, _, Vec<String>>("ListNames", &())?
        .into_iter()
        .filter(|n| n.starts_with(MPRIS_PREFIX))
        .collect();
    let mut t = tracker.lock().unwrap_or_else(|e| e.into_inner());
    for name in &names {
        if let Some(snap) = read_player(conn, name, roots) {
            t.upsert(snap, tick);
        }
    }
    for gone in known.iter().filter(|k| !names.contains(k)) {
        t.remove(gone);
    }
    *known = names;
    Ok(t.active().cloned())
}

fn str_of(v: &OwnedValue) -> Option<String> {
    <&str>::try_from(v).ok().map(str::to_owned)
}

fn read_player(
    conn: &blocking::Connection,
    bus: &str,
    roots: &[&Path],
) -> Option<MediaSnapshot> {
    let props =
        blocking::Proxy::new(conn, bus, PLAYER_PATH, "org.freedesktop.DBus.Properties").ok()?;
    let all: HashMap<String, OwnedValue> = props.call("GetAll", &PLAYER_IFACE).ok()?;
    let meta: HashMap<String, OwnedValue> = all
        .get("Metadata")
        .and_then(|v| v.clone().try_into().ok())
        .unwrap_or_default();
    let flag = |k: &str| all.get(k).is_some_and(|v| bool::try_from(v).unwrap_or(false));
    let player_name = props
        .call::<_, _, OwnedValue>("Get", &(ROOT_IFACE, "Identity"))
        .ok()
        .and_then(|v| str_of(&v))
        .unwrap_or_else(|| bus.trim_start_matches(MPRIS_PREFIX).to_owned());
    Some(MediaSnapshot {
        bus_name: bus.to_owned(),
        player_name,
        title: meta.get("xesam:title").and_then(str_of).unwrap_or_default(),
        artist: meta
            .get("xesam:artist")
            .and_then(|v| <Vec<String>>::try_from(v.clone()).ok())
            .unwrap_or_default()
            .join(", "),
        playing: all.get("PlaybackStatus").and_then(str_of).as_deref() == Some("Playing"),
        can_go_next: flag("CanGoNext"),
        can_go_previous: flag("CanGoPrevious"),
        art_path: meta
            .get("mpris:artUrl")
            .and_then(str_of)
            .and_then(|u| validate_art_url(&u, roots))
            .map(|p| p.to_string_lossy().into_owned()),
    })
}

fn send_command(conn: &blocking::Connection, tracker: &Mutex<MprisTracker>, cmd: MediaCommand) {
    let bus = tracker
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .active()
        .map(|s| s.bus_name.clone());
    let Some(bus) = bus else { return };
    let method = match cmd {
        MediaCommand::PlayPause => "PlayPause",
        MediaCommand::Next => "Next",
        MediaCommand::Previous => "Previous",
    };
    let proxy = blocking::Proxy::new(conn, bus.as_str(), PLAYER_PATH, PLAYER_IFACE);
    if let Ok(p) = proxy {
        let _: Result<(), _> = p.call(method, &());
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
