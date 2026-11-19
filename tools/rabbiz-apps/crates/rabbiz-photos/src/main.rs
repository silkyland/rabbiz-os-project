use std::path::PathBuf;

use rabbiz_app_kit::{
    ui, AppEvent, AppFrame, AppState, Canvas, Rect, TextInputCtl, Theme,
};

struct PhotosApp {
    photos: Vec<PathBuf>,
    selected: Option<usize>,
    pressed: Option<usize>,
}

impl PhotosApp {
    fn scan() -> Vec<PathBuf> {
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"));
        let mut out = Vec::new();
        for sub in ["Pictures", "Downloads"] {
            let dir = home.join(sub);
            if let Ok(rd) = std::fs::read_dir(dir) {
                for e in rd.flatten() {
                    let p = e.path();
                    if let Some(ext) = p.extension().and_then(|s| s.to_str()).map(str::to_lowercase) {
                        if matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "webp") {
                            out.push(p);
                        }
                    }
                }
            }
        }
        out.sort();
        out
    }
}

impl AppState for PhotosApp {
    fn app_id(&self) -> &'static str { "rabbiz-photos" }
    fn title(&self) -> &'static str { "Photos" }

    fn handle_event(&mut self, event: AppEvent, _ti: &TextInputCtl) -> AppFrame {
        match event {
            AppEvent::TouchDown { pos, .. } => {
                let mut hit = None;
                if self.selected.is_some() {
                    if Rect::new(ui::MARGIN, ui::TOP_SAFE + 16, 120, 48).contains(pos) {
                        hit = Some(900);
                    }
                } else {
                    for i in 0..self.photos.len().min(12) {
                        let r = Rect::new(ui::MARGIN, ui::TOP_SAFE + 96 + i as i32 * 96, 720 - ui::MARGIN * 2, 84);
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
                    Some(900) => self.selected = None,
                    Some(i) if i < self.photos.len() => self.selected = Some(i),
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
        if let Some(idx) = self.selected {
            ui::draw_pill_button(canvas, theme, Rect::new(ui::MARGIN, ui::TOP_SAFE + 16, 120, 48), "Back", false, self.pressed == Some(900));
            let name = self.photos.get(idx).and_then(|p| p.file_name()).and_then(|s| s.to_str()).unwrap_or("Photo");
            ui::draw_centered(canvas, name, &ui::body(theme.text), Rect::new(0, h / 2 - 40, w, 80));
        } else {
            ui::draw_header(canvas, theme, "Photos", &format!("{} items", self.photos.len()));
            if self.photos.is_empty() {
                ui::draw_centered(canvas, "No photos in ~/Pictures or ~/Downloads", &ui::body(theme.text3), Rect::new(0, 320, w, 80));
            }
            for (i, p) in self.photos.iter().take(12).enumerate() {
                let r = Rect::new(ui::MARGIN, ui::TOP_SAFE + 96 + i as i32 * 96, w - ui::MARGIN * 2, 84);
                ui::draw_card(canvas, theme, r, self.pressed == Some(i));
                let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("Photo");
                ui::draw_text(canvas, name, &ui::label(theme.text), r.x + 24, r.y + 26, Some(r.w - 48));
            }
        }
    }
}

fn main() {
    rabbiz_app_kit::run_app(PhotosApp {
        photos: PhotosApp::scan(),
        selected: None,
        pressed: None,
    });
}
