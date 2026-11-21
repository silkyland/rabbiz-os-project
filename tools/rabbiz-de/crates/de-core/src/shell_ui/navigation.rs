//! Runtime state for edge feedback and the Home backdrop behind Recents.
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::state::{De, Foreground};
use de_api::{Pixels, Point, Size};
use de_design::motion::{MotionKind, SpringPreset};
use de_paint::animation::SpringValue;
use de_ui::gestures::Zone;
use de_ui::pager::Pager;

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
    /// Recents was entered from an app, so Back returns to that app
    /// (Android) instead of going Home.
    pub recents_from_app: bool,
    /// A Recents card being swiped up to close its app.
    pub card_dismiss: Option<CardDismiss>,
    /// Whether `Screen::Settings` is currently showing the Keyboard subpage.
    pub settings_keyboard: bool,
    /// GPU-cached static backdrop + per-card textures for 60 FPS Recents scrolling.
    pub recents_layers: Option<RecentsLayers>,
    /// Bottom gesture-bar touch glow/expand spring (`0.0` rest → `1.0` touched).
    pub nav_bar_active: SpringValue,
    pub nav_bar_last_tick: Option<Instant>,
    pub nav_bar_last_drawn: f32,
    /// Finger-tracked Recents pull from Home (`0.0` Home → `1.0` Recents).
    pub nav_recents_pull: SpringValue,
    pub nav_recents_active: bool,
    pub nav_recents_dragging: bool,
    pub nav_recents_commit: bool,
    pub nav_recents_last_tick: Option<Instant>,
}

pub(crate) struct RecentsCardCache {
    pub id: String,
    pub snap_ptr: Option<usize>,
    pub px: Option<Pixels>,
    pub tex: Option<Box<dyn std::any::Any>>,
}

pub(crate) struct RecentsLayers {
    pub bg_key: (Size, bool, bool, u64),
    pub bg_px: Option<Pixels>,
    pub bg_tex: Option<Box<dyn std::any::Any>>,
    pub cards: Vec<RecentsCardCache>,
}

/// Swipe-up-to-close on a Recents card. Tracked by window, not index, so the
/// list can shrink under it when the app exits.
pub(crate) struct CardDismiss {
    pub window: smithay::desktop::Window,
    /// Vertical offset in px (negative = up), finger-tracked then sprung.
    pub offset: SpringValue,
    pub dragging: bool,
    /// The close request was sent; the card flies off.
    pub closing: bool,
    pub last_tick: Option<Instant>,
}
impl Navigation {
    pub fn new() -> Self {
        Self {
            zone: Zone::Content,
            start: Point::ZERO,
            back: None,
            back_texture: None,
            back_key: None,
            recents_backdrop: None,
            recents_blur_rx: None,
            drawer_scroll: 0,
            drawer_drag_origin: 0,
            drawer_pull_y: 0.0,
            drawer_slide: SpringValue::new(0.0, SpringPreset::Smooth, MotionKind::Spatial),
            drawer_dragging: false,
            drawer_closing: false,
            drawer_last_tick: None,
            home_underlay_px: None,
            home_underlay_tex: None,
            recents_pager: Pager::new(1),
            recents_last_tick: None,
            recents_axis: None,
            recents_fade: None,
            search_query: String::new(),
            recents_from_app: false,
            card_dismiss: None,
            settings_keyboard: false,
            recents_layers: None,
            nav_bar_active: SpringValue::new(0.0, SpringPreset::Snappy, MotionKind::Effects),
            nav_bar_last_tick: None,
            nav_bar_last_drawn: -1.0,
            nav_recents_pull: SpringValue::new(0.0, SpringPreset::Smooth, MotionKind::Spatial),
            nav_recents_active: false,
            nav_recents_dragging: false,
            nav_recents_commit: false,
            nav_recents_last_tick: None,
        }
    }
}

impl De {
    /// Highlight/expand the bottom gesture pill on touch and emit a light haptic.
    pub(crate) fn set_nav_bar_active(&mut self, active: bool) {
        let target = if active { 1.0 } else { 0.0 };
        if (self.navigation.nav_bar_active.target - target).abs() < f32::EPSILON {
            return;
        }
        if active {
            crate::haptics::pulse(crate::haptics::HapticKind::NavTouch);
        }
        self.navigation.nav_bar_active.retarget(target);
        self.navigation.nav_bar_last_tick = Some(Instant::now());
        self.needs_present = true;
    }

