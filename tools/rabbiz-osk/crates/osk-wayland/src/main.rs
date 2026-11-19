//! `rabbiz-osk` — the real keyboard: a bottom layer-shell surface that draws `osk-core` layouts
//! with `osk-render` and delivers text through `zwp_input_method_v2`.
//!
//! It never takes keyboard focus; text goes only through the input-method protocol, so no
//! keymap tricks are needed for Thai or emoji. squeekboard must not run at the same time.
//!
//! Compatibility: it also owns the `sm.puri.OSK0` D-Bus name (SetVisible/Visible) so the shell's
//! Quick Settings tiles and `rabbiz-osk-ctl` keep working, plus `org.rabbiz.OSK1.ReloadTheme`.

mod app;
mod clipboard;
mod dbus;
mod input_method;
mod shell_bridge;
mod surface;
mod theme;
mod touch;

use std::collections::{HashMap, HashSet};
use std::os::fd::AsRawFd;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use osk_core::gesture::GestureDetector;
use osk_core::layout::Layout;
use osk_core::state::Keyboard;
use osk_render::{Renderer, Theme};

use smithay_client_toolkit::reexports::client::{
    globals::registry_queue_init, protocol::wl_seat, Connection,
};
use smithay_client_toolkit::{
    compositor::CompositorState,
    output::OutputState,
    registry::RegistryState,
    seat::SeatState,
    shell::{
        wlr_layer::{Anchor, KeyboardInteractivity, Layer, LayerShell},
        WaylandSurface,
    },
    shm::{slot::SlotPool, Shm},
};

use wayland_protocols_misc::zwp_input_method_v2::client::zwp_input_method_manager_v2::ZwpInputMethodManagerV2;
use wayland_protocols_misc::zwp_virtual_keyboard_v1::client::zwp_virtual_keyboard_manager_v1::ZwpVirtualKeyboardManagerV1;

use app::App;
use clipboard::spawn_clipboard_watch;
use dbus::spawn_dbus;
use input_method::upload_keymap;
use shell_bridge::ShellImClient;
use surface::NAV_BAR_H;
use theme::{load_theme, theme_path};

fn emoji_state_path() -> std::path::PathBuf {
    osk_core::predict::state_path().with_file_name("emoji-recent.txt")
}

fn poll_fd(fd: std::os::fd::BorrowedFd<'_>, timeout_ms: i32) -> bool {
    let mut pollfd = libc::pollfd {
        fd: fd.as_raw_fd(),
        events: libc::POLLIN,
        revents: 0,
    };
    unsafe { libc::poll(&mut pollfd, 1, timeout_ms) > 0 }
}

