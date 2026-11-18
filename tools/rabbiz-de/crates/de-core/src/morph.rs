//! Runtime ownership of the launcher morph and its app handoff.

use std::time::{Duration, Instant};

use de_api::{Pixels, Rect, Size};
use de_design::motion::{FormFactor, APP_CONTENT_FADE_MS};
use de_paint::animation::AnimationClock;
use de_ui::launch_morph::{MorphAnimator, MorphPose};
use de_ui::nav::Screen;
use smithay::backend::renderer::element::solid::SolidColorBuffer;
use smithay::desktop::Window;

use crate::state::{De, Foreground};

#[derive(Clone)]
pub(crate) struct LaunchOrigin {
    pub rect: Rect,
    pub icon: Pixels,
    pub screen: Screen,
    pub home_page: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    Opening,
    Closing,
    Dragging,
    Returning,
    Recents,
}

pub(crate) struct WindowMorph {
    pub phase: Phase,
    pub origin: LaunchOrigin,
    pub window: Option<Window>,
    pub icon_texture: Option<Box<dyn std::any::Any>>,
    pub backing: SolidColorBuffer,
    pub backdrop: Option<Pixels>,
    pub backdrop_texture: Option<Box<dyn std::any::Any>>,
    animator: MorphAnimator,
    ready_at: Option<Instant>,
    content_fade: Duration,
    last_frame: Option<Instant>,
    last_drag_ms: Option<u64>,
    navigation: bool,
}

impl WindowMorph {
    fn new(
        origin: LaunchOrigin,
        window: Option<Window>,
        phase: Phase,
        form_factor: FormFactor,
    ) -> Self {
        let opening = phase == Phase::Opening;
        let mut animator = MorphAnimator::new(if opening { 0.0 } else { 1.0 });
        if phase != Phase::Dragging {
            animator.retarget(opening);
        }
        let color = de_design::DEFAULT.surface1.map(|v| f32::from(v) / 255.0);
        Self {
            phase,
            origin,
            window,
            icon_texture: None,
            backing: SolidColorBuffer::new((1, 1), color),
            backdrop: None,
            backdrop_texture: None,
            animator,
            ready_at: None,
            content_fade: Duration::from_millis(form_factor.scale_ms(APP_CONTENT_FADE_MS).into()),
            last_frame: None,
            last_drag_ms: None,
            navigation: phase == Phase::Dragging,
        }
    }

    pub fn tick(&mut self, now: Instant, clock: &AnimationClock) -> bool {
        let delta = self
            .last_frame
            .replace(now)
            .map_or(0.0, |last| now.duration_since(last).as_secs_f32());
        if self.phase == Phase::Dragging {
            return false;
        }
        self.animator.advance(delta, clock);
        self.animator.settled() && (self.phase != Phase::Opening || self.content_ready(now) >= 1.0)
    }

    fn content_ready(&self, now: Instant) -> f32 {
        if self.phase == Phase::Opening {
            self.ready_at.map_or(0.0, |ready| {
                (now.saturating_duration_since(ready).as_secs_f32()
                    / self.content_fade.as_secs_f32())
                .min(1.0)
            })
        } else if self.window.is_some() {
            1.0
        } else {
            0.0
        }
    }

    pub fn pose(&self, dst: Rect, now: Instant) -> MorphPose {
        let progress = self.animator.progress();
        let mut pose = MorphPose::at(progress, self.origin.rect, dst, self.content_ready(now));
        if self.navigation {
            pose.card = de_ui::launch_morph::navigation_card(progress, self.origin.rect, dst);
            pose.icon.x = pose.card.x + (pose.card.w - pose.icon.w) / 2;
            pose.icon.y = pose.card.y + (pose.card.h - pose.icon.h) / 2;
            pose.app_alpha = (progress * 2.0).clamp(0.0, 1.0);
            pose.icon_alpha = 1.0 - pose.app_alpha;
        }
        pose
    }

    pub fn backdrop_alpha(&self) -> f32 {
        ((1.0 - self.animator.progress()) * 3.0).clamp(0.0, 1.0)
    }

