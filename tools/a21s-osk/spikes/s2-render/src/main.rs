//! K0 spike S2/S3/S4: layer-shell surface + tiny-skia + cosmic-text on the A21s.
//! `--png out.png`  render once and exit (no Wayland) -> inspect Thai/emoji shaping.
//! default          show a keyboard-like surface for 25 s, log touch->commit latency + RSS.
use cosmic_text::{Attrs, Buffer, Color, Family, FontSystem, Metrics, Shaping, SwashCache};
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_layer, delegate_output, delegate_registry, delegate_seat,
    delegate_shm, delegate_touch,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        touch::TouchHandler, Capability, SeatHandler, SeatState,
    },
    shell::{
        wlr_layer::{
            Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
            LayerSurfaceConfigure,
        },
        WaylandSurface,
    },
    shm::{slot::SlotPool, Shm, ShmHandler},
};
use std::{collections::HashMap, time::Instant};
use tiny_skia::{FillRule, Paint, PathBuilder, Pixmap, Transform};
use smithay_client_toolkit::reexports::client::{
    globals::registry_queue_init,
    protocol::{wl_output, wl_seat, wl_shm, wl_surface, wl_touch},
    Connection, QueueHandle,
};

const H: u32 = 640;
const ROWS: [&str; 6] = [
    "1 2 3 4 5 6 7 8 9 0",
    "q w e r t y u i o p",
    "ฟ ห ก ด เ ้ ่ า ส ว",
    "ก็ ที่ น้ำ ผู้ ให้ ซึ่ง",
    "😀 🎉 🇹🇭 👍🏽 ❤️ 🧑‍🚀",
    "a s d f g h j k l",
];

fn rss_kb() -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| s.lines().find(|l| l.starts_with("VmRSS")).map(|l| l.to_string()))
        .and_then(|l| l.split_whitespace().nth(1).and_then(|v| v.parse().ok()))
        .unwrap_or(0)
}

struct Renderer {
    fs: FontSystem,
    sc: SwashCache,
}

impl Renderer {
    fn new() -> Self {
        Self { fs: FontSystem::new(), sc: SwashCache::new() }
    }

    fn text(&mut self, pm: &mut Pixmap, s: &str, cx: f32, cy: f32, size: f32) {
        let mut b = Buffer::new(&mut self.fs, Metrics::new(size, size * 1.3));
        b.set_size(&mut self.fs, Some(400.0), Some(size * 2.0));
        b.set_text(&mut self.fs, s, Attrs::new().family(Family::SansSerif), Shaping::Advanced);
        b.shape_until_scroll(&mut self.fs, false);
        let w: f32 = b.layout_runs().map(|r| r.line_w).fold(0.0, f32::max);
        let (ox, oy) = ((cx - w / 2.0) as i32, (cy - size * 0.65) as i32);
        let (pw, ph) = (pm.width() as i32, pm.height() as i32);
        let data = pm.data_mut();
        b.draw(&mut self.fs, &mut self.sc, Color::rgb(0xee, 0xee, 0xee), |x, y, _w, _h, c| {
            let (px, py) = (x + ox, y + oy);
            if px < 0 || py < 0 || px >= pw || py >= ph { return; }
            let i = ((py * pw + px) * 4) as usize;
            let a = c.a() as u32;
            let inv = 255 - a;
            // premultiplied RGBA source-over
            data[i] = ((c.r() as u32 * a + data[i] as u32 * inv) / 255) as u8;
            data[i + 1] = ((c.g() as u32 * a + data[i + 1] as u32 * inv) / 255) as u8;
            data[i + 2] = ((c.b() as u32 * a + data[i + 2] as u32 * inv) / 255) as u8;
            data[i + 3] = (a + data[i + 3] as u32 * inv / 255) as u8;
        });
    }

