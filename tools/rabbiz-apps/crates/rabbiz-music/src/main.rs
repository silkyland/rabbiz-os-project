//! rabbiz-music — Music app for RabbizOS (Rust/native Wayland build).
//!
//! Library: audio files under `~/Music` + `~/Downloads` (see `library.rs`).
//! Playback: one persistent `mpv --idle=yes --input-ipc-server` process
//! driven over JSON-lines IPC (see `player.rs`) — real position, duration,
//! pause and end-of-file state, real `seek`, real transport. Volume follows
//! the PipeWire/PulseAudio default sink (`--ao=pulse,pipewire`); the app
//! never touches it.
//!
//! Layout follows Material 3 media-player conventions on a phone: scrolling
//! track list, now-playing card pinned above the gesture bar with art tile,
//! seek bar (drag + rubber band), elapsed/total times and prev/play/next
//! transport. Hit-testing uses the last rendered size (`vp`), never a
//! hardcoded panel size.

mod headless;
mod library;
mod player;

use std::cmp::Ordering;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::time::Instant;

use de_api::Pixels;
use de_design::motion::{bezier_eval, BEZIER_STANDARD, RUBBER_FACTOR};
use rabbiz_app_kit::{
    text, ui, AppEvent, AppFrame, AppState, Canvas, Point, Rect, Size, TextInputCtl, Theme,
};

use library::{auto_next_index, format_time, next_index, prev_index, seek_seconds, Scope, Track};
use player::{mpv_available, socket_path, Line, Mpv};

// Row metrics (pitch includes the gap).
const ROW_H: i32 = 76;
const ROW_PITCH: i32 = 84;
/// Now-playing card height (art row + seek + times + transport + padding).
const NP_H: i32 = 272;
/// Seek thumb radius / track height.
const SEEK_THUMB: i32 = 9;
const SEEK_TRACK_H: i32 = 8;
/// Scroll settle duration — inside the ≤600 ms motion budget, on the
/// `DURATIONS_MS` 160/240 ladder.
const SCROLL_SETTLE_S: f32 = 0.18;

// Material Icons Round codepoints (verified against the bundled TTF cmap).
const IC_PLAY: &str = "\u{e037}";
const IC_PAUSE: &str = "\u{e034}";
const IC_STOP: &str = "\u{e047}";
const IC_PREV: &str = "\u{e045}";
const IC_NEXT: &str = "\u{e044}";
const IC_NOTE: &str = "\u{e405}";
const IC_MUSIC: &str = "\u{e030}";
const IC_EQ: &str = "\u{e1b8}";
const IC_MUSIC_OFF: &str = "\u{e440}";
const IC_REFRESH: &str = "\u{e5d5}";
const IC_ERROR: &str = "\u{e001}";

// ---------------------------------------------------------------------------
// Player — mpv process + observed playback state
// ---------------------------------------------------------------------------

struct Player {
    mpv: Option<Mpv>,
    /// `mpv --version` probe result from startup; gates the honest banner.
    mpv_ok: bool,
    /// Properties subscribed for the current connection.
    observed: bool,
    /// Commands issued while the socket isn't up yet (FIFO, flushed on connect).
    pending_cmds: VecDeque<Vec<String>>,
    current: Option<usize>,
    pause: bool,
    /// Last observed `time-pos` and when it was observed — the display
    /// extrapolates from this while playing.
    pos: f64,
    pos_at: Instant,
    duration: f64,
    eof: bool,
    media_title: String,
    /// Last observed mpv `path` property — the only "what is loaded" signal
    /// a headless client gets when it attaches to a player it didn't load.
    file_path: String,
    error: Option<String>,
    last_get: Instant,
}

impl Player {
    fn new(mpv_ok: bool) -> Self {
        Self {
            mpv: None,
            mpv_ok,
            observed: false,
            pending_cmds: VecDeque::new(),
            current: None,
            pause: true,
            pos: 0.0,
            pos_at: Instant::now(),
            duration: 0.0,
            eof: false,
            media_title: String::new(),
            file_path: String::new(),
            error: None,
            last_get: Instant::now(),
        }
    }

    fn has_track(&self) -> bool {
        self.current.is_some() || !self.file_path.is_empty()
    }

    fn is_playing(&self) -> bool {
        self.has_track() && !self.pause && !self.eof
    }

    /// Display position: observed `time-pos` extrapolated by wall clock
    /// while playing, clamped to the known duration.
    fn est_pos(&self) -> f64 {
        let base = if self.pause {
            self.pos
        } else {
            self.pos + self.pos_at.elapsed().as_secs_f64()
        };
        if self.duration > 0.0 {
            base.min(self.duration)
        } else {
            base
        }
    }

    fn progress(&self) -> f32 {
        library::progress_fraction(self.est_pos(), self.duration)
    }

    fn display_title<'a>(&'a self, tracks: &'a [Track]) -> &'a str {
        if !self.media_title.is_empty() {
            return self.media_title.as_str();
        }
        self.current
            .and_then(|i| tracks.get(i))
            .map(|t| t.title.as_str())
            .unwrap_or("")
    }

    /// Connect to an mpv that is already running (the GUI's player or a
    /// detached headless `play`) without spawning a new one. Headless
    /// transport commands use this so they never start a player just to
    /// pause it. Returns `false` when no session exists.
    fn attach(&mut self) -> bool {
        if self.mpv.is_some() {
            return true;
        }
        if !self.mpv_ok {
            return false;
        }
        match Mpv::connect_existing(&socket_path()) {
            Ok(m) => {
                self.mpv = Some(m);
                true
            }
            Err(_) => false,
        }
    }

    /// Ensure the mpv process exists (spawn on first use, not at startup —
    /// an honest app doesn't run a player it may never need). Prefers
    /// attaching to a live player so the GUI and headless modes share one
    /// mpv instead of racing two processes onto the same socket path.
    fn ensure_started(&mut self) {
        if self.mpv.is_some() || !self.mpv_ok {
            return;
        }
        let sock = socket_path();
        let res = Mpv::connect_existing(&sock).or_else(|_| Mpv::spawn(&sock));
        match res {
            Ok(m) => self.mpv = Some(m),
            Err(e) => {
                self.error = Some(format!("Could not start mpv: {e}"));
            }
        }
    }

