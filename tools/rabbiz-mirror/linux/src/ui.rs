use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

use gtk4::cairo;
use gtk4::gdk;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{
    Align, Application, ApplicationWindow, Box as GtkBox, Button, CssProvider, DrawingArea, Entry,
    EventControllerKey, EventControllerMotion, EventControllerScroll, EventControllerScrollFlags,
    GestureClick, Label, Orientation,
};

use crate::device_service;
use crate::keymap;
use crate::rfb::{FrameSnapshot, RfbClient, RfbEvent};

#[derive(Clone, Copy, PartialEq, Eq)]
enum DotColor {
    Gray,
    Yellow,
    Green,
    Red,
}

impl DotColor {
    fn rgb(self) -> (f64, f64, f64) {
        match self {
            DotColor::Gray => (0.55, 0.55, 0.58),
            DotColor::Yellow => (0.95, 0.72, 0.15),
            DotColor::Green => (0.20, 0.78, 0.35),
            DotColor::Red => (0.92, 0.26, 0.24),
        }
    }
}

struct AppState {
    client: Option<RfbClient>,
    events_rx: Option<mpsc::Receiver<RfbEvent>>,
    ssh_rx: Option<mpsc::Receiver<Result<(), String>>>,
    connected: bool,
    user_stopped: bool,
    retry_attempts: u32,
    retry_source: Option<glib::SourceId>,
    fb_size: (usize, usize),
    latest_frame: Option<FrameSnapshot>,
    placeholder: String,
    frames_this_sec: u32,
    buttons_mask: u8,
    last_pointer: (f64, f64),
    dot_color: DotColor,
}

impl AppState {
    fn new() -> Self {
        Self {
            client: None,
            events_rx: None,
            ssh_rx: None,
            connected: false,
            user_stopped: false,
            retry_attempts: 0,
            retry_source: None,
            fb_size: (720, 1600),
            latest_frame: None,
            placeholder: "Not connected\n\nStart the mirror on the device, then Connect.\nUSB defaults to 172.16.42.1.".to_string(),
            frames_this_sec: 0,
            buttons_mask: 0,
            last_pointer: (0.0, 0.0),
            dot_color: DotColor::Gray,
        }
    }
}

#[derive(Clone)]
struct UiWidgets {
    window: ApplicationWindow,
    host_entry: Entry,
    connect_btn: Button,
    start_btn: Button,
    fullscreen_btn: Button,
    mirror_area: DrawingArea,
    status_dot: DrawingArea,
    status_label: Label,
    stats_label: Label,
}

pub fn run(initial_host: String, autoconnect: bool) -> i32 {
    let app = Application::builder()
        .application_id("dev.devded.RabbizMirror")
        .build();

    let host_cell = RefCell::new(initial_host);
    app.connect_activate(move |app| {
        install_css();
        build_window(app, host_cell.borrow().clone(), autoconnect);
    });

    app.run_with_args::<&str>(&[]).into()
}

