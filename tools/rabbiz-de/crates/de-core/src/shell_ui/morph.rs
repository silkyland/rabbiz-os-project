//! Runtime ownership of the launcher morph and its app handoff.

use std::sync::{mpsc, Arc};
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

/// Recents preview during a held navigation drag (Android "swipe up and
/// hold"): the card springs to the overview size while the finger rests,
/// then springs back under the finger when it moves on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Peek {
    Off,
    Holding,
    CatchingUp,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    Opening,
    Closing,
    Dragging,
    Returning,
    Recents,
    Restoring,
}

pub(crate) struct WindowMorph {
    pub phase: Phase,
    pub origin: LaunchOrigin,
    pub window: Option<Window>,
    pub icon_texture: Option<Box<dyn std::any::Any>>,
    pub backing: SolidColorBuffer,
    pub backdrop: Option<Pixels>,
    pub backdrop_texture: Option<Box<dyn std::any::Any>>,
    pub backdrop_rx: Option<mpsc::Receiver<Pixels>>,
    pub snapshot: Option<Arc<Pixels>>,
    pub snapshot_texture: Option<Box<dyn std::any::Any>>,
    animator: MorphAnimator,
    ready_at: Option<Instant>,
    content_fade: Duration,
    last_frame: Option<Instant>,
    last_drag_ms: Option<u64>,
    navigation: bool,
    drag_offset_x: f32,
    peek: Peek,
}