    /// Queue a command; sent immediately when connected, otherwise FIFO
    /// until the socket comes up.
    fn cmd(&mut self, args: &[&str]) -> bool {
        let owned: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        if let Some(m) = self.mpv.as_mut() {
            if m.connected() {
                return m.command(args);
            }
        }
        self.pending_cmds.push_back(owned);
        true
    }

    /// Load `tracks[i]` (replace current). Resets all observed state so the
    /// UI reflects the new track immediately, before mpv confirms.
    fn play_track(&mut self, i: usize, tracks: &[Track]) {
        let Some(t) = tracks.get(i) else { return };
        self.ensure_started();
        if self.mpv.is_none() {
            if self.error.is_none() {
                self.error = Some("mpv is not installed, so nothing can play.".into());
            }
            return;
        }
        self.pending_cmds.clear(); // fresh load supersedes queued transport
        let path = t.path.to_string_lossy().into_owned();
        let _ = self.cmd(&["loadfile", path.as_str(), "replace"]);
        self.current = Some(i);
        self.pause = false;
        self.pos = 0.0;
        self.pos_at = Instant::now();
        self.duration = 0.0;
        self.eof = false;
        self.media_title.clear();
        self.file_path = t.path.to_string_lossy().into_owned();
        self.error = None;
    }

    /// play/pause toggle. Caller guarantees a track is loaded (the app picks
    /// the first visible one otherwise). At end-of-file, "play" means replay
    /// the track (mpv stays idle after EOF with `--idle=yes`).
    fn toggle(&mut self, tracks: &[Track]) {
        if self.eof {
            if let Some(i) = self.current {
                self.play_track(i, tracks);
            }
            return;
        }
        self.cmd(&["cycle", "pause"]);
        self.pause = !self.pause; // optimistic; property-change corrects
    }

    fn seek_to_fraction(&mut self, frac: f32) {
        if !self.has_track() || self.duration.partial_cmp(&0.0) != Some(Ordering::Greater) {
            return;
        }
        let secs = seek_seconds(frac, self.duration);
        let arg = format!("{secs:.2}");
        self.cmd(&["seek", arg.as_str(), "absolute"]);
        self.pos = secs;
        self.pos_at = Instant::now();
    }

    /// Absolute seek in seconds — the headless `seek <sec>` command.
    fn seek_absolute(&mut self, secs: f64) {
        if !self.has_track() || !secs.is_finite() {
            return;
        }
        let arg = format!("{:.2}", secs.max(0.0));
        self.cmd(&["seek", arg.as_str(), "absolute"]);
        self.pos = secs.max(0.0);
        self.pos_at = Instant::now();
    }

    /// Explicit pause/resume (headless `pause`; the GUI uses `toggle`).
    /// Resuming at end-of-file rewinds first so mpv actually replays.
    fn set_paused(&mut self, paused: bool) {
        if paused {
            self.cmd(&["set_property", "pause", "true"]);
        } else {
            if self.eof {
                let _ = self.cmd(&["seek", "0", "absolute"]);
                self.eof = false;
            }
            self.cmd(&["set_property", "pause", "false"]);
        }
        self.pause = paused;
        self.pos_at = Instant::now();
    }

    /// Stop playback entirely and forget the current track.
    fn stop(&mut self) {
        self.pending_cmds.clear();
        let _ = self.cmd(&["stop"]);
        self.current = None;
        self.pause = true;
        self.pos = 0.0;
        self.pos_at = Instant::now();
        self.duration = 0.0;
        self.eof = false;
        self.media_title.clear();
        self.file_path.clear();
    }

    /// Advance playback: connect retry, command flush, socket drain,
    /// throttled `time-pos` poll, dead-child detection. Returns `true` when
    /// visible state changed.
    fn pump(&mut self, tracks: &[Track]) -> bool {
        let mut changed = false;
        let mut lines: Vec<Line> = Vec::new();

        // Take the Mpv out so socket ops and state application never hold
        // overlapping borrows; it goes back before `apply_line` runs (which
        // may itself queue commands through `self.mpv`).
        let mut slot = self.mpv.take();
        if let Some(m) = slot.as_mut() {
            if !m.alive() {
                // mpv exited on its own (crash, `quit` command, …).
                self.observed = false;
                self.error = Some(
                    "mpv exited unexpectedly — playback stopped. Tap a track to retry.".into(),
                );
                slot = None; // Drop kills/reaps nothing — child already exited.
                changed = true;
            } else {
                if !m.connected() && m.try_connect() {
                    changed = true;
                }
                if m.connected() && !self.observed {
                    self.observed = m.observe_defaults();
                }
                if m.connected() && !self.pending_cmds.is_empty() {
                    while let Some(args) = self.pending_cmds.pop_front() {
                        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
                        m.command(&refs);
                    }
                    changed = true;
                }
                // Throttled position poll — mpv's property-change cadence for
                // time-pos is not contractual, so ask directly.
                if m.connected()
                    && self.has_track()
                    && self.last_get.elapsed().as_millis() >= player::POLL_INTERVAL_MS
                {
                    self.last_get = Instant::now();
                    let _ = m.get_property("time-pos");
                }
                lines = m.poll();
                if !m.connected() {
                    // Socket dropped: the next connection needs fresh
                    // observe_property subscriptions.
                    self.observed = false;
                }
            }
        }
        self.mpv = slot;

        for line in lines {
            changed |= self.apply_line(line, tracks);
        }
        changed
    }

