//! Non-blocking haptic + key-click audio feedback for `rabbiz-osk`.
//!
//! Performance-first design:
//! - Touch/render thread only does a non-blocking `try_send` into a tiny
//!   bounded channel (`cap = 4`), so typing latency is never affected.
//! - One background worker thread handles both ERM vibration
//!   (`/sys/kernel/a21s_vib/duration_ms`) and synthesized click playback
//!   (`pw-play` / `paplay`), with 32–35 ms cooldowns to prevent motor blur or
//!   audio process bursts.
//! - Preferences (`deOskHaptic`, `deOskSound` in `$XDG_CONFIG_HOME/rabbiz-de/prefs.json`)
//!   default to `true` and live-reload by checking file `mtime` at most every
//!   400 ms, so toggling in Settings takes effect immediately without restart.

use std::f32::consts::TAU;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, SyncSender};
use std::time::{Duration, Instant, SystemTime};

const VIB_DURATION_PATH: &str = "/sys/kernel/a21s_vib/duration_ms";
const HAPTIC_COOLDOWN: Duration = Duration::from_millis(32);
const SOUND_COOLDOWN: Duration = Duration::from_millis(35);
const PREFS_RECHECK: Duration = Duration::from_millis(400);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FeedbackKind {
    /// Regular letter / symbol key tap.
    Key,
    /// Space / Backspace / Enter / Shift / Layer / Long-press.
    Mod,
    /// Auto-repeat (held Backspace).
    Repeat,
}

impl FeedbackKind {
    const fn haptic_ms(self) -> u32 {
        match self {
            Self::Key => 22,
            Self::Mod => 28,
            Self::Repeat => 18,
        }
    }
}

pub(crate) struct Feedback {
    tx: SyncSender<FeedbackKind>,
}

impl Feedback {
    pub(crate) fn start() -> Self {
        let (tx, rx) = mpsc::sync_channel::<FeedbackKind>(4);
        let _ = std::thread::Builder::new()
            .name("rabbiz-osk-fb".into())
            .spawn(move || worker(rx));
        Self { tx }
    }

    /// Fire-and-forget from the Wayland touch thread; never blocks.
    pub(crate) fn trigger(&self, kind: FeedbackKind) {
        let _ = self.tx.try_send(kind);
    }
}

struct PrefsCache {
    path: PathBuf,
    last_check: Instant,
    last_mtime: Option<SystemTime>,
    haptic: bool,
    sound: bool,
}

impl PrefsCache {
    fn new() -> Self {
        let path = prefs_path();
        let mut cache = Self {
            path,
            last_check: Instant::now() - PREFS_RECHECK,
            last_mtime: None,
            haptic: true,
            sound: true,
        };
        cache.refresh(Instant::now());
        cache
    }

    fn refresh(&mut self, now: Instant) {
        if now.saturating_duration_since(self.last_check) < PREFS_RECHECK {
            return;
        }
        self.last_check = now;
        let mtime = std::fs::metadata(&self.path)
            .ok()
            .and_then(|m| m.modified().ok());
        if mtime == self.last_mtime && self.last_mtime.is_some() {
            return;
        }
        self.last_mtime = mtime;
        if let Ok(text) = std::fs::read_to_string(&self.path) {
            self.haptic = parse_json_bool(&text, "deOskHaptic").unwrap_or(true);
            self.sound = parse_json_bool(&text, "deOskSound").unwrap_or(true);
        } else {
            self.haptic = true;
            self.sound = true;
        }
    }
}

fn prefs_path() -> PathBuf {
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME").filter(|v| !v.is_empty()) {
        return PathBuf::from(xdg).join("rabbiz-de").join("prefs.json");
    }
    if let Some(home) = std::env::var_os("HOME").filter(|v| !v.is_empty()) {
        return PathBuf::from(home)
            .join(".config")
            .join("rabbiz-de")
            .join("prefs.json");
    }
    PathBuf::from("/tmp/rabbiz-de-prefs.json")
}

/// Extract a boolean value for `"key": true|false` from flat JSON without
/// pulling serde into `osk-wayland`.
fn parse_json_bool(json: &str, key: &str) -> Option<bool> {
    let needle = format!("\"{key}\"");
    let idx = json.find(&needle)?;
    let after_key = &json[idx + needle.len()..];
    let colon = after_key.find(':')?;
    let val = after_key[colon + 1..].trim_start();
    if val.starts_with("true") {
        Some(true)
    } else if val.starts_with("false") {
        Some(false)
    } else {
        None
    }
}

fn runtime_dir() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
}

