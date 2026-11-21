//! Touch handling, toolbar/suggestion/sheet hit-testing, alternates chooser, and gesture dispatch.

use osk_core::gesture::{GestureEvent, TouchEvent};
use osk_core::layout::{KeyAction, KeyPos, BOTTOM_BAR_HEIGHT_PX};
use osk_render::{
    bottom_button_rect, sheet_row_rect, suggestion_rect, toolbar_button_rect, TOOLBAR_ICONS,
};

use smithay_client_toolkit::reexports::client::{
    protocol::{wl_surface, wl_touch},
    Connection, QueueHandle,
};
use smithay_client_toolkit::{delegate_touch, seat::touch::TouchHandler};

use crate::app::App;
use crate::feedback::FeedbackKind;

pub(crate) struct AltState {
    pub(crate) key: KeyPos,
    pub(crate) items: Vec<String>,
    pub(crate) selected: usize,
}

impl App {
    /// Toolbar buttons live in the strip, above the keys.
    fn on_toolbar_tap(&mut self, x: f32, y: f32) {
        let mut hit = None;
        for index in 0..TOOLBAR_ICONS.len() {
            let rect = toolbar_button_rect(index, self.keyboard.layout(), 1.0);
            if rect.contains(x, y) {
                hit = Some(index);
                break;
            }
        }
        match hit {
            Some(0) => {
                if !self.keyboard.privacy_policy().allow_clipboard_strip() {
                    return;
                }
                let items = self.clipboard.lock().map(|g| g.clone()).unwrap_or_default();
                self.sheet = Some(items);
                self.draw();
                return;
            }
            Some(1) => {
                eprintln!("rabbiz-osk: voice typing is not available on this device");
                return;
            }
            _ => {}
        }
        if self.keyboard.is_panel() {
            return;
        }
        // Suggestion slots: replace the partial word and continue typing.
        let picks = self.keyboard.suggestions();
        for (index, pick) in picks.iter().enumerate() {
            let rect = suggestion_rect(index, self.keyboard.layout(), 1.0);
            if rect.contains(x, y) {
                let commands = self.keyboard.apply_suggestion(pick);
                self.send_commands(commands);
                self.draw();
                return;
            }
        }
    }

    /// Bottom accessory bar: left = dismiss keyboard, right = cycle language.
    fn on_bottom_bar_tap(&mut self, x: f32, y: f32) {
        let layout = self.keyboard.layout();
        if bottom_button_rect(0, layout, 1.0).contains(x, y) {
            self.feedback.trigger(FeedbackKind::Mod);
            self.dismissed_at = Some(std::time::Instant::now());
            self.hide();
        } else if bottom_button_rect(1, layout, 1.0).contains(x, y) {
            self.feedback.trigger(FeedbackKind::Mod);
            let commands = self.keyboard.cycle_language();
            self.send_commands(commands);
            self.sync_size();
            self.draw();
        }
    }

    fn apply_tap(&mut self, pos: KeyPos) {
        let commands = self.keyboard.press(pos, self.now_ms());
        self.send_commands(commands);
        self.sync_size();
        self.draw();
    }

    /// Index of the alternates slot under `x`, in the strip.
    fn alt_index_at(&self, x: f32) -> usize {
        let Some(alt) = &self.alt else { return 0 };
        let width = self.keyboard.layout().geometry.width_px;
        let margin = self.keyboard.layout().geometry.margin_px;
        let n = alt.items.len().max(1) as f32;
        let slot = (width - 2.0 * margin) / n;
        (((x - margin) / slot).floor() as i32).clamp(0, alt.items.len() as i32 - 1) as usize
    }

    fn commit_alternate(&mut self) {
        if let Some(alt) = self.alt.take() {
            if let Some(item) = alt.items.get(alt.selected).cloned() {
                let commands = self.keyboard.insert_text(&item);
                self.send_commands(commands);
            }
        }
        self.draw();
    }