    /// Apply one IPC message. Returns `true` on visible-state change.
    fn apply_line(&mut self, line: Line, tracks: &[Track]) -> bool {
        match line {
            Line::Property { name, value } => match name.as_str() {
                "pause" => {
                    if let Some(b) = value.as_bool() {
                        if self.pause != b {
                            self.pause = b;
                            return true;
                        }
                    }
                    false
                }
                "time-pos" => {
                    if let Some(f) = value.as_f64() {
                        self.pos = f.max(0.0);
                        self.pos_at = Instant::now();
                        return true;
                    }
                    false
                }
                "duration" => {
                    let d = value.as_f64().unwrap_or(0.0);
                    if (self.duration - d).abs() > 0.05 {
                        self.duration = d.max(0.0);
                        return true;
                    }
                    false
                }
                "eof-reached" => {
                    let eof = value.as_bool().unwrap_or(false);
                    if eof && !self.eof {
                        self.eof = true;
                        self.finish_track(tracks);
                        return true;
                    }
                    if !eof && self.eof {
                        self.eof = false;
                        return true;
                    }
                    false
                }
                "media-title" => {
                    let s = value.as_str().unwrap_or("").to_string();
                    if s != self.media_title {
                        self.media_title = s;
                        return true;
                    }
                    false
                }
                // Which file mpv actually has loaded — the only "now
                // playing" signal a freshly attached headless client gets.
                "path" => {
                    let s = value.as_str().unwrap_or("").to_string();
                    if s != self.file_path {
                        self.file_path = s;
                        return true;
                    }
                    false
                }
                _ => false,
            },
            Line::EndFile { reason } => match reason.as_str() {
                "eof" => {
                    if !self.eof {
                        self.eof = true;
                        self.finish_track(tracks);
                        return true;
                    }
                    false
                }
                "error" => {
                    self.error =
                        Some("mpv reported a playback error — the file may be unsupported.".into());
                    true
                }
                // "stop"/"quit" are either ours or handled by alive().
                _ => false,
            },
            // Fire-and-forget command responses carry no state we need.
            Line::Response { .. } => false,
        }
    }

    /// End of the current file: advance to the next library track, or park
    /// paused at the end of the list (no infinite loop).
    fn finish_track(&mut self, tracks: &[Track]) {
        match auto_next_index(self.current, tracks.len()) {
            Some(i) => self.play_track(i, tracks),
            None => self.pause = true,
        }
    }
}

// ---------------------------------------------------------------------------
// App
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Hit {
    Rescan,
    Chip(Scope),
    Row(usize), // index into `tracks`
    Seek,
    Prev,
    Toggle,
    Next,
    Stop,
    EmptyAction,
}

struct MusicApp {
    tracks: Vec<Track>,
    scope: Scope,
    player: Player,
    /// Last rendered size — hit-testing uses it between frames.
    vp: (i32, i32),
    /// List scroll offset px (content pixels).
    scroll: i32,
    max_scroll: i32,
    /// List drag: (start_y, scroll_at_start, moved).
    drag: Option<(i32, i32, bool)>,
    /// Scroll settle animation: (from, to, elapsed s).
    scroll_anim: Option<(f32, f32, f32)>,
    /// Seek drag: (touch id, raw fraction — may leave 0..=1 for rubber band).
    seek_drag: Option<(i32, f32)>,
    pressed: Option<Hit>,
    last_instant: Option<Instant>,
}

/// Everything `render` needs that `hit_at` must agree on.
struct Layout {
    header_bottom: i32,
    rescan: Rect,
    chips: [Rect; 3],
    list: Rect,
    np: Option<Rect>,
    empty_btn: Option<Rect>,
}

impl MusicApp {
    fn new() -> Self {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/home/rabbiz"));
        let scan = library::scan(&home);
        Self {
            tracks: scan.tracks,
            scope: Scope::All,
            player: Player::new(mpv_available()),
            vp: (720, 1600),
            scroll: 0,
            max_scroll: 0,
            drag: None,
            scroll_anim: None,
            seek_drag: None,
            pressed: None,
            last_instant: None,
        }
    }

    fn visible(&self) -> Vec<usize> {
        library::visible_indices(&self.tracks, self.scope)
    }

    // ---------------- wall-clock pump (runs on events AND render) ---------

    fn pump(&mut self) -> bool {
        let now = Instant::now();
        let dt = self
            .last_instant
            .map_or(0.0, |t| now.saturating_duration_since(t).as_secs_f32());
        self.last_instant = Some(now);
        let mut changed = false;

        if let Some((from, to, t)) = self.scroll_anim {
            let nt = t + dt / SCROLL_SETTLE_S;
            if nt >= 1.0 {
                self.scroll = to.round() as i32;
                self.scroll_anim = None;
            } else {
                let e = bezier_eval(BEZIER_STANDARD, nt);
                self.scroll = (from + (to - from) * e).round() as i32;
                self.scroll_anim = Some((from, to, nt));
            }
            changed = true;
        }
        changed |= self.player.pump(&self.tracks);
        changed
    }

    // ---------------- layout ----------------------------------------------

    fn layout(&self, w: i32, h: i32) -> Layout {
        // draw_header returns TOP_SAFE+16 + 44 + 24 when a subtitle is set.
        let header_bottom = ui::TOP_SAFE + 16 + 80;

        let rescan = Rect::new(w - ui::MARGIN - 48, ui::TOP_SAFE + 16, 48, 48);
        let mut y = header_bottom + 8;
        let mut banner_h = 0;
        if !self.player.mpv_ok {
            banner_h += 88 + 8;
        }
        if self.player.error.is_some() {
            banner_h += 88 + 8;
        }
        y += banner_h;

        let mut chips = [Rect::ZERO; 3];
        let mut empty_btn = None;
        if !self.tracks.is_empty() {
            let gap = 8;
            let cw = (w - ui::MARGIN * 2 - gap * 2) / 3;
            for (i, chip) in chips.iter_mut().enumerate() {
                *chip = Rect::new(ui::MARGIN + i as i32 * (cw + gap), y, cw, 48);
            }
            y += 48 + 8 + 26; // chips + gap + section label line
        } else {
            y += 8;
        }

        let np = if self.player.has_track() {
            Some(Rect::new(
                ui::MARGIN,
                h - ui::BOTTOM_SAFE - 8 - NP_H,
                w - ui::MARGIN * 2,
                NP_H,
            ))
        } else {
            None
        };
        let list_bottom = np.map_or(h - ui::BOTTOM_SAFE - 8, |r| r.y - 8);
        let list = Rect::new(ui::MARGIN, y, w - ui::MARGIN * 2, (list_bottom - y).max(80));

        if self.tracks.is_empty() || self.visible().is_empty() {
            empty_btn = Some(Rect::new(w / 2 - 110, list.y + list.h / 2 + 90, 220, 48));
        }

        Layout {
            header_bottom,
            rescan,
            chips,
            list,
            np,
            empty_btn,
        }
    }

