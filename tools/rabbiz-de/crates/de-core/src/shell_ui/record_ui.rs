//! Screen recording (Quick Settings tile): the compositor spawns `wf-recorder`
//! over a private `WAYLAND_SOCKET` pair with only the screencopy grant
//! (`DE-PRIVILEGED-PROTOCOLS.md` — never the public-socket allow-list).
//! Toggling off sends SIGINT, which is how wf-recorder finalizes the file;
//! when the compositor exits the private socket breaks and the recorder
//! stops on its own.

use std::path::PathBuf;
use std::process::Child;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use de_config::Privileged;
use smithay::reexports::wayland_server::DisplayHandle;

use crate::state::De;

pub(crate) struct RecordState {
    child: Option<Child>,
    /// Where the current recording is being written (for feedback/UX).
    pub path: Option<PathBuf>,
    pub started: Option<Instant>,
}

impl Default for RecordState {
    fn default() -> Self {
        Self {
            child: None,
            path: None,
            started: None,
        }
    }
}

impl RecordState {
    #[must_use]
    pub fn recording(&self) -> bool {
        self.child.is_some()
    }
}

/// True when a wf-recorder binary is installed; unavailable tiles are dimmed.
#[must_use]
pub fn recorder_available() -> bool {
    ["/usr/bin/wf-recorder", "/usr/local/bin/wf-recorder"]
        .iter()
        .any(|p| std::path::Path::new(p).exists())
}

fn output_path() -> Option<PathBuf> {
    let videos = std::env::var_os("XDG_VIDEOS_DIR")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .filter(|v| !v.is_empty())
                .map(|h| PathBuf::from(h).join("Videos"))
        })?;
    let dir = videos.join("Screenrecordings");
    std::fs::create_dir_all(&dir).ok()?;
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    Some(dir.join(format!("Screenrecord_{ms}.mp4")))
}

impl De {
    /// Toggle screen recording. Returns the new recording state, or an error
    /// string when starting failed (stop never fails — the child either takes
    /// the signal or is already gone).
    pub(crate) fn toggle_recording(&mut self) -> Result<bool, String> {
        if self.record.child.is_some() {
            if let Some(mut child) = self.record.child.take() {
                // SIGINT, not kill(): wf-recorder only writes a valid mp4
                // trailer when it shuts down through its signal handler.
                unsafe {
                    libc::kill(child.id() as i32, libc::SIGINT);
                }
                // Do not wait on the frame thread; the finalizer thread
                // inside wf-recorder exits by itself and the zombie is
                // reaped on the next poll or drop.
                std::thread::spawn(move || {
                    let _ = child.wait();
                });
            }
            self.record.path = None;
            self.record.started = None;
            return Ok(false);
        }
        let path = output_path().ok_or("no_videos_dir")?;
        let path_str = path.to_string_lossy().into_owned();
        let child = crate::protocols::privileged::spawn_private_helper(
            &mut self.display_handle,
            "rabbiz-de-record",
            "wf-recorder",
            &["-f", &path_str],
            vec![Privileged::Screencopy],
        )
        .map_err(|e| format!("spawn wf-recorder: {e}"))?;
        self.record.child = Some(child);
        self.record.path = Some(path);
        self.record.started = Some(Instant::now());
        Ok(true)
    }

    /// Reap a recorder that exited on its own (error, closed output, broken
    /// stream) so the tile never shows a stale recording state.
    pub(crate) fn poll_recording(&mut self) {
        let exited = self
            .record
            .child
            .as_mut()
            .is_some_and(|c| matches!(c.try_wait(), Ok(Some(_)) | Err(_)));
        if exited {
            self.record.child = None;
            self.record.path = None;
            self.record.started = None;
            self.panels.set_tile_on(10, false);
            self.shell_dirty = true;
        }
    }
}