    fn render(&mut self, w: u32, h: u32, pressed: &[(f32, f32)]) -> Pixmap {
        let mut pm = Pixmap::new(w, h).unwrap();
        pm.fill(tiny_skia::Color::from_rgba8(0x20, 0x21, 0x24, 0xff));
        let rh = h as f32 / ROWS.len() as f32;
        for (r, row) in ROWS.iter().enumerate() {
            let items: Vec<&str> = row.split(' ').collect();
            let kw = w as f32 / items.len().max(10) as f32;
            let x0 = (w as f32 - kw * items.len() as f32) / 2.0;
            for (i, it) in items.iter().enumerate() {
                let (x, y) = (x0 + i as f32 * kw + 3.0, r as f32 * rh + 4.0);
                let (kwid, khei) = (kw - 6.0, rh - 8.0);
                let hit = pressed.iter().any(|(px, py)| *px >= x && *px < x + kwid && *py >= y && *py < y + khei);
                let mut p = Paint::default();
                p.set_color_rgba8(if hit { 0x5a } else { 0x3c }, if hit { 0x7a } else { 0x40 }, if hit { 0xd0 } else { 0x43 }, 0xff);
                let mut pb = PathBuilder::new();
                pb.push_rect(tiny_skia::Rect::from_xywh(x, y, kwid, khei).unwrap());
                pm.fill_path(&pb.finish().unwrap(), &p, FillRule::Winding, Transform::identity(), None);
                self.text(&mut pm, it, x + kwid / 2.0, y + khei / 2.0, 34.0);
            }
        }
        pm
    }
}

struct App {
    reg: RegistryState,
    seats: SeatState,
    out: OutputState,
    shm: Shm,
    pool: SlotPool,
    layer: LayerSurface,
    r: Renderer,
    w: u32,
    h: u32,
    configured: bool,
    touches: HashMap<i32, (f32, f32)>,
    t0: Instant,
    exit: bool,
    pending: Option<Instant>,
    lat: Vec<u128>,
}

impl App {
    fn draw(&mut self, qh: &QueueHandle<Self>) {
        if !self.configured { return; }
        let pressed: Vec<(f32, f32)> = self.touches.values().cloned().collect();
        let pm = self.r.render(self.w, self.h, &pressed);
        let stride = self.w as i32 * 4;
        let (buf, canvas) = self
            .pool
            .create_buffer(self.w as i32, self.h as i32, stride, wl_shm::Format::Argb8888)
            .unwrap();
        for (d, s) in canvas.chunks_exact_mut(4).zip(pm.data().chunks_exact(4)) {
            d.copy_from_slice(&[s[2], s[1], s[0], s[3]]);
        }
        let surf = self.layer.wl_surface();
        surf.damage_buffer(0, 0, self.w as i32, self.h as i32);
        surf.frame(qh, surf.clone());
        buf.attach_to(surf).unwrap();
        self.layer.commit();
        if let Some(t) = self.pending.take() {
            self.lat.push(t.elapsed().as_micros());
        }
    }
}

impl CompositorHandler for App {
    fn scale_factor_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: i32) {}
    fn transform_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: wl_output::Transform) {}
    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: u32) {}
    fn surface_enter(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}
    fn surface_leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}
}
impl OutputHandler for App {
    fn output_state(&mut self) -> &mut OutputState { &mut self.out }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}
impl LayerShellHandler for App {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface) { self.exit = true; }
    fn configure(&mut self, _: &Connection, qh: &QueueHandle<Self>, _: &LayerSurface, c: LayerSurfaceConfigure, _: u32) {
        if c.new_size.0 > 0 { self.w = c.new_size.0; }
        if c.new_size.1 > 0 { self.h = c.new_size.1; }
        self.configured = true;
        self.pool.resize((self.w * self.h * 4 * 2) as usize).ok();
        eprintln!("configured {}x{} rss={}kB", self.w, self.h, rss_kb());
        self.draw(qh);
        eprintln!("first draw {}ms rss={}kB", self.t0.elapsed().as_millis(), rss_kb());
    }
}
impl SeatHandler for App {
    fn seat_state(&mut self) -> &mut SeatState { &mut self.seats }
    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
    fn new_capability(&mut self, _: &Connection, qh: &QueueHandle<Self>, seat: wl_seat::WlSeat, cap: Capability) {
        if cap == Capability::Touch { self.seats.get_touch(qh, &seat).ok(); eprintln!("touch capability bound"); }
    }
    fn remove_capability(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat, _: Capability) {}
    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}