fn install_css() {
    let provider = CssProvider::new();
    provider.load_from_data(
        "
        .mirror-topbar {
            padding: 8px 10px;
            border-bottom: 1px solid alpha(@borders, 0.6);
        }
        .mirror-bottombar {
            padding: 5px 12px;
            border-top: 1px solid alpha(@borders, 0.6);
        }
        .mirror-host {
            font-family: monospace;
            font-size: 12px;
        }
        .mirror-status {
            font-size: 11px;
            opacity: 0.8;
        }
        .mirror-stats {
            font-family: monospace;
            font-size: 11px;
            opacity: 0.65;
        }
        ",
    );
    if let Some(display) = gdk::Display::default() {
        gtk4::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}

fn build_window(app: &Application, initial_host: String, autoconnect: bool) {
    let window = ApplicationWindow::builder()
        .application(app)
        .title("Rabbiz Mirror")
        .default_width(400)
        .default_height(860)
        .build();
    window.set_size_request(380, 560);

    let root = GtkBox::new(Orientation::Vertical, 0);

    // Top bar
    let topbar = GtkBox::new(Orientation::Horizontal, 6);
    topbar.add_css_class("mirror-topbar");

    let host_entry = Entry::new();
    host_entry.set_text(&initial_host);
    host_entry.set_placeholder_text(Some("172.16.42.1"));
    host_entry.set_tooltip_text(Some("Device address over USB (172.16.42.1) or Wi-Fi"));
    host_entry.set_width_chars(15);
    host_entry.set_hexpand(true);
    host_entry.add_css_class("mirror-host");

    let connect_btn = Button::with_label("Connect");
    connect_btn.add_css_class("suggested-action");
    connect_btn.set_tooltip_text(Some("Connect to the device's wayvnc (Ctrl+R)"));

    let start_btn = Button::from_icon_name("media-playback-start-symbolic");
    start_btn.set_tooltip_text(Some("Start wayvnc on the device over SSH (Ctrl+Shift+R)"));

    let fullscreen_btn = Button::from_icon_name("view-fullscreen-symbolic");
    fullscreen_btn.set_tooltip_text(Some("Fullscreen (Ctrl+F / F11)"));

    topbar.append(&host_entry);
    topbar.append(&connect_btn);
    topbar.append(&start_btn);
    topbar.append(&fullscreen_btn);

    // Center framebuffer view
    let mirror_area = DrawingArea::new();
    mirror_area.set_hexpand(true);
    mirror_area.set_vexpand(true);
    mirror_area.set_focusable(true);
    mirror_area.set_can_focus(true);

    // Bottom status bar
    let bottombar = GtkBox::new(Orientation::Horizontal, 7);
    bottombar.add_css_class("mirror-bottombar");

    let status_dot = DrawingArea::new();
    status_dot.set_content_width(8);
    status_dot.set_content_height(8);
    status_dot.set_valign(Align::Center);

    let status_label = Label::new(Some("Not connected"));
    status_label.set_halign(Align::Start);
    status_label.set_hexpand(true);
    status_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    status_label.add_css_class("mirror-status");

    let stats_label = Label::new(Some("—"));
    stats_label.set_halign(Align::End);
    stats_label.add_css_class("mirror-stats");

    bottombar.append(&status_dot);
    bottombar.append(&status_label);
    bottombar.append(&stats_label);

    root.append(&topbar);
    root.append(&mirror_area);
    root.append(&bottombar);
    window.set_child(Some(&root));

    let state = Rc::new(RefCell::new(AppState::new()));
    let ui = UiWidgets {
        window: window.clone(),
        host_entry: host_entry.clone(),
        connect_btn: connect_btn.clone(),
        start_btn: start_btn.clone(),
        fullscreen_btn: fullscreen_btn.clone(),
        mirror_area: mirror_area.clone(),
        status_dot: status_dot.clone(),
        status_label: status_label.clone(),
        stats_label: stats_label.clone(),
    };

    // Draw status dot
    {
        let state = Rc::clone(&state);
        status_dot.set_draw_func(move |_, cr, w, h| {
            let (r, g, b) = state.borrow().dot_color.rgb();
            let radius = (w.min(h) as f64) / 2.0;
            cr.set_source_rgb(r, g, b);
            cr.arc(w as f64 / 2.0, h as f64 / 2.0, radius, 0.0, std::f64::consts::TAU);
            let _ = cr.fill();
        });
    }

    // Draw framebuffer
    {
        let state = Rc::clone(&state);
        mirror_area.set_draw_func(move |_, cr, w, h| {
            let st = state.borrow();
            cr.set_source_rgb(0.0, 0.0, 0.0);
            let _ = cr.paint();

            if let Some(frame) = &st.latest_frame {
                if let Some((rx, ry, rw, rh)) = image_rect(w as f64, h as f64, frame.width, frame.height) {
                    let bgra = frame.bgra.clone();
                    if let Ok(surface) = cairo::ImageSurface::create_for_data(
                        bgra,
                        cairo::Format::ARgb32,
                        frame.width as i32,
                        frame.height as i32,
                        (frame.width * 4) as i32,
                    ) {
                        cr.save().ok();
                        cr.translate(rx, ry);
                        cr.scale(rw / frame.width as f64, rh / frame.height as f64);
                        let _ = cr.set_source_surface(&surface, 0.0, 0.0);
                        cr.source().set_filter(cairo::Filter::Bilinear);
                        let _ = cr.paint();
                        cr.restore().ok();
                    }
                }
            } else {
                draw_placeholder(cr, w as f64, h as f64, &st.placeholder);
            }
        });
    }

    wire_input(&ui, &state);
    wire_actions(&ui, &state);

    // Poll RFB & SSH channels on GTK main loop (~120 Hz)
    {
        let state = Rc::clone(&state);
        let ui = ui.clone();
        glib::timeout_add_local(Duration::from_millis(8), move || {
            poll_events(&ui, &state);
            glib::ControlFlow::Continue
        });
    }

    // 1 Hz FPS & stats clock
    {
        let state = Rc::clone(&state);
        let ui = ui.clone();
        glib::timeout_add_local(Duration::from_secs(1), move || {
            update_stats(&ui, &state);
            glib::ControlFlow::Continue
        });
    }

    // Clean up on window close
    {
        let state = Rc::clone(&state);
        let ui = ui.clone();
        window.connect_close_request(move |_| {
            stop_client(&ui, &state, false);
            glib::Propagation::Proceed
        });
    }

    window.present();

    if autoconnect {
        connect_to_device(&ui, &state);
    }
}

fn image_rect(view_w: f64, view_h: f64, fb_w: usize, fb_h: usize) -> Option<(f64, f64, f64, f64)> {
    if fb_w == 0 || fb_h == 0 || view_w <= 1.0 || view_h <= 1.0 {
        return None;
    }
    let aspect = fb_w as f64 / fb_h as f64;
    let (rw, rh) = if view_w / view_h > aspect {
        (view_h * aspect, view_h)
    } else {
        (view_w, view_w / aspect)
    };
    let rx = (view_w - rw) / 2.0;
    let ry = (view_h - rh) / 2.0;
    Some((rx, ry, rw, rh))
}

fn draw_placeholder(cr: &cairo::Context, w: f64, h: f64, text: &str) {
    cr.set_source_rgb(0.65, 0.65, 0.68);
    cr.select_font_face("Sans", cairo::FontSlant::Normal, cairo::FontWeight::Normal);
    cr.set_font_size(13.0);

    let lines: Vec<&str> = text.lines().collect();
    let line_height = 20.0;
    let total_h = lines.len() as f64 * line_height;
    let start_y = (h - total_h) / 2.0 + 13.0;

    for (i, line) in lines.iter().enumerate() {
        if line.is_empty() {
            continue;
        }
        if let Ok(ext) = cr.text_extents(line) {
            let x = (w - ext.width()) / 2.0 - ext.x_bearing();
            let y = start_y + i as f64 * line_height;
            cr.move_to(x, y);
            let _ = cr.show_text(line);
        }
    }
}

fn map_pointer(ui: &UiWidgets, state: &AppState, px: f64, py: f64) -> Option<(i32, i32)> {
    let w = ui.mirror_area.width() as f64;
    let h = ui.mirror_area.height() as f64;
    let (fb_w, fb_h) = state.fb_size;
    let (rx, ry, rw, rh) = image_rect(w, h, fb_w, fb_h)?;
    if rw <= 0.0 || rh <= 0.0 {
        return None;
    }
    let nx = ((px - rx) / rw).clamp(0.0, 1.0);
    let ny = ((py - ry) / rh).clamp(0.0, 1.0);
    let x = (nx * fb_w as f64).round() as i32;
    let y = (ny * fb_h as f64).round() as i32;
    Some((x, y))
}

fn wire_input(ui: &UiWidgets, state: &Rc<RefCell<AppState>>) {
    // Mouse motion
    let motion = EventControllerMotion::new();
    {
        let ui = ui.clone();
        let state = Rc::clone(state);
        motion.connect_motion(move |_, x, y| {
            let mut st = state.borrow_mut();
            st.last_pointer = (x, y);
            if let Some(client) = &st.client {
                if let Some((fx, fy)) = map_pointer(&ui, &st, x, y) {
                    client.send_pointer(fx, fy, st.buttons_mask);
                }
            }
        });
    }
    ui.mirror_area.add_controller(motion);

    // Mouse clicks (button 0 = any)
    let click = GestureClick::new();
    click.set_button(0);
    {
        let ui = ui.clone();
        let state = Rc::clone(state);
        click.connect_pressed(move |gesture, _, x, y| {
            ui.mirror_area.grab_focus();
            let bit = match gesture.current_button() {
                1 => 1u8,
                2 => 2u8,
                3 => 4u8,
                _ => 0u8,
            };
            let mut st = state.borrow_mut();
            st.last_pointer = (x, y);
            st.buttons_mask |= bit;
            if let Some(client) = &st.client {
                if let Some((fx, fy)) = map_pointer(&ui, &st, x, y) {
                    client.send_pointer(fx, fy, st.buttons_mask);
                }
            }
        });
    }
    {
        let ui = ui.clone();
        let state = Rc::clone(state);
        click.connect_released(move |gesture, _, x, y| {
            let bit = match gesture.current_button() {
                1 => 1u8,
                2 => 2u8,
                3 => 4u8,
                _ => 0u8,
            };
            let mut st = state.borrow_mut();
            st.last_pointer = (x, y);
            st.buttons_mask &= !bit;
            if let Some(client) = &st.client {
                if let Some((fx, fy)) = map_pointer(&ui, &st, x, y) {
                    client.send_pointer(fx, fy, st.buttons_mask);
                }
            }
        });
    }
    ui.mirror_area.add_controller(click);

    // Scroll wheel
    let scroll = EventControllerScroll::new(EventControllerScrollFlags::BOTH_AXES);
    {
        let ui = ui.clone();
        let state = Rc::clone(state);
        scroll.connect_scroll(move |_, dx, dy| {
            let st = state.borrow();
            let Some(client) = &st.client else {
                return glib::Propagation::Proceed;
            };
            let (lx, ly) = st.last_pointer;
            let Some((fx, fy)) = map_pointer(&ui, &st, lx, ly) else {
                return glib::Propagation::Proceed;
            };
            if dy.abs() >= dx.abs() && dy.abs() > 0.01 {
                let wheel = if dy < 0.0 { 8u8 } else { 16u8 };
                client.send_pointer(fx, fy, st.buttons_mask | wheel);
                client.send_pointer(fx, fy, st.buttons_mask);
            } else if dx.abs() > 0.01 {
                let wheel = if dx < 0.0 { 32u8 } else { 64u8 };
                client.send_pointer(fx, fy, st.buttons_mask | wheel);
                client.send_pointer(fx, fy, st.buttons_mask);
            }
            glib::Propagation::Stop
        });
    }
    ui.mirror_area.add_controller(scroll);

    // Keyboard input on mirror_area
    let key_ctrl = EventControllerKey::new();
    {
        let state = Rc::clone(state);
        key_ctrl.connect_key_pressed(move |_, keyval, _, _| {
            if let Some(keysym) = keymap::keysym_from_gdk(keyval) {
                if let Some(client) = &state.borrow().client {
                    client.send_key(keysym, true);
                }
            }
            glib::Propagation::Stop
        });
    }
    {
        let state = Rc::clone(state);
        key_ctrl.connect_key_released(move |_, keyval, _, _| {
            if let Some(keysym) = keymap::keysym_from_gdk(keyval) {
                if let Some(client) = &state.borrow().client {
                    client.send_key(keysym, false);
                }
            }
        });
    }
    ui.mirror_area.add_controller(key_ctrl);

    // Window-level shortcuts: Ctrl+F / F11 (fullscreen), Ctrl+R (connect/stop), Ctrl+Shift+R (start)
    let win_keys = EventControllerKey::new();
    {
        let ui = ui.clone();
        let state = Rc::clone(state);
        win_keys.connect_key_pressed(move |_, keyval, _, mods| {
            let ctrl = mods.contains(gdk::ModifierType::CONTROL_MASK);
            let shift = mods.contains(gdk::ModifierType::SHIFT_MASK);
            if keyval == gdk::Key::F11 || (ctrl && (keyval == gdk::Key::f || keyval == gdk::Key::F)) {
                if ui.window.is_fullscreen() {
                    ui.window.unfullscreen();
                } else {
                    ui.window.fullscreen();
                }
                return glib::Propagation::Stop;
            }
            if ctrl && shift && (keyval == gdk::Key::r || keyval == gdk::Key::R) {
                start_on_device(&ui, &state);
                return glib::Propagation::Stop;
            }
            if ctrl && !shift && (keyval == gdk::Key::r || keyval == gdk::Key::R) {
                toggle_connect(&ui, &state);
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
    }
    ui.window.add_controller(win_keys);
}

fn wire_actions(ui: &UiWidgets, state: &Rc<RefCell<AppState>>) {
    {
        let btn = ui.connect_btn.clone();
        let ui = ui.clone();
        let state = Rc::clone(state);
        btn.connect_clicked(move |_| {
            toggle_connect(&ui, &state);
        });
    }
    {
        let btn = ui.start_btn.clone();
        let ui = ui.clone();
        let state = Rc::clone(state);
        btn.connect_clicked(move |_| {
            start_on_device(&ui, &state);
        });
    }
    {
        let btn = ui.fullscreen_btn.clone();
        let ui = ui.clone();
        btn.connect_clicked(move |_| {
            if ui.window.is_fullscreen() {
                ui.window.unfullscreen();
            } else {
                ui.window.fullscreen();
            }
        });
    }
    {
        let entry = ui.host_entry.clone();
        let ui = ui.clone();
        let state = Rc::clone(state);
        entry.connect_activate(move |_| {
            if state.borrow().client.is_none() {
                connect_to_device(&ui, &state);
            }
            ui.mirror_area.grab_focus();
        });
    }
}

fn normalized_host(ui: &UiWidgets) -> (String, u16) {
    let raw = ui.host_entry.text().to_string();
    let trimmed = raw.trim();
    let without_scheme = trimmed.strip_prefix("vnc://").unwrap_or(trimmed);
    if let Some((h, p)) = without_scheme.rsplit_once(':') {
        if !h.contains(':') {
            if let Ok(port) = p.parse::<u16>() {
                return (h.to_string(), port);
            }
            return (h.to_string(), 5900);
        }
    }
    (without_scheme.to_string(), 5900)
}

fn set_status(ui: &UiWidgets, state: &Rc<RefCell<AppState>>, text: &str, color: DotColor) {
    ui.status_label.set_text(text);
    state.borrow_mut().dot_color = color;
    ui.status_dot.queue_draw();
}

fn toggle_connect(ui: &UiWidgets, state: &Rc<RefCell<AppState>>) {
    if state.borrow().client.is_some() {
        stop_client(ui, state, true);
    } else {
        connect_to_device(ui, state);
    }
}

fn connect_to_device(ui: &UiWidgets, state: &Rc<RefCell<AppState>>) {
    {
        let mut st = state.borrow_mut();
        st.user_stopped = false;
        st.retry_attempts = 0;
        if let Some(src) = st.retry_source.take() {
            src.remove();
        }
    }
    start_connection(ui, state);
    schedule_retries(ui, state);
}

fn start_connection(ui: &UiWidgets, state: &Rc<RefCell<AppState>>) {
    {
        let st = state.borrow();
        if st.user_stopped || st.client.is_some() {
            return;
        }
    }
    let (host, port) = normalized_host(ui);
    if host.is_empty() {
        if let Some(src) = state.borrow_mut().retry_source.take() {
            src.remove();
        }
        set_status(ui, state, "Enter the device address first.", DotColor::Red);
        return;
    }

    stop_client(ui, state, false);
    set_status(ui, state, &format!("Connecting to {host}…"), DotColor::Yellow);
    ui.connect_btn.set_label("Stop");
    ui.connect_btn.remove_css_class("suggested-action");

    let (tx, rx) = mpsc::channel();
    let client = RfbClient::connect(host, port, tx);
    let mut st = state.borrow_mut();
    st.client = Some(client);
    st.events_rx = Some(rx);
}

fn schedule_retries(ui: &UiWidgets, state: &Rc<RefCell<AppState>>) {
    let ui = ui.clone();
    let state_weak = Rc::downgrade(state);
    let source_id = glib::timeout_add_local(Duration::from_millis(2500), move || {
        let Some(state) = state_weak.upgrade() else {
            return glib::ControlFlow::Break;
        };
        let (should_stop, has_client, attempts) = {
            let mut st = state.borrow_mut();
            if st.user_stopped || st.connected {
                st.retry_source = None;
                return glib::ControlFlow::Break;
            }
            if st.client.is_some() {
                return glib::ControlFlow::Continue;
            }
            st.retry_attempts += 1;
            (false, false, st.retry_attempts)
        };
        let _ = (should_stop, has_client);
        if attempts > 10 {
            state.borrow_mut().retry_source = None;
            set_status(&ui, &state, "Could not reach the device", DotColor::Red);
            return glib::ControlFlow::Break;
        }
        start_connection(&ui, &state);
        glib::ControlFlow::Continue
    });
    state.borrow_mut().retry_source = Some(source_id);
}

fn stop_client(ui: &UiWidgets, state: &Rc<RefCell<AppState>>, announce: bool) {
    let client = {
        let mut st = state.borrow_mut();
        st.events_rx = None;
        st.connected = false;
        st.latest_frame = None;
        st.placeholder = "Not connected\n\nStart the mirror on the device, then Connect.\nUSB defaults to 172.16.42.1.".to_string();
        if announce {
            st.user_stopped = true;
            if let Some(src) = st.retry_source.take() {
                src.remove();
            }
        }
        st.client.take()
    };
    if let Some(c) = client {
        c.disconnect();
    }
    ui.connect_btn.set_label("Connect");
    ui.connect_btn.add_css_class("suggested-action");
    ui.mirror_area.queue_draw();
    if announce {
        set_status(ui, state, "Disconnected", DotColor::Gray);
        ui.stats_label.set_text("—");
    }
}

fn start_on_device(ui: &UiWidgets, state: &Rc<RefCell<AppState>>) {
    let (host, _) = normalized_host(ui);
    if host.is_empty() {
        set_status(ui, state, "Enter the device address first.", DotColor::Red);
        return;
    }
    ui.start_btn.set_sensitive(false);
    set_status(ui, state, "Starting wayvnc on the device…", DotColor::Yellow);
    let rx = device_service::start_remote_wayvnc(host);
    state.borrow_mut().ssh_rx = Some(rx);
}

fn poll_events(ui: &UiWidgets, state: &Rc<RefCell<AppState>>) {
    // Poll RFB events
    let mut rfb_events = Vec::new();
    {
        let st = state.borrow();
        if let Some(rx) = &st.events_rx {
            while let Ok(ev) = rx.try_recv() {
                rfb_events.push(ev);
            }
        }
    }

    for ev in rfb_events {
        match ev {
            RfbEvent::Connected { width, height } => {
                {
                    let mut st = state.borrow_mut();
                    st.connected = true;
                    st.retry_attempts = 0;
                    if let Some(src) = st.retry_source.take() {
                        src.remove();
                    }
                    st.fb_size = (width, height);
                }
                ui.connect_btn.set_label("Stop");
                ui.connect_btn.remove_css_class("suggested-action");
                set_status(ui, state, "Connected", DotColor::Green);
                ui.mirror_area.grab_focus();
                update_stats(ui, state);
            }
            RfbEvent::Frame(snap) => {
                {
                    let mut st = state.borrow_mut();
                    st.frames_this_sec += 1;
                    st.fb_size = (snap.width, snap.height);
                    st.latest_frame = Some(snap);
                }
                ui.mirror_area.queue_draw();
            }
            RfbEvent::Disconnected(reason) => {
                {
                    let mut st = state.borrow_mut();
                    st.client = None;
                    st.events_rx = None;
                    st.connected = false;
                    st.latest_frame = None;
                    st.placeholder = format!(
                        "{reason}\n\nPress Connect to retry, or Start to relaunch wayvnc."
                    );
                }
                ui.connect_btn.set_label("Connect");
                ui.connect_btn.add_css_class("suggested-action");
                set_status(ui, state, &reason, DotColor::Red);
                ui.mirror_area.queue_draw();
                update_stats(ui, state);
            }
        }
    }

    // Poll SSH start result
    let ssh_done = {
        let mut st = state.borrow_mut();
        if let Some(rx) = &st.ssh_rx {
            match rx.try_recv() {
                Ok(res) => {
                    st.ssh_rx = None;
                    Some(res)
                }
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => {
                    st.ssh_rx = None;
                    Some(Err("ssh helper exited".to_string()))
                }
            }
        } else {
            None
        }
    };

    if let Some(res) = ssh_done {
        ui.start_btn.set_sensitive(true);
        match res {
            Ok(()) => {
                if state.borrow().connected {
                    set_status(ui, state, "Connected", DotColor::Green);
                } else {
                    set_status(ui, state, "wayvnc is listening", DotColor::Green);
                    let ui_c = ui.clone();
                    let st_c = Rc::clone(state);
                    glib::timeout_add_local_once(Duration::from_millis(300), move || {
                        connect_to_device(&ui_c, &st_c);
                    });
                }
            }
            Err(err) => {
                set_status(ui, state, &err, DotColor::Red);
            }
        }
    }
}

fn update_stats(ui: &UiWidgets, state: &Rc<RefCell<AppState>>) {
    let mut st = state.borrow_mut();
    let fps = st.frames_this_sec;
    st.frames_this_sec = 0;
    if st.connected && st.fb_size.0 > 0 {
        let rate = if fps == 0 {
            "idle".to_string()
        } else {
            format!("{fps} fps")
        };
        ui.stats_label
            .set_text(&format!("{}×{} · {rate}", st.fb_size.0, st.fb_size.1));
    } else {
        ui.stats_label.set_text("—");
    }
}