    fn subtitle(&self) -> String {
        if self.tracks.is_empty() {
            return "No audio in ~/Music or ~/Downloads".into();
        }
        let n = self.tracks.len();
        format!("{n} track{} · newest first", if n == 1 { "" } else { "s" })
    }

    fn update_scroll_limits(&mut self, w: i32, h: i32) {
        let l = self.layout(w, h);
        let content = self.visible().len() as i32 * ROW_PITCH;
        self.max_scroll = (content - l.list.h).max(0);
        // Rubber-band drags and the settle animation may legitimately sit
        // outside [0, max_scroll]; only snap when idle.
        if self.drag.is_none() && self.scroll_anim.is_none() {
            self.scroll = self.scroll.clamp(0, self.max_scroll);
        }
    }

    // ---------------- hit testing ------------------------------------------

    fn seek_touch_rect(np: Rect) -> Rect {
        Rect::new(np.x + 16, np.y + 16 + 72 + 10, np.w - 32, 44)
    }

    fn seek_track_rect(np: Rect) -> Rect {
        let t = Self::seek_touch_rect(np);
        Rect::new(t.x, t.y + (t.h - SEEK_TRACK_H) / 2, t.w, SEEK_TRACK_H)
    }

    fn transport_rects(np: Rect) -> [Rect; 3] {
        // Row of prev(56) / play(72) / next(56) centered in the card's
        // bottom 84 px band.
        let y = np.y + NP_H - 16 - 84;
        let total = 56 + 28 + 72 + 28 + 56;
        let x0 = np.x + (np.w - total) / 2;
        [
            Rect::new(x0, y + 14, 56, 56),
            Rect::new(x0 + 84, y, 72, 72),
            Rect::new(x0 + 184, y + 14, 56, 56),
        ]
    }

    fn stop_rect(np: Rect) -> Rect {
        Rect::new(np.x + np.w - 16 - 44, np.y + 16, 44, 44)
    }

    /// Fraction (0..=1, unclamped for rubber) of a point inside the seek
    /// track rect.
    fn seek_fraction(np: Rect, pos: Point) -> f32 {
        let t = Self::seek_track_rect(np);
        if t.w <= 0 {
            return 0.0;
        }
        (pos.x - t.x) as f32 / t.w as f32
    }

    fn hit_at(&self, w: i32, h: i32, pos: Point) -> Option<Hit> {
        let l = self.layout(w, h);
        // Now-playing zone first (it sits below the list; no overlap anyway).
        if let Some(np) = l.np {
            if Self::stop_rect(np).contains(pos) {
                return Some(Hit::Stop);
            }
            if Self::seek_touch_rect(np).contains(pos) {
                return Some(Hit::Seek);
            }
            for (i, r) in Self::transport_rects(np).iter().enumerate() {
                if r.contains(pos) {
                    return Some([Hit::Prev, Hit::Toggle, Hit::Next][i]);
                }
            }
        }
        // Track rows inside the list viewport.
        if l.list.contains(pos) {
            let vis = self.visible();
            let row_i = (pos.y - l.list.y + self.scroll) / ROW_PITCH;
            if row_i >= 0 && (row_i as usize) < vis.len() {
                let track_idx = vis[row_i as usize];
                let row_top = l.list.y + row_i * ROW_PITCH - self.scroll;
                if pos.y >= row_top && pos.y < row_top + ROW_H {
                    return Some(Hit::Row(track_idx));
                }
            }
            return None; // inside list but not on a row → scroll drag only
        }
        for (i, r) in l.chips.iter().enumerate() {
            if r.contains(pos) {
                return Some(Hit::Chip(Scope::ALL[i]));
            }
        }
        if l.rescan.contains(pos) {
            return Some(Hit::Rescan);
        }
        if l.empty_btn.is_some_and(|r| r.contains(pos)) {
            return Some(Hit::EmptyAction);
        }
        None
    }

    // ---------------- actions ----------------------------------------------

    /// Central play/pause entry: with nothing loaded, play the first track
    /// visible under the current scope; otherwise toggle.
    fn toggle_play(&mut self) {
        if self.player.has_track() {
            self.player.toggle(&self.tracks);
        } else if let Some(&i) = self.visible().first() {
            self.player.play_track(i, &self.tracks);
        }
    }

    fn rescan(&mut self) {
        // Keep `current` pointed at the same file across re-sorts: match by
        // path, not index. A vanished file stops playback honestly.
        let old_path = self
            .player
            .current
            .and_then(|i| self.tracks.get(i))
            .map(|t| t.path.clone());
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/home/rabbiz"));
        let scan = library::scan(&home);
        self.tracks = scan.tracks;
        if let Some(p) = old_path {
            match self.tracks.iter().position(|t| t.path == p) {
                Some(i) => self.player.current = Some(i),
                None => self.player.stop(),
            }
        }
        self.scroll = 0;
        self.scroll_anim = None;
    }

