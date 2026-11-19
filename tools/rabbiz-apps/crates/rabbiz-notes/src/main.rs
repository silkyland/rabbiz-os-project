use std::path::PathBuf;

use rabbiz_app_kit::{
    ui, AppEvent, AppFrame, AppState, Canvas, Rect, TextInputCtl, Theme,
};
use serde::{Deserialize, Serialize};
use wayland_protocols::wp::text_input::zv3::client::zwp_text_input_v3::ContentPurpose;

#[derive(Clone, Serialize, Deserialize)]
struct Note {
    id: u64,
    text: String,
}

struct NotesApp {
    notes: Vec<Note>,
    editing: Option<usize>,
    pressed: Option<usize>,
}

impl NotesApp {
    fn dir() -> PathBuf {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("rabbiz-notes")
    }

    fn load() -> Vec<Note> {
        let p = Self::dir().join("notes.json");
        std::fs::read_to_string(p)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    fn save(&self) {
        let dir = Self::dir();
        let _ = std::fs::create_dir_all(&dir);
        if let Ok(s) = serde_json::to_string_pretty(&self.notes) {
            let _ = std::fs::write(dir.join("notes.json"), s);
        }
    }
}

impl AppState for NotesApp {
    fn app_id(&self) -> &'static str { "rabbiz-notes" }
    fn title(&self) -> &'static str { "Notes" }

    fn handle_event(&mut self, event: AppEvent, ti: &TextInputCtl) -> AppFrame {
        match event {
            AppEvent::TouchDown { pos, .. } => {
                let mut hit = None;
                if self.editing.is_some() {
                    if Rect::new(ui::MARGIN, ui::TOP_SAFE + 16, 120, 48).contains(pos) {
                        hit = Some(900);
                    } else if Rect::new(720 - ui::MARGIN - 120, ui::TOP_SAFE + 16, 120, 48).contains(pos) {
                        hit = Some(901);
                    }
                } else {
                    if Rect::new(720 - ui::MARGIN - 160, 1600 - ui::BOTTOM_SAFE - 72, 160, 56).contains(pos) {
                        hit = Some(902);
                    } else {
                        for (i, _) in self.notes.iter().enumerate() {
                            let r = Rect::new(ui::MARGIN, ui::TOP_SAFE + 96 + i as i32 * 104, 720 - ui::MARGIN * 2, 92);
                            if r.contains(pos) {
                                hit = Some(i);
                                break;
                            }
                        }
                    }
                }
                self.pressed = hit;
                AppFrame { dirty: true, ..Default::default() }
            }
            AppEvent::TouchUp { .. } => {
                let hit = self.pressed.take();
                match hit {
                    Some(900) => {
                        self.save();
                        self.editing = None;
                        ti.deactivate();
                    }
                    Some(901) => {
                        if let Some(i) = self.editing.take() {
                            if i < self.notes.len() {
                                self.notes.remove(i);
                                self.save();
                            }
                        }
                        ti.deactivate();
                    }
                    Some(902) => {
                        let id = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_secs())
                            .unwrap_or(1);
                        self.notes.insert(0, Note { id, text: String::new() });
                        self.editing = Some(0);
                        ti.activate(ContentPurpose::Normal);
                        ti.set_surrounding("", 0);
                    }
                    Some(i) if i < self.notes.len() => {
                        self.editing = Some(i);
                        ti.activate(ContentPurpose::Normal);
                        let t = &self.notes[i].text;
                        ti.set_surrounding(t, t.len());
                    }
                    _ => {}
                }
                AppFrame { dirty: true, ..Default::default() }
            }
            AppEvent::TextCommit(s) => {
                if let Some(i) = self.editing {
                    if let Some(n) = self.notes.get_mut(i) {
                        n.text.push_str(&s);
                        ti.set_surrounding(&n.text, n.text.len());
                    }
                    self.save();
                }
                AppFrame { dirty: true, ..Default::default() }
            }
            AppEvent::TextBackspace(bytes) => {
                if let Some(i) = self.editing {
                    if let Some(n) = self.notes.get_mut(i) {
                        let mut target = n.text.len().saturating_sub(bytes);
                        while target > 0 && !n.text.is_char_boundary(target) {
                            target -= 1;
                        }
                        n.text.truncate(target);
                        ti.set_surrounding(&n.text, n.text.len());
                    }
                    self.save();
                }
                AppFrame { dirty: true, ..Default::default() }
            }
            AppEvent::Key { keysym: 0xff08, .. } => {
                if let Some(i) = self.editing {
                    if let Some(n) = self.notes.get_mut(i) {
                        n.text.pop();
                        ti.set_surrounding(&n.text, n.text.len());
                    }
                    self.save();
                }
                AppFrame { dirty: true, ..Default::default() }
            }
            AppEvent::Key { keysym: 0xff0d, .. } => {
                if let Some(i) = self.editing {
                    if let Some(n) = self.notes.get_mut(i) {
                        n.text.push('\n');
                        ti.set_surrounding(&n.text, n.text.len());
                    }
                    self.save();
                }
                AppFrame { dirty: true, ..Default::default() }
            }
            AppEvent::Key { utf8: Some(s), .. } => {
                if let Some(i) = self.editing {
                    if let Some(n) = self.notes.get_mut(i) {
                        n.text.push_str(&s);
                        ti.set_surrounding(&n.text, n.text.len());
                    }
                    self.save();
                }
                AppFrame { dirty: true, ..Default::default() }
            }
            _ => AppFrame::default(),
        }
    }

