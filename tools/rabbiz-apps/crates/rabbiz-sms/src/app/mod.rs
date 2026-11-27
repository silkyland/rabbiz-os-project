mod input;
mod render;

use std::time::Instant;

use de_design::motion::{bezier_eval, Bezier, BEZIER_ENTER, BEZIER_EXIT};
use rabbiz_app_kit::{AppEvent, AppFrame, AppState, Canvas, Rect, TextInputCtl, Theme};
use wayland_protocols::wp::text_input::zv3::client::zwp_text_input_v3::ContentPurpose;

use crate::backend::Backend;
use crate::contacts::Contacts;
use crate::model::{self, format_phone, thread_key, Message, MsgState};
use crate::store;

// ---- Layout constants (px @ 720x1600; all positions derive from the live
// canvas size, never hardcoded) ----
pub(super) const GUTTER: i32 = 16;
pub(super) const BUBBLE_MAX_PCT: i32 = 72; // max bubble width, % of content width
pub(super) const BUBBLE_PAD_X: i32 = 18;
pub(super) const BUBBLE_PAD_Y: i32 = 10;
pub(super) const STATE_H: i32 = 24; // delivery-state line inside outgoing bubbles
pub(super) const RUN_GAP: i32 = 4; // between bubbles of one run
pub(super) const RUN_SEP: i32 = 14; // between runs
pub(super) const DAY_SEP_H: i32 = 30;
pub(super) const TIME_CAPTION_H: i32 = 24;
pub(super) const ROW_H: i32 = 92;
const ANIM_ENTER_MS: u32 = 360; // list → thread (BEZIER_ENTER)
const ANIM_EXIT_MS: u32 = 160; // thread → list (BEZIER_EXIT)
pub(super) const SEND_W: i32 = 104;
pub(super) const FIELD_LINE_H: i32 = 28; // wrapped body line height inside the field

