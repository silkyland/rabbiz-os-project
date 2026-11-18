//! Runtime state for edge feedback and the Home backdrop behind Recents.
use de_api::{Pixels, Point, Size};
use de_ui::gestures::Zone;
use crate::state::{De, Foreground};

pub(crate) struct Navigation {
    pub zone: Zone,
    pub start: Point,
    pub back: Option<(Pixels, Point)>,
    pub back_texture: Option<Box<dyn std::any::Any>>,
    pub recents_backdrop: Option<Pixels>,
}
impl Navigation {
    pub fn new() -> Self {
        Self { zone: Zone::Content, start: Point::ZERO, back: None,
            back_texture: None, recents_backdrop: None }
    }
}
impl De {
    pub(crate) fn back_progress(&mut self, zone: Zone, dx: f32, dy: f32) {
        if self.foreground != Foreground::App || self.panels.shown() { return; }
        let left = zone == Zone::EdgeLeft;
        let inward = if left { dx } else { -dx };
        if inward <= 0.0 || inward < dy.abs() * 1.4 { return; }
        let size = Size::new(self.output.width as i32, self.output.height as i32);
        let (px, at) = de_ui::gestures::back_feedback(size, left,
            self.navigation.start.y + dy as i32, inward);
        self.navigation.back = Some((px, at));
        self.navigation.back_texture = None;
        self.needs_present = true;
    }
    pub(crate) fn clear_back_feedback(&mut self) {
        if self.navigation.back.take().is_some() {
            self.navigation.back_texture = None;
            self.needs_present = true;
        }
    }
    pub(crate) fn prepare_recents_backdrop(&mut self) {
        use de_ui::nav::Screen;
        let screen = self.nav.current;
        self.nav.current = Screen::Home;
        let home = self.shell_pixels();
        self.nav.current = screen;
        self.navigation.recents_backdrop = Some(if self.motion_clock.motion_restricted() {
            home
        } else { de_paint::glass::blurred_backdrop(&home) });
    }
}
