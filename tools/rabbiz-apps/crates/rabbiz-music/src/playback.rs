//! Playback controller — manages the `mpv` child process and tracks observed
//! playback state (position, duration, pause, EOF, media title, file path).

use std::cmp::Ordering;
use std::collections::VecDeque;
use std::time::Instant;

use crate::library::{self, finish_next_index, seek_seconds, Repeat, Track};
use crate::player::{self, socket_path, Line, Mpv};

/// Cheap entropy for shuffle picks — subsec nanos of the wall clock.
fn shuffle_seed() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| (d.as_nanos() & 0xFFFF_FFFF_FFFF_FFFF) as u64)
        .unwrap_or(0x9E3779B9)
}

pub(crate) struct Player {
    pub(crate) mpv: Option<Mpv>,
    /// `mpv --version` probe result from startup; gates the honest banner.
    pub(crate) mpv_ok: bool,
    /// GUI transport toggles — shuffle picks a random track on advance.
    pub(crate) shuffle: bool,
    /// Repeat mode honored at end-of-file (off/all/one).
    pub(crate) repeat: Repeat,
    /// Properties subscribed for the current connection.
    observed: bool,
    /// Commands issued while the socket isn't up yet (FIFO, flushed on connect).
    pending_cmds: VecDeque<Vec<String>>,
    pub(crate) current: Option<usize>,
    pause: bool,
    /// Last observed `time-pos` and when it was observed — the display
    /// extrapolates from this while playing.
    pos: f64,
    pos_at: Instant,
    pub(crate) duration: f64,
    pub(crate) eof: bool,
    media_title: String,
    /// Last observed mpv `path` property — the only "what is loaded" signal
    /// a headless client gets when it attaches to a player it didn't load.
    pub(crate) file_path: String,
    pub(crate) error: Option<String>,
    last_get: Instant,
}

impl Player {
    pub(crate) fn new(mpv_ok: bool) -> Self {
        Self {
            mpv: None,
            mpv_ok,
            shuffle: false,
            repeat: Repeat::Off,
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

    pub(crate) fn has_track(&self) -> bool {
        self.current.is_some() || !self.file_path.is_empty()
    }

    pub(crate) fn is_playing(&self) -> bool {
        self.has_track() && !self.pause && !self.eof
    }

    /// Display position: observed `time-pos` extrapolated by wall clock
    /// while playing, clamped to the known duration.
    pub(crate) fn est_pos(&self) -> f64 {
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

    pub(crate) fn progress(&self) -> f32 {
        library::progress_fraction(self.est_pos(), self.duration)
    }

    pub(crate) fn display_title<'a>(&'a self, tracks: &'a [Track]) -> &'a str {
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
    pub(crate) fn attach(&mut self) -> bool {
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
    pub(crate) fn ensure_started(&mut self) {
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
    pub(crate) fn cmd(&mut self, args: &[&str]) -> bool {
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
    pub(crate) fn play_track(&mut self, i: usize, tracks: &[Track]) {
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
    pub(crate) fn toggle(&mut self, tracks: &[Track]) {
        if self.eof {
            if let Some(i) = self.current {
                self.play_track(i, tracks);
            }
            return;
        }
        self.cmd(&["cycle", "pause"]);
        self.pause = !self.pause; // optimistic; property-change corrects
    }

    pub(crate) fn seek_to_fraction(&mut self, frac: f32) {
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
    pub(crate) fn seek_absolute(&mut self, secs: f64) {
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
    pub(crate) fn set_paused(&mut self, paused: bool) {
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
    pub(crate) fn stop(&mut self) {
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
    pub(crate) fn pump(&mut self, tracks: &[Track]) -> bool {
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

    /// End of the current file: advance per repeat/shuffle, or park paused
    /// at the end of the list (repeat-off never loops).
    fn finish_track(&mut self, tracks: &[Track]) {
        match finish_next_index(
            self.current,
            tracks.len(),
            self.repeat,
            self.shuffle,
            shuffle_seed(),
        ) {
            Some(i) => self.play_track(i, tracks),
            None => self.pause = true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
