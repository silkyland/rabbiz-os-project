//! Session calloop event loop and shell input-method bridge polling.

use std::sync::Arc;
use std::time::{Duration, Instant};

use smithay::reexports::calloop::generic::Generic;
use smithay::reexports::calloop::timer::{TimeoutAction, Timer};
use smithay::reexports::calloop::{EventLoop, Interest, Mode, PostAction};
use smithay::reexports::wayland_server::Display;
use smithay::wayland::socket::ListeningSocketSource;

use crate::runtime::frame_io::{dump_frame_if_triggered, minute_of_day};
use crate::state::{De, DeClientState};

use super::Run;

/// Run the session event loop until `run.duration` elapses (session mode:
/// no duration → run until killed).
#[allow(unsafe_code)] // wayland-server's dispatch_clients is an unsafe FFI call
#[allow(clippy::too_many_lines)] // source registration order is the documentation
pub fn event_loop(mut run: Run, display: Display<De>, print_stats: bool) -> Result<u8, String> {
    let mut event_loop: EventLoop<Run> =
        EventLoop::try_new().map_err(|e| format!("event loop: {e}"))?;
    let handle = event_loop.handle();

    // Wayland client socket: fixed name `wayland-0` (§7.20).
    let socket = ListeningSocketSource::with_name("wayland-0")
        .map_err(|e| format!("wayland socket: {e:?}"))?;
    let socket_name = socket.socket_name().to_string_lossy().into_owned();
    run.de.socket_name = Some(socket_name.clone());
    handle
        .insert_source(socket, |stream, (), run: &mut Run| {
            let state = Arc::new(DeClientState::default());
            match run.de.display_handle.insert_client(stream, state.clone()) {
                Ok(client) => crate::privileged::identify(&run.de.display_handle, &client, &state),
                Err(err) => eprintln!("insert_client: {err}"),
            }
        })
        .map_err(|e| format!("socket source: {e}"))?;

    // Client dispatch source.
    handle
        .insert_source(
            Generic::new(display, Interest::READ, Mode::Level),
            |_, display, run: &mut Run| {
                profiling_scope_display_dispatch();
                unsafe {
                    display.get_mut().dispatch_clients(&mut run.de).unwrap();
                }
                // Events we queued during dispatch (registry globals,
                // configures, frame callbacks) only reach the client after
                // an explicit flush — without this clients stall after
                // wl_display.sync.
                if let Err(e) = run.de.display_handle.flush_clients() {
                    eprintln!("flush_clients: {e}");
                }
                run.pump();
                Ok(PostAction::Continue)
            },
        )
        .map_err(|e| format!("display source: {e}"))?;

    // libseat notifier: keep the session dispatchable.
    let seat = run
        .de
        .seat_notifier
        .take()
        .ok_or("seat notifier consumed")?;
    handle
        .insert_source(seat, |_, (), _: &mut Run| {})
        .map_err(|e| format!("seat source: {e}"))?;

    // libinput.
    let libinput = de_backend::libinput::init(&run.de.session, &run.de.seat_name);
    handle
        .insert_source(libinput, |event, (), run: &mut Run| {
            run.de.process_input_event(event);
            run.pump();
        })
        .map_err(|e| format!("libinput source: {e}"))?;

    // Gesture holds must work without manufactured touch motion.
    handle
        .insert_source(Timer::immediate(), |_, (), run: &mut Run| {
            run.de.tick_gestures();
            if run.de.needs_present {
                run.pump();
            }
            TimeoutAction::ToDuration(Duration::from_millis(if run.de.gestures.active() {
                16
            } else {
                100
            }))
        })
        .map_err(|e| format!("gesture timer: {e}"))?;

    // Clock tick — re-raster the shell frame each minute.
    let timer = Timer::immediate();
    handle
        .insert_source(timer, |_, (), run: &mut Run| {
            if run.de.shell_minute != minute_of_day() {
                run.de.needs_present = true;
                run.pump();
            }
            TimeoutAction::ToDuration(Duration::from_secs(30))
        })
        .map_err(|e| format!("timer source: {e}"))?;

    // Frame dump trigger: touch `$XDG_RUNTIME_DIR/rabbiz-de.shot` → the next
    // tick writes the composited shell frame to `rabbiz-de-shot.ppm`. Device
    // debug path — clients use `zwlr_screencopy_manager_v1` (shm only, so
    // dmabuf/EGL content is not captured there either).
    let shot_timer = Timer::immediate();
    handle
        .insert_source(shot_timer, |_, (), run: &mut Run| {
            dump_frame_if_triggered(&run.de);
            run.de.check_osk();
            poll_shell_im_bridge(&mut run.de);
            run.de.poll_ctl_ipc();
            run.de.poll_panels();
            run.de.poll_power();
            run.de.poll_recents_backdrop();
            if run.de.needs_present {
                run.pump();
            }
            TimeoutAction::ToDuration(Duration::from_millis(500))
        })
        .map_err(|e| format!("shot timer source: {e}"))?;

    // DRM vblank source.
    let notifier = run
        .de
        .setup
        .notifier
        .take()
        .ok_or("drm notifier consumed")?;
    handle
        .insert_source(notifier, |event, _meta, run: &mut Run| {
            if let smithay::backend::drm::DrmEvent::VBlank(_) = event {
                run.on_vblank();
            }
        })
        .map_err(|e| format!("drm source: {e}"))?;

    let deadline = run.duration.map(|d| Instant::now() + d);
    // Kick off the first frame.
    run.present()?;
    let sock = run.de.shell_im.sock_path().cloned();
    let kb_exec = de_config::keyboards::resolve_active_keyboard(run.de.app_prefs.prefs()).exec;
    match crate::osk::spawn(
        &mut run.de.display_handle,
        &socket_name,
        &kb_exec,
        sock.as_deref(),
        run.de.hw_keyboard,
    ) {
        Ok(child) => {
            run.de.osk_spawned_at = Some(Instant::now());
            run.de.osk = Some(child);
        }
        Err(error) => {
            eprintln!("rabbiz-de: could not start the session OSK: {error}");
            run.de.osk_respawn_at = Some(Instant::now() + Duration::from_secs(30));
        }
    }

    loop {
        let wait = [deadline, run.motion_wake_at()]
            .into_iter()
            .flatten()
            .min()
            .map(|d| d.saturating_duration_since(Instant::now()));
        event_loop
            .dispatch(wait, &mut run)
            .map_err(|e| format!("dispatch: {e}"))?;
        poll_shell_im_bridge(&mut run.de);
        run.de.poll_ctl_ipc();
        if run.de.needs_present {
            run.pump();
        }
        if !run.de.running.load(std::sync::atomic::Ordering::Relaxed) {
            break;
        }
        if deadline.is_some_and(|d| Instant::now() >= d) {
            break;
        }
        if run.motion_wake_at().is_some_and(|d| Instant::now() >= d) {
            if run.de.foreground == crate::state::Foreground::Shell
                && run.de.nav.current == de_ui::nav::Screen::Search
            {
                run.de.shell_dirty = true;
            }
            run.de.needs_present = true;
            run.pump();
        }
    }
    run.finish(print_stats);
    Ok(0)
}

