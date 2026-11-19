//! rabbiz-sms — Messages app for RabbizOS.
//!
//! Conversation list → thread view with M3-style bubbles (incoming left on
//! `surface1`, outgoing right on `accent`), run grouping with a single
//! timestamp per run, day separators, and honest delivery states
//! (Queued/Sent/Failed). Messages persist to
//! `~/.local/share/rabbiz-sms/messages.json` (atomic tmp+rename).
//!
//! Telephony reality (device.toml `modem = "planned"`, mm-backend.sh is an
//! unwired reference): sends go through `backend::Backend`, which probes for
//! `mmcli` once. No modem → messages stay `Queued` and are still saved —
//! nothing is faked as delivered.

mod backend;
mod contacts;
mod headless;
mod model;
mod store;

use std::time::Instant;

use de_api::Pixels;
use de_design::motion::{bezier_eval, Bezier, BEZIER_ENTER, BEZIER_EXIT};
use rabbiz_app_kit::text::{self, LabelStyle};
use rabbiz_app_kit::ui;
use rabbiz_app_kit::{
    AppEvent, AppFrame, AppState, Canvas, Point, Rect, Size, TextInputCtl, Theme,
};
use wayland_protocols::wp::text_input::zv3::client::zwp_text_input_v3::ContentPurpose;

use backend::Backend;
use contacts::Contacts;
use model::{
    day_label, delete_before, format_phone, format_time, local_time, same_run, state_label,
    thread_key, Message, MsgState,
};

// ---- Layout constants (px @ 720x1600; all positions derive from the live
// canvas size, never hardcoded) ----
const GUTTER: i32 = 16;
const BUBBLE_MAX_PCT: i32 = 72; // max bubble width, % of content width
const BUBBLE_PAD_X: i32 = 18;
const BUBBLE_PAD_Y: i32 = 10;
const STATE_H: i32 = 24; // delivery-state line inside outgoing bubbles
const RUN_GAP: i32 = 4; // between bubbles of one run
const RUN_SEP: i32 = 14; // between runs
const DAY_SEP_H: i32 = 30;
const TIME_CAPTION_H: i32 = 24;
const ROW_H: i32 = 92;
const ROW_GAP: i32 = 12;
const ANIM_ENTER_MS: u32 = 360; // list → thread (BEZIER_ENTER)
const ANIM_EXIT_MS: u32 = 160; // thread → list (BEZIER_EXIT)
const SEND_W: i32 = 104;
const FIELD_LINE_H: i32 = 28; // wrapped body line height inside the field

// ---- Cached text rasterization ----

/// Rasterize `s` through the shared LRU label cache. `max_lines ==
/// usize::MAX` means "wrap, don't clamp".
fn label_px(s: &str, st: LabelStyle, max_w: Option<i32>, max_lines: usize) -> Option<Pixels> {
    use text::cache::{to_pixels, with_cache, LabelKey};
    let key = LabelKey {
        text: s.to_string(),
        family: st.family,
        size_px: st.size as u32,
        line_height_px: st.line_height as u32,
        weight: st.weight,
        color: st.color,
        max_width_px: max_w.map(|w| w.max(1) as u32),
        max_lines,
    };
    with_cache(|c| {
        c.get_or_insert_with(key, || match (max_w, max_lines) {
            (Some(w), n) if n != usize::MAX => text::label_clamped(s, st, w, n),
            (Some(w), _) => text::label(s, st, Some(w)),
            (None, _) => text::label(s, st, None),
        })
    })
    .map(|px| to_pixels(&px))
}

fn blit_label(
    c: &mut Canvas,
    s: &str,
    st: LabelStyle,
    x: i32,
    y: i32,
    max_w: Option<i32>,
    max_lines: usize,
) {
    if let Some(px) = label_px(s, st, max_w, max_lines) {
        c.blit(&px, Point::new(x, y));
    }
}

// ---- Pure layout math ----

fn clamp_scroll(scroll: i32, max_scroll: i32) -> i32 {
    scroll.clamp(0, max_scroll.max(0))
}

fn bubble_width(text_w: i32, max_bubble_w: i32) -> i32 {
    (text_w + BUBBLE_PAD_X * 2).min(max_bubble_w).max(48)
}

