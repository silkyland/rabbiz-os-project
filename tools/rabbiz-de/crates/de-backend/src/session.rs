//! libseat session handling: open the seat, open device nodes through it.

use std::path::Path;

use smithay::backend::session::libseat::{LibSeatSession, LibSeatSessionNotifier};
use smithay::backend::session::Session;
use smithay::reexports::rustix::fs::OFlags;
use std::os::unix::io::OwnedFd;

/// Errors opening a session or device.
#[derive(Debug)]
pub enum SessionError {
    Session(String),
    Open { path: String, source: String },
}

impl std::fmt::Display for SessionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Session(e) => write!(f, "session: {e}"),
            Self::Open { path, source } => write!(f, "open {path}: {source}"),
        }
    }
}

impl std::error::Error for SessionError {}

/// Activate a libseat session (uses `LIBSEAT_BACKEND`, e.g. `seatd`).
///
/// On a VT-bound seat, seatd grants DRM master only while the seat is
/// active — and unlike wlroots (which performs the VT dance itself),
/// smithay leaves the switch to the caller. If the session comes up
/// inactive, ask seatd to switch to the foreground VT and give it a
/// moment to enable the client before the caller opens the DRM device.
/// Best effort only: a `SET_MASTER` ioctl on the vended fd still fails
/// EACCES because seatd is the fd's opener — that warning is benign.
///
/// The notifier should be inserted into the event loop; for headless
/// probes it can simply be kept alive.
pub fn activate() -> Result<(LibSeatSession, LibSeatSessionNotifier), SessionError> {
    let (mut session, notifier) =
        LibSeatSession::new().map_err(|e| SessionError::Session(format!("{e:?}")))?;
    if !session.is_active() {
        if let Some(vt) = foreground_vt() {
            // Best effort — non-VT-bound seats (nested, logind-less setups)
            // simply ignore the request.
            let _ = session.change_vt(vt);
            std::thread::sleep(std::time::Duration::from_millis(400));
        }
    }
    Ok((session, notifier))
}

/// Foreground VT number from `/sys/class/tty/tty0/active` (`"tty1"` → 1).
fn foreground_vt() -> Option<i32> {
    parse_active_vt(&std::fs::read_to_string("/sys/class/tty/tty0/active").ok()?)
}

/// Parse the sysfs `ttyN` value into a VT number.
fn parse_active_vt(s: &str) -> Option<i32> {
    s.trim()
        .strip_prefix("tty")?
        .parse::<i32>()
        .ok()
        .filter(|vt| *vt > 0)
}

/// Open a device node through the active session (e.g. `/dev/dri/card0`,
/// `/dev/input/event*`).
pub fn open_device(session: &mut LibSeatSession, path: &Path) -> Result<OwnedFd, SessionError> {
    session
        .open(
            path,
            OFlags::RDWR | OFlags::CLOEXEC | OFlags::NOCTTY | OFlags::NONBLOCK,
        )
        .map_err(|source| SessionError::Open {
            path: path.display().to_string(),
            source: format!("{source}"),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tty_names_and_rejects_garbage() {
        assert_eq!(parse_active_vt("tty1\n"), Some(1));
        assert_eq!(parse_active_vt("tty7"), Some(7));
        assert_eq!(parse_active_vt("  tty12  "), Some(12));
        assert_eq!(parse_active_vt("tty0"), None);
        assert_eq!(parse_active_vt("tty"), None);
        assert_eq!(parse_active_vt("vt3"), None);
        assert_eq!(parse_active_vt(""), None);
        assert_eq!(parse_active_vt("tty-1"), None);
        assert_eq!(parse_active_vt("tty1x"), None);
    }
}
