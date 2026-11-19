use std::path::PathBuf;
use std::process::Command;

use rabbiz_app_kit::{
    ui, AppEvent, AppFrame, AppState, Canvas, Rect, TextInputCtl, Theme,
};
use wayland_protocols::wp::text_input::zv3::client::zwp_text_input_v3::ContentPurpose;

struct TerminalApp {
    cwd: PathBuf,
    input: String,
    lines: Vec<String>,
    pressed: Option<usize>,
    osk_active: bool,
}

impl TerminalApp {
    fn new() -> Self {
        let cwd = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/"));
        Self {
            cwd,
            input: String::new(),
            lines: vec!["RabbizOS Terminal (tap prompt to type)".into()],
            pressed: None,
            osk_active: false,
        }
    }

    fn run_line(&mut self) {
        let cmd = self.input.trim().to_string();
        self.input.clear();
        if cmd.is_empty() {
            return;
        }
        self.lines.push(format!("$ {cmd}"));
        if cmd == "clear" {
            self.lines.clear();
            return;
        }
        if let Some(rest) = cmd.strip_prefix("cd") {
            let target = rest.trim();
            let next = if target.is_empty() || target == "~" {
                std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"))
            } else {
                self.cwd.join(target)
            };
            if next.is_dir() {
                self.cwd = next.canonicalize().unwrap_or(next);
            } else {
                self.lines.push(format!("cd: no such directory: {target}"));
            }
            return;
        }
        match Command::new("sh").arg("-c").arg(&cmd).current_dir(&self.cwd).output() {
            Ok(out) => {
                for s in String::from_utf8_lossy(&out.stdout).lines() {
                    self.lines.push(s.to_string());
                }
                for s in String::from_utf8_lossy(&out.stderr).lines() {
                    self.lines.push(s.to_string());
                }
            }
            Err(e) => self.lines.push(format!("exec error: {e}")),
        }
        if self.lines.len() > 200 {
            let drain = self.lines.len() - 200;
            self.lines.drain(..drain);
        }
    }

    fn quick_keys(w: i32, h: i32) -> Vec<(Rect, &'static str)> {
        let labels = ["Clear", "ls", "pwd", "uname", "/", "-", "~", "Run"];
        let y = h - ui::BOTTOM_SAFE - 64;
        let gap = 8;
        let bw = (w - ui::MARGIN * 2 - gap * (labels.len() as i32 - 1)) / labels.len() as i32;
        labels
            .iter()
            .enumerate()
            .map(|(i, l)| (Rect::new(ui::MARGIN + i as i32 * (bw + gap), y, bw, 48), *l))
            .collect()
    }
}

impl AppState for TerminalApp {
    fn app_id(&self) -> &'static str { "rabbiz-terminal" }
    fn title(&self) -> &'static str { "Terminal" }

    fn handle_event(&mut self, event: AppEvent, ti: &TextInputCtl) -> AppFrame {
        match event {
            AppEvent::TouchDown { pos, .. } => {
                let keys = Self::quick_keys(720, 1600);
                self.pressed = keys.iter().position(|(r, _)| r.contains(pos));
                if self.pressed.is_none() {
                    self.osk_active = true;
                    ti.activate(ContentPurpose::Terminal);
                }
                AppFrame { dirty: true, ..Default::default() }
            }
            AppEvent::TouchUp { pos, .. } => {
                let keys = Self::quick_keys(720, 1600);
                if let Some(i) = self.pressed.take() {
                    if let Some((r, lbl)) = keys.get(i) {
                        if r.contains(pos) {
                            match *lbl {
                                "Clear" => self.lines.clear(),
                                "ls" | "pwd" => { self.input = (*lbl).into(); self.run_line(); }
                                "uname" => { self.input = "uname -a".into(); self.run_line(); }
                                "Run" => self.run_line(),
                                ch => self.input.push_str(ch),
                            }
                        }
                    }
                }
                AppFrame { dirty: true, ..Default::default() }
            }
            AppEvent::TextCommit(s) => {
                self.input.push_str(&s);
                AppFrame { dirty: true, ..Default::default() }
            }
            AppEvent::TextBackspace(_) | AppEvent::Key { keysym: 0xff08, .. } => {
                self.input.pop();
                AppFrame { dirty: true, ..Default::default() }
            }
            AppEvent::Key { keysym: 0xff0d, .. } => {
                self.run_line();
                AppFrame { dirty: true, ..Default::default() }
            }
            AppEvent::Key { utf8: Some(s), .. } => {
                self.input.push_str(&s);
                AppFrame { dirty: true, ..Default::default() }
            }
            _ => AppFrame::default(),
        }
    }

    fn render(&mut self, canvas: &mut Canvas, theme: &Theme) {
        let w = canvas.size().w;
        let h = canvas.size().h;
        let top = ui::draw_header(canvas, theme, "Terminal", &self.cwd.display().to_string());
        let keys = Self::quick_keys(w, h);
        let prompt_y = keys[0].0.y - 68;
        let log_rect = Rect::new(ui::MARGIN, top + 8, w - ui::MARGIN * 2, (prompt_y - top - 20).max(120));
        ui::draw_card(canvas, theme, log_rect, false);

        let max_lines = ((log_rect.h - 24) / 26).max(1) as usize;
        let start = self.lines.len().saturating_sub(max_lines);
        for (i, line) in self.lines[start..].iter().enumerate() {
            ui::draw_text(
                canvas,
                line,
                &ui::styled(17.0, 400, theme.text),
                log_rect.x + 16,
                log_rect.y + 14 + i as i32 * 26,
                Some(log_rect.w - 32),
            );
        }

        let prompt_rect = Rect::new(ui::MARGIN, prompt_y, w - ui::MARGIN * 2, 54);
        ui::draw_card(canvas, theme, prompt_rect, false);
        let ptxt = format!("$ {}_", self.input);
        ui::draw_text(canvas, &ptxt, &ui::body(theme.accent), prompt_rect.x + 16, prompt_rect.y + 14, Some(prompt_rect.w - 32));

        for (i, (r, lbl)) in keys.iter().enumerate() {
            ui::draw_pill_button(canvas, theme, *r, lbl, *lbl == "Run", self.pressed == Some(i));
        }
    }
}

fn main() {
    rabbiz_app_kit::run_app(TerminalApp::new());
}