    fn render(&mut self, canvas: &mut Canvas, theme: &Theme) {
        let w = canvas.size().w;
        let h = canvas.size().h;
        if let Some(i) = self.editing {
            ui::draw_pill_button(canvas, theme, Rect::new(ui::MARGIN, ui::TOP_SAFE + 16, 120, 48), "Back", false, self.pressed == Some(900));
            ui::draw_pill_button(canvas, theme, Rect::new(w - ui::MARGIN - 120, ui::TOP_SAFE + 16, 120, 48), "Delete", false, self.pressed == Some(901));
            let editor = Rect::new(ui::MARGIN, ui::TOP_SAFE + 88, w - ui::MARGIN * 2, h - ui::TOP_SAFE - ui::BOTTOM_SAFE - 120);
            ui::draw_card(canvas, theme, editor, false);
            let txt = self.notes.get(i).map(|n| n.text.as_str()).unwrap_or("");
            if txt.is_empty() {
                ui::draw_text(canvas, "Tap to type with keyboard...", &ui::body(theme.text3), editor.x + 24, editor.y + 24, Some(editor.w - 48));
            } else {
                for (ln, line) in txt.lines().take(24).enumerate() {
                    ui::draw_text(canvas, line, &ui::body(theme.text), editor.x + 24, editor.y + 24 + ln as i32 * 32, Some(editor.w - 48));
                }
            }
        } else {
            ui::draw_header(canvas, theme, "Notes", "");
            if self.notes.is_empty() {
                ui::draw_centered(canvas, "No notes yet", &ui::body(theme.text3), Rect::new(0, 300, w, 80));
            }
            for (i, n) in self.notes.iter().take(10).enumerate() {
                let r = Rect::new(ui::MARGIN, ui::TOP_SAFE + 96 + i as i32 * 104, w - ui::MARGIN * 2, 92);
                ui::draw_card(canvas, theme, r, self.pressed == Some(i));
                let title = n.text.lines().next().filter(|l| !l.trim().is_empty()).unwrap_or("Untitled note");
                ui::draw_text(canvas, title, &ui::label(theme.text), r.x + 24, r.y + 28, Some(r.w - 48));
            }
            ui::draw_pill_button(canvas, theme, Rect::new(w - ui::MARGIN - 160, h - ui::BOTTOM_SAFE - 72, 160, 56), "+ New", true, self.pressed == Some(902));
        }
    }
}

fn main() {
    rabbiz_app_kit::run_app(NotesApp {
        notes: NotesApp::load(),
        editing: None,
        pressed: None,
    });
}