/// Synthesize a crisp, short mobile key-click ("tock") 16-bit 48 kHz mono WAV.
fn synthesize_click_wav(base_hz: f32, duration_ms: u32, gain: f32) -> Vec<u8> {
    const RATE: u32 = 48_000;
    let samples = (RATE * duration_ms / 1000) as usize;
    let data_bytes = (samples * 2) as u32;
    let mut wav = Vec::with_capacity(44 + data_bytes as usize);
    // RIFF/WAVE PCM16 mono header.
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_bytes).to_le_bytes());
    wav.extend_from_slice(b"WAVE");
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
    wav.extend_from_slice(&1u16.to_le_bytes()); // mono
    wav.extend_from_slice(&RATE.to_le_bytes());
    wav.extend_from_slice(&(RATE * 2).to_le_bytes()); // byte rate
    wav.extend_from_slice(&2u16.to_le_bytes()); // block align
    wav.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_bytes.to_le_bytes());

    // Deterministic PRNG for the 1.5 ms percussive transient tap.
    let mut rng: u32 = 0x51A7_9E3B;
    for i in 0..samples {
        let t = i as f32 / RATE as f32;
        let progress = i as f32 / samples.max(1) as f32;
        // Fast 0.4 ms attack so there's no harsh DC click, exponential decay.
        let attack = (t / 0.0004).min(1.0);
        let env = attack * (-t * 420.0).exp() * (1.0 - progress);
        // Short woody click body (fundamental + 1.85x overtone).
        let body = (TAU * base_hz * t).sin() * 0.72 + (TAU * base_hz * 1.85 * t).sin() * 0.28;
        // Brief filtered transient burst in the first 1.5 ms.
        rng = rng.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let noise = ((rng >> 16) as f32 / 32768.0) - 1.0;
        let transient = noise * (-t * 1800.0).exp() * 0.45;
        let sample = ((body * env + transient * attack) * gain).clamp(-1.0, 1.0);
        let pcm = (sample * i16::MAX as f32) as i16;
        wav.extend_from_slice(&pcm.to_le_bytes());
    }
    wav
}

fn prepare_click_wavs() -> (Option<PathBuf>, Option<PathBuf>) {
    let dir = runtime_dir();
    let key_path = dir.join("rabbiz-osk-click.wav");
    let mod_path = dir.join("rabbiz-osk-click-mod.wav");
    let key_ok = std::fs::write(&key_path, synthesize_click_wav(1850.0, 11, 0.34)).is_ok();
    let mod_ok = std::fs::write(&mod_path, synthesize_click_wav(1250.0, 13, 0.38)).is_ok();
    (key_ok.then_some(key_path), mod_ok.then_some(mod_path))
}

#[derive(Clone, Copy)]
enum AudioPlayer {
    PwPlay,
    Paplay,
}

fn detect_player() -> Option<AudioPlayer> {
    if Path::new("/usr/bin/pw-play").exists() {
        Some(AudioPlayer::PwPlay)
    } else if Path::new("/usr/bin/paplay").exists() {
        Some(AudioPlayer::Paplay)
    } else {
        None
    }
}

fn play_wav(player: AudioPlayer, path: &Path, repeat: bool) -> Option<Child> {
    let mut cmd = match player {
        AudioPlayer::PwPlay => {
            let mut c = Command::new("pw-play");
            c.arg("--volume")
                .arg(if repeat { "0.22" } else { "0.38" })
                .arg(path);
            c
        }
        AudioPlayer::Paplay => {
            let mut c = Command::new("paplay");
            c.arg(if repeat {
                "--volume=18000"
            } else {
                "--volume=32000"
            })
            .arg(path);
            c
        }
    };
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .ok()
}

fn reap_children(children: &mut Vec<Child>) {
    children.retain_mut(|c| matches!(c.try_wait(), Ok(None)));
}

fn worker(rx: mpsc::Receiver<FeedbackKind>) {
    let mut prefs = PrefsCache::new();
    let (key_wav, mod_wav) = prepare_click_wavs();
    let player = detect_player();
    let has_vib = Path::new(VIB_DURATION_PATH).exists();
    let mut last_haptic: Option<Instant> = None;
    let mut last_sound: Option<Instant> = None;
    let mut children: Vec<Child> = Vec::new();

    while let Ok(kind) = rx.recv() {
        // Drain burst to the newest event so rapid taps never lag behind.
        let mut latest = kind;
        while let Ok(next) = rx.try_recv() {
            latest = next;
        }
        let now = Instant::now();
        prefs.refresh(now);
        reap_children(&mut children);

        if prefs.haptic && has_vib {
            let ok = last_haptic.is_none_or(|p| now.saturating_duration_since(p) >= HAPTIC_COOLDOWN);
            if ok {
                last_haptic = Some(now);
                let _ = std::fs::write(VIB_DURATION_PATH, format!("{}\n", latest.haptic_ms()));
            }
        }

        if prefs.sound {
            if let Some(p) = player {
                let ok =
                    last_sound.is_none_or(|prev| now.saturating_duration_since(prev) >= SOUND_COOLDOWN);
                let wav = match latest {
                    FeedbackKind::Mod => mod_wav.as_deref().or(key_wav.as_deref()),
                    FeedbackKind::Key | FeedbackKind::Repeat => key_wav.as_deref(),
                };
                if ok && children.len() < 3 {
                    if let Some(path) = wav {
                        last_sound = Some(now);
                        if let Some(child) = play_wav(p, path, latest == FeedbackKind::Repeat) {
                            children.push(child);
                        }
                    }
                }
            }
        }
    }
    for mut child in children {
        let _ = child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_json_bool_handles_true_false_and_missing() {
        let json = r#"{"deTheme":"dark","deOskHaptic": false, "deOskSound":true}"#;
        assert_eq!(parse_json_bool(json, "deOskHaptic"), Some(false));
        assert_eq!(parse_json_bool(json, "deOskSound"), Some(true));
        assert_eq!(parse_json_bool(json, "missing"), None);
    }

    #[test]
    fn synthesize_click_wav_has_valid_header_and_signal() {
        let wav = synthesize_click_wav(1850.0, 11, 0.34);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(&wav[36..40], b"data");
        assert!(wav.len() > 44 + 800);
        // Must have non-zero audio energy in the attack window.
        let peak = wav[44..]
            .chunks_exact(2)
            .map(|c| i16::from_le_bytes([c[0], c[1]]).unsigned_abs())
            .max()
            .unwrap_or(0);
        assert!(peak > 1000, "expected audible click peak, got {peak}");
    }
}