    fn dispatch(&mut self, hit: Hit) {
        match hit {
            Hit::Rescan => self.rescan(),
            Hit::Chip(s) => {
                self.scope = s;
                self.scroll = 0;
                self.scroll_anim = None;
            }
            Hit::Row(i) => {
                if self.player.current == Some(i) {
                    self.player.toggle(&self.tracks);
                } else {
                    self.player.play_track(i, &self.tracks);
                }
            }
            Hit::Seek => {} // handled by seek_drag on release
            Hit::Prev => {
                if let Some(i) = prev_index(self.player.current, self.tracks.len()) {
                    self.player.play_track(i, &self.tracks);
                }
            }
            Hit::Next => {
                if let Some(i) = next_index(self.player.current, self.tracks.len()) {
                    self.player.play_track(i, &self.tracks);
                }
            }
            Hit::Toggle => self.toggle_play(),
            Hit::Stop => self.player.stop(),
            Hit::EmptyAction => {
                if self.tracks.is_empty() {
                    self.rescan();
                } else {
                    self.scope = Scope::All;
                    self.scroll = 0;
                }
            }
        }
    }
}

impl AppState for MusicApp {
    fn app_id(&self) -> &'static str {
        "rabbiz-music"
    }
    fn title(&self) -> &'static str {
        "Music"
    }

    fn handle_event(&mut self, event: AppEvent, _ti: &TextInputCtl) -> AppFrame {
        let changed = self.pump();
        let (w, h) = self.vp;
        let mut dirty = changed;

        match event {
            AppEvent::TouchDown { id, pos } => {
                self.update_scroll_limits(w, h);
                let hit = self.hit_at(w, h, pos);
                self.pressed = hit;
                self.seek_drag = None;
                self.drag = None;
                // Seek drag captures the touch anywhere in its band.
                if hit == Some(Hit::Seek) {
                    if let Some(np) = self.layout(w, h).np {
                        self.seek_drag = Some((id, Self::seek_fraction(np, pos)));
                    }
                } else if self.layout(w, h).list.contains(pos) {
                    self.drag = Some((pos.y, self.scroll, false));
                }
                dirty = true;
            }
            AppEvent::TouchMove { id, pos } => {
                if let Some((did, _)) = self.seek_drag {
                    if did == id {
                        if let Some(np) = self.layout(w, h).np {
                            self.seek_drag = Some((id, Self::seek_fraction(np, pos)));
                            dirty = true;
                        }
                    }
                } else if let Some((sy, ss, _)) = self.drag {
                    if (sy - pos.y).abs() > 10 {
                        self.drag = Some((sy, ss, true));
                        self.pressed = None;
                    }
                    let raw = ss + sy - pos.y;
                    self.scroll_anim = None;
                    self.scroll = rubber_scroll(raw, self.max_scroll);
                    dirty = true;
                }
            }
            AppEvent::TouchUp { id, pos } => {
                // Seek release: commit the (clamped) fraction.
                if let Some((did, raw)) = self.seek_drag.take() {
                    if did == id {
                        self.player.seek_to_fraction(raw.clamp(0.0, 1.0));
                        self.pressed = None;
                        dirty = true;
                    }
                } else if let Some((_, _, moved)) = self.drag.take() {
                    self.pressed = None;
                    if !moved {
                        if let Some(hit) = self.hit_at(w, h, pos) {
                            self.dispatch(hit);
                        }
                    }
                    // Settle back inside bounds with a short ease.
                    let clamped = self.scroll.clamp(0, self.max_scroll);
                    if (self.scroll - clamped).abs() > 1 {
                        self.scroll_anim = Some((self.scroll as f32, clamped as f32, 0.0));
                    } else {
                        self.scroll = clamped;
                    }
                    dirty = true;
                } else {
                    let hit = self.pressed.take();
                    self.pressed = None;
                    if let Some(hit) = hit {
                        if self.hit_at(w, h, pos) == Some(hit) {
                            self.dispatch(hit);
                        }
                    }
                    dirty = true;
                }
            }
            AppEvent::Key { utf8: Some(s), .. } if s == " " => {
                self.toggle_play();
                dirty = true;
            }
            AppEvent::Tick(_) => dirty = true,
            _ => {}
        }

        AppFrame {
            dirty,
            animating: self.player.has_track() || self.scroll_anim.is_some() || self.drag.is_some(),
            quit: false,
        }
    }

    fn render(&mut self, canvas: &mut Canvas, theme: &Theme) {
        let changed = self.pump();
        let _ = changed;
        let size = canvas.size();
        let (w, h) = (size.w, size.h);
        self.vp = (w, h);
        self.update_scroll_limits(w, h);
        let l = self.layout(w, h);

        ui::draw_header(canvas, theme, "Music", &self.subtitle());
        self.draw_rescan(canvas, theme, l.rescan);
        self.draw_banners(canvas, theme, l.header_bottom);
        self.draw_chips(canvas, theme, &l);
        self.draw_section_label(canvas, theme, &l);
        self.draw_list(canvas, theme, &l);
        self.draw_empty(canvas, theme, &l);
        if let Some(np) = l.np {
            self.draw_now_playing(canvas, theme, np);
        }
    }
}

// ---------------------------------------------------------------------------
// Drawing helpers
// ---------------------------------------------------------------------------

/// Single-line ellipsized text (the kit's `draw_text` can wrap; list rows
/// and the now-playing card need one line).
fn draw_line(c: &mut Canvas, s: &str, st: &text::LabelStyle, x: i32, y: i32, max_w: i32) {
    if s.is_empty() {
        return;
    }
    if let Some(px) = text::label_clamped(s, *st, max_w.max(8), 1) {
        c.blit(
            &Pixels::new(
                Size::new(px.width() as i32, px.height() as i32),
                px.data().to_vec(),
            ),
            Point::new(x, y),
        );
    }
}

/// Rasterize + blit a Material icon with ink bounds centered on (cx, cy).
fn draw_icon(c: &mut Canvas, codepoint: &str, size: f32, color: [u8; 4], cx: i32, cy: i32) {
    if let Some(px) = text::icon(codepoint, size, color) {
        let (x0, y0, x1, y1) =
            text::ink_bounds(&px).unwrap_or((0, 0, px.width() as i32, px.height() as i32));
        c.blit(
            &Pixels::new(
                Size::new(px.width() as i32, px.height() as i32),
                px.data().to_vec(),
            ),
            Point::new(cx - (x0 + x1) / 2, cy - (y0 + y1) / 2),
        );
    }
}

