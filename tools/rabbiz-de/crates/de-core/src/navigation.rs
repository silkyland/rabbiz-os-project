//! Runtime state for edge feedback and the Home backdrop behind Recents.
use std::sync::mpsc;
use std::time::{Duration, Instant};

use de_api::{Pixels, Point, Size};
use de_design::motion::{MotionKind, SpringPreset};
use de_paint::animation::SpringValue;
use de_ui::gestures::Zone;
use de_ui::pager::Pager;
use crate::state::{De, Foreground};

pub(crate) struct Navigation {
    pub zone: Zone,
    pub start: Point,
    pub back: Option<(Pixels, Point)>,
    pub back_texture: Option<Box<dyn std::any::Any>>,
    pub back_key: Option<(bool, i32, bool)>,
    pub recents_backdrop: Option<Pixels>,
    pub recents_blur_rx: Option<mpsc::Receiver<Pixels>>,
    pub drawer_scroll: i32,
    pub drawer_drag_origin: i32,
    pub drawer_pull_y: f32,
    pub drawer_slide: SpringValue,
    pub drawer_dragging: bool,
    pub drawer_closing: bool,
    pub drawer_last_tick: Option<Instant>,
    pub home_underlay_px: Option<Pixels>,
    pub home_underlay_tex: Option<Box<dyn std::any::Any>>,
    pub recents_pager: Pager,
    pub recents_last_tick: Option<Instant>,
    pub recents_axis: Option<RecentsAxis>,
    pub recents_fade: Option<OverviewFade>,
    pub search_query: String,
}
impl Navigation {
    pub fn new() -> Self {
        Self { zone: Zone::Content, start: Point::ZERO, back: None,
            back_texture: None, back_key: None, recents_backdrop: None,
            recents_blur_rx: None, drawer_scroll: 0, drawer_drag_origin: 0,
            drawer_pull_y: 0.0,
            drawer_slide: SpringValue::new(0.0, SpringPreset::Smooth, MotionKind::Spatial),
            drawer_dragging: false, drawer_closing: false, drawer_last_tick: None,
            home_underlay_px: None, home_underlay_tex: None,
            recents_pager: Pager::new(1), recents_last_tick: None,
            recents_axis: None,
            recents_fade: None,
            search_query: String::new() }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum RecentsAxis {
    Horizontal,
    Vertical,
}

pub(crate) struct OverviewFade {
    started: Instant,
    from: f32,
    to: f32,
    duration: Duration,
}

impl OverviewFade {
    pub fn mix(&self, now: Instant) -> f32 {
        let p = (now.saturating_duration_since(self.started).as_secs_f32()
            / self.duration.as_secs_f32()).clamp(0.0, 1.0);
        self.to + (self.from - self.to) * (1.0 - p).powi(3)
    }
}

impl De {
    pub(crate) fn fade_recents(&mut self, entering: bool) {
        let now = Instant::now();
        let to = if entering { 1.0 } else { 0.0 };
        if self.navigation.recents_fade.as_ref().is_some_and(|f| f.to == to) {
            return;
        }
        let from = self.navigation.recents_fade.as_ref().map_or(1.0 - to, |f| f.mix(now));
        self.navigation.recents_fade = Some(OverviewFade {
            started: now,
            from,
            to,
            duration: Duration::from_millis(if entering { 240 } else { 180 }),
        });
        self.needs_present = true;
    }

    pub(crate) fn advance_recents_fade(&mut self, now: Instant) {
        let Some(fade) = self.navigation.recents_fade.as_ref() else { return; };
        self.needs_present = true;
        if now.saturating_duration_since(fade.started) >= fade.duration {
            if fade.to == 0.0 {
                self.nav.current = de_ui::nav::Screen::Home;
                self.shell_dirty = true;
                self.navigation.recents_backdrop = None;
                self.navigation.recents_blur_rx = None;
            }
            self.navigation.recents_fade = None;
        }
    }

    pub(crate) fn reset_recents_pager(&mut self) {
        self.navigation.recents_pager.set_pages(self.recent_windows().len());
        self.navigation.recents_pager.jump_to(0);
        self.navigation.recents_axis = None;
    }

    pub(crate) fn release_recents_pager(&mut self, vx: f32) {
        let size = Size::new(self.output.width as i32, self.output.height as i32);
        self.navigation.recents_pager.release(vx, de_ui::recents::card_stride(size) as f32);
        self.navigation.recents_last_tick = Some(Instant::now());
        self.navigation.recents_axis = None;
        self.needs_present = true;
    }

    pub(crate) fn back_progress(&mut self, zone: Zone, dx: f32, dy: f32) {
        if self.panels.shown() || self.locked() { return; }
        if self.foreground == Foreground::Shell && self.nav.current == de_ui::nav::Screen::Home {
            return;
        }
        let left = zone == Zone::EdgeLeft;
        let inward = if left { dx } else { -dx };
        if inward <= 0.0 || inward < dy.abs() * 1.4 {
            self.clear_back_feedback();
            return;
        }
        let size = Size::new(self.output.width as i32, self.output.height as i32);
        let progress = (inward / 96.0).clamp(0.0, 1.0);
        let w = (16.0 + 44.0 * progress).round() as i32;
        let triggered = inward >= 48.0;
        let key = (left, w, triggered);
        if self.navigation.back_key == Some(key) {
            if let Some((_, at)) = self.navigation.back.as_mut() {
                const H: i32 = 260;
                at.x = if left { 0 } else { size.w - w };
                at.y = (self.navigation.start.y + dy as i32 - H / 2).clamp(0, (size.h - H).max(0));
                self.needs_present = true;
                return;
            }
        }
        let (px, at) = de_ui::gestures::back_feedback(size, left,
            self.navigation.start.y + dy as i32, inward);
        self.navigation.back = Some((px, at));
        self.navigation.back_texture = None;
        self.navigation.back_key = Some(key);
        self.needs_present = true;
    }
    pub(crate) fn clear_back_feedback(&mut self) {
        self.navigation.back_key = None;
        if self.navigation.back.take().is_some() {
            self.navigation.back_texture = None;
            self.needs_present = true;
        }
    }
    pub(crate) fn open_drawer_animated(&mut self, from_y: Option<f32>) {
        use de_ui::nav::Screen;
        if self.nav.current == Screen::Home && !self.shell_dirty {
            if let Some(px) = self.shell_px.as_ref() {
                self.navigation.home_underlay_px = Some(px.clone());
                self.navigation.home_underlay_tex = None;
            }
        }
        if self.navigation.home_underlay_px.is_none() {
            let prev = self.nav.current;
            self.nav.current = Screen::Home;
            self.navigation.home_underlay_px = Some(self.shell_pixels());
            self.navigation.home_underlay_tex = None;
            self.nav.current = prev;
        }
        let h = self.output.height.max(1) as f32;
        let start = from_y.unwrap_or(h).clamp(0.0, h);
        self.navigation.drawer_scroll = 0;
        self.navigation.drawer_pull_y = 0.0;
        self.navigation.drawer_dragging = false;
        self.navigation.drawer_closing = false;
        self.navigation.drawer_slide.value = start;
        self.navigation.drawer_slide.retarget(0.0);
        self.navigation.drawer_last_tick = Some(Instant::now());
        self.show_shell(Screen::Drawer);
    }

    pub(crate) fn prepare_recents_backdrop(&mut self) {
        use de_ui::nav::Screen;
        let home = if self.nav.current == Screen::Home && !self.shell_dirty {
            if let Some(px) = self.shell_px.as_ref() {
                px.clone()
            } else {
                self.shell_pixels()
            }
        } else {
            let screen = self.nav.current;
            self.nav.current = Screen::Home;
            let px = self.shell_pixels();
            self.nav.current = screen;
            px
        };
        self.navigation.home_underlay_px = Some(home.clone());
        self.navigation.home_underlay_tex = None;
        if self.motion_clock.motion_restricted() {
            self.navigation.recents_blur_rx = None;
            self.navigation.recents_backdrop = Some(home);
            return;
        }
        let (tx, rx) = mpsc::channel();
        let src = home.clone();
        std::thread::spawn(move || {
            let _ = tx.send(de_paint::glass::blurred_backdrop(&src));
        });
        self.navigation.recents_blur_rx = Some(rx);
        self.navigation.recents_backdrop = Some(home);
    }

    pub(crate) fn poll_recents_backdrop(&mut self) {
        let Some(rx) = self.navigation.recents_blur_rx.take() else { return; };
        match rx.try_recv() {
            Ok(px) => {
                self.navigation.recents_backdrop = Some(px);
                if self.nav.current == de_ui::nav::Screen::Recents {
                    self.shell_dirty = true;
                    self.needs_present = true;
                }
            }
            Err(mpsc::TryRecvError::Empty) => {
                self.navigation.recents_blur_rx = Some(rx);
            }
            Err(mpsc::TryRecvError::Disconnected) => {}
        }
    }
}
