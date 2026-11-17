//! Protocol smoke clients for the M1 compositor acceptance run.
//!
//! Each subcommand is a small wayland-client that connects to the running
//! `rabbiz-de`, performs one protocol interaction and exits 0 on success,
//! non-zero on protocol error or timeout. Run on the device (or arm64
//! container) with `WAYLAND_DISPLAY` pointing at the compositor socket.
//!
//!   de-smoke popup        - xdg_popup configure + map + destroy
//!   de-smoke clipboard    - client A copies, client B pastes (wl_data_device)
//!   de-smoke datacontrol  - wlr data-control watch (`wl-paste --watch` shape)
//!   de-smoke framecb      - wl_surface.frame after an unchanged commit

use std::io::{Read, Write};
use std::os::unix::io::{AsFd, AsRawFd, FromRawFd, OwnedFd};
use std::time::{Duration, Instant};

use wayland_client::protocol::{
    wl_buffer, wl_callback, wl_compositor, wl_data_device, wl_data_device_manager, wl_data_offer,
    wl_data_source, wl_keyboard, wl_output, wl_registry, wl_seat, wl_shm, wl_shm_pool, wl_surface,
    wl_touch,
};
use wayland_client::{Connection, Dispatch, EventQueue, QueueHandle};
use wayland_protocols::xdg::shell::client::{
    xdg_popup, xdg_positioner, xdg_surface, xdg_toplevel, xdg_wm_base,
};
use wayland_protocols_wlr::data_control::v1::client::{
    zwlr_data_control_device_v1, zwlr_data_control_manager_v1, zwlr_data_control_offer_v1,
    zwlr_data_control_source_v1,
};

const TIMEOUT: Duration = Duration::from_secs(5);
const MIME: &str = "text/plain;charset=utf-8";
const PAYLOAD: &[u8] = b"rabbiz-de-m1";

fn fail(msg: &str) -> ! {
    eprintln!("FAIL: {msg}");
    std::process::exit(1)
}

fn connect() -> (Connection, EventQueue<State>) {
    let conn = match Connection::connect_to_env() {
        Ok(c) => c,
        Err(e) => fail(&format!("connect_to_env: {e}")),
    };
    let queue = conn.new_event_queue();
    (conn, queue)
}

fn make_buffer(
    qh: &QueueHandle<State>,
    shm: &wl_shm::WlShm,
    w: i32,
    h: i32,
) -> wl_buffer::WlBuffer {
    let size = (w * h * 4) as usize;
    let mut file = tempfile::tempfile().expect("tempfile");
    file.set_len(size as u64).expect("set_len");
    file.write_all(&vec![0xCCu8; size]).expect("fill");
    let pool = shm.create_pool(file.as_fd(), size as i32, qh, ());
    pool.create_buffer(0, w, h, w * 4, wl_shm::Format::Argb8888, qh, ())
}

#[derive(Default)]
struct State {
    compositor: Option<wl_compositor::WlCompositor>,
    shm: Option<wl_shm::WlShm>,
    seat: Option<wl_seat::WlSeat>,
    xdg_base: Option<xdg_wm_base::XdgWmBase>,
    data_device_manager: Option<wl_data_device_manager::WlDataDeviceManager>,
    data_control_manager: Option<zwlr_data_control_manager_v1::ZwlrDataControlManagerV1>,
    configured: bool,
    popup_configured: bool,
    popup_done: bool,
    got_frame_done: bool,
    got_selection_offer: Option<wl_data_offer::WlDataOffer>,
    got_ctrl_offer: Option<zwlr_data_control_offer_v1::ZwlrDataControlOfferV1>,
    got_source_send: bool,
    got_ctrl_source_send: bool,
    got_selection: bool,
    kb_enter_serial: Option<u32>,
    got_key: bool,
    got_touch_down: bool,
}

impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(
        state: &mut Self,
        proxy: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _data: &(),
        _conn: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        {
            match interface.as_str() {
                "wl_compositor" => {
                    state.compositor = Some(proxy.bind(name, version.min(5), qh, ()))
                }
                "wl_shm" => state.shm = Some(proxy.bind(name, version.min(1), qh, ())),
                "wl_seat" => state.seat = Some(proxy.bind(name, version.min(7), qh, ())),
                "wl_output" => {
                    let _: wl_output::WlOutput = proxy.bind(name, version.min(1), qh, ());
                }
                "xdg_wm_base" => state.xdg_base = Some(proxy.bind(name, version.min(6), qh, ())),
                "wl_data_device_manager" => {
                    state.data_device_manager = Some(proxy.bind(name, version.min(3), qh, ()))
                }
                "zwlr_data_control_manager_v1" => {
                    state.data_control_manager = Some(proxy.bind(name, version.min(2), qh, ()))
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<wl_surface::WlSurface, ()> for State {
    fn event(
        _state: &mut Self,
        _p: &wl_surface::WlSurface,
        _e: wl_surface::Event,
        _d: &(),
        _c: &Connection,
        _q: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<xdg_surface::XdgSurface, ()> for State {
    fn event(
        state: &mut Self,
        proxy: &xdg_surface::XdgSurface,
        event: xdg_surface::Event,
        _d: &(),
        _c: &Connection,
        _q: &QueueHandle<Self>,
    ) {
        if let xdg_surface::Event::Configure { serial } = event {
            proxy.ack_configure(serial);
            state.configured = true;
        }
    }
}

impl Dispatch<xdg_popup::XdgPopup, ()> for State {
    fn event(
        state: &mut Self,
        _p: &xdg_popup::XdgPopup,
        event: xdg_popup::Event,
        _d: &(),
        _c: &Connection,
        _q: &QueueHandle<Self>,
    ) {
        match event {
            xdg_popup::Event::Configure { .. } => state.popup_configured = true,
            xdg_popup::Event::PopupDone => state.popup_done = true,
            _ => {}
        }
    }
}

impl Dispatch<wl_callback::WlCallback, ()> for State {
    fn event(
        state: &mut Self,
        _p: &wl_callback::WlCallback,
        event: wl_callback::Event,
        _d: &(),
        _c: &Connection,
        _q: &QueueHandle<Self>,
    ) {
        if let wl_callback::Event::Done { .. } = event {
            state.got_frame_done = true;
        }
    }
}

impl Dispatch<wl_data_offer::WlDataOffer, ()> for State {
    fn event(
        _state: &mut Self,
        _p: &wl_data_offer::WlDataOffer,
        _e: wl_data_offer::Event,
        _d: &(),
        _c: &Connection,
        _q: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_data_device::WlDataDevice, ()> for State {
    fn event(
        state: &mut Self,
        _p: &wl_data_device::WlDataDevice,
        event: wl_data_device::Event,
        _d: &(),
        _c: &Connection,
        _q: &QueueHandle<Self>,
    ) {
        match event {
            wl_data_device::Event::DataOffer { id } => state.got_selection_offer = Some(id),
            wl_data_device::Event::Selection { id } => {
                if id.is_some() {
                    state.got_selection = true;
                }
            }
            _ => {}
        }
    }

    wayland_client::event_created_child!(State, wl_data_device::WlDataDevice, [
        wl_data_device::EVT_DATA_OFFER_OPCODE => (wl_data_offer::WlDataOffer, ()),
    ]);
}

impl Dispatch<wl_data_source::WlDataSource, ()> for State {
    fn event(
        state: &mut Self,
        _p: &wl_data_source::WlDataSource,
        event: wl_data_source::Event,
        _d: &(),
        _c: &Connection,
        _q: &QueueHandle<Self>,
    ) {
        if let wl_data_source::Event::Send { mime_type, fd } = event {
            eprintln!("clipboard: source send mime={mime_type}");
            let mut file = std::fs::File::from(fd);
            let _ = file.write_all(PAYLOAD);
            state.got_source_send = true;
        }
    }
}

impl Dispatch<wl_keyboard::WlKeyboard, ()> for State {
    fn event(
        state: &mut Self,
        _p: &wl_keyboard::WlKeyboard,
        event: wl_keyboard::Event,
        _d: &(),
        _c: &Connection,
        _q: &QueueHandle<Self>,
    ) {
        match event {
            wl_keyboard::Event::Enter { serial, .. } => {
                state.kb_enter_serial = Some(serial);
            }
            wl_keyboard::Event::Key {
                state: key_state, ..
            } => {
                if key_state == wayland_client::WEnum::Value(wl_keyboard::KeyState::Pressed) {
                    state.got_key = true;
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_touch::WlTouch, ()> for State {
    fn event(
        state: &mut Self,
        _p: &wl_touch::WlTouch,
        event: wl_touch::Event,
        _d: &(),
        _c: &Connection,
        _q: &QueueHandle<Self>,
    ) {
        if let wl_touch::Event::Down { .. } = event {
            state.got_touch_down = true;
        }
    }
}

impl Dispatch<zwlr_data_control_offer_v1::ZwlrDataControlOfferV1, ()> for State {
    fn event(
        _s: &mut Self,
        _p: &zwlr_data_control_offer_v1::ZwlrDataControlOfferV1,
        _e: zwlr_data_control_offer_v1::Event,
        _d: &(),
        _c: &Connection,
        _q: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<zwlr_data_control_device_v1::ZwlrDataControlDeviceV1, ()> for State {
    fn event(
        state: &mut Self,
        _p: &zwlr_data_control_device_v1::ZwlrDataControlDeviceV1,
        event: zwlr_data_control_device_v1::Event,
        _d: &(),
        _c: &Connection,
        _q: &QueueHandle<Self>,
    ) {
        match event {
            zwlr_data_control_device_v1::Event::DataOffer { id } => state.got_ctrl_offer = Some(id),
            zwlr_data_control_device_v1::Event::Selection { id } => {
                if id.is_some() {
                    state.got_selection = true;
                }
            }
            _ => {}
        }
    }

    wayland_client::event_created_child!(State, zwlr_data_control_device_v1::ZwlrDataControlDeviceV1, [
        zwlr_data_control_device_v1::EVT_DATA_OFFER_OPCODE => (zwlr_data_control_offer_v1::ZwlrDataControlOfferV1, ()),
    ]);
}

impl Dispatch<zwlr_data_control_source_v1::ZwlrDataControlSourceV1, ()> for State {
    fn event(
        state: &mut Self,
        _p: &zwlr_data_control_source_v1::ZwlrDataControlSourceV1,
        event: zwlr_data_control_source_v1::Event,
        _d: &(),
        _c: &Connection,
        _q: &QueueHandle<Self>,
    ) {
        if let zwlr_data_control_source_v1::Event::Send { mime_type, fd } = event {
            eprintln!("datacontrol: source send mime={mime_type}");
            let mut file = std::fs::File::from(fd);
            let _ = file.write_all(PAYLOAD);
            state.got_ctrl_source_send = true;
        }
    }
}

macro_rules! ignored {
    ($($t:ty),* $(,)?) => {$(
        impl Dispatch<$t, ()> for State {
            fn event(
                _state: &mut Self,
                _proxy: &$t,
                _event: <$t as wayland_client::Proxy>::Event,
                _data: &(),
                _conn: &Connection,
                _qh: &QueueHandle<Self>,
            ) {
            }
        }
    )*};
}

ignored!(
    wl_shm::WlShm,
    wl_shm_pool::WlShmPool,
    wl_buffer::WlBuffer,
    wl_output::WlOutput,
    wl_seat::WlSeat,
    wl_compositor::WlCompositor,
    xdg_wm_base::XdgWmBase,
    xdg_toplevel::XdgToplevel,
    xdg_positioner::XdgPositioner,
    wl_data_device_manager::WlDataDeviceManager,
    zwlr_data_control_manager_v1::ZwlrDataControlManagerV1,
);

fn dispatch_until<F: Fn(&State) -> bool>(
    conn: &Connection,
    queue: &mut EventQueue<State>,
    state: &mut State,
    deadline: Instant,
    done: F,
) -> bool {
    while !done(state) && Instant::now() < deadline {
        if let Err(e) = conn.flush() {
            eprintln!("flush: {e}");
            return done(state);
        }
        if queue.dispatch_pending(state).is_err() {
            return done(state);
        }
        if done(state) {
            break;
        }
        let Some(guard) = queue.prepare_read() else {
            continue;
        };
        let remaining = deadline.saturating_duration_since(Instant::now());
        let ms = remaining.as_millis().min(250) as libc::c_int;
        let mut pfd = libc::pollfd {
            fd: conn.backend().poll_fd().as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        let n = unsafe { libc::poll(&mut pfd, 1, ms) };
        if n > 0 && pfd.revents & libc::POLLIN != 0 {
            if guard.read().is_err() {
                return done(state);
            }
            let _ = queue.dispatch_pending(state);
        }
    }
    done(state)
}

fn pipe() -> (OwnedFd, OwnedFd) {
    let mut fds = [0i32; 2];
    unsafe {
        if libc::pipe(fds.as_mut_ptr()) != 0 {
            fail("pipe() failed");
        }
    }
    unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) }
}

struct Toplevel {
    surface: wl_surface::WlSurface,
    xdg_surface: xdg_surface::XdgSurface,
    #[allow(dead_code)]
    toplevel: xdg_toplevel::XdgToplevel,
}

fn map_toplevel(
    conn: &Connection,
    queue: &mut EventQueue<State>,
    state: &mut State,
    title: &str,
) -> Toplevel {
    let qh = queue.handle();
    let compositor = state.compositor.clone().expect("wl_compositor");
    let shm = state.shm.clone().expect("wl_shm");
    let xdg_base = state.xdg_base.clone().expect("xdg_wm_base");

    let surface = compositor.create_surface(&qh, ());
    let xsurface = xdg_base.get_xdg_surface(&surface, &qh, ());
    let toplevel = xsurface.get_toplevel(&qh, ());
    toplevel.set_title(title.to_string());
    surface.commit();
    let deadline = Instant::now() + TIMEOUT;
    if !dispatch_until(conn, queue, state, deadline, |s| s.configured) {
        fail("toplevel configure not received");
    }
    let buf = make_buffer(&qh, &shm, 64, 64);
    surface.attach(Some(&buf), 0, 0);
    surface.commit();
    conn.flush().ok();
    Toplevel {
        surface,
        xdg_surface: xsurface,
        toplevel,
    }
}

fn test_popup() {
    let (conn, mut queue) = connect();
    let qh = queue.handle();
    let mut state = State::default();
    let registry = conn.display().get_registry(&qh, ());
    let deadline = Instant::now() + TIMEOUT;
    if !dispatch_until(&conn, &mut queue, &mut state, deadline, |s| {
        s.xdg_base.is_some() && s.shm.is_some() && s.compositor.is_some()
    }) {
        fail("popup: required globals missing");
    }

    let parent = map_toplevel(&conn, &mut queue, &mut state, "smoke-popup-parent");

    let compositor = state.compositor.clone().unwrap();
    let xdg_base = state.xdg_base.clone().unwrap();
    let shm = state.shm.clone().unwrap();

    let pos = xdg_base.create_positioner(&qh, ());
    pos.set_size(100, 50);
    pos.set_anchor_rect(0, 0, 10, 10);
    pos.set_anchor(xdg_positioner::Anchor::TopLeft);
    pos.set_gravity(xdg_positioner::Gravity::BottomRight);

    let popup_surface = compositor.create_surface(&qh, ());
    let popup_xsurface = xdg_base.get_xdg_surface(&popup_surface, &qh, ());
    let _popup = popup_xsurface.get_popup(Some(&parent.xdg_surface), &pos, &qh, ());
    popup_surface.commit();

    let deadline = Instant::now() + TIMEOUT;
    if !dispatch_until(&conn, &mut queue, &mut state, deadline, |s| {
        s.popup_configured
    }) {
        fail("popup: xdg_popup.configure never received (compositor never configured the popup)");
    }

    let pbuf = make_buffer(&qh, &shm, 100, 50);
    popup_surface.attach(Some(&pbuf), 0, 0);
    popup_surface.commit();

    // Give the compositor a moment to render the popup, then verify the
    // parent-topelevel teardown path dismisses it: destroy the parent
    // toplevel, expect xdg_popup.popup_done on the popup.
    let deadline = Instant::now() + Duration::from_secs(2);
    let _ = dispatch_until(&conn, &mut queue, &mut state, deadline, |_s| false);

    parent.toplevel.destroy();
    parent.xdg_surface.destroy();
    parent.surface.destroy();
    conn.flush().ok();

    let deadline = Instant::now() + TIMEOUT;
    if !dispatch_until(&conn, &mut queue, &mut state, deadline, |s| s.popup_done) {
        fail("popup: popup_done never received after parent toplevel destroy");
    }
    let _ = registry;
    println!("PASS popup");
}

fn test_clipboard() {
    let qh_a;
    let (conn_a, mut queue_a) = connect();
    qh_a = queue_a.handle();
    let mut state_a = State::default();
    let _reg_a = conn_a.display().get_registry(&qh_a, ());
    let deadline = Instant::now() + TIMEOUT;
    if !dispatch_until(&conn_a, &mut queue_a, &mut state_a, deadline, |s| {
        s.xdg_base.is_some()
            && s.shm.is_some()
            && s.seat.is_some()
            && s.data_device_manager.is_some()
    }) {
        fail("clipboard: client A missing globals (seat/data_device_manager?)");
    }

    // A maps a toplevel -> becomes the focused client (keyboard.enter).
    let seat_a = state_a.seat.clone().unwrap();
    let _kb = seat_a.get_keyboard(&qh_a, ());
    let _top_a = map_toplevel(&conn_a, &mut queue_a, &mut state_a, "smoke-clip-A");
    let deadline = Instant::now() + TIMEOUT;
    if !dispatch_until(&conn_a, &mut queue_a, &mut state_a, deadline, |s| {
        s.kb_enter_serial.is_some()
    }) {
        fail("clipboard: client A never received keyboard focus");
    }

    // A creates a data source and sets the clipboard selection while focused.
    let mgr_a = state_a.data_device_manager.clone().unwrap();
    let device_a = mgr_a.get_data_device(&seat_a, &qh_a, ());
    let source_a = mgr_a.create_data_source(&qh_a, ());
    source_a.offer(MIME.to_string());
    device_a.set_selection(Some(&source_a), state_a.kb_enter_serial.unwrap_or(0));
    conn_a.flush().ok();

    // B connects, maps a toplevel (becomes focused) and should then see the
    // selection via wl_data_device.selection (set_data_device_focus path).
    let (conn_b, mut queue_b) = connect();
    let qh_b = queue_b.handle();
    let mut state_b = State::default();
    let _reg_b = conn_b.display().get_registry(&qh_b, ());
    let deadline = Instant::now() + TIMEOUT;
    if !dispatch_until(&conn_b, &mut queue_b, &mut state_b, deadline, |s| {
        s.xdg_base.is_some()
            && s.shm.is_some()
            && s.seat.is_some()
            && s.data_device_manager.is_some()
    }) {
        fail("clipboard: client B missing globals");
    }
    let seat_b = state_b.seat.clone().unwrap();
    let mgr_b = state_b.data_device_manager.clone().unwrap();
    let _device_b = mgr_b.get_data_device(&seat_b, &qh_b, ());
    let _top_b = map_toplevel(&conn_b, &mut queue_b, &mut state_b, "smoke-clip-B");

    // Interleave both connections while waiting for B's selection event and
    // A's source.send.
    let deadline = Instant::now() + TIMEOUT;
    loop {
        if state_b.got_selection && state_a.got_source_send {
            break;
        }
        if Instant::now() >= deadline {
            fail("clipboard: B got no selection event or A got no send request");
        }
        conn_a.flush().ok();
        conn_b.flush().ok();
        let _ = queue_b.roundtrip(&mut state_b);
        let _ = queue_a.roundtrip(&mut state_a);
        // Ask B to receive once the offer exists.
        if state_b.got_selection && !state_a.got_source_send {
            if let Some(offer) = &state_b.got_selection_offer {
                let (_r, w) = pipe();
                offer.receive(MIME.to_string(), w.as_fd());
                drop(w);
                conn_b.flush().ok();
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    println!("PASS clipboard");
}

fn test_datacontrol() {
    let (conn, mut queue) = connect();
    let qh = queue.handle();
    let mut state = State::default();
    let _reg = conn.display().get_registry(&qh, ());
    let deadline = Instant::now() + TIMEOUT;
    if !dispatch_until(&conn, &mut queue, &mut state, deadline, |s| {
        s.seat.is_some() && s.data_control_manager.is_some()
    }) {
        fail("datacontrol: zwlr_data_control_manager_v1 not advertised");
    }

    let mgr = state.data_control_manager.clone().unwrap();
    let seat = state.seat.clone().unwrap();

    // Watcher device (the `wl-paste --watch` role): receives offers.
    let _watcher = mgr.get_data_device(&seat, &qh, ());
    // Setter device: installs a selection without needing focus.
    let setter = mgr.get_data_device(&seat, &qh, ());
    let src = mgr.create_data_source(&qh, ());
    src.offer(MIME.to_string());
    setter.set_selection(Some(&src));
    conn.flush().ok();

    let deadline = Instant::now() + TIMEOUT;
    if !dispatch_until(&conn, &mut queue, &mut state, deadline, |s| s.got_selection) {
        fail("datacontrol: watcher device received no selection event");
    }

    // Read the offer back (paste side of the watch).
    if let Some(offer) = &state.got_ctrl_offer {
        let (r, w) = pipe();
        offer.receive(MIME.to_string(), w.as_fd());
        drop(w);
        conn.flush().ok();
        let deadline = Instant::now() + TIMEOUT;
        if !dispatch_until(&conn, &mut queue, &mut state, deadline, |s| {
            s.got_ctrl_source_send
        }) {
            fail("datacontrol: source.send never fired for receive request");
        }
        let mut file = std::fs::File::from(r);
        let mut buf = Vec::new();
        let _ = file.read_to_end(&mut buf);
        if buf != PAYLOAD {
            fail("datacontrol: pasted payload mismatch");
        }
    } else {
        fail("datacontrol: no data offer object received");
    }
    println!("PASS datacontrol");
}

fn test_framecb() {
    let (conn, mut queue) = connect();
    let qh = queue.handle();
    let mut state = State::default();
    let _reg = conn.display().get_registry(&qh, ());
    let deadline = Instant::now() + TIMEOUT;
    if !dispatch_until(&conn, &mut queue, &mut state, deadline, |s| {
        s.xdg_base.is_some() && s.shm.is_some() && s.compositor.is_some()
    }) {
        fail("framecb: required globals missing");
    }

    let top = map_toplevel(&conn, &mut queue, &mut state, "smoke-framecb");

    // First callback: an explicit damage region forces a real frame, so the
    // callback is delivered through the normal (non-empty) path.
    let _cb1 = top.surface.frame(&qh, ());
    top.surface.damage_buffer(0, 0, 64, 64);
    top.surface.commit();
    let deadline = Instant::now() + TIMEOUT;
    if !dispatch_until(&conn, &mut queue, &mut state, deadline, |s| {
        s.got_frame_done
    }) {
        fail("framecb: first frame callback not delivered");
    }

    // The regression case: an unchanged commit + frame request. With the
    // is_empty early-return bug the callback is never sent.
    state.got_frame_done = false;
    let _cb2 = top.surface.frame(&qh, ());
    top.surface.commit();
    let deadline = Instant::now() + Duration::from_secs(3);
    if !dispatch_until(&conn, &mut queue, &mut state, deadline, |s| {
        s.got_frame_done
    }) {
        fail("framecb: frame callback after unchanged commit never arrived");
    }
    println!("PASS framecb");
}

/// End-to-end input path (M1-T1/T8): maps a toplevel (takes keyboard + touch
/// focus), then waits for a real key press and a touch-down forwarded by the
/// compositor. The events are injected from the device side via uinput while
/// this client waits - a pass proves libinput -> seat -> client works.
fn test_input() {
    let (conn, mut queue) = connect();
    let qh = queue.handle();
    let mut state = State::default();
    let _reg = conn.display().get_registry(&qh, ());
    let deadline = Instant::now() + TIMEOUT;
    if !dispatch_until(&conn, &mut queue, &mut state, deadline, |s| {
        s.xdg_base.is_some() && s.shm.is_some() && s.compositor.is_some() && s.seat.is_some()
    }) {
        fail("input: required globals missing");
    }

    let seat = state.seat.clone().unwrap();
    let _kb = seat.get_keyboard(&qh, ());
    let _touch = seat.get_touch(&qh, ());

    let _top = map_toplevel(&conn, &mut queue, &mut state, "smoke-input");
    let deadline = Instant::now() + TIMEOUT;
    if !dispatch_until(&conn, &mut queue, &mut state, deadline, |s| {
        s.kb_enter_serial.is_some()
    }) {
        fail("input: mapped toplevel never received keyboard focus");
    }

    println!("input: focused; waiting for injected key + touch (15s)");
    let deadline = Instant::now() + Duration::from_secs(15);
    if !dispatch_until(&conn, &mut queue, &mut state, deadline, |s| {
        s.got_key && s.got_touch_down
    }) {
        fail(&format!(
            "input: got_key={} got_touch_down={} before timeout",
            state.got_key, state.got_touch_down
        ));
    }
    println!("PASS input");
}

fn main() {
    let arg = std::env::args()
        .nth(1)
        .unwrap_or_else(|| fail("usage: de-smoke <popup|clipboard|datacontrol|framecb|input>"));
    match arg.as_str() {
        "popup" => test_popup(),
        "clipboard" => test_clipboard(),
        "datacontrol" => test_datacontrol(),
        "framecb" => test_framecb(),
        "input" => test_input(),
        other => fail(&format!("unknown smoke test: {other}")),
    }
}
