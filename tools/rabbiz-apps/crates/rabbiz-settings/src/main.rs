use std::process::Command;

use de_config::keyboards::{discover_keyboards, KeyboardInfo};
use de_config::Prefs;
use rabbiz_app_kit::{
    ui, AppEvent, AppFrame, AppState, Canvas, Rect, TextInputCtl, Theme,
};

struct SettingsApp {
    prefs: Prefs,
    keyboards: Vec<KeyboardInfo>,
    status: String,
    pressed: Option<usize>,
}

impl SettingsApp {
    fn new() -> Self {
        let prefs = Prefs::load();
        let keyboards = discover_keyboards(prefs.allow_third_party_keyboards);
        Self {
            prefs,
            keyboards,
            status: "RabbizOS".into(),
            pressed: None,
        }
    }

    fn reload_kbs(&mut self) {
        self.keyboards = discover_keyboards(self.prefs.allow_third_party_keyboards);
    }

    fn row_rect(i: usize) -> Rect {
        Rect::new(ui::MARGIN, ui::TOP_SAFE + 104 + i as i32 * 96, 720 - ui::MARGIN * 2, 84)
    }
}

impl AppState for SettingsApp {
    fn app_id(&self) -> &'static str { "rabbiz-settings" }
    fn title(&self) -> &'static str { "Settings" }

    fn handle_event(&mut self, event: AppEvent, _ti: &TextInputCtl) -> AppFrame {
        match event {
            AppEvent::TouchDown { pos, .. } => {
                let total = 3 + self.keyboards.len().min(5);
                self.pressed = (0..total).find(|&i| Self::row_rect(i).contains(pos));
                AppFrame { dirty: true, ..Default::default() }
            }
            AppEvent::TouchUp { pos, .. } => {
                if let Some(i) = self.pressed.take() {
                    if Self::row_rect(i).contains(pos) {
                        match i {
                            0 => {
                                self.prefs.theme_mode = if self.prefs.theme_mode == "dark" { "light".into() } else { "dark".into() };
                                let _ = self.prefs.save();
                                self.status = format!("Theme: {} (relaunch to apply)", self.prefs.theme_mode);
                            }
                            1 => {
                                let _ = Command::new("wpctl").args(["set-volume", "@DEFAULT_AUDIO_SINK@", "5%+"]).status();
                                self.status = "Volume +5%".into();
                            }
                            2 => {
                                self.prefs.allow_third_party_keyboards = !self.prefs.allow_third_party_keyboards;
                                if !self.prefs.allow_third_party_keyboards {
                                    self.prefs.keyboard_id = "rabbiz-osk".into();
                                }
                                let _ = self.prefs.save();
                                self.reload_kbs();
                                self.status = format!("3rd-party keyboards: {}", self.prefs.allow_third_party_keyboards);
                            }
                            k => {
                                let idx = k - 3;
                                if let Some(kb) = self.keyboards.get(idx) {
                                    if kb.installed {
                                        self.prefs.keyboard_id = kb.id.clone();
                                        let _ = self.prefs.save();
                                        self.status = format!("Keyboard set to {}", kb.name);
                                    } else {
                                        self.status = format!("{} not installed", kb.name);
                                    }
                                }
                            }
                        }
                    }
                }
                AppFrame { dirty: true, ..Default::default() }
            }
            _ => AppFrame::default(),
        }
    }

    fn render(&mut self, canvas: &mut Canvas, theme: &Theme) {
        ui::draw_header(canvas, theme, "Settings", &self.status);

        let dark = self.prefs.theme_mode == "dark";
        let r0 = Self::row_rect(0);
        ui::draw_card(canvas, theme, r0, self.pressed == Some(0));
        ui::draw_text(canvas, "Dark Mode", &ui::label(theme.text), r0.x + 24, r0.y + 26, None);
        ui::draw_switch(canvas, theme, r0.x + r0.w - 80, r0.y + 26, dark);

        let r1 = Self::row_rect(1);
        ui::draw_card(canvas, theme, r1, self.pressed == Some(1));
        ui::draw_text(canvas, "Volume +5%", &ui::label(theme.text), r1.x + 24, r1.y + 26, None);

        let r2 = Self::row_rect(2);
        ui::draw_card(canvas, theme, r2, self.pressed == Some(2));
        ui::draw_text(canvas, "Allow 3rd-party Keyboards", &ui::label(theme.text), r2.x + 24, r2.y + 26, Some(r2.w - 120));
        ui::draw_switch(canvas, theme, r2.x + r2.w - 80, r2.y + 26, self.prefs.allow_third_party_keyboards);

        for (idx, kb) in self.keyboards.iter().take(5).enumerate() {
            let r = Self::row_rect(3 + idx);
            ui::draw_card(canvas, theme, r, self.pressed == Some(3 + idx));
            let col = if kb.installed { theme.text } else { theme.text3 };
            ui::draw_text(canvas, &kb.name, &ui::label(col), r.x + 24, r.y + 26, Some(r.w - 200));
            let badge = if self.prefs.keyboard_id == kb.id { "Active" } else if kb.installed { "Installed" } else { "Missing" };
            let bcol = if self.prefs.keyboard_id == kb.id { theme.accent } else { theme.text3 };
            ui::draw_text(canvas, badge, &ui::caption(bcol), r.x + r.w - 130, r.y + 28, Some(110));
        }
    }
}

fn main() {
    rabbiz_app_kit::run_app(SettingsApp::new());
}