/// Compose field height for `lines` of wrapped text (1..=3).
fn field_height(lines: i32) -> i32 {
    (lines * (FIELD_LINE_H + 6) + 16).clamp(56, 140)
}

// ---- View/state types ----

#[derive(Clone, Copy, PartialEq, Eq)]
enum View {
    List,
    Thread,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Focus {
    None,
    To,
    Body,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Hit {
    NewMsg,
    Row(usize),
    Back,
    Send,
    ToField,
    BodyField,
}

/// A laid-out element in thread content coordinates (y measured from the top
/// of the scrollable content; the viewport transform happens at draw time).
enum Elem {
    Day {
        y: i32,
        label: String,
    },
    Time {
        y: i32,
        right: bool,
        label: String,
    },
    Bubble {
        y: i32,
        msg: usize,
        right: bool,
        w: i32,
        h: i32,
    },
}

struct ThreadLayout {
    elems: Vec<Elem>,
    content_h: i32,
}

struct Transition {
    start: Instant,
    dur_ms: u32,
    curve: Bezier,
    exit: bool,
}

impl Transition {
    fn progress(&self) -> f32 {
        let t = self.start.elapsed().as_secs_f32() * 1000.0 / self.dur_ms as f32;
        bezier_eval(self.curve, t.min(1.0))
    }
    fn done(&self) -> bool {
        self.start.elapsed().as_secs_f32() * 1000.0 >= self.dur_ms as f32
    }
}

struct SmsApp {
    msgs: Vec<Message>,
    /// Read-only contacts-store lookup for peer display names.
    contacts: Contacts,
    view: View,
    peer_key: String,
    peer_label: String,
    is_new: bool,
    to_draft: String,
    body_draft: String,
    focus: Focus,
    scroll: i32,
    drag_last_y: Option<i32>,
    pressed: Option<Hit>,
    transition: Option<Transition>,
    backend: Backend,
    next_id: u64,
    status: String,
    // Hit-test rects captured during the last render.
    row_rects: Vec<Rect>,
    new_msg_rect: Rect,
    back_rect: Rect,
    send_rect: Rect,
    to_rect: Rect,
    body_rect: Rect,
    msg_area: Rect,
}

impl SmsApp {
    fn new(initial_to: Option<String>) -> Self {
        let msgs = store::load(&store::store_path());
        let next_id = msgs.iter().map(|m| m.id).max().unwrap_or(0) + 1;
        let backend = Backend::detect();
        let status = match backend {
            Backend::Mmcli => "Modem ready".to_string(),
            Backend::Unavailable => "No modem yet — messages are saved on this device".to_string(),
        };
        let mut app = Self {
            msgs,
            contacts: Contacts::load(),
            view: View::List,
            peer_key: String::new(),
            peer_label: String::new(),
            is_new: false,
            to_draft: String::new(),
            body_draft: String::new(),
            focus: Focus::None,
            scroll: 0,
            drag_last_y: None,
            pressed: None,
            transition: None,
            backend,
            next_id,
            status,
            row_rects: Vec::new(),
            new_msg_rect: Rect::new(0, 0, 0, 0),
            back_rect: Rect::new(0, 0, 0, 0),
            send_rect: Rect::new(0, 0, 0, 0),
            to_rect: Rect::new(0, 0, 0, 0),
            body_rect: Rect::new(0, 0, 0, 0),
            msg_area: Rect::new(0, 0, 0, 0),
        };
        // `--to <number>` hand-off from rabbiz-contacts: jump straight into
        // the matching thread, or into a compose view when none exists.
        if let Some(num) = initial_to {
            app.open_to(num);
        }
        app
    }

    /// Open the thread for `num` (thread_key-normalized) or start a
    /// prefilled compose when no conversation exists yet.
    fn open_to(&mut self, num: String) {
        let key = thread_key(num.trim());
        if let Some((k, peer, _)) = self.threads().into_iter().find(|(k, _, _)| *k == key) {
            self.open_thread(k, peer, false);
            return;
        }
        // Compose: prefill To with the normalized number and focus the
        // message body — the user still has to type and press Send.
        let draft = if key.chars().any(|c| c.is_ascii_digit()) {
            key
        } else {
            num.trim().to_string()
        };
        let draft_key = thread_key(&draft);
        self.open_thread(draft_key, String::new(), true);
        self.to_draft = draft;
        self.focus = Focus::Body;
    }

    /// Display name for a peer: contacts-store name when known, else the
    /// formatted number. Read-only — never writes contacts.
    fn display_peer(&self, peer: &str) -> String {
        if peer.trim().is_empty() {
            return "Unknown".to_string();
        }
        self.contacts
            .name_for(peer)
            .map_or_else(|| format_phone(peer), str::to_string)
    }

    fn save(&self) {
        let _ = store::save(&store::store_path(), &self.msgs);
    }

    fn now_ts() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
    }

    /// `(thread_key, display_peer, last_msg_index)` per conversation,
    /// most-recent-activity first.
    fn threads(&self) -> Vec<(String, String, usize)> {
        model::threads(&self.msgs)
    }

    fn open_thread(&mut self, key: String, peer: String, is_new: bool) {
        self.peer_key = key;
        self.peer_label = peer;
        self.is_new = is_new;
        self.body_draft.clear();
        if is_new {
            self.to_draft.clear();
        }
        self.scroll = 0;
        self.focus = if is_new { Focus::To } else { Focus::None };
        self.view = View::Thread;
        self.transition = Some(Transition {
            start: Instant::now(),
            dur_ms: ANIM_ENTER_MS,
            curve: BEZIER_ENTER,
            exit: false,
        });
    }

    fn close_thread(&mut self, ti: &TextInputCtl) {
        ti.deactivate();
        self.focus = Focus::None;
        self.transition = Some(Transition {
            start: Instant::now(),
            dur_ms: ANIM_EXIT_MS,
            curve: BEZIER_EXIT,
            exit: true,
        });
        // `view` flips to List when the exit transition completes (Tick).
    }

    fn send(&mut self, ti: &TextInputCtl) {
        let peer = if self.is_new {
            self.to_draft.trim().to_string()
        } else {
            self.peer_label.clone()
        };
        let body = self.body_draft.trim().to_string();
        if peer.is_empty() {
            self.status = "Enter a phone number first".to_string();
            self.focus = Focus::To;
            ti.activate(ContentPurpose::Phone);
            return;
        }
        if body.is_empty() {
            self.status = "Type a message first".to_string();
            self.focus = Focus::Body;
            ti.activate(ContentPurpose::Normal);
            return;
        }
        let mut m = Message::new(self.next_id, &peer, &body, true, Self::now_ts());
        self.next_id += 1;
        match self.backend.send(&peer, &body) {
            Ok(()) => {
                m.state = MsgState::Sent;
                self.status = "Sent".to_string();
            }
            Err(_) => {
                // Unavailable = never attempted → stays Queued. Mmcli
                // present but rejected = attempted and failed → Failed.
                m.state = if self.backend == Backend::Unavailable {
                    MsgState::Queued
                } else {
                    MsgState::Failed
                };
                self.status = if self.backend == Backend::Unavailable {
                    "No modem — message saved on this device".to_string()
                } else {
                    "Send failed — saved on this device".to_string()
                };
            }
        }
        self.msgs.push(m);
        self.save();
        self.body_draft.clear();
        if self.is_new {
            self.is_new = false;
            self.peer_key = thread_key(&peer);
            self.peer_label = peer;
            self.to_draft.clear();
        }
        self.scroll = 0;
        self.focus = Focus::Body;
        ti.set_surrounding("", 0);
    }

    fn append_to_focus(&mut self, s: &str) {
        match self.focus {
            Focus::To => self.to_draft.push_str(s),
            Focus::Body => self.body_draft.push_str(s),
            Focus::None => {}
        }
    }

    fn sync_surrounding(&self, ti: &TextInputCtl) {
        match self.focus {
            Focus::To => ti.set_surrounding(&self.to_draft, self.to_draft.len()),
            Focus::Body => ti.set_surrounding(&self.body_draft, self.body_draft.len()),
            Focus::None => {}
        }
    }

    // ================= thread layout =================

    /// Lay out the open thread's messages into content coordinates.
    /// `view_w`/`view_h` describe the message viewport in px.
    fn layout_thread(&self, theme: &Theme, view_w: i32, view_h: i32) -> ThreadLayout {
        let content_w = view_w - GUTTER * 2;
        let max_bubble_w = content_w * BUBBLE_MAX_PCT / 100;
        let max_text_w = max_bubble_w - BUBBLE_PAD_X * 2;
        // Measure with the incoming-bubble color; metrics are color-independent.
        let body_style = ui::body(theme.text);
        let now = Self::now_ts();

        let mut elems: Vec<Elem> = Vec::new();
        let mut y = 8;
        let mut prev: Option<&Message> = None;
        // Index of the next message in this thread (for run-gap decisions).
        let mut iter = self
            .msgs
            .iter()
            .enumerate()
            .filter(|(_, m)| thread_key(&m.peer) == self.peer_key)
            .peekable();

        while let Some((mi, m)) = iter.next() {
            let new_day = prev.is_none_or(|p| {
                let (a, b) = (local_time(p.ts), local_time(m.ts));
                a.year != b.year || a.yday != b.yday
            });
            if new_day {
                elems.push(Elem::Day {
                    y,
                    label: day_label(m.ts, now),
                });
                y += DAY_SEP_H;
            }
            let new_run = prev.is_none_or(|p| !same_run(p, m));
            if new_run {
                elems.push(Elem::Time {
                    y,
                    right: m.outgoing,
                    label: format_time(m.ts),
                });
                y += TIME_CAPTION_H;
            }
            let (tw, th) = label_px(&m.body, body_style, Some(max_text_w), usize::MAX)
                .map(|px| (px.size.w, px.size.h))
                .unwrap_or((max_text_w / 2, body_style.line_height as i32));
            let bw = bubble_width(tw, max_bubble_w);
            let extra = if m.outgoing { STATE_H } else { 0 };
            let bh = th + BUBBLE_PAD_Y * 2 + extra;
            elems.push(Elem::Bubble {
                y,
                msg: mi,
                right: m.outgoing,
                w: bw,
                h: bh,
            });
            y += bh;
            let next_same_run = iter.peek().is_some_and(|(_, n)| same_run(m, n));
            y += if next_same_run { RUN_GAP } else { RUN_SEP };
            prev = Some(m);
        }
        let _ = view_h;

        ThreadLayout {
            elems,
            content_h: y,
        }
    }
}

impl AppState for SmsApp {
    fn app_id(&self) -> &'static str {
        "rabbiz-sms"
    }
    fn title(&self) -> &'static str {
        "Messages"
    }

    fn handle_event(&mut self, event: AppEvent, ti: &TextInputCtl) -> AppFrame {
        let mut frame = AppFrame::default();

        if let AppEvent::Tick(_) = event {
            if let Some(t) = &self.transition {
                if t.done() {
                    if t.exit {
                        self.view = View::List;
                        self.peer_key.clear();
                        self.peer_label.clear();
                        self.is_new = false;
                        self.body_draft.clear();
                        self.to_draft.clear();
                    }
                    self.transition = None;
                }
            }
            frame.dirty = true;
            frame.animating = self.transition.is_some();
            return frame;
        }

        let animating = self.transition.is_some();

        match event {
            AppEvent::TouchDown { pos, .. } => {
                frame.dirty = true;
                if animating {
                    return frame;
                }
                self.pressed = None;
                self.drag_last_y = None;
                match self.view {
                    View::List => {
                        if self.new_msg_rect.contains(pos) {
                            self.pressed = Some(Hit::NewMsg);
                        } else {
                            for (i, r) in self.row_rects.iter().enumerate() {
                                if r.contains(pos) {
                                    self.pressed = Some(Hit::Row(i));
                                    break;
                                }
                            }
                        }
                        if self.pressed.is_none() {
                            ti.deactivate();
                            self.focus = Focus::None;
                        }
                    }
                    View::Thread => {
                        if self.back_rect.contains(pos) {
                            self.pressed = Some(Hit::Back);
                        } else if self.send_rect.contains(pos) {
                            self.pressed = Some(Hit::Send);
                        } else if self.is_new && self.to_rect.contains(pos) {
                            self.pressed = Some(Hit::ToField);
                            self.focus = Focus::To;
                            ti.activate(ContentPurpose::Phone);
                            ti.set_surrounding(&self.to_draft, self.to_draft.len());
                        } else if self.body_rect.contains(pos) {
                            self.pressed = Some(Hit::BodyField);
                            self.focus = Focus::Body;
                            ti.activate(ContentPurpose::Normal);
                            ti.set_surrounding(&self.body_draft, self.body_draft.len());
                        } else if self.msg_area.contains(pos) {
                            // Message area: begin scroll drag, drop keyboard.
                            self.drag_last_y = Some(pos.y);
                            ti.deactivate();
                            self.focus = Focus::None;
                        } else {
                            ti.deactivate();
                            self.focus = Focus::None;
                        }
                    }
                }
            }
            AppEvent::TouchMove { pos, .. } => {
                if self.drag_last_y.is_some() {
                    frame.dirty = true;
                    let last = self.drag_last_y.replace(pos.y).unwrap_or(pos.y);
                    // Finger moving up (pos.y < last) reveals older messages.
                    self.scroll += last - pos.y;
                }
            }
            AppEvent::TouchUp { .. } => {
                frame.dirty = true;
                match self.pressed.take() {
                    Some(Hit::NewMsg) => {
                        self.open_thread(String::new(), String::new(), true);
                        ti.activate(ContentPurpose::Phone);
                        ti.set_surrounding("", 0);
                    }
                    Some(Hit::Row(i)) => {
                        let threads = self.threads();
                        if let Some((key, peer, _)) = threads.get(i) {
                            let (k, p) = (key.clone(), peer.clone());
                            self.open_thread(k, p, false);
                        }
                    }
                    Some(Hit::Back) => self.close_thread(ti),
                    Some(Hit::Send) => self.send(ti),
                    _ => {}
                }
                self.drag_last_y = None;
            }
            AppEvent::TextCommit(s) => {
                self.append_to_focus(&s);
                self.sync_surrounding(ti);
                frame.dirty = true;
            }
            AppEvent::TextBackspace(bytes) => {
                match self.focus {
                    Focus::To => delete_before(&mut self.to_draft, bytes),
                    Focus::Body => delete_before(&mut self.body_draft, bytes),
                    Focus::None => {}
                }
                self.sync_surrounding(ti);
                frame.dirty = true;
            }
            AppEvent::Key { keysym, utf8 } => {
                frame.dirty = true;
                match (keysym, utf8) {
                    (0xff0d, _) => {
                        if self.view == View::Thread && !self.body_draft.trim().is_empty() {
                            self.send(ti);
                        }
                    }
                    (0xff08, _) => {
                        match self.focus {
                            Focus::To => {
                                self.to_draft.pop();
                            }
                            Focus::Body => {
                                self.body_draft.pop();
                            }
                            Focus::None => {}
                        }
                        self.sync_surrounding(ti);
                    }
                    (_, Some(s)) => {
                        self.append_to_focus(&s);
                        self.sync_surrounding(ti);
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        frame.animating = self.transition.is_some();
        frame
    }

    fn render(&mut self, canvas: &mut Canvas, theme: &Theme) {
        let size = canvas.size();
        let w = size.w;
        let h = size.h;

        // The list is always the backdrop: the thread slides over it during
        // transitions (and is the only view when settled on the list).
        self.render_list(canvas, theme, w, h);
        if self.view != View::Thread {
            return;
        }
        match self.transition.as_ref().map(|t| (t.progress(), t.exit)) {
            None => self.render_thread(canvas, theme, w, h),
            Some((p, exit)) => {
                // Slide distance in px; 0 means "settled, draw directly".
                let off = if exit {
                    (p * w as f32) as i32
                } else {
                    ((1.0 - p) * w as f32) as i32
                };
                if off <= 0 {
                    self.render_thread(canvas, theme, w, h);
                } else {
                    // Composite the thread on an offscreen canvas, then blit
                    // it shifted right — the list shows through on the left.
                    let mut tc = Canvas::filled(Size::new(w, h), theme.bg);
                    self.render_thread(&mut tc, theme, w, h);
                    canvas.blit(&tc.to_pixels(), Point::new(off, 0));
                }
            }
        }
    }
}

impl SmsApp {
    // ================= list view =================

    fn render_list(&mut self, canvas: &mut Canvas, theme: &Theme, w: i32, h: i32) {
        let header_bottom = ui::draw_header(canvas, theme, "Messages", &self.status);

        // "+ New message" pill under the header, right-aligned.
        let new_r = Rect::new(w - ui::MARGIN - 190, header_bottom + 8, 190, 52);
        self.new_msg_rect = new_r;
        ui::draw_pill_button(
            canvas,
            theme,
            new_r,
            "New message",
            true,
            self.pressed == Some(Hit::NewMsg),
        );

        let threads = self.threads();
        self.row_rects.clear();
        let top = header_bottom + 76;
        let avail = h - ui::BOTTOM_SAFE - top;
        let visible = (avail / (ROW_H + ROW_GAP)).max(0) as usize;

        if threads.is_empty() {
            let msg = "No conversations yet.\nTap “New message” to write the first SMS.";
            let st = ui::body(theme.text3);
            if let Some(px) = label_px(msg, st, Some(w - ui::MARGIN * 2), usize::MAX) {
                canvas.blit(&px, Point::new(ui::MARGIN, top + 40));
            }
            return;
        }

        let name_style = ui::styled(24.0, 600, theme.text);
        let preview_style = ui::caption(theme.text2);
        let time_style = ui::caption(theme.text3);
        let now = Self::now_ts();
        let ln = local_time(now);

        for (i, (_, peer, last_idx)) in threads.iter().enumerate().take(visible) {
            let m = &self.msgs[*last_idx];
            let r = Rect::new(
                ui::MARGIN,
                top + i as i32 * (ROW_H + ROW_GAP),
                w - ui::MARGIN * 2,
                ROW_H,
            );
            self.row_rects.push(r);
            ui::draw_card(canvas, theme, r, self.pressed == Some(Hit::Row(i)));

            let name = self.display_peer(peer);
            blit_label(
                canvas,
                &name,
                name_style,
                r.x + 20,
                r.y + 14,
                Some(r.w - 180),
                1,
            );
            blit_label(
                canvas,
                &m.body,
                preview_style,
                r.x + 20,
                r.y + 48,
                Some(r.w - 190),
                1,
            );

            let lt = local_time(m.ts);
            let when = if lt.year == ln.year && lt.yday == ln.yday {
                format_time(m.ts)
            } else {
                day_label(m.ts, now)
            };
            if let Some(px) = label_px(&when, time_style, None, 1) {
                canvas.blit(&px, Point::new(r.x + r.w - 20 - px.size.w, r.y + 16));
            }
        }
    }

    // ================= thread view =================

    /// Compose-bar rects (absolute coords). Returns
    /// `(to_rect, body_rect, send_rect, msg_area)`.
    fn compose_layout(&self, theme: &Theme, w: i32, h: i32) -> (Rect, Rect, Rect, Rect) {
        let field_w = w - GUTTER * 2 - SEND_W - 12;
        let body_style = ui::body(theme.text);
        let lines = if self.body_draft.trim().is_empty() {
            1
        } else {
            label_px(&self.body_draft, body_style, Some(field_w - 40), 3)
                .map(|px| ((px.size.h + FIELD_LINE_H - 1) / FIELD_LINE_H).clamp(1, 3))
                .unwrap_or(1)
        };
        let field_h = field_height(lines);

        let bottom = h - ui::BOTTOM_SAFE - 12; // bottom edge of the compose block
        let send_r = Rect::new(w - GUTTER - SEND_W, bottom - field_h, SEND_W, field_h);
        let body_r = Rect::new(GUTTER, bottom - field_h, field_w, field_h);
        let mut top = body_r.y - 10;
        let to_r = if self.is_new {
            let r = Rect::new(GUTTER, top - 52, w - GUTTER * 2, 52);
            top = r.y - 10;
            r
        } else {
            Rect::new(0, 0, 0, 0)
        };
        let msg_top = ui::TOP_SAFE + 78;
        let msg_area = Rect::new(0, msg_top, w, (top - 8 - msg_top).max(0));
        (to_r, body_r, send_r, msg_area)
    }

    fn render_thread(&mut self, canvas: &mut Canvas, theme: &Theme, w: i32, h: i32) {
        // Header: back pill + peer name.
        let back_r = Rect::new(GUTTER, ui::TOP_SAFE + 12, 116, 50);
        self.back_rect = back_r;
        ui::draw_pill_button(
            canvas,
            theme,
            back_r,
            "‹ Back",
            false,
            self.pressed == Some(Hit::Back),
        );

        let title = if self.is_new {
            // Compose for a known contact shows their name; otherwise the
            // generic "New message" caption.
            self.contacts
                .name_for(&self.to_draft)
                .map_or_else(|| "New message".to_string(), str::to_string)
        } else {
            self.display_peer(&self.peer_label)
        };
        let title_style = ui::styled(28.0, 600, theme.text);
        let tx = back_r.x + back_r.w + 16;
        let max_tw = (w - ui::MARGIN - tx).max(40);
        blit_label(
            canvas,
            &title,
            title_style,
            tx,
            ui::TOP_SAFE + 22,
            Some(max_tw),
            1,
        );

        let (to_r, body_r, send_r, msg_area) = self.compose_layout(theme, w, h);
        self.to_rect = to_r;
        self.body_rect = body_r;
        self.send_rect = send_r;
        self.msg_area = msg_area;

        self.draw_messages(canvas, theme, w, msg_area);
        self.draw_compose(canvas, theme, to_r, body_r, send_r);
    }

    fn draw_messages(&mut self, canvas: &mut Canvas, theme: &Theme, w: i32, area: Rect) {
        let layout = self.layout_thread(theme, w, area.h);
        if layout.elems.is_empty() || area.h <= 0 {
            return;
        }
        let view_h = area.h;
        let max_scroll = (layout.content_h - view_h).max(0);
        self.scroll = clamp_scroll(self.scroll, max_scroll);
        // Content bottom-anchored when scroll == 0: an element at content
        // y = content_h lands on the viewport bottom.
        let top_idx = layout.content_h - view_h - self.scroll;

        let cap_style = ui::caption(theme.text3);
        let state_style = ui::styled(16.0, 400, theme.text_on_accent);

        for elem in &layout.elems {
            let ey = area.y + elem_y(elem) - top_idx;
            if ey + elem_h(elem) < area.y || ey > area.y + area.h {
                continue; // culled
            }
            match elem {
                Elem::Day { label, .. } => {
                    if let Some(px) = label_px(label, cap_style, None, 1) {
                        canvas.blit(&px, Point::new((w - px.size.w) / 2, ey + 4));
                    }
                }
                Elem::Time { right, label, .. } => {
                    if let Some(px) = label_px(label, cap_style, None, 1) {
                        let ex = if *right {
                            w - GUTTER - px.size.w
                        } else {
                            GUTTER
                        };
                        canvas.blit(&px, Point::new(ex, ey));
                    }
                }
                Elem::Bubble {
                    msg,
                    right,
                    w: bw,
                    h: bh,
                    ..
                } => {
                    let m = &self.msgs[*msg];
                    let bx = if *right { w - GUTTER - bw } else { GUTTER };
                    let br = Rect::new(bx, ey, *bw, *bh);
                    let bg = if *right { theme.accent } else { theme.surface1 };
                    canvas.fill_rounded_rect(br, 20.0, bg);
                    let fg = if *right {
                        theme.text_on_accent
                    } else {
                        theme.text
                    };
                    let max_tw = bw - BUBBLE_PAD_X * 2;
                    let style = ui::styled(23.0, 400, fg);
                    if let Some(px) = label_px(&m.body, style, Some(max_tw), usize::MAX) {
                        canvas.blit(&px, Point::new(bx + BUBBLE_PAD_X, ey + BUBBLE_PAD_Y));
                    }
                    if m.outgoing {
                        let sl = state_label(m.state);
                        if let Some(px) = label_px(sl, state_style, None, 1) {
                            canvas.blit(
                                &px,
                                Point::new(
                                    bx + bw - BUBBLE_PAD_X - px.size.w,
                                    ey + bh - BUBBLE_PAD_Y - 18,
                                ),
                            );
                        }
                    }
                }
            }
        }
    }

    fn draw_compose(
        &mut self,
        canvas: &mut Canvas,
        theme: &Theme,
        to_r: Rect,
        body_r: Rect,
        send_r: Rect,
    ) {
        if self.is_new && !to_r.is_empty() {
            let focused = self.focus == Focus::To;
            ui::draw_card(canvas, theme, to_r, focused);
            if focused {
                canvas.fill_rounded_rect(to_r, 20.0, theme.surface3);
            }
            let (shown, col) = if self.to_draft.is_empty() {
                ("Phone number", theme.text3)
            } else {
                (self.to_draft.as_str(), theme.text)
            };
            blit_label(
                canvas,
                shown,
                ui::styled(22.0, 400, col),
                to_r.x + 18,
                to_r.y + 14,
                Some(to_r.w - 36),
                1,
            );
        }

        let focused = self.focus == Focus::Body;
        ui::draw_card(canvas, theme, body_r, focused);
        if focused {
            canvas.fill_rounded_rect(body_r, 20.0, theme.surface3);
        }
        let max_tw = body_r.w - 40;
        if self.body_draft.trim().is_empty() {
            blit_label(
                canvas,
                "Message",
                ui::styled(22.0, 400, theme.text3),
                body_r.x + 18,
                body_r.y + (body_r.h - 28) / 2,
                Some(max_tw),
                1,
            );
        } else {
            blit_label(
                canvas,
                &self.body_draft,
                ui::styled(22.0, 400, theme.text),
                body_r.x + 18,
                body_r.y + 12,
                Some(max_tw),
                3,
            );
        }

        let can_send = !self.body_draft.trim().is_empty()
            && (!self.is_new || !self.to_draft.trim().is_empty());
        ui::draw_pill_button(
            canvas,
            theme,
            send_r,
            "Send",
            can_send,
            self.pressed == Some(Hit::Send),
        );
    }
}

fn elem_y(e: &Elem) -> i32 {
    match e {
        Elem::Day { y, .. } | Elem::Time { y, .. } | Elem::Bubble { y, .. } => *y,
    }
}

fn elem_h(e: &Elem) -> i32 {
    match e {
        Elem::Day { .. } => DAY_SEP_H,
        Elem::Time { .. } => TIME_CAPTION_H,
        Elem::Bubble { h, .. } => *h,
    }
}

/// Value of a GUI hand-off flag: `--flag <value>` or `--flag=<value>`.
fn gui_arg(args: &[String], flag: &str) -> Option<String> {
    if let Some(i) = args.iter().position(|a| a == flag) {
        return args.get(i + 1).cloned();
    }
    let prefix = format!("{flag}=");
    args.iter()
        .find_map(|a| a.strip_prefix(&prefix).map(str::to_string))
        .filter(|v| !v.is_empty())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--mcp") {
        headless::mcp_serve();
    }
    if let Some(pos) = args.iter().position(|a| a == "--cli") {
        std::process::exit(headless::run_cli(&args[pos + 1..]));
    }
    // GUI hand-off: `--to <number>` opens the thread or a prefilled
    // compose (from rabbiz-contacts). Never auto-sends.
    rabbiz_app_kit::run_app(SmsApp::new(gui_arg(&args, "--to")));
}

// ---- pure-logic tests ----
#[cfg(test)]
mod layout_tests {
    use super::*;

    #[test]
    fn clamp_scroll_bounds() {
        assert_eq!(clamp_scroll(-5, 100), 0);
        assert_eq!(clamp_scroll(50, 100), 50);
        assert_eq!(clamp_scroll(500, 100), 100);
        assert_eq!(clamp_scroll(10, 0), 0);
    }

    #[test]
    fn bubble_width_caps_at_max() {
        let max = 400;
        assert_eq!(bubble_width(1000, max), max);
        assert_eq!(bubble_width(50, max), 50 + BUBBLE_PAD_X * 2);
        assert!(bubble_width(10, max) >= 48);
    }

    #[test]
    fn field_height_grows_and_caps() {
        assert_eq!(field_height(1), 56);
        assert!(field_height(2) > field_height(1));
        assert_eq!(field_height(9), 140);
    }

    #[test]
    fn gui_arg_parses_space_and_equals_forms() {
        let strs = |xs: &[&str]| xs.iter().map(|s| (*s).to_string()).collect::<Vec<_>>();
        assert_eq!(
            gui_arg(&strs(&["--to", "0812345678"]), "--to"),
            Some("0812345678".to_string())
        );
        assert_eq!(
            gui_arg(&strs(&["--to=+66 81 234 5678"]), "--to"),
            Some("+66 81 234 5678".to_string())
        );
        assert_eq!(gui_arg(&strs(&["--to"]), "--to"), None);
        assert_eq!(gui_arg(&strs(&["--to="]), "--to"), None);
        assert_eq!(gui_arg(&strs(&[]), "--to"), None);
        assert_eq!(gui_arg(&strs(&["--dial", "081"]), "--to"), None);
    }
}
