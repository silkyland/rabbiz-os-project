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
/// The notifier should be inserted into the event loop; for headless
/// probes it can simply be kept alive.
pub fn activate() -> Result<(LibSeatSession, LibSeatSessionNotifier), SessionError> {
    LibSeatSession::new().map_err(|e| SessionError::Session(format!("{e:?}")))
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