fn main() {
    let layouts = [
        ("en", include_str!("../../../data/layouts/en-us.toml")),
        ("th", include_str!("../../../data/layouts/th-kedmanee.toml")),
        ("emoji", include_str!("../../../data/layouts/emoji.toml")),
    ]
    .iter()
    .map(|(_, src)| {
        Layout::parse(src).unwrap_or_else(|err| {
            eprintln!("rabbiz-osk: bad layout: {err}");
            std::process::exit(1);
        })
    })
    .collect::<Vec<_>>();

    let mut keyboard = match Keyboard::new(layouts) {
        Ok(keyboard) => keyboard,
        Err(err) => {
            eprintln!("rabbiz-osk: {err}");
            std::process::exit(1);
        }
    };
    // K6: predictions learn from the user's own typing; nothing is shipped but the seeds.
    keyboard.predictor = osk_core::predict::Predictor::load(&osk_core::predict::state_path());
    keyboard.load_recents(&emoji_state_path());

    let hw_keyboard = osk_core::hw_keyboard_mode();
    if hw_keyboard {
        eprintln!(
            "rabbiz-osk: hardware keyboard detected — window hidden, input method still active"
        );
    }

    let connection = Connection::connect_to_env().expect("rabbiz-osk needs a Wayland session");
    let (globals, mut queue) = registry_queue_init(&connection).expect("registry");
    let qh = queue.handle();
    let compositor = CompositorState::bind(&globals, &qh).expect("wl_compositor");
    let layer_shell = LayerShell::bind(&globals, &qh).expect("zwlr_layer_shell_v1");
    let shm = Shm::bind(&globals, &qh).expect("wl_shm");
    let im_manager = globals
        .bind::<ZwpInputMethodManagerV2, App, ()>(&qh, 1..=1, ())
        .expect("zwp_input_method_manager_v2 (labwc 0.20 advertises it)");
    // A seat proxy of our own: the input method is created per seat, and the surface only
    // needs text delivery, not keyboard focus.
    let seat = globals
        .bind::<wl_seat::WlSeat, App, ()>(&qh, 1..=7, ())
        .expect("wl_seat");

    let surface = compositor.create_surface(&qh);
    let layer =
        layer_shell.create_layer_surface(&qh, surface, Layer::Top, Some("rabbiz-osk"), None);
    layer.set_anchor(Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
    let h = keyboard.layout().height_px().round() as u32;
    layer.set_size(0, h);
    // Start hidden below the screen edge without reserving exclusive space.
    layer.set_margin(0, 0, -(h as i32 + NAV_BAR_H), 0);
    layer.set_exclusive_zone(0);
    layer.set_keyboard_interactivity(KeyboardInteractivity::None);
    layer.commit();

    let visible = Arc::new(AtomicBool::new(true));
    let reload_theme = Arc::new(AtomicBool::new(false));
    let next_language = Arc::new(AtomicBool::new(false));
    spawn_dbus(visible.clone(), reload_theme.clone(), next_language.clone());

    let clipboard: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let clipboard_changed = Arc::new(AtomicBool::new(false));
    spawn_clipboard_watch(clipboard.clone(), clipboard_changed.clone());

    let pool = SlotPool::new(720 * h as usize * 4 * 2, &shm).expect("shm pool");
    let mut app = App {
        reg: RegistryState::new(&globals),
        seats: SeatState::new(&globals, &qh),
        out: OutputState::new(&globals, &qh),
        shm,
        pool,
        layer: layer.clone(),
        im: im_manager.get_input_method(&seat, &qh, ()),
        virtual_keyboard: {
            let manager = globals
                .bind::<ZwpVirtualKeyboardManagerV1, App, ()>(&qh, 1..=1, ())
                .expect("zwp_virtual_keyboard_manager_v1");
            let keyboard = manager.create_virtual_keyboard(&seat, &qh, ());
            upload_keymap(&keyboard);
            keyboard
        },
        done_count: 0,
        keyboard,
        gesture: GestureDetector::new(),
        renderer: Renderer::new(),
        theme: load_theme(&theme_path()).unwrap_or_else(Theme::dark),
        active: false,
        shell_active: false,
        has_surrounding_text: false,
        user_visible: true,
        draw_pending: false,
        configured: false,
        visible: false,
        w: 720,
        h,
        held: None,
        pressed: Vec::new(),
        alt: None,
        touch_pos: HashMap::new(),
        strip_touches: HashSet::new(),
        shell_im: ShellImClient::connect_from_env(),
        conn: connection.clone(),
        hw_keyboard,
        start: Instant::now(),
        predictor_saved: Instant::now(),
        clipboard: clipboard.clone(),
        clipboard_changed: clipboard_changed.clone(),
        sheet: None,
    };
    eprintln!(
        "rabbiz-osk: up, layout {} ({}x{}), waiting for text-input activation",
        app.keyboard.layout().id,
        app.w,
        app.h
    );

    loop {
        if let Err(err) = queue.dispatch_pending(&mut app) {
            eprintln!("rabbiz-osk: dispatch: {err}");
            break;
        }
        let _ = app.conn.flush();

        // D-Bus compat + theme reload are polled (cheap: one atomic load per turn).
        if app.clipboard_changed.swap(false, Ordering::SeqCst) && app.sheet.is_some() {
            app.sheet = Some(app.clipboard.lock().map(|g| g.clone()).unwrap_or_default());
            app.draw();
        }
        if let Some(mut bridge) = app.shell_im.take() {
            let cmds = bridge.poll_events();
            app.shell_im = Some(bridge);
            for cmd in cmds {
                match cmd.as_str() {
                    "SHOW" => {
                        app.keyboard.reset_for_activation();
                        app.has_surrounding_text = false;
                        app.shell_active = true;
                        if app.user_visible {
                            app.show();
                        }
                    }
                    "HIDE" => {
                        app.shell_active = false;
                        if !app.active {
                            app.hide();
                        }
                    }
                    _ => {}
                }
            }
        }
        let want = visible.load(Ordering::SeqCst);
        if want != app.user_visible {
            app.user_visible = want;
            if app.active || app.shell_active {
                if want {
                    app.show();
                } else {
                    app.hide();
                }
            }
        }
        if app.keyboard.predictor.is_dirty() && app.predictor_saved.elapsed().as_secs() >= 2 {
            let path = osk_core::predict::state_path();
            if let Err(err) = app.keyboard.predictor.save(&path) {
                eprintln!("rabbiz-osk: predictor save failed: {err}");
            }
            app.keyboard.predictor.mark_clean();
            app.predictor_saved = Instant::now();
        }
        if app.keyboard.recents_dirty() {
            if let Err(err) = app.keyboard.save_recents(&emoji_state_path()) {
                eprintln!("rabbiz-osk: emoji recents save failed: {err}");
            }
            app.keyboard.mark_recents_clean();
        }
        if next_language.swap(false, Ordering::SeqCst) {
            let commands = app.keyboard.cycle_language();
            app.send_commands(commands);
            app.sync_size();
            app.draw();
        }
        if reload_theme.swap(false, Ordering::SeqCst) {
            if let Some(theme) = load_theme(&theme_path()) {
                app.theme = theme;
            }
            app.draw();
        }

        let touching = app.gesture.active_touches() > 0;
        if let Some(guard) = queue.prepare_read() {
            let ready = poll_fd(guard.connection_fd(), if touching { 16 } else { 100 });
            if ready {
                let _ = guard.read();
            }
        }
        if touching {
            let events = app.gesture.tick(app.now_ms());
            app.handle_gestures(events);
        }
    }
}