#[inline]
fn profiling_scope_display_dispatch() {}

pub(super) fn run_de_shell_im_wake(run: &Run) -> bool {
    run.de.shell_im.is_shown()
}

fn poll_shell_im_bridge(de: &mut De) {
    let want_visible = de.foreground == crate::state::Foreground::Shell
        && !de.locked()
        && !de.sleeping()
        && matches!(
            de.nav.current,
            de_ui::nav::Screen::Search | de_ui::nav::Screen::Drawer
        );
    if !want_visible && de.shell_im.is_shown() {
        de.shell_im.set_visible(false);
    }
    let events = de.shell_im.poll();
    for event in events {
        if !want_visible {
            continue;
        }
        match event {
            crate::osk::ShellImEvent::Commit(text) => {
                de.navigation.search_query.push_str(&text);
                de.navigation.drawer_scroll = 0;
                de.shell_dirty = true;
                de.needs_present = true;
            }
            crate::osk::ShellImEvent::Backspace => {
                de.navigation.search_query.pop();
                de.navigation.drawer_scroll = 0;
                de.shell_dirty = true;
                de.needs_present = true;
            }
            crate::osk::ShellImEvent::Enter => {
                de.shell_im.set_visible(false);
                if de.nav.current == de_ui::nav::Screen::Search {
                    let rows = de.search_rows();
                    if let Some(first) = rows.first() {
                        let id = first.id.clone();
                        let size =
                            de_api::Size::new(de.output.width as i32, de.output.height as i32);
                        let rect = de_ui::search::icon_rect(size, &rows, 0);
                        de.try_launch(&id, rect);
                    }
                }
                de.needs_present = true;
            }
        }
    }
}
