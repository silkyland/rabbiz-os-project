use std::process::Command;

use rabbiz_app_kit::{
    ui, AppEvent, AppFrame, AppState, Canvas, Rect, TextInputCtl, Theme,
};
use wayland_protocols::wp::text_input::zv3::client::zwp_text_input_v3::ContentPurpose;

struct SmsApp {
    recipient: String,
    body: String,
    focus_body: bool,
    status: String,
    messages: Vec<String>,
    pressed: Option<usize>,
}

impl SmsApp {
    fn new() -> Self {
        let mut app = Self {
            recipient: String::new(),
            body: String::new(),
            focus_body: false,
            status: "Ready".into(),
            messages: Vec::new(),
            pressed: None,
        };
        app.refresh();
        app
    }

    fn refresh(&mut self) {
        if let Ok(out) = Command::new("mmcli").args(["-m", "0", "--messaging-list-sms"]).output() {
            let txt = String::from_utf8_lossy(&out.stdout);
            self.messages = txt.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect();
        }
    }

    fn send(&mut self) {
        if self.recipient.trim().is_empty() || self.body.trim().is_empty() {
            self.status = "Enter recipient and message".into();
            return;
        }
        let arg = format!("text='{}',number='{}'", self.body, self.recipient);
        match Command::new("mmcli").args(["-m", "0", "--messaging-create-sms", &arg]).output() {
            Ok(out) if out.status.success() => {
                self.status = format!("Created SMS to {}", self.recipient);
                self.body.clear();
                self.refresh();
            }
            Ok(out) => self.status = String::from_utf8_lossy(&out.stderr).trim().to_string(),
            Err(e) => self.status = format!("Modem unavailable: {e}"),
        }
    }
}

impl AppState for SmsApp {
    fn app_id(&self) -> &'static str { "rabbiz-sms" }
    fn title(&self) -> &'static str { "Messages" }

    fn handle_event(&mut self, event: AppEvent, ti: &TextInputCtl) -> AppFrame {
        match event {
            AppEvent::TouchDown { pos, .. } => {
                let to_r = Rect::new(ui::MARGIN, ui::TOP_SAFE + 96, 720 - ui::MARGIN * 2, 64);
                let msg_r = Rect::new(ui::MARGIN, 1600 - ui::BOTTOM_SAFE - 144, 720 - ui::MARGIN * 2 - 140, 56);
                let send_r = Rect::new(720 - ui::MARGIN - 124, 1600 - ui::BOTTOM_SAFE - 144, 124, 56);
                if to_r.contains(pos) {
                    self.focus_body = false;
                    ti.activate(ContentPurpose::Phone);
                    self.pressed = Some(1);
                } else if msg_r.contains(pos) {
                    self.focus_body = true;
                    ti.activate(ContentPurpose::Normal);
                    self.pressed = Some(2);
                } else if send_r.contains(pos) {
                    self.pressed = Some(3);
                }
                AppFrame { dirty: true, ..Default::default() }
            }
            AppEvent::TouchUp { .. } => {
                if self.pressed.take() == Some(3) {
                    self.send();
                }
                AppFrame { dirty: true, ..Default::default() }
            }
            AppEvent::TextCommit(s) => {
                if self.focus_body { self.body.push_str(&s); } else { self.recipient.push_str(&s); }
                AppFrame { dirty: true, ..Default::default() }
            }
            AppEvent::TextBackspace(_) | AppEvent::Key { keysym: 0xff08, .. } => {
                if self.focus_body { self.body.pop(); } else { self.recipient.pop(); }
                AppFrame { dirty: true, ..Default::default() }
            }
            AppEvent::Key { utf8: Some(s), .. } => {
                if self.focus_body { self.body.push_str(&s); } else { self.recipient.push_str(&s); }
                AppFrame { dirty: true, ..Default::default() }
            }
            _ => AppFrame::default(),
        }
    }

    fn render(&mut self, canvas: &mut Canvas, theme: &Theme) {
        let w = canvas.size().w;
        let h = canvas.size().h;
        ui::draw_header(canvas, theme, "Messages", &self.status);

        let to_r = Rect::new(ui::MARGIN, ui::TOP_SAFE + 96, w - ui::MARGIN * 2, 64);
        ui::draw_card(canvas, theme, to_r, !self.focus_body);
        let to_txt = if self.recipient.is_empty() { "To: Phone number" } else { &self.recipient };
        ui::draw_text(canvas, to_txt, &ui::body(if self.recipient.is_empty() { theme.text3 } else { theme.text }), to_r.x + 20, to_r.y + 18, Some(to_r.w - 40));

        for (i, m) in self.messages.iter().take(8).enumerate() {
            let r = Rect::new(ui::MARGIN, ui::TOP_SAFE + 180 + i as i32 * 82, w - ui::MARGIN * 2, 72);
            ui::draw_card(canvas, theme, r, false);
            ui::draw_text(canvas, m, &ui::body(theme.text), r.x + 20, r.y + 20, Some(r.w - 40));
        }

        let comp_y = h - ui::BOTTOM_SAFE - 144;
        let msg_r = Rect::new(ui::MARGIN, comp_y, w - ui::MARGIN * 2 - 140, 56);
        ui::draw_card(canvas, theme, msg_r, self.focus_body);
        let m_txt = if self.body.is_empty() { "Message..." } else { &self.body };
        ui::draw_text(canvas, m_txt, &ui::body(if self.body.is_empty() { theme.text3 } else { theme.text }), msg_r.x + 20, msg_r.y + 14, Some(msg_r.w - 40));

        ui::draw_pill_button(canvas, theme, Rect::new(w - ui::MARGIN - 124, comp_y, 124, 56), "Send", true, self.pressed == Some(3));
    }
}

fn main() {
    rabbiz_app_kit::run_app(SmsApp::new());
}