    pub(crate) fn handle_gestures(&mut self, events: Vec<GestureEvent>) {
        for event in events {
            match event {
                GestureEvent::Press(pos) => {
                    if !self.pressed.contains(&pos) {
                        self.pressed.push(pos);
                        let kind = match self.keyboard.key(pos).map(|k| &k.action) {
                            Some(
                                KeyAction::Backspace
                                | KeyAction::Return
                                | KeyAction::Space
                                | KeyAction::Shift
                                | KeyAction::Layer(_)
                                | KeyAction::Language
                                | KeyAction::Panel(_)
                                | KeyAction::PanelBack,
                            ) => FeedbackKind::Mod,
                            _ => FeedbackKind::Key,
                        };
                        self.feedback.trigger(kind);
                    }
                    self.draw();
                }
                GestureEvent::Cancel(pos) => {
                    // A long-press chooser stays open until the finger lifts.
                    if self.alt.is_none() {
                        self.pressed.retain(|&p| p != pos);
                        self.draw();
                    }
                }
                GestureEvent::Tap(pos) => {
                    self.pressed.retain(|&p| p != pos);
                    self.alt = None;
                    self.apply_tap(pos);
                }
                GestureEvent::LongPress(pos) => {
                    let items = self.keyboard.alternates(pos);
                    if items.is_empty() {
                        continue;
                    }
                    self.feedback.trigger(FeedbackKind::Mod);
                    self.alt = Some(AltState {
                        key: pos,
                        items,
                        selected: 0,
                    });
                    self.draw();
                }
                GestureEvent::Repeat(pos) => {
                    // When surrounding_text is tracked, stop at start so GTK does not assert on
                    // negative positions. When no surrounding_text is available (e.g. terminals or
                    // shell search), allow held Backspace repeat via KEY_BACKSPACE / shell bridge.
                    let backspace = matches!(
                        self.keyboard.key(pos).map(|k| &k.action),
                        Some(KeyAction::Backspace)
                    );
                    if backspace && (!self.has_surrounding_text || !self.keyboard.text().is_empty())
                    {
                        self.feedback.trigger(FeedbackKind::Repeat);
                        let commands = self.keyboard.backspace();
                        self.send_commands(commands);
                        self.draw();
                    }
                }
                GestureEvent::Swipe { key, dx, .. } => {
                    self.pressed.retain(|&p| p != key);
                    if self.keyboard.is_space(key) {
                        let step = self.keyboard.layout().geometry.width_px / 10.0;
                        let steps = (dx / step).round() as i32;
                        let commands = self.keyboard.space_swipe(steps);
                        self.send_commands(commands);
                    }
                    self.draw();
                }
            }
        }
    }
}

impl TouchHandler for App {
    fn down(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_touch::WlTouch,
        _serial: u32,
        time: u32,
        _: wl_surface::WlSurface,
        id: i32,
        pos: (f64, f64),
    ) {
        self.touch_pos.insert(id, (pos.0 as f32, pos.1 as f32));
        if self.alt.is_some() {
            return;
        }
        if let Some(items) = self.sheet.clone() {
            if !self.keyboard.privacy_policy().allow_clipboard_strip() {
                self.sheet = None;
                self.draw();
                return;
            }
            for (index, item) in items.iter().enumerate() {
                let rect = sheet_row_rect(index, self.keyboard.layout(), 1.0);
                if pos.1 as f32 >= rect.y && pos.1 as f32 <= rect.y + rect.h {
                    let commands = self.keyboard.insert_text(item);
                    self.send_commands(commands);
                    self.sheet = None;
                    self.draw();
                    return;
                }
            }
            self.sheet = None;
            self.draw();
            return;
        }
        let strip_h = self.keyboard.layout().geometry.strip_height_px;
        if (pos.1 as f32) <= strip_h {
            self.strip_touches.insert(id);
            self.on_toolbar_tap(pos.0 as f32, pos.1 as f32);
            return;
        }
        let bottom_y = self.keyboard.layout().height_px() - BOTTOM_BAR_HEIGHT_PX;
        if (pos.1 as f32) >= bottom_y {
            self.strip_touches.insert(id);
            self.on_bottom_bar_tap(pos.0 as f32, pos.1 as f32);
            return;
        }
        let hit = self
            .keyboard
            .layout()
            .hit_test(&self.layer_name(), pos.0 as f32, pos.1 as f32);
        if let Some(key) = hit {
            if self.space_touch.is_none() && self.keyboard.is_space(key) {
                self.space_touch = Some((id, key));
                self.scrubber.on_down(pos.0 as f32);
            }
        }
        // Use our own monotonic clock: wl_touch timestamps come from the compositor and the
        // long-press/repeat tick in `GestureDetector` compares against `now_ms()`.
        let _ = time;
        let events = self.gesture.handle(
            TouchEvent::down(id as u64, pos.0 as f32, pos.1 as f32, self.now_ms()),
            hit,
        );
        self.handle_gestures(events);
    }