    /// Advance the bottom gesture-bar glow/expand spring.
    pub(crate) fn advance_nav_bar(&mut self, now: Instant) {
        if self.navigation.nav_bar_active.settled() {
            self.navigation.nav_bar_last_tick = None;
            return;
        }
        let dt = self
            .navigation
            .nav_bar_last_tick
            .replace(now)
            .map_or(0.0, |last| now.duration_since(last).as_secs_f32());
        self.navigation
            .nav_bar_active
            .advance(dt, &self.motion_clock);
        if (self.navigation.nav_bar_active.target - self.navigation.nav_bar_active.value).abs()
            < 0.005
            && self.navigation.nav_bar_active.velocity.abs() < 0.05
        {
            self.navigation.nav_bar_active.snap();
        }
        self.needs_present = true;
    }

    /// Track an upward bottom-bar drag from Home so Recents rises live under
    /// the finger instead of waiting for hold/release to appear.
    pub(crate) fn follow_nav_recents_drag(&mut self, dy: f32) {
        if self.motion_clock.motion_restricted() || self.panels.shown() || self.locked() {
            return;
        }
        if !self.navigation.nav_recents_active {
            if dy >= -12.0 {
                return;
            }
            self.navigation.nav_recents_active = true;
            self.navigation.nav_recents_dragging = true;
            self.navigation.nav_recents_commit = false;
            self.navigation.nav_recents_last_tick = None;
            self.reset_recents_pager();
            self.prepare_recents_backdrop();
            self.navigation.recents_layers = None;
        }
        if !self.navigation.nav_recents_dragging {
            return;
        }
        let travel = (self.output.height.max(1) as f32 * 0.24).max(1.0);
        let progress = (-dy / travel).clamp(0.0, 1.0);
        self.navigation.nav_recents_pull.value = progress;
        self.navigation.nav_recents_pull.target = progress;
        self.navigation.nav_recents_pull.velocity = 0.0;
        self.needs_present = true;
    }

    /// Finger paused during Home→Recents drag: settle preview to 1.0 while held.
    pub(crate) fn hold_nav_recents_drag(&mut self) {
        if !self.navigation.nav_recents_active {
            return;
        }
        self.navigation.nav_recents_dragging = false;
        self.navigation.nav_recents_pull.retarget(1.0);
        self.navigation.nav_recents_last_tick = Some(Instant::now());
        self.needs_present = true;
    }

    /// Finger resumed moving after pause: hand tracking back to the finger.
    pub(crate) fn resume_nav_recents_drag(&mut self) {
        if !self.navigation.nav_recents_active {
            return;
        }
        self.navigation.nav_recents_dragging = true;
        self.navigation.nav_recents_commit = false;
        self.navigation.nav_recents_last_tick = None;
        self.needs_present = true;
    }

    /// Finger lifted with Recents destination: spring to 1.0 and commit Screen::Recents.
    pub(crate) fn commit_nav_recents_drag(&mut self) -> bool {
        if !self.navigation.nav_recents_active {
            return false;
        }
        self.navigation.nav_recents_dragging = false;
        self.navigation.nav_recents_commit = true;
        self.navigation.nav_recents_pull.retarget(1.0);
        self.navigation.nav_recents_last_tick = Some(Instant::now());
        self.needs_present = true;
        true
    }

    /// Finger lifted with Cancel destination: spring back to Home.
    pub(crate) fn dismiss_nav_recents_drag(&mut self) -> bool {
        if !self.navigation.nav_recents_active {
            return false;
        }
        self.navigation.nav_recents_dragging = false;
        self.navigation.nav_recents_commit = false;
        self.navigation.nav_recents_pull.retarget(0.0);
        self.navigation.nav_recents_last_tick = Some(Instant::now());
        self.needs_present = true;
        true
    }