/// Circular icon button (transport / rescan / stop).
fn draw_circle_button(
    c: &mut Canvas,
    r: Rect,
    icon: &str,
    icon_size: f32,
    bg: [u8; 4],
    fg: [u8; 4],
    pressed: bool,
) {
    let rr = if pressed {
        Rect::new(r.x + 2, r.y + 2, (r.w - 4).max(1), (r.h - 4).max(1))
    } else {
        r
    };
    let rad = (rr.h as f32) / 2.0;
    c.fill_rounded_rect(rr, rad, bg);
    if pressed {
        c.fill_rounded_rect(rr, rad, [255, 255, 255, 28]);
    }
    draw_icon(c, icon, icon_size, fg, rr.x + rr.w / 2, rr.y + rr.h / 2);
}

/// Rubber-band response past a `[0, max]` limit (de-design §7.17a):
/// displacement = RUBBER_FACTOR × overshoot.
fn rubber_scroll(raw: i32, max: i32) -> i32 {
    if raw < 0 {
        (raw as f32 * RUBBER_FACTOR) as i32
    } else if raw > max {
        max + ((raw - max) as f32 * RUBBER_FACTOR) as i32
    } else {
        raw
    }
}

/// Seek-bar rubber: fraction may leave 0..=1 while dragging.
fn rubber_frac(raw: f32) -> f32 {
    if raw < 0.0 {
        raw * RUBBER_FACTOR
    } else if raw > 1.0 {
        1.0 + (raw - 1.0) * RUBBER_FACTOR
    } else {
        raw
    }
}

impl MusicApp {
    fn draw_rescan(&self, c: &mut Canvas, t: &Theme, r: Rect) {
        draw_circle_button(
            c,
            r,
            IC_REFRESH,
            24.0,
            t.surface2,
            t.text,
            self.pressed == Some(Hit::Rescan),
        );
    }

    fn draw_banners(&self, c: &mut Canvas, t: &Theme, top: i32) {
        let w = c.size().w;
        let mut y = top;
        if !self.player.mpv_ok {
            let r = Rect::new(ui::MARGIN, y, w - ui::MARGIN * 2, 80);
            ui::draw_card(c, t, r, false);
            draw_icon(c, IC_ERROR, 22.0, t.danger, r.x + 34, r.y + 26);
            draw_line(
                c,
                "mpv is not installed, so nothing can play. Install it (pacman -S mpv), then tap the refresh button.",
                &ui::styled(16.0, 400, t.danger),
                r.x + 60,
                r.y + 12,
                r.w - 84,
            );
            draw_line(
                c,
                "Audio output uses PipeWire/PulseAudio — the system volume keys control it.",
                &ui::caption(t.text3),
                r.x + 60,
                r.y + 40,
                r.w - 84,
            );
            y += 88;
        }
        if let Some(err) = &self.player.error {
            let r = Rect::new(ui::MARGIN, y, w - ui::MARGIN * 2, 80);
            ui::draw_card(c, t, r, false);
            draw_icon(c, IC_ERROR, 22.0, t.danger, r.x + 34, r.y + 26);
            draw_line(
                c,
                err,
                &ui::styled(16.0, 400, t.danger),
                r.x + 60,
                r.y + 22,
                r.w - 84,
            );
        }
    }

    fn draw_chips(&self, c: &mut Canvas, t: &Theme, l: &Layout) {
        if self.tracks.is_empty() {
            return;
        }
        for (i, s) in Scope::ALL.iter().enumerate() {
            let r = l.chips[i];
            let active = self.scope == *s;
            let bg = if active { t.accent } else { t.surface2 };
            let fg = if active { t.text_on_accent } else { t.text };
            c.fill_rounded_rect(r, (r.h as f32) / 2.0, bg);
            if self.pressed == Some(Hit::Chip(*s)) {
                c.fill_rounded_rect(r, (r.h as f32) / 2.0, [255, 255, 255, 28]);
            }
            let count = match s.folder() {
                None => self.tracks.len(),
                Some(f) => library::count_folder(&self.tracks, f),
            };
            let label = format!("{} · {count}", s.label());
            ui::draw_centered(c, &label, &ui::styled(18.0, 600, fg), r);
        }
    }

    fn draw_section_label(&self, c: &mut Canvas, t: &Theme, l: &Layout) {
        if self.tracks.is_empty() {
            return;
        }
        let y = l.chips[0].y + 48 + 8;
        draw_line(
            c,
            "Library",
            &ui::styled(16.0, 600, t.text3),
            ui::MARGIN,
            y,
            200,
        );
        draw_line(
            c,
            "Newest first",
            &ui::caption(t.text3),
            c.size().w - ui::MARGIN - 140,
            y + 2,
            140,
        );
    }