    /// The render can return `EmptyFrame` near an endpoint. Wake even without a flip.
    pub fn next_frame_at(&self, interval: Duration) -> Option<Instant> {
        if self.phase == Phase::Dragging
            || (self.animator.settled() && self.phase == Phase::Opening && self.ready_at.is_none())
        {
            return None;
        }
        self.last_frame.map(|last| last + interval)
    }

    fn drag(&mut self, progress: f32, time_ms: u64) {
        let progress = progress.clamp(0.0, 1.0);
        let velocity = self
            .last_drag_ms
            .replace(time_ms)
            .filter(|last| time_ms > *last)
            .map_or(0.0, |last| {
                (progress - self.animator.progress()) * 1000.0 / (time_ms - last) as f32
            });
        self.animator.track(progress, velocity);
    }

    fn release(&mut self, close: bool) {
        self.phase = if close {
            Phase::Closing
        } else {
            Phase::Returning
        };
        self.animator.retarget(!close);
    }
}

impl De {
    pub(crate) fn begin_launch_morph(&mut self, desktop_id: &str, rect: Rect) {
        let icon = self.app_icon(desktop_id);
        let origin = LaunchOrigin {
            rect,
            icon: de_ui::launch_morph::morph_icon(&de_design::DEFAULT, &icon, rect),
            screen: self.nav.current,
            home_page: self.home_page,
        };
        self.morph = Some(WindowMorph::new(
            origin,
            None,
            Phase::Opening,
            self.motion_clock.form_factor(),
        ));
    }

    pub(crate) fn first_app_frame(&mut self, window: Window) {
        let app_id = window.toplevel().map(crate::snapshots::window_app_id);
        let launched = self.launch.on_toplevel_mapped(app_id.as_deref()).is_some();
        if let Some(morph) = self.morph.as_mut() {
            if launched && morph.phase == Phase::Opening && morph.window.is_none() {
                self.space.raise_element(&window, true);
                window
                    .user_data()
                    .insert_if_missing(|| morph.origin.clone());
                morph.window = Some(window);
                morph.ready_at = Some(Instant::now());
            }
            return;
        }
        self.space.raise_element(&window, true);
        self.foreground = Foreground::App;
        self.focus_foreground();
    }

    fn origin_for(&self, window: &Window) -> Option<LaunchOrigin> {
        let origin = window.user_data().get::<LaunchOrigin>()?;
        (origin.screen == self.nav.current
            && (origin.screen != Screen::Home || origin.home_page == self.home_page))
            .then(|| origin.clone())
    }

    /// Find the current home destination even when the app was launched
    /// from search, another page, an external client, or the dock.
    fn home_origin(&mut self, window: &Window) -> LaunchOrigin {
        use de_ui::home::{self, HomeHit};
        let size = Size::new(self.output.width as i32, self.output.height as i32);
        let app_id = window.toplevel().map(crate::snapshots::window_app_id).unwrap_or_default();
        let matches = |id: &str| id.trim_end_matches(".desktop") == app_id.trim_end_matches(".desktop");
        let prefs = self.app_prefs.prefs();
        let dock = self.catalog.dock(prefs);
        let grid = self.catalog.home_order(prefs);
        let dock_hit = dock.iter().position(|e| matches(&e.id));
        let grid_hit = grid.iter().enumerate()
            .skip(self.home_page * home::PAGE_SIZE).take(home::PAGE_SIZE)
            .find(|(_, e)| matches(&e.id)).map(|(i, _)| i % home::PAGE_SIZE);
        let hit = dock_hit.map(HomeHit::Dock).or_else(|| grid_hit.map(HomeHit::Grid));
        let rect = hit.and_then(|h| home::icon_rect(size, h, dock.len()))
            .unwrap_or(Rect::new((size.w - 112) / 2, size.h - 260, 112, 112));
        let icon = window.user_data().get::<LaunchOrigin>().map(|o| o.icon.clone())
            .unwrap_or_else(|| {
                let icon = self.app_icon(&app_id);
                de_ui::launch_morph::morph_icon(&de_design::DEFAULT, &icon, rect)
            });
        LaunchOrigin { rect, icon, screen: Screen::Home, home_page: self.home_page }
    }