impl TouchHandler for App {
    fn down(&mut self, _: &Connection, qh: &QueueHandle<Self>, _: &wl_touch::WlTouch, _: u32, time: u32, _: wl_surface::WlSurface, id: i32, pos: (f64, f64)) {
        self.pending = Some(Instant::now());
        self.touches.insert(id, (pos.0 as f32, pos.1 as f32));
        eprintln!("down id={id} t={time} active={}", self.touches.len());
        self.draw(qh);
    }
    fn up(&mut self, _: &Connection, qh: &QueueHandle<Self>, _: &wl_touch::WlTouch, _: u32, _: u32, id: i32) {
        self.touches.remove(&id);
        self.draw(qh);
    }
    fn motion(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_touch::WlTouch, _: u32, id: i32, pos: (f64, f64)) {
        self.touches.insert(id, (pos.0 as f32, pos.1 as f32));
    }
    fn shape(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_touch::WlTouch, _: i32, _: f64, _: f64) {}
    fn orientation(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_touch::WlTouch, _: i32, _: f64) {}
    fn cancel(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_touch::WlTouch) { self.touches.clear(); }
}
impl ShmHandler for App { fn shm_state(&mut self) -> &mut Shm { &mut self.shm } }
impl ProvidesRegistryState for App {
    fn registry(&mut self) -> &mut RegistryState { &mut self.reg }
    registry_handlers![OutputState, SeatState];
}
delegate_compositor!(App);
delegate_output!(App);
delegate_shm!(App);
delegate_seat!(App);
delegate_touch!(App);
delegate_layer!(App);
delegate_registry!(App);

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--png") {
        let t = Instant::now();
        let mut r = Renderer::new();
        eprintln!("font system {}ms rss={}kB", t.elapsed().as_millis(), rss_kb());
        let t = Instant::now();
        let pm = r.render(720, H, &[]);
        eprintln!("render {}ms rss={}kB", t.elapsed().as_millis(), rss_kb());
        pm.save_png(args.get(2).map(String::as_str).unwrap_or("osk-spike.png")).unwrap();
        return;
    }
    let t0 = Instant::now();
    let conn = Connection::connect_to_env().expect("wayland");
    let (globals, mut queue) = registry_queue_init(&conn).unwrap();
    let qh = queue.handle();
    let compositor = CompositorState::bind(&globals, &qh).unwrap();
    let layer_shell = LayerShell::bind(&globals, &qh).expect("layer shell");
    let shm = Shm::bind(&globals, &qh).unwrap();
    let surface = compositor.create_surface(&qh);
    let layer = layer_shell.create_layer_surface(&qh, surface, Layer::Top, Some("osk-spike"), None);
    layer.set_anchor(Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
    layer.set_size(0, H);
    layer.set_keyboard_interactivity(KeyboardInteractivity::None);
    layer.commit();
    let pool = SlotPool::new(720 * H as usize * 4 * 2, &shm).unwrap();
    let mut app = App {
        reg: RegistryState::new(&globals), seats: SeatState::new(&globals, &qh), out: OutputState::new(&globals, &qh),
        shm, pool, layer, r: Renderer::new(), w: 720, h: H, configured: false, touches: HashMap::new(),
        t0, exit: false, pending: None, lat: vec![],
    };
    eprintln!("setup {}ms rss={}kB", t0.elapsed().as_millis(), rss_kb());
    while !app.exit && t0.elapsed().as_secs() < 25 {
        queue.flush().ok();
        if let Some(g) = queue.prepare_read() { drop(g); }
        queue.dispatch_pending(&mut app).unwrap();
        queue.blocking_dispatch(&mut app).ok();
        if t0.elapsed().as_secs() >= 25 { break; }
    }
    let mut l = app.lat.clone();
    l.sort();
    if !l.is_empty() {
        eprintln!("touch->commit(us) n={} p50={} p99={} max={}", l.len(), l[l.len() / 2], l[(l.len() * 99 / 100).min(l.len() - 1)], l[l.len() - 1]);
    }
    eprintln!("exit rss={}kB", rss_kb());
}
