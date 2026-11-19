use std::process::Command;

use rabbiz_app_kit::{
    ui, AppEvent, AppFrame, AppState, Canvas, Rect, TextInputCtl, Theme,
};

struct DialerApp {
    number: String,
    status: String,
    pressed: Option<usize>,
}

impl DialerApp {
    fn key_rects(w: i32, h: i32) -> Vec<(Rect, &'static str)> {
        let keys = ["1", "2", "3", "4", "5", "6", "7", "8", "9", "*", "0", "#"];
        let gap = 18;
        let pad_x = 56;
        let bw = (w - pad_x * 2 - gap * 2) / 3;
        let bh = 96;
        let start_y = h - ui::BOTTOM_SAFE - 110 - (bh * 4 + gap * 3);
        keys.iter()
            .enumerate()
            .map(|(i, k)| {
                let r = i as i32 / 3;
                let c = i as i32 % 3;
                (Rect::new(pad_x + c * (bw + gap), start_y + r * (bh + gap), bw, bh), *k)
            })
            .collect()
    }

    fn call_number(&mut self) {
        if self.number.is_empty() {
            return;
        }
        let arg = format!("number={}", self.number);
        match Command::new("mmcli").args(["-m", "0", "--voice-create-call", &arg]).output() {
            Ok(out) if out.status.success() => self.status = format!("Calling {}...", self.number),
            Ok(out) => self.status = String::from_utf8_lossy(&out.stderr).trim().to_string(),
            Err(e) => self.status = format!("Modem unavailable: {e}"),
        }
    }
}

impl AppState for DialerApp {
    fn app_id(&self) -> &'static str { "rabbiz-dialer" }
    fn title(&self) -> &'static str { "Phone" }

    fn handle_event(&mut self, event: AppEvent, _ti: &TextInputCtl) -> AppFrame {
        match event {
            AppEvent::TouchDown { pos, .. } => {
                let keys = Self::key_rects(720, 1600);
                let mut hit = keys.iter().position(|(r, _)| r.contains(pos));
                let call_r = Rect::new(220, 1600 - ui::BOTTOM_SAFE - 76, 280, 62);
                let back_r = Rect::new(520, 1600 - ui::BOTTOM_SAFE - 76, 120, 62);
                if hit.is_none() && call_r.contains(pos) { hit = Some(900); }
                if hit.is_none() && back_r.contains(pos) { hit = Some(901); }
                self.pressed = hit;
                AppFrame { dirty: true, ..Default::default() }
            }
            AppEvent::TouchUp { pos, .. } => {
                let keys = Self::key_rects(720, 1600);
                match self.pressed.take() {
                    Some(900) => self.call_number(),
                    Some(901) => { self.number.pop(); }
                    Some(i) if i < keys.len() && keys[i].0.contains(pos) => {
                        self.number.push_str(keys[i].1);
                    }
                    _ => {}
                }
                AppFrame { dirty: true, ..Default::default() }
            }
            _ => AppFrame::default(),
        }
    }

    fn render(&mut self, canvas: &mut Canvas, theme: &Theme) {
        let w = canvas.size().w;
        let h = canvas.size().h;
        ui::draw_header(canvas, theme, "Phone", &self.status);

        let disp = Rect::new(ui::MARGIN, ui::TOP_SAFE + 110, w - ui::MARGIN * 2, 110);
        ui::draw_card(canvas, theme, disp, false);
        let num_str = if self.number.is_empty() { "Enter number" } else { &self.number };
        let col = if self.number.is_empty() { theme.text3 } else { theme.text };
        ui::draw_centered(canvas, num_str, &ui::styled(42.0, 600, col), disp);

        for (i, (r, k)) in Self::key_rects(w, h).iter().enumerate() {
            ui::draw_pill_button(canvas, theme, *r, k, false, self.pressed == Some(i));
        }

        ui::draw_pill_button(canvas, theme, Rect::new(220, h - ui::BOTTOM_SAFE - 76, 280, 62), "Call", true, self.pressed == Some(900));
        ui::draw_pill_button(canvas, theme, Rect::new(520, h - ui::BOTTOM_SAFE - 76, 120, 62), "⌫", false, self.pressed == Some(901));
    }
}

fn main() {
    rabbiz_app_kit::run_app(DialerApp {
        number: String::new(),
        status: String::new(),
        pressed: None,
    });
}