    /// Immediately clear an in-flight Home→Recents drag (e.g. fast fling to Drawer).
    pub(crate) fn cancel_nav_recents_drag(&mut self) {
        self.navigation.nav_recents_active = false;
        self.navigation.nav_recents_dragging = false;
        self.navigation.nav_recents_commit = false;
        self.navigation.nav_recents_last_tick = None;
        self.navigation.nav_recents_pull.value = 0.0;
        self.navigation.nav_recents_pull.target = 0.0;
        self.navigation.nav_recents_pull.velocity = 0.0;
    }

    /// Advance the Home→Recents pull spring and finalize the screen handoff.
    pub(crate) fn advance_nav_recents_drag(&mut self, now: Instant) {
        if !self.navigation.nav_recents_active || self.navigation.nav_recents_dragging {
            self.navigation.nav_recents_last_tick = None;
            return;
        }
        if !self.navigation.nav_recents_pull.settled() {
            let dt = self
                .navigation
                .nav_recents_last_tick
                .replace(now)
                .map_or(0.0, |last| now.duration_since(last).as_secs_f32());
            self.navigation
                .nav_recents_pull
                .advance(dt, &self.motion_clock);
            if (self.navigation.nav_recents_pull.target - self.navigation.nav_recents_pull.value)
                .abs()
                < 0.005
                && self.navigation.nav_recents_pull.velocity.abs() < 0.05
            {
                self.navigation.nav_recents_pull.snap();
            }
            self.needs_present = true;
        }
        if self.navigation.nav_recents_pull.settled() {
            self.navigation.nav_recents_last_tick = None;
            if self.navigation.nav_recents_pull.target >= 0.5 {
                if self.navigation.nav_recents_commit {
                    self.navigation.nav_recents_active = false;
                    self.navigation.nav_recents_commit = false;
                    self.navigation.recents_from_app = false;
                    self.foreground = Foreground::Shell;
                    self.nav.current = de_ui::nav::Screen::Recents;
                    self.focus_foreground();
                    self.shell_dirty = true;
                    self.needs_present = true;
                }
            } else {
                self.navigation.nav_recents_active = false;
                self.navigation.nav_recents_commit = false;
                self.needs_present = true;
            }
        }
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
    motion: de_design::motion::TimedMotion,
    duration: Duration,
}

impl OverviewFade {
    pub fn mix(&self, now: Instant) -> f32 {
        let elapsed_ms = now.saturating_duration_since(self.started).as_secs_f32() * 1000.0;
        let eased = self.motion.progress_at(elapsed_ms);
        self.from + (self.to - self.from) * eased
    }
}

impl De {
    pub(crate) fn fade_recents(&mut self, entering: bool) {
        let now = Instant::now();
        let to = if entering { 1.0 } else { 0.0 };
        if self
            .navigation
            .recents_fade
            .as_ref()
            .is_some_and(|f| f.to == to)
        {
            return;
        }
        let from = self
            .navigation
            .recents_fade
            .as_ref()
            .map_or(1.0 - to, |f| f.mix(now));
        let timing = de_design::motion::Transition::Recents
            .timing()
            .for_form_factor(self.motion_clock.form_factor());
        let motion = if entering {
            timing.enter()
        } else {
            timing.exit()
        };
        self.navigation.recents_fade = Some(OverviewFade {
            started: now,
            from,
            to,
            motion,
            duration: Duration::from_millis(motion.duration_ms.into()),
        });
        self.needs_present = true;
    }

    pub(crate) fn advance_recents_fade(&mut self, now: Instant) {
        let Some(fade) = self.navigation.recents_fade.as_ref() else {
            return;
        };
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
        self.navigation
            .recents_pager
            .set_pages(self.recent_windows().len());
        self.navigation.recents_pager.jump_to(0);
        self.navigation.recents_axis = None;
    }