    fn motion(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_touch::WlTouch,
        _time: u32,
        id: i32,
        pos: (f64, f64),
    ) {
        self.touch_pos.insert(id, (pos.0 as f32, pos.1 as f32));
        if self.strip_touches.contains(&id) {
            return;
        }
        if self.alt.is_some() {
            let index = self.alt_index_at(pos.0 as f32);
            if let Some(alt) = &mut self.alt {
                if alt.selected != index {
                    alt.selected = index;
                    self.draw();
                }
            }
            return;
        }
        if let Some((sid, _)) = self.space_touch {
            if sid == id {
                let steps = self.scrubber.on_move(pos.0 as f32);
                for step in steps {
                    let commands = self.keyboard.space_swipe(step.delta());
                    self.send_commands(commands);
                }
                if self.scrubber.is_scrubbing() {
                    return;
                }
            }
        }
        let hit = self
            .keyboard
            .layout()
            .hit_test(&self.layer_name(), pos.0 as f32, pos.1 as f32);
        let events = self.gesture.handle(
            TouchEvent::mv(id as u64, pos.0 as f32, pos.1 as f32, self.now_ms()),
            hit,
        );
        self.handle_gestures(events);
    }

    fn up(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_touch::WlTouch,
        _serial: u32,
        time: u32,
        id: i32,
    ) {
        let pos = self.touch_pos.remove(&id).unwrap_or((0.0, 0.0));
        if self.strip_touches.remove(&id) {
            return;
        }
        if self.alt.is_some() {
            self.commit_alternate();
            self.gesture.cancel_all(self.now_ms());
            return;
        }
        if let Some((sid, space_key)) = self.space_touch {
            if sid == id {
                self.space_touch = None;
                if self.scrubber.on_up() {
                    let _ = self
                        .gesture
                        .handle(TouchEvent::cancel(id as u64, self.now_ms()), None);
                    self.pressed.retain(|&p| p != space_key);
                    self.draw();
                    return;
                }
            }
        }
        let hit = self
            .keyboard
            .layout()
            .hit_test(&self.layer_name(), pos.0, pos.1);
        let _ = time;
        let events = self
            .gesture
            .handle(TouchEvent::up(id as u64, pos.0, pos.1, self.now_ms()), hit);
        self.handle_gestures(events);
    }

    fn shape(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_touch::WlTouch,
        _: i32,
        _: f64,
        _: f64,
    ) {
    }
    fn orientation(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_touch::WlTouch,
        _: i32,
        _: f64,
    ) {
    }
    fn cancel(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_touch::WlTouch) {
        self.touch_pos.clear();
        self.strip_touches.clear();
        self.space_touch = None;
        self.scrubber.reset();
        if self.alt.is_some() {
            self.alt = None;
            self.draw();
            return;
        }
        let events = self.gesture.cancel_all(self.now_ms());
        self.handle_gestures(events);
    }
}

delegate_touch!(App);