    fn draw_list(&self, c: &mut Canvas, t: &Theme, l: &Layout) {
        let vis = self.visible();
        if vis.is_empty() {
            return;
        }
        for (vi, &ti) in vis.iter().enumerate() {
            let row_top = l.list.y + vi as i32 * ROW_PITCH - self.scroll;
            // Clamp the row to the viewport (no scissor on Canvas — same
            // convention as the dialer app).
            let top = row_top.max(l.list.y);
            let bottom = (row_top + ROW_H).min(l.list.y + l.list.h);
            let rh = bottom - top;
            if rh <= 0 {
                continue;
            }
            let r = Rect::new(l.list.x, top, l.list.w, rh);
            let track = &self.tracks[ti];
            let is_current = self.player.current == Some(ti);
            let pressed = self.pressed == Some(Hit::Row(ti));
            c.fill_rounded_rect(r, 20.0, if pressed { t.surface3 } else { t.surface1 });
            if is_current {
                // Accent spine marks the loaded track.
                c.fill_rounded_rect(
                    Rect::new(r.x + 4, r.y + 14, 4, (r.h - 28).max(4)),
                    2.0,
                    t.accent,
                );
            }
            // Art tile: format glyph stands in for artwork (MVP — no art).
            let tile = Rect::new(r.x + 16, r.y + (rh - 56).max(0) / 2, 56, 56);
            c.fill_rounded_rect(tile, 16.0, t.surface3);
            let (icon, icol) = if is_current && self.player.is_playing() {
                (IC_EQ, t.accent)
            } else if is_current {
                (IC_PAUSE, t.text2)
            } else {
                (IC_NOTE, t.text3)
            };
            draw_icon(c, icon, 26.0, icol, tile.x + 28, tile.y + 28);

            let tx = r.x + 88;
            let title_col = if is_current { t.accent } else { t.text };
            draw_line(
                c,
                &track.title,
                &ui::styled(20.0, 600, title_col),
                tx,
                r.y + 14,
                r.w - 88 - 90,
            );
            draw_line(
                c,
                &track.caption(),
                &ui::caption(t.text3),
                tx,
                r.y + 38,
                r.w - 88 - 90,
            );
            // Format badge, right-aligned in a fixed slot.
            let badge = Rect::new(r.x + r.w - 84, r.y + (rh - 30).max(0) / 2, 68, 30);
            draw_line(
                c,
                &track.ext.to_uppercase(),
                &ui::styled(14.0, 600, t.text3),
                badge.x,
                badge.y + 4,
                badge.w,
            );
        }
    }

    fn draw_empty(&self, c: &mut Canvas, t: &Theme, l: &Layout) {
        let Some(btn) = l.empty_btn else { return };
        let cy = btn.y - 150;
        if self.tracks.is_empty() {
            draw_icon(c, IC_MUSIC_OFF, 64.0, t.text3, c.size().w / 2, cy);
            ui::draw_centered(
                c,
                "No audio files yet",
                &ui::styled(24.0, 600, t.text),
                Rect::new(0, cy + 48, c.size().w, 40),
            );
            ui::draw_centered(
                c,
                "Put audio in ~/Music or ~/Downloads, then tap Scan again.",
                &ui::body(t.text3),
                Rect::new(ui::MARGIN * 2, cy + 92, c.size().w - ui::MARGIN * 4, 60),
            );
            ui::draw_pill_button(
                c,
                t,
                btn,
                "Scan again",
                true,
                self.pressed == Some(Hit::EmptyAction),
            );
        } else {
            // Scope filter produced an empty list.
            draw_icon(c, IC_MUSIC, 56.0, t.text3, c.size().w / 2, cy);
            ui::draw_centered(
                c,
                &format!("Nothing in {}", self.scope.label()),
                &ui::styled(24.0, 600, t.text),
                Rect::new(0, cy + 48, c.size().w, 40),
            );
            ui::draw_centered(
                c,
                "Tap All to see every track, or add audio to this folder.",
                &ui::body(t.text3),
                Rect::new(ui::MARGIN * 2, cy + 92, c.size().w - ui::MARGIN * 4, 60),
            );
            ui::draw_pill_button(
                c,
                t,
                btn,
                "Show all tracks",
                true,
                self.pressed == Some(Hit::EmptyAction),
            );
        }
    }