// ---- View/state types ----

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum View {
    List,
    Thread,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Focus {
    None,
    To,
    Body,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Hit {
    NewMsg,
    Row(usize),
    Back,
    Send,
    ToField,
    BodyField,
}

pub(super) struct Transition {
    start: Instant,
    dur_ms: u32,
    curve: Bezier,
    pub(super) exit: bool,
}

impl Transition {
    pub(super) fn progress(&self) -> f32 {
        let t = self.start.elapsed().as_secs_f32() * 1000.0 / self.dur_ms as f32;
        bezier_eval(self.curve, t.min(1.0))
    }
    pub(super) fn done(&self) -> bool {
        self.start.elapsed().as_secs_f32() * 1000.0 >= self.dur_ms as f32
    }
}

pub struct SmsApp {
    pub(super) msgs: Vec<Message>,
    /// Read-only contacts-store lookup for peer display names.
    pub(super) contacts: Contacts,
    pub(super) view: View,
    pub(super) peer_key: String,
    pub(super) peer_label: String,
    pub(super) is_new: bool,
    pub(super) to_draft: String,
    pub(super) body_draft: String,
    pub(super) focus: Focus,
    pub(super) scroll: i32,
    pub(super) list_scroll: i32,
    pub(super) list_area: Rect,
    pub(super) drag_origin_y: Option<i32>,
    pub(super) drag_last_y: Option<i32>,
    pub(super) pressed: Option<Hit>,
    pub(super) transition: Option<Transition>,
    backend: Backend,
    next_id: u64,
    pub(super) status: String,
    // Hit-test rects captured during the last render.
    pub(super) row_rects: Vec<(usize, Rect)>,
    pub(super) new_msg_rect: Rect,
    pub(super) back_rect: Rect,
    pub(super) send_rect: Rect,
    pub(super) to_rect: Rect,
    pub(super) body_rect: Rect,
    pub(super) msg_area: Rect,
}

impl SmsApp {
    pub fn new(initial_to: Option<String>) -> Self {
        let msgs = store::load(&store::store_path());
        let next_id = msgs.iter().map(|m| m.id).max().unwrap_or(0) + 1;
        let backend = Backend::detect();
        let status = match backend {
            Backend::Mmcli => String::new(),
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
            list_scroll: 0,
            list_area: Rect::new(0, 0, 0, 0),
            drag_origin_y: None,
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
    pub(super) fn display_peer(&self, peer: &str) -> String {
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

    pub(super) fn now_ts() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
    }

    /// `(thread_key, display_peer, last_msg_index)` per conversation,
    /// most-recent-activity first.
    pub(super) fn threads(&self) -> Vec<(String, String, usize)> {
        model::threads(&self.msgs)
    }

    pub(super) fn open_thread(&mut self, key: String, peer: String, is_new: bool) {
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

    pub(super) fn close_thread(&mut self, ti: &TextInputCtl) {
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

    pub(super) fn send(&mut self, ti: &TextInputCtl) {
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

    pub(super) fn append_to_focus(&mut self, s: &str) {
        match self.focus {
            Focus::To => self.to_draft.push_str(s),
            Focus::Body => self.body_draft.push_str(s),
            Focus::None => {}
        }
    }

    pub(super) fn sync_surrounding(&self, ti: &TextInputCtl) {
        match self.focus {
            Focus::To => ti.set_surrounding(&self.to_draft, self.to_draft.len()),
            Focus::Body => ti.set_surrounding(&self.body_draft, self.body_draft.len()),
            Focus::None => {}
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
        self.on_event(event, ti)
    }

    fn render(&mut self, canvas: &mut Canvas, theme: &Theme) {
        self.render_frame(canvas, theme);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rabbiz_app_kit::{Point, TestHarness};
    use std::path::PathBuf;
    use std::time::Duration;

    fn shot_dir() -> PathBuf {
        let d = PathBuf::from("/tmp/rabbiz-shots");
        let _ = std::fs::create_dir_all(&d);
        d
    }

    /// Inject a deterministic conversation set: enough threads that the
    /// list must scroll (>12), plus a peer thread with in/out bubbles,
    /// a Failed outgoing and a Thai message.
    fn seed_msgs(app: &mut SmsApp) {
        let now = SmsApp::now_ts();
        let mut msgs = vec![
            Message::new(
                1,
                "0812345678",
                "Hey, are we still on for tonight?",
                false,
                now - 7_200,
            ),
            Message::new(2, "0812345678", "Yes! See you at 7.", true, now - 7_140),
            Message::new(3, "0812345678", "Don't be late.", false, now - 7_080),
            Message::new(4, "0812345678", "สวัสดีครับ เจอกันครับ", true, now - 3_600),
        ];
        // Long thread list so scrolling is exercised.
        for i in 0..14 {
            let peer = format!("089111{:04}", i);
            msgs.push(Message::new(
                100 + i as u64,
                &peer,
                &format!("Thread message {}", i + 1),
                i % 3 == 0,
                now - 86_400 + i * 600,
            ));
        }
        let mut failed = Message::new(
            200,
            "0865551234",
            "Call me back when you can",
            true,
            now - 900,
        );
        failed.state = MsgState::Failed;
        msgs.push(failed);
        app.msgs = msgs;
    }

    /// Drives the app through list + thread and saves PNGs (the
    /// before/after evidence contract).
    #[test]
    fn renders_main_screens_to_png() {
        let dir = shot_dir();
        let mut h = TestHarness::new(SmsApp::new(None));
        seed_msgs(h.state_mut());

        h.save_png(&dir.join("sms-after-list.png"))
            .expect("save list shot");

        let area = h.state().list_area;
        h.swipe(
            Point::new(150, area.bottom() - 16),
            Point::new(150, area.y + 16),
            8,
            Duration::from_millis(16),
        );
        assert!(
            h.state().view == View::List,
            "a swipe does not open a thread"
        );
        let _ = h.render();
        assert_eq!(
            h.state().row_rects.last().unwrap().0,
            h.state().threads().len() - 1,
        );

        h.state_mut()
            .open_thread("0812345678".into(), "0812345678".into(), false);
        h.state_mut().transition = None;
        h.save_png(&dir.join("sms-after-thread.png"))
            .expect("save thread shot");
    }
}