impl WindowMorph {
    fn new(
        origin: LaunchOrigin,
        window: Option<Window>,
        phase: Phase,
        form_factor: FormFactor,
    ) -> Self {
        let opening = matches!(phase, Phase::Opening | Phase::Restoring);
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
            backdrop_rx: None,
            snapshot: None,
            snapshot_texture: None,
            animator,
            ready_at: None,
            content_fade: Duration::from_millis(form_factor.scale_ms(APP_CONTENT_FADE_MS).into()),
            last_frame: None,
            last_drag_ms: None,
            navigation: phase == Phase::Dragging,
            drag_offset_x: 0.0,
            peek: Peek::Off,
        }
    }

    pub fn tick(&mut self, now: Instant, clock: &AnimationClock) -> bool {
        let delta = self
            .last_frame
            .replace(now)
            .map_or(0.0, |last| now.duration_since(last).as_secs_f32());
        if self.phase == Phase::Dragging {
            if self.peek == Peek::Holding {
                self.drag_offset_x *= (-15.0 * delta.max(0.0)).exp();
            }
            if self.peek != Peek::Off {
                self.animator.advance(delta, clock);
            }
            return false;
        }
        self.drag_offset_x *= (-15.0 * delta.max(0.0)).exp();
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
            let overview = de_ui::recents::card_rect(dst.size());
            if progress >= 0.5 {
                pose.card = de_ui::launch_morph::navigation_card(progress, self.origin.rect, dst);
            } else {
                // Keep closing/minimizing centered around the overview card rather
                // than separating off toward the home-screen icon rect.
                let t = (progress * 2.0).clamp(0.0, 1.0);
                let scale = 0.72 + 0.28 * t;
                let w = ((overview.w as f32) * scale).round().max(1.0) as i32;
                let h = ((overview.h as f32) * scale).round().max(1.0) as i32;
                let cx = overview.x + overview.w / 2;
                let cy = overview.y + overview.h / 2;
                pose.card = Rect::new(cx - w / 2, cy - h / 2, w, h);
            }
            pose.icon.x = pose.card.x + (pose.card.w - pose.icon.w) / 2;
            pose.icon.y = pose.card.y + (pose.card.h - pose.icon.h) / 2;
            pose.icon_alpha = 0.0;
            if self.phase == Phase::Dragging {
                pose.app_alpha = 1.0;
            } else {
                pose.app_alpha = (progress * 2.0).clamp(0.0, 1.0);
            }
        }
        if self.phase == Phase::Restoring {
            pose.icon_alpha = 0.0;
            pose.app_alpha = if self.snapshot.is_some() {
                ((progress - 0.72) / (1.0 - 0.72)).clamp(0.0, 1.0)
            } else {
                1.0
            };
            pose.card_alpha = if self.snapshot.is_some() { 0.0 } else { 1.0 };
        } else if self.phase == Phase::Recents && self.snapshot.is_some() {
            pose.app_alpha = 0.0;
            pose.card_alpha = 0.0;
            pose.icon_alpha = 0.0;
        }
        pose
    }

    pub fn snapshot_alpha(&self) -> f32 {
        if self.phase == Phase::Restoring {
            1.0 - ((self.animator.progress() - 0.84) / (1.0 - 0.84)).clamp(0.0, 1.0)
        } else {
            1.0
        }
    }

    pub fn overview_window(&self) -> Option<&Window> {
        matches!(self.phase, Phase::Recents | Phase::Restoring)
            .then_some(self.window.as_ref())
            .flatten()
    }

    pub fn reuse_window(&mut self, window: Window) {
        self.window = Some(window);
        self.ready_at = Some(Instant::now());
    }

    pub fn backdrop_alpha(&self) -> f32 {
        ((1.0 - self.animator.progress()) * 3.0).clamp(0.0, 1.0)
    }

    /// The render can return `EmptyFrame` near an endpoint. Wake even without a flip.
    pub fn next_frame_at(&self, interval: Duration) -> Option<Instant> {
        if (self.phase == Phase::Dragging && self.peek == Peek::Off)
            || (self.animator.settled() && self.phase == Phase::Opening && self.ready_at.is_none())
        {
            return None;
        }
        self.last_frame.map(|last| last + interval)
    }

    /// Start or leave the Recents preview of a held navigation drag.
    pub fn set_preview(&mut self, on: bool) {
        if self.phase != Phase::Dragging {
            return;
        }
        if on {
            self.peek = Peek::Holding;
            self.animator.settle_recents();
        } else if self.peek == Peek::Holding {
            self.peek = Peek::CatchingUp;
        }
    }

    /// The preview spring still moves (the frame loop must keep drawing).
    pub fn previewing(&self) -> bool {
        self.phase == Phase::Dragging && self.peek != Peek::Off && !self.animator.settled()
    }

    fn drag(&mut self, progress: f32, time_ms: u64) {
        let progress = progress.clamp(0.0, 1.0);
        match self.peek {
            Peek::Holding => {
                self.last_drag_ms = Some(time_ms);
                return;
            }
            Peek::CatchingUp => {
                self.last_drag_ms = Some(time_ms);
                if (self.animator.progress() - progress).abs() > 0.01 {
                    self.animator.chase(progress);
                    return;
                }
                self.peek = Peek::Off;
            }
            Peek::Off => {}
        }
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
        self.peek = Peek::Off;
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
            icon: de_ui::launch_morph::morph_icon(&self.theme, &icon, rect),
            screen: self.nav.current,
            home_page: self.home_page,
        };
        let color = self.theme.surface1.map(|v| f32::from(v) / 255.0);
        let mut morph = WindowMorph::new(
            origin,
            None,
            Phase::Opening,
            self.motion_clock.form_factor(),
        );
        morph.backing =
            smithay::backend::renderer::element::solid::SolidColorBuffer::new((1, 1), color);
        self.morph = Some(morph);
    }

    pub(crate) fn first_app_frame(&mut self, window: Window) {
        let app_id = window.toplevel().map(crate::snapshots::window_app_id);
        if let Some(id) = app_id.as_deref() {
            self.lifecycle_on_mapped(id);
        }
        let launched = self.launch.on_toplevel_mapped(app_id.as_deref()).is_some();
        if let Some(morph) = self.morph.as_mut() {
            if launched && morph.phase == Phase::Opening && morph.window.is_none() {
                self.space.raise_element(&window, true);
                window
                    .user_data()
                    .insert_if_missing(|| morph.origin.clone());
                morph.window = Some(window);
                morph.ready_at = Some(Instant::now());
                return;
            }
            if morph.phase == Phase::Opening {
                return;
            }
        }
        if self.foreground == Foreground::App {
            if let Some(previous) = self.recent_windows().into_iter().find(|w| w != &window) {
                self.capture_recent_window(&previous);
            }
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
        let app_id = window
            .toplevel()
            .map(crate::snapshots::window_app_id)
            .unwrap_or_default();
        let matches =
            |id: &str| id.trim_end_matches(".desktop") == app_id.trim_end_matches(".desktop");
        let prefs = self.app_prefs.prefs();
        let dock = self.catalog.dock(prefs);
        let grid = self.catalog.home_order(prefs);
        let dock_hit = dock.iter().position(|e| matches(&e.id));
        let grid_hit = grid
            .iter()
            .enumerate()
            .skip(self.home_page * home::PAGE_SIZE)
            .take(home::PAGE_SIZE)
            .find(|(_, e)| matches(&e.id))
            .map(|(i, _)| i % home::PAGE_SIZE);
        let hit = dock_hit
            .map(HomeHit::Dock)
            .or_else(|| grid_hit.map(HomeHit::Grid));
        let rect = hit
            .and_then(|h| home::icon_rect(size, h, dock.len()))
            .unwrap_or(Rect::new((size.w - 112) / 2, size.h - 260, 112, 112));
        let icon = window
            .user_data()
            .get::<LaunchOrigin>()
            .map(|o| o.icon.clone())
            .unwrap_or_else(|| {
                let icon = self.app_icon(&app_id);
                de_ui::launch_morph::morph_icon(&self.theme, &icon, rect)
            });
        LaunchOrigin {
            rect,
            icon,
            screen: Screen::Home,
            home_page: self.home_page,
        }
    }

    pub(crate) fn follow_home_drag(&mut self, _dx: f32, dy: f32, time_ms: u64) {
        if self.motion_clock.motion_restricted() || self.panels.shown() || self.locked() {
            return;
        }
        if self.morph.is_none() && dy < -12.0 && self.foreground == Foreground::App {
            if let Some(window) = self.recent_windows().into_iter().next() {
                let origin = self.home_origin(&window);
                let reuse_home = self.nav.current == Screen::Home && !self.shell_dirty;
                self.nav.current = Screen::Home;
                // Always render Home here; shell_px may still contain the
                // drawer/search screen from which the app was launched.
                let home = if reuse_home {
                    if let Some(px) = self.shell_px.as_ref() {
                        px.clone()
                    } else {
                        self.shell_pixels()
                    }
                } else {
                    self.shell_pixels()
                };
                if !reuse_home || self.shell_px.is_none() {
                    self.shell_px = Some(home.clone());
                    self.shell_tex = None;
                }
                self.shell_dirty = false;
                let mut morph = WindowMorph::new(
                    origin,
                    Some(window),
                    Phase::Dragging,
                    self.motion_clock.form_factor(),
                );
                if !self.motion_clock.motion_restricted() {
                    let (tx, rx) = mpsc::channel();
                    std::thread::spawn(move || {
                        let _ = tx.send(de_paint::glass::blurred_backdrop(&home));
                    });
                    morph.backdrop_rx = Some(rx);
                }
                self.morph = Some(morph);
            }
        }
        if let Some(morph) = self.morph.as_mut().filter(|m| m.phase == Phase::Dragging) {
            // Pulling up ~35% of screen height shrinks the card smoothly to
            // the centered Recents overview size (progress = 0.5). Clamp at 0.5
            // so over-dragging never separates off toward the closing icon, and
            // keep horizontal position centered (`drag_offset_x = 0.0`).
            let travel = (self.output.height.max(1) as f32 * 0.70).max(1.0);
            let progress = (1.0 + dy / travel).clamp(0.50, 1.0);
            morph.drag_offset_x = 0.0;
            morph.drag(progress, time_ms);
            self.needs_present = true;
        }
    }

    /// Recents preview on/off for the in-flight navigation drag.
    pub(crate) fn nav_preview(&mut self, on: bool) {
        if let Some(morph) = self.morph.as_mut().filter(|m| m.phase == Phase::Dragging) {
            morph.set_preview(on);
            self.needs_present = true;
        }
    }

    pub(crate) fn edge_back_to_home(&mut self, time_ms: u64) -> bool {
        self.follow_home_drag(0.0, -48.0, time_ms);
        let Some(morph) = self.morph.as_mut().filter(|m| m.phase == Phase::Dragging) else {
            return false;
        };
        morph.animator.track(0.92, -3.2);
        self.release_home_drag(true)
    }

    pub(crate) fn minimize_foreground(&mut self) {
        if !self.release_home_drag(true) && !self.edge_back_to_home(0) {
            self.show_shell(Screen::Home);
        }
    }

    pub(crate) fn release_home_drag(&mut self, minimize: bool) -> bool {
        if minimize {
            if let Some(window) = self
                .morph
                .as_ref()
                .filter(|m| m.phase == Phase::Dragging)
                .and_then(|m| m.window.clone())
            {
                self.capture_recent_window(&window);
                self.focus_surface(None, smithay::utils::SERIAL_COUNTER.next_serial());
            }
        }
        let Some(morph) = self.morph.as_mut().filter(|m| m.phase == Phase::Dragging) else {
            return false;
        };
        morph.release(minimize);
        self.needs_present = true;
        true
    }

    pub(crate) fn release_home_to_recents(&mut self) -> bool {
        let Some(window) = self
            .morph
            .as_ref()
            .filter(|m| m.phase == Phase::Dragging)
            .and_then(|m| m.window.clone())
        else {
            return false;
        };
        self.capture_recent_window(&window);
        self.reset_recents_pager();
        self.navigation.home_underlay_px = self.shell_px.clone();
        self.navigation.home_underlay_tex = None;
        let Some(morph) = self.morph.as_mut().filter(|m| m.phase == Phase::Dragging) else {
            return false;
        };
        self.navigation.recents_backdrop = morph.backdrop.take().or_else(|| self.shell_px.clone());
        self.navigation.recents_blur_rx = morph.backdrop_rx.take();
        let already_settled = morph.peek == Peek::Holding && morph.animator.settled();
        if already_settled {
            // Recents preview already settled during the hold — hand off
            // straight to Screen::Recents without replaying the settle animation.
            self.morph = None;
        } else {
            morph.snapshot = crate::snapshots::snapshot(&window);
            morph.peek = Peek::Off;
            morph.phase = Phase::Recents;
            morph.animator.settle_recents();
        }
        self.foreground = Foreground::Shell;
        self.focus_foreground();
        self.nav.current = Screen::Recents;
        self.shell_dirty = true;
        self.needs_present = true;
        true
    }

    pub(crate) fn restore_recent(&mut self, window: Window, card: Rect) {
        self.launch.cancel();
        let snapshot = crate::snapshots::snapshot(&window);
        if self.motion_clock.motion_restricted() {
            self.morph = None;
            self.show_app_window(&window);
            return;
        }
        let origin = LaunchOrigin {
            rect: card,
            icon: Pixels::empty(Size::new(1, 1)),
            screen: Screen::Recents,
            home_page: self.home_page,
        };
        let mut morph = WindowMorph::new(
            origin,
            Some(window),
            Phase::Restoring,
            self.motion_clock.form_factor(),
        );
        morph.snapshot = snapshot;
        self.morph = Some(morph);
        self.shell_dirty = true;
        self.needs_present = true;
    }

    pub(crate) fn show_app_window(&mut self, window: &Window) {
        self.space.raise_element(window, true);
        self.foreground = Foreground::App;
        self.focus_foreground();
        self.needs_present = true;
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
        if let Some(rx) = morph.backdrop_rx.take() {
            match rx.try_recv() {
                Ok(px) => {
                    morph.backdrop = Some(px);
                    morph.backdrop_texture = None;
                    self.needs_present = true;
                }
                Err(mpsc::TryRecvError::Empty) => {
                    morph.backdrop_rx = Some(rx);
                }
                Err(mpsc::TryRecvError::Disconnected) => {}
            }
        }
        let ticked = morph.tick(now, &self.motion_clock);
        if morph.previewing() {
            self.needs_present = true;
        }
        if ticked {
            self.foreground = match morph.phase {
                Phase::Opening | Phase::Returning | Phase::Restoring => {
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

    #[test]
    fn overview_settles_on_the_same_card_and_restore_starts_there() {
        let output = Rect::new(0, 0, 720, 1600);
        let card = de_ui::recents::card_rect(output.size());
        let origin = LaunchOrigin {
            rect: Rect::new(40, 440, 96, 96),
            icon: Pixels::empty(Size::new(96, 96)),
            screen: Screen::Home,
            home_page: 0,
        };
        let now = Instant::now();
        let clock = AnimationClock::new();
        let mut morph =
            WindowMorph::new(origin.clone(), None, Phase::Dragging, FormFactor::Handset);
        morph.animator.track(0.68, -1.2);
        morph.phase = Phase::Recents;
        morph.snapshot = Some(Arc::new(Pixels::empty(Size::new(1, 1))));
        morph.animator.settle_recents();
        assert!(morph.pose(output, now).card.w > card.w);
        for frame in 0..60 {
            morph.tick(now + Duration::from_millis(frame * 16), &clock);
        }
        assert_eq!(morph.pose(output, now).card, card);
        let mut restore = WindowMorph::new(
            LaunchOrigin {
                rect: card,
                ..origin
            },
            None,
            Phase::Restoring,
            FormFactor::Handset,
        );
        restore.snapshot = morph.snapshot;
        assert_eq!(restore.pose(output, now).card, card);
        assert_eq!(restore.snapshot_alpha(), 1.0);
        restore.tick(now, &clock);
        assert!(restore.tick(now + Duration::from_millis(600), &clock));
        assert_eq!(restore.pose(output, now).card, output);
        assert_eq!(restore.pose(output, now).app_alpha, 1.0);
        assert_eq!(restore.snapshot_alpha(), 0.0);
    }
}