    fn draw_now_playing(&self, c: &mut Canvas, t: &Theme, np: Rect) {
        ui::draw_card(c, t, np, false);
        let ix = np.x + 16;

        // Art row: tile + title/caption + stop.
        let tile = Rect::new(ix, np.y + 16, 72, 72);
        c.fill_rounded_rect(tile, 20.0, t.surface3);
        let (icon, icol) = if self.player.is_playing() {
            (IC_EQ, t.accent)
        } else if self.player.eof {
            (IC_MUSIC, t.text3)
        } else {
            (IC_PAUSE, t.text2)
        };
        draw_icon(c, icon, 34.0, icol, tile.x + 36, tile.y + 36);

        let title = self.player.display_title(&self.tracks);
        let folder = self
            .player
            .current
            .and_then(|i| self.tracks.get(i))
            .map(|tr| tr.folder.label())
            .unwrap_or("");
        let ext = self
            .player
            .current
            .and_then(|i| self.tracks.get(i))
            .map(|tr| tr.ext.to_uppercase())
            .unwrap_or_default();
        let state = if self.player.is_playing() {
            "Playing"
        } else if self.player.eof {
            "Finished"
        } else {
            "Paused"
        };
        let caption = [folder, ext.as_str(), state]
            .iter()
            .filter(|s| !s.is_empty())
            .copied()
            .collect::<Vec<_>>()
            .join(" · ");

        draw_line(
            c,
            title,
            &ui::styled(22.0, 600, t.text),
            ix + 88,
            np.y + 26,
            np.w - 88 - 16 - 52,
        );
        draw_line(
            c,
            &caption,
            &ui::caption(t.text3),
            ix + 88,
            np.y + 54,
            np.w - 88 - 16 - 52,
        );
        draw_circle_button(
            c,
            Self::stop_rect(np),
            IC_STOP,
            22.0,
            t.surface2,
            t.text,
            self.pressed == Some(Hit::Stop),
        );

        // Seek bar.
        let st = Self::seek_touch_rect(np);
        let track_r = Self::seek_track_rect(np);
        c.fill_rounded_rect(track_r, (SEEK_TRACK_H as f32) / 2.0, t.surface3);
        // Drag fraction may leave 0..=1 (rubber band); live progress is
        // always in range, so rubber_frac is the identity there.
        let raw_frac = self
            .seek_drag
            .map(|(_, r)| r)
            .unwrap_or_else(|| self.player.progress());
        let disp = rubber_frac(raw_frac);
        let dur_known = self.player.duration > 0.0;
        if dur_known || self.seek_drag.is_some() {
            let fill_w = (track_r.w as f32 * disp.clamp(0.0, 1.0)).round() as i32;
            if fill_w > 0 {
                c.fill_rounded_rect(
                    Rect::new(track_r.x, track_r.y, fill_w, track_r.h),
                    (SEEK_TRACK_H as f32) / 2.0,
                    t.accent,
                );
            }
            if dur_known {
                // Thumb may overshoot with the rubber band but stays on the track.
                let lo = (track_r.x + SEEK_THUMB) as f32;
                let hi = (track_r.x + track_r.w - SEEK_THUMB) as f32;
                let cx = (track_r.x as f32 + track_r.w as f32 * disp).clamp(lo, hi) as i32;
                let thumb = Rect::new(
                    cx - SEEK_THUMB,
                    track_r.y + track_r.h / 2 - SEEK_THUMB,
                    SEEK_THUMB * 2,
                    SEEK_THUMB * 2,
                );
                c.fill_rounded_rect(thumb, SEEK_THUMB as f32, t.accent);
            }
        }
        // Times.
        let times_y = st.y + st.h + 2;
        draw_line(
            c,
            &format_time(self.player.est_pos()),
            &ui::styled(15.0, 400, t.text3),
            ix,
            times_y,
            120,
        );
        let dur_s = if self.player.duration > 0.0 {
            format_time(self.player.duration)
        } else {
            "--:--".to_string()
        };
        // Right-align by measuring is overkill; draw at fixed right slot.
        draw_line(
            c,
            &dur_s,
            &ui::styled(15.0, 400, t.text3),
            np.x + np.w - 16 - 80,
            times_y,
            80,
        );

        // Transport.
        let [prev_r, play_r, next_r] = Self::transport_rects(np);
        draw_circle_button(
            c,
            prev_r,
            IC_PREV,
            26.0,
            t.surface2,
            t.text,
            self.pressed == Some(Hit::Prev),
        );
        draw_circle_button(
            c,
            play_r,
            if self.player.is_playing() {
                IC_PAUSE
            } else {
                IC_PLAY
            },
            34.0,
            t.accent,
            t.text_on_accent,
            self.pressed == Some(Hit::Toggle),
        );
        draw_circle_button(
            c,
            next_r,
            IC_NEXT,
            26.0,
            t.surface2,
            t.text,
            self.pressed == Some(Hit::Next),
        );
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--mcp") {
        headless::serve_mcp();
    }
    if let Some(tokens) = headless::cli_tokens(&args) {
        match headless::parse_cli(&tokens) {
            Ok(cli) => std::process::exit(headless::run_cli(&cli)),
            Err(e) => {
                eprintln!("rabbiz-music: {e}");
                eprintln!("{}", headless::HELP);
                std::process::exit(2);
            }
        }
    }
    rabbiz_app_kit::run_app(MusicApp::new());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rubber_frac_damps_past_edges() {
        assert_eq!(rubber_frac(0.5), 0.5);
        assert_eq!(rubber_frac(0.0), 0.0);
        assert_eq!(rubber_frac(1.0), 1.0);
        // -0.5 past the start → -0.275 displayed.
        assert!((rubber_frac(-0.5) - (-0.5 * RUBBER_FACTOR)).abs() < 1e-6);
        // 1.5 → 1 + 0.5*0.55.
        assert!((rubber_frac(1.5) - (1.0 + 0.5 * RUBBER_FACTOR)).abs() < 1e-6);
    }

    #[test]
    fn rubber_scroll_respects_bounds() {
        assert_eq!(rubber_scroll(50, 100), 50);
        assert_eq!(rubber_scroll(-100, 100), (-100.0 * RUBBER_FACTOR) as i32);
        assert_eq!(
            rubber_scroll(200, 100),
            100 + ((100.0) * RUBBER_FACTOR) as i32
        );
    }

    #[test]
    fn est_pos_extrapolates_only_while_playing() {
        let mut p = Player::new(true);
        p.current = Some(0);
        p.pause = true;
        p.pos = 10.0;
        assert_eq!(p.est_pos(), 10.0);
        p.pause = false;
        p.duration = 100.0;
        let est = p.est_pos();
        assert!((10.0..30.0).contains(&est), "est {est}");
        p.pos = 95.0;
        p.pos_at = Instant::now() - std::time::Duration::from_secs(60);
        assert!(p.est_pos() <= 100.0, "clamped to duration");
    }

    #[test]
    fn seek_fraction_maps_track_width() {
        let np = Rect::new(24, 1200, 672, NP_H);
        let t = MusicApp::seek_track_rect(np);
        let mid = Point::new(t.x + t.w / 2, t.y + t.h / 2);
        assert!((MusicApp::seek_fraction(np, mid) - 0.5).abs() < 0.01);
        let left = Point::new(t.x - 40, mid.y);
        assert!(MusicApp::seek_fraction(np, left) < 0.0);
        let right = Point::new(t.x + t.w + 40, mid.y);
        assert!(MusicApp::seek_fraction(np, right) > 1.0);
    }

    #[test]
    fn transport_rects_are_distinct_and_inside_card() {
        let np = Rect::new(24, 1200, 672, NP_H);
        let [a, b, c] = MusicApp::transport_rects(np);
        assert!(a.w == 56 && b.w == 72 && c.w == 56);
        assert!(a.x < b.x && b.x < c.x);
        assert!(b.y >= np.y && c.y + c.h <= np.y + np.h);
    }

    #[test]
    fn layout_never_hardcodes_panel_size() {
        let app = MusicApp::new();
        for (w, h) in [(720, 1600), (1080, 2400), (540, 960)] {
            let l = app.layout(w, h);
            assert!(l.list.x >= ui::MARGIN);
            assert!(l.list.x + l.list.w <= w - ui::MARGIN + 1);
            assert!(l.list.y < l.list.y + l.list.h);
            if let Some(np) = l.np {
                assert!(np.y + np.h <= h - ui::BOTTOM_SAFE);
            }
        }
    }
}