    /// Finger moved vertically on Recents: lift the card it started on.
    /// Returns false when the touch did not start on a card.
    pub(crate) fn card_dismiss_drag(&mut self, dy: f32) -> bool {
        if self
            .navigation
            .card_dismiss
            .as_ref()
            .is_some_and(|d| d.closing)
        {
            return true;
        }
        if self.navigation.card_dismiss.is_none() {
            let size = Size::new(self.output.width as i32, self.output.height as i32);
            let windows = self.recent_windows();
            let hit = de_ui::recents::hit_test(
                size,
                windows.len(),
                self.navigation.recents_pager.pos(),
                self.navigation.start,
            );
            let Some(de_ui::recents::RecentsHit::Card(i)) = hit else {
                return false;
            };
            let Some(window) = windows.into_iter().nth(i) else {
                return false;
            };
            self.navigation.card_dismiss = Some(CardDismiss {
                window,
                offset: SpringValue::new(0.0, SpringPreset::Smooth, MotionKind::Spatial),
                dragging: true,
                closing: false,
                last_tick: None,
            });
        }
        if let Some(d) = self.navigation.card_dismiss.as_mut() {
            // Up follows 1:1; down only gives a little (nothing to do there).
            let y = if dy < 0.0 {
                dy
            } else {
                dy * de_design::motion::RUBBER_FACTOR * 0.5
            };
            d.dragging = true;
            d.offset.value = y;
            d.offset.target = y;
            d.offset.velocity = 0.0;
        }
        if !self.recents_layers_active() {
            self.shell_dirty = true;
        }
        self.needs_present = true;
        true
    }

    /// Finger lifted from a lifted card: close the app or put the card back.
    /// Returns false when no card was being lifted.
    pub(crate) fn card_dismiss_release(&mut self, dy: f32, vy: f32) -> bool {
        let size = Size::new(self.output.width as i32, self.output.height as i32);
        let Some(d) = self.navigation.card_dismiss.as_mut() else {
            return false;
        };
        if d.closing {
            return true;
        }
        d.dragging = false;
        d.last_tick = Some(Instant::now());
        if de_ui::recents::dismiss_on_release(size, dy, vy) {
            d.closing = true;
            d.offset
                .retarget_with_velocity(-(size.h as f32), vy.min(0.0));
            if let Some(toplevel) = d.window.toplevel() {
                toplevel.send_close();
            }
        } else {
            d.offset.retarget_with_velocity(0.0, vy);
        }
        self.needs_present = true;
        true
    }

    /// Advance the card spring; drop it once it has flown off or returned.
    pub(crate) fn advance_card_dismiss(&mut self, now: Instant) {
        let Some(d) = self.navigation.card_dismiss.as_mut() else {
            return;
        };
        if d.dragging {
            d.last_tick = None;
            return;
        }
        let dt = d
            .last_tick
            .replace(now)
            .map_or(0.0, |last| now.duration_since(last).as_secs_f32());
        let moving = d.offset.advance(dt, &self.motion_clock);
        // Off-screen counts as done; the window leaves when the app exits.
        let gone = d.closing && d.offset.value <= -(self.output.height as f32) * 0.98;
        if gone || !moving || d.offset.settled() {
            self.navigation.card_dismiss = None;
            if self.foreground == crate::state::Foreground::Shell
                && self.nav.current == de_ui::nav::Screen::Recents
                && self.morph.is_none()
                && self.recent_windows().is_empty()
            {
                self.show_shell(de_ui::nav::Screen::Home);
            }
        }
        if !self.recents_layers_active() {
            self.shell_dirty = true;
        }
        self.needs_present = true;
    }

    /// The card lift to paint, if its window is still listed.
    pub(crate) fn card_lift(
        &self,
        windows: &[smithay::desktop::Window],
    ) -> Option<de_ui::recents::CardLift> {
        let d = self.navigation.card_dismiss.as_ref()?;
        let index = windows.iter().position(|w| w == &d.window)?;
        Some(de_ui::recents::CardLift {
            index,
            dy: d.offset.value,
        })
    }

    pub(crate) fn release_recents_pager(&mut self, vx: f32) {
        let size = Size::new(self.output.width as i32, self.output.height as i32);
        self.navigation
            .recents_pager
            .release(vx, de_ui::recents::card_stride(size) as f32);
        self.navigation.recents_last_tick = Some(Instant::now());
        self.navigation.recents_axis = None;
        self.needs_present = true;
    }

