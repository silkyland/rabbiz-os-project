use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

use rabbiz_app_kit::{
    ui, AppEvent, AppFrame, AppState, Canvas, Rect, TextInputCtl, Theme,
};

struct MusicApp {
    tracks: Vec<PathBuf>,
    current: Option<usize>,
    playing: bool,
    player: Option<Child>,
    pressed: Option<usize>,
}

impl MusicApp {
    fn scan() -> Vec<PathBuf> {
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"));
        let mut out = Vec::new();
        for sub in ["Music", "Downloads"] {
            let dir = home.join(sub);
            if let Ok(rd) = std::fs::read_dir(dir) {
                for e in rd.flatten() {
                    let p = e.path();
                    if let Some(ext) = p.extension().and_then(|s| s.to_str()).map(str::to_lowercase) {
                        if matches!(ext.as_str(), "mp3" | "flac" | "wav" | "ogg" | "m4a" | "opus") {
                            out.push(p);
                        }
                    }
                }
            }
        }
        out.sort();
        out
    }

    fn stop_player(&mut self) {
        if let Some(mut c) = self.player.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
        self.playing = false;
    }

    fn play_idx(&mut self, idx: usize) {
        self.stop_player();
        let Some(path) = self.tracks.get(idx) else { return };
        let child = Command::new("ffplay")
            .args(["-nodisp", "-autoexit", "-loglevel", "quiet"])
            .arg(path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .or_else(|_| {
                Command::new("pw-play")
                    .arg(path)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
            });
        if let Ok(c) = child {
            self.player = Some(c);
            self.current = Some(idx);
            self.playing = true;
        }
    }
}

impl Drop for MusicApp {
    fn drop(&mut self) {
        self.stop_player();
    }
}

impl AppState for MusicApp {
    fn app_id(&self) -> &'static str { "rabbiz-music" }
    fn title(&self) -> &'static str { "Music" }

    fn handle_event(&mut self, event: AppEvent, _ti: &TextInputCtl) -> AppFrame {
        match event {
            AppEvent::TouchDown { pos, .. } => {
                let mut hit = None;
                let ctrl_y = 1600 - ui::BOTTOM_SAFE - 84;
                if Rect::new(ui::MARGIN, ctrl_y, 180, 56).contains(pos) {
                    hit = Some(900);
                } else if Rect::new(270, ctrl_y, 180, 56).contains(pos) {
                    hit = Some(901);
                } else if Rect::new(720 - ui::MARGIN - 180, ctrl_y, 180, 56).contains(pos) {
                    hit = Some(902);
                } else {
                    for i in 0..self.tracks.len().min(10) {
                        let r = Rect::new(ui::MARGIN, ui::TOP_SAFE + 240 + i as i32 * 84, 720 - ui::MARGIN * 2, 74);
                        if r.contains(pos) {
                            hit = Some(i);
                            break;
                        }
                    }
                }
                self.pressed = hit;
                AppFrame { dirty: true, ..Default::default() }
            }
            AppEvent::TouchUp { .. } => {
                match self.pressed.take() {
                    Some(900) => {
                        if let Some(c) = self.current {
                            let prev = if c == 0 { self.tracks.len().saturating_sub(1) } else { c - 1 };
                            self.play_idx(prev);
                        }
                    }
                    Some(901) => {
                        if self.playing {
                            self.stop_player();
                        } else if !self.tracks.is_empty() {
                            self.play_idx(self.current.unwrap_or(0));
                        }
                    }
                    Some(902) => {
                        if !self.tracks.is_empty() {
                            let next = self.current.map_or(0, |c| (c + 1) % self.tracks.len());
                            self.play_idx(next);
                        }
                    }
                    Some(i) if i < self.tracks.len() => self.play_idx(i),
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
        ui::draw_header(canvas, theme, "Music", &format!("{} tracks", self.tracks.len()));

        let now_card = Rect::new(ui::MARGIN, ui::TOP_SAFE + 96, w - ui::MARGIN * 2, 120);
        ui::draw_card(canvas, theme, now_card, false);
        let now_name = self.current
            .and_then(|i| self.tracks.get(i))
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
            .unwrap_or("Nothing playing");
        ui::draw_text(canvas, now_name, &ui::label(theme.text), now_card.x + 24, now_card.y + 24, Some(now_card.w - 48));
        ui::draw_text(canvas, if self.playing { "Playing" } else { "Paused" }, &ui::caption(theme.accent), now_card.x + 24, now_card.y + 66, None);

        for (i, p) in self.tracks.iter().take(10).enumerate() {
            let r = Rect::new(ui::MARGIN, ui::TOP_SAFE + 240 + i as i32 * 84, w - ui::MARGIN * 2, 74);
            ui::draw_card(canvas, theme, r, self.pressed == Some(i));
            let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("Track");
            let col = if self.current == Some(i) { theme.accent } else { theme.text };
            ui::draw_text(canvas, name, &ui::body(col), r.x + 24, r.y + 22, Some(r.w - 48));
        }

        let ctrl_y = h - ui::BOTTOM_SAFE - 84;
        ui::draw_pill_button(canvas, theme, Rect::new(ui::MARGIN, ctrl_y, 180, 56), "Prev", false, self.pressed == Some(900));
        ui::draw_pill_button(canvas, theme, Rect::new(270, ctrl_y, 180, 56), if self.playing { "Stop" } else { "Play" }, true, self.pressed == Some(901));
        ui::draw_pill_button(canvas, theme, Rect::new(w - ui::MARGIN - 180, ctrl_y, 180, 56), "Next", false, self.pressed == Some(902));
    }
}

fn main() {
    rabbiz_app_kit::run_app(MusicApp {
        tracks: MusicApp::scan(),
        current: None,
        playing: false,
        player: None,
        pressed: None,
    });
}
