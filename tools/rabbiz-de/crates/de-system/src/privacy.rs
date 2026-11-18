//! Privacy indicators (M3-T1): camera / mic / location in-use flags.
//!
//! Best-effort kernel probing, no D-Bus:
//! - **camera**: a `/dev/video*` node with an open fd (walk `/proc/*/fd`,
//!   bounded — the first hit wins, we never scan more than ~256 processes).
//! - **mic**: an ALSA capture PCM in `RUNNING` state
//!   (`/proc/asound/card*/pcm*c/sub*/status`).
//! - **location**: no provider exists on the A21s yet — the flag stays
//!   `false` until a geoclue/GNSS source lands (kept for seam completeness).
//!
//! Everything degrades to `false` on hosts without `/proc` (macOS) — the
//! worker thread polls this, so a slow scan never touches the frame path.

use de_api::services::PrivacyState;
use std::path::Path;

/// Poll all indicators (call from the `de-system` worker, ~1/s).
#[must_use]
pub fn poll_privacy() -> PrivacyState {
    poll_privacy_in(Path::new("/proc"))
}

/// Testable variant with an injectable procfs root.
#[must_use]
pub fn poll_privacy_in(procfs: &Path) -> PrivacyState {
    PrivacyState {
        camera: camera_in_use(procfs),
        mic: mic_running(procfs),
        location: false,
    }
}

fn camera_in_use(procfs: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(procfs) else {
        return false;
    };
    for e in entries.flatten().take(256) {
        let Ok(fds) = std::fs::read_dir(e.path().join("fd")) else {
            continue;
        };
        for fd in fds.flatten().take(64) {
            if let Ok(target) = std::fs::read_link(fd.path()) {
                if target.starts_with("/dev/video") {
                    return true;
                }
            }
        }
    }
    false
}

fn mic_running(procfs: &Path) -> bool {
    let asound = procfs.join("asound");
    let Ok(cards) = std::fs::read_dir(&asound) else {
        return false;
    };
    for card in cards.flatten() {
        let pcms = card.path();
        let Ok(pcm_entries) = std::fs::read_dir(&pcms) else {
            continue;
        };
        for pcm in pcm_entries.flatten() {
            // Capture PCMs are named `pcm*c` under /proc/asound/cardN/.
            if !pcm.file_name().to_string_lossy().ends_with('c') {
                continue;
            }
            let Ok(subs) = std::fs::read_dir(pcm.path()) else {
                continue;
            };
            for sub in subs.flatten() {
                let status = sub.path().join("status");
                if let Ok(text) = std::fs::read_to_string(&status) {
                    if text.contains("RUNNING") {
                        return true;
                    }
                }
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_procfs_is_all_clear() {
        let s = poll_privacy_in(Path::new("/nonexistent/proc"));
        assert_eq!(s, PrivacyState::default());
    }

    #[test]
    fn detects_running_capture() {
        let procfs = tempfile::tempdir().unwrap();
        let sub = procfs.path().join("asound/card0/pcm0c/sub0");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join("status"), "RUNNING\n").unwrap();
        let s = poll_privacy_in(procfs.path());
        assert!(s.mic);
        assert!(!s.camera);
    }
}
