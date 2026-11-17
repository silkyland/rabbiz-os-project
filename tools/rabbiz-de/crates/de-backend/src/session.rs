use std::os::unix::io::OwnedFd;
use std::path::Path;

use smithay::backend::session::libseat::{LibSeatSession, LibSeatSessionNotifier};
use smithay::backend::session::Session;
use smithay::reexports::rustix::fs::OFlags;

/// Candidate DRM device paths on Exynos 850 (A21s) and generic Linux.
pub const DRM_CANDIDATE_PATHS: &[&str] = &[
    "/dev/dri/by-path/platform-exynos-drm-card",
    "/dev/dri/card0",
    "/dev/dri/card1",
];

/// Open a libseat session. Respects `LIBSEAT_BACKEND` (e.g. `seatd`).
pub fn open_session() -> Result<(LibSeatSession, LibSeatSessionNotifier), String> {
    LibSeatSession::new().map_err(|err| format!("libseat session init failed: {err}"))
}

/// Open the primary DRM device node through `session`.
pub fn open_drm_fd(session: &mut LibSeatSession) -> Result<(OwnedFd, String), String> {
    let flags = OFlags::RDWR | OFlags::CLOEXEC | OFlags::NOCTTY | OFlags::NONBLOCK;
    let mut last_err = String::from("no candidate DRM path found");

    for path in DRM_CANDIDATE_PATHS {
        let p = Path::new(path);
        if !p.exists() {
            continue;
        }
        match session.open(p, flags) {
            Ok(fd) => return Ok((fd, (*path).to_string())),
            Err(err) => {
                last_err = format!("session.open({path}): {err}");
            }
        }
    }

    Err(last_err)
}
