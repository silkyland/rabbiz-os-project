use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};

use smithay::backend::libinput::{LibinputInputBackend, LibinputSessionInterface};
use smithay::backend::session::libseat::LibSeatSession;
use smithay::reexports::input::Libinput;

/// Create a `LibinputInputBackend` bound to the active libseat session and seat name.
pub fn create_libinput_backend(
    session: &LibSeatSession,
    seat_name: &str,
) -> Result<LibinputInputBackend, String> {
    let mut libinput = Libinput::new_with_udev(LibinputSessionInterface::from(session.clone()));
    libinput
        .udev_assign_seat(seat_name)
        .map_err(|()| format!("libinput udev_assign_seat({seat_name}) failed"))?;
    Ok(LibinputInputBackend::new(libinput))
}

/// Spawn `/usr/local/bin/a21s-power-key toggle` detached (`setsid`, no wait)
/// so its 300 ms combo guard never stalls the compositor event loop.
pub fn spawn_power_key_toggle() {
    let mut cmd = Command::new("/usr/local/bin/a21s-power-key");
    cmd.arg("toggle")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    unsafe {
        cmd.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    if let Err(err) = cmd.spawn() {
        eprintln!("failed to spawn a21s-power-key toggle: {err}");
    }
}