    pub(crate) fn back_progress(&mut self, zone: Zone, dx: f32, dy: f32) {
        if self.panels.shown() || self.locked() {
            return;
        }
        if self.foreground == Foreground::Shell && self.nav.current == de_ui::nav::Screen::Home {
            return;
        }
        let left = zone == Zone::EdgeLeft;
        let inward = if left { dx } else { -dx };
        if inward <= 0.0 || inward < dy.abs() * 1.25 {
            self.clear_back_feedback();
            return;
        }
        let size = Size::new(self.output.width as i32, self.output.height as i32);
        let progress = (inward / 96.0).clamp(0.0, 1.0);
        let w = (16.0 + 44.0 * progress).round() as i32;
        let triggered = inward >= de_ui::gestures::BACK_COMMIT_PX;
        let prev_triggered = self.navigation.back_key.map(|(_, _, t)| t);
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
        if prev_triggered != Some(triggered) {
            if triggered {
                crate::haptics::pulse(crate::haptics::HapticKind::GestureArm);
            } else if prev_triggered == Some(true) {
                crate::haptics::pulse(crate::haptics::HapticKind::GestureDisarm);
            }
        }
        let (px, at) = de_ui::gestures::back_feedback_themed(
            &self.theme,
            size,
            left,
            self.navigation.start.y + dy as i32,
            inward,
        );
        self.navigation.back = Some((px, at));
        self.navigation.back_texture = None;
        self.navigation.back_key = Some(key);
        self.needs_present = true;
    }
    /// Back inside an app, as on Android: the app gets the first say.
    /// An open menu closes first; a dialog (a toplevel with a parent) gets
    /// Escape, which every toolkit treats as "close"; anything else gets the
    /// Back key (evdev `KEY_BACK`, keysym `XF86Back`) that browsers, Qt and
    /// many GTK apps map to "go back". Leaving the app is the home swipe.
    pub(crate) fn app_back(&mut self, time_ms: u64) {
        use smithay::backend::input::KeyState;
        use smithay::input::keyboard::{FilterResult, Keycode};
        if let Some(mut grab) = self.popup_grab.take() {
            if !grab.has_ended() {
                grab.ungrab(smithay::desktop::PopupUngrabStrategy::All);
                self.needs_present = true;
                return;
            }
        }
        let Some(window) = self.recent_windows().into_iter().next() else {
            return;
        };
        let is_dialog = window.toplevel().is_some_and(|t| t.parent().is_some());
        // xkb keycode = evdev code + 8: KEY_ESC 1, KEY_BACK 158.
        let keycode = Keycode::new(if is_dialog { 1 + 8 } else { 158 + 8 });
        self.focus_foreground();
        let Some(keyboard) = self.seat.get_keyboard() else {
            return;
        };
        let time = time_ms as u32;
        for state in [KeyState::Pressed, KeyState::Released] {
            let serial = smithay::utils::SERIAL_COUNTER.next_serial();
            keyboard.input::<(), _>(self, keycode, state, serial, time, |_, _, _| {
                FilterResult::Forward
            });
        }
    }

    pub(crate) fn clear_back_feedback(&mut self) {
        self.navigation.back_key = None;
        if self.navigation.back.take().is_some() {
            self.navigation.back_texture = None;
            self.needs_present = true;
        }
    }
    /// Start 1:1 finger-tracked opening of the Drawer from Home.
    pub(crate) fn begin_drawer_drag(&mut self) {
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
        self.navigation.drawer_scroll = 0;
        self.navigation.search_query.clear();
        self.navigation.drawer_pull_y = 0.0;
        self.navigation.drawer_dragging = true;
        self.navigation.drawer_closing = false;
        self.navigation.drawer_slide.value = h;
        self.navigation.drawer_slide.target = h;
        self.navigation.drawer_last_tick = None;
        self.show_shell(Screen::Drawer);
    }

    /// Update 1:1 drawer position during an upward opening drag (`dy` < 0).
    pub(crate) fn drag_drawer(&mut self, dy: f32) {
        let h = self.output.height.max(1) as f32;
        let y = (h + dy).clamp(0.0, h);
        self.navigation.drawer_slide.value = y;
        self.navigation.drawer_slide.target = y;
        self.needs_present = true;
    }

    /// Release an opening drawer drag: commit to open (`0.0`) or fall back to Home (`h`).
    pub(crate) fn release_drawer_drag(&mut self, vy: f32) {
        let h = self.output.height.max(1) as f32;
        self.navigation.drawer_dragging = false;
        self.navigation.drawer_last_tick = Some(Instant::now());
        let projected =
            self.navigation.drawer_slide.value + vy * de_design::motion::PROJECT_AHEAD_S;
        if projected < h * 0.72 || vy <= -350.0 {
            self.navigation.drawer_closing = false;
            self.navigation.drawer_slide.retarget_with_velocity(0.0, vy);
        } else {
            self.navigation.drawer_closing = true;
            self.navigation
                .drawer_slide
                .retarget_with_velocity(h, vy.max(0.0));
        }
        self.needs_present = true;
    }