    pub(crate) fn follow_home_drag(&mut self, dy: f32, time_ms: u64) {
        if self.morph.is_none() && dy < -12.0 && self.foreground == Foreground::App {
            if let Some(window) = self.space.elements().last().cloned() {
                let origin = self.home_origin(&window);
                self.nav.current = Screen::Home;
                // Always render Home here; shell_px may still contain the
                // drawer/search screen from which the app was launched.
                let home = self.shell_pixels();
                let backdrop = (!self.motion_clock.motion_restricted())
                    .then(|| de_paint::glass::blurred_backdrop(&home));
                self.shell_px = Some(home);
                self.shell_tex = None;
                self.shell_dirty = false;
                let mut morph = WindowMorph::new(origin, Some(window), Phase::Dragging,
                    self.motion_clock.form_factor());
                morph.backdrop = backdrop;
                self.morph = Some(morph);
            }
        }
        if let Some(morph) = self.morph.as_mut().filter(|m| m.phase == Phase::Dragging) {
            morph.drag(1.0 + dy / self.output.height.max(1) as f32, time_ms);
            self.needs_present = true;
        }
    }

    pub(crate) fn release_home_drag(&mut self, close: bool) -> bool {
        let Some(morph) = self.morph.as_mut().filter(|m| m.phase == Phase::Dragging) else {
            return false;
        };
        morph.release(close);
        self.needs_present = true;
        true
    }

    pub(crate) fn release_home_to_recents(&mut self) -> bool {
        let Some(morph) = self.morph.as_mut().filter(|m| m.phase == Phase::Dragging) else { return false; };
        morph.phase = Phase::Recents;
        morph.animator.settle_recents();
        self.needs_present = true;
        true
    }

    /// Close the last app after its window disappears. Keep the icon as the
    /// fallback visual for clients that have already released their buffers.
    pub(crate) fn app_disappeared(&mut self, window: &Window) {
        if self.morph.as_ref().and_then(|m| m.window.as_ref()) == Some(window) {
            self.morph = None;
        } else if self.foreground == Foreground::App && self.space.elements().next().is_none() {
            if let Some(origin) = self.origin_for(window) {
                self.morph = Some(WindowMorph::new(
                    origin,
                    None,
                    Phase::Closing,
                    self.motion_clock.form_factor(),
                ));
            }
        }
    }

    pub(crate) fn advance_morph(&mut self, now: Instant) {
        let Some(morph) = self.morph.as_mut() else {
            return;
        };
        if morph.tick(now, &self.motion_clock) {
            self.foreground = match morph.phase {
                Phase::Opening | Phase::Returning => {
                    if let Some(window) = &morph.window {
                        self.space.raise_element(window, true);
                    }
                    Foreground::App
                }
                Phase::Recents => {
                    self.nav.current = Screen::Recents;
                    self.shell_dirty = true;
                    Foreground::Shell
                }
                Phase::Closing | Phase::Dragging => Foreground::Shell,
            };
            self.morph = None;
            self.focus_foreground();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opening_waits_for_content_then_crossfades() {
        let origin = LaunchOrigin {
            rect: Rect::new(40, 45, 96, 96),
            icon: Pixels::empty(de_api::Size::new(96, 96)),
            screen: Screen::Home,
            home_page: 0,
        };
        let mut morph = WindowMorph::new(origin, None, Phase::Opening, FormFactor::Handset);
        let clock = AnimationClock::new();
        let now = Instant::now();
        let output = Rect::new(0, 0, 720, 1600);
        assert!(!morph.tick(now, &clock));
        let waiting = now + Duration::from_millis(520);
        assert!(!morph.tick(waiting, &clock));
        let pose = morph.pose(output, waiting);
        assert_eq!(pose.card, output);
        assert_eq!((pose.icon_alpha, pose.app_alpha), (1.0, 0.0));
        assert_eq!(morph.next_frame_at(Duration::from_millis(16)), None);

        morph.ready_at = Some(waiting);
        let half = waiting + morph.content_fade / 2;
        assert!(!morph.tick(half, &clock));
        assert_eq!(morph.pose(output, half).app_alpha, 0.5);
        assert!(morph.tick(waiting + morph.content_fade, &clock));
    }
}
