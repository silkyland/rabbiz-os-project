//! libinput backend on the active libseat session (M1-T1).

use smithay::backend::libinput::{LibinputInputBackend, LibinputSessionInterface};
use smithay::backend::session::libseat::LibSeatSession;
use smithay::reexports::input::Libinput;

/// Create the libinput context bound to `seat` and wrap it in the calloop
/// event source. Device nodes are opened through the session, so no extra
/// privileges are needed.
#[must_use]
pub fn init(session: &LibSeatSession, seat: &str) -> LibinputInputBackend {
    let mut ctx =
        Libinput::new_with_udev::<LibinputSessionInterface<LibSeatSession>>(session.clone().into());
    ctx.udev_assign_seat(seat)
        .expect("libinput udev_assign_seat failed");
    LibinputInputBackend::new(ctx)
}