    pub(crate) fn open_drawer_animated(&mut self, from_y: Option<f32>, vy: f32) {
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
        self.navigation.search_query.clear();
        self.navigation.drawer_pull_y = 0.0;
        self.navigation.drawer_dragging = false;
        self.navigation.drawer_closing = false;
        self.navigation.drawer_slide.value = start;
        self.navigation.drawer_slide.retarget_with_velocity(0.0, vy);
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

    pub(crate) fn recents_layers_active(&self) -> bool {
        (self.foreground == Foreground::Shell
            && self.nav.current == de_ui::nav::Screen::Recents
            && self.morph.is_none()
            && self.navigation.recents_fade.is_none())
            || self.navigation.nav_recents_active
    }

    /// Prepare cached GPU textures for the Recents carousel so horizontal swipes
    /// and card-dismiss drags composite at 60 FPS with zero per-frame CPU raster.
    pub(crate) fn recents_layers_prepare(&mut self, size: Size, minute: u64) {
        let windows = self.recent_windows();
        let model = self.recents_model(&windows);
        let entries = model.entries();
        let count = entries.len();
        self.navigation.recents_pager.set_pages(count.max(1));

        let bg_key = (size, count == 0, count > 1, minute);
        let mut old_cards = self
            .navigation
            .recents_layers
            .as_mut()
            .map(|l| std::mem::take(&mut l.cards))
            .unwrap_or_default();

        let card_size = de_ui::recents::card_rect(size).size();
        let mut cards = Vec::with_capacity(count);
        for e in entries {
            let snap_ptr = e.snapshot.as_ref().map(|a| std::sync::Arc::as_ptr(a) as usize);
            if let Some(pos) = old_cards
                .iter()
                .position(|c| c.id == e.id && c.snap_ptr == snap_ptr)
            {
                cards.push(old_cards.swap_remove(pos));
            } else {
                let px = de_ui::recents::render_card_tile(&self.theme, e, card_size);
                cards.push(RecentsCardCache {
                    id: e.id.clone(),
                    snap_ptr,
                    px: Some(px),
                    tex: None,
                });
            }
        }

        let need_bg = match self.navigation.recents_layers.as_ref() {
            Some(l) => l.bg_key != bg_key || (l.bg_px.is_none() && l.bg_tex.is_none()),
            None => true,
        };
        if need_bg {
            let (status, _) = self.shell_meta();
            let bg_px = de_ui::recents::render_backdrop(
                size,
                &self.strings,
                &self.theme,
                &status,
                self.navigation.recents_backdrop.as_ref(),
                count == 0,
                count > 1,
            );
            self.navigation.recents_layers = Some(RecentsLayers {
                bg_key,
                bg_px: Some(bg_px),
                bg_tex: None,
                cards,
            });
        } else if let Some(l) = self.navigation.recents_layers.as_mut() {
            l.cards = cards;
        }
        self.shell_minute = minute;
        self.shell_dirty = false;
    }

    pub(crate) fn poll_recents_backdrop(&mut self) {
        let Some(rx) = self.navigation.recents_blur_rx.take() else {
            return;
        };
        match rx.try_recv() {
            Ok(px) => {
                self.navigation.recents_backdrop = Some(px);
                if let Some(layers) = self.navigation.recents_layers.as_mut() {
                    layers.bg_px = None;
                    layers.bg_tex = None;
                }
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

impl Navigation {
    /// Output size changed (rotation): drop every size-bound cache.
    pub(crate) fn on_resize(&mut self) {
        self.back = None;
        self.back_texture = None;
        self.back_key = None;
        self.recents_backdrop = None;
        self.recents_blur_rx = None;
        self.home_underlay_px = None;
        self.home_underlay_tex = None;
        self.recents_layers = None;
        self.drawer_pull_y = 0.0;
        self.drawer_dragging = false;
    }
}
