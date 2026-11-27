//! rabbiz-dialer — Phone app for RabbizOS (Rust/native Wayland build).
//!
//! Layout and behaviour mirror the removed shell-owned QML dialer
//! (`shell/apps/dialer/Dialer.qml`, git history): Keypad / Recents / Contacts tabs,
//! T9 sub-labels, `3-3-4` number grouping, caller-ID matching, and the
//! same simulated call progression until a real telephony backend exists
//! (`backend.rs` documents the ModemManager/`mmcli` path).

mod call_ui;
mod chrome;
mod input;
mod keypad;
mod lists;

use std::path::PathBuf;
use std::time::Instant;

use rabbiz_app_kit::{AppEvent, AppFrame, AppState, Canvas, Rect, TextInputCtl, Theme};

use crate::backend::{self, Backend, BackendKind};
use crate::contacts;
use crate::model::{self, CallDir, CallSession, Contact, Recents};

pub(super) const MAX_DIGITS: usize = 24;
/// Release "pop" duration — inside the ≤600 ms motion budget, near the
/// `de_design::motion::DURATIONS_MS` 160/240 ladder.
pub(super) const POP_S: f32 = 0.18;
/// Pressed-state shrink derived from `de_design::motion::PRESS_SCALE`.
pub(super) const PRESS_SHRINK: f32 = 1.0 - de_design::motion::PRESS_SCALE;

// Row metrics (pitch includes the gap).
pub(super) const ROW_PITCH: i32 = 92;
pub(super) const ROW_H: i32 = 88;

/// Visible pages — M3 bottom-navigation destinations. The keypad is not a
/// tab: it is a bottom-sheet overlay opened from the dialpad FAB, like the
/// AOSP Phone app where the call log is the hero surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Tab {
    Favorites,
    Recents,
    Contacts,
}

impl Tab {
    pub(super) const ALL: [Tab; 3] = [Tab::Favorites, Tab::Recents, Tab::Contacts];
    pub(super) fn label(self) -> &'static str {
        match self {
            Tab::Favorites => "Favorites",
            Tab::Recents => "Recents",
            Tab::Contacts => "Contacts",
        }
    }
    pub(super) fn icon(self) -> &'static str {
        match self {
            Tab::Favorites => rabbiz_app_kit::icons::STAR,
            Tab::Recents => rabbiz_app_kit::icons::HISTORY,
            Tab::Contacts => rabbiz_app_kit::icons::GROUP,
        }
    }
}

/// One visual unit in a scrolling list — section headers interleave with
/// data rows, so hit-testing and scrolling share a single geometry walk.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Row {
    /// Day/letter section header band.
    Header(String),
    /// Index into `recents.entries`.
    Recent(usize),
    /// Index into `contacts`.
    Contact(usize),
}

/// Section header band height (caption baseline + breathing room).
pub(super) const HEADER_H: i32 = 44;

/// Everything the finger can land on. Copy so `pressed` can snapshot it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum Hit {
    Pad(usize),
    Dtmf(usize),
    /// Keypad-sheet backspace *and* the DTMF-sheet "hide keypad" button
    /// (context decides).
    Bksp,
    /// Keypad call FAB *and* in-call end/decline buttons (context decides).
    Call,
    /// Header "Ring" simulator *and* the incoming-call answer button.
    RingSim,
    Tab(Tab),
    Search,
    SearchClear,
    RecClear,
    FilterAll,
    FilterMissed,
    RecRow(usize),
    RecCall(usize),
    ConRow(usize),
    ConCall(usize),
    /// Bottom-center dialpad FAB opening the keypad sheet.
    KeypadFab,
    /// Scrim tap or the keypad sheet's hide button — closes the sheet.
    KeypadHide,
    /// In-call toggle buttons (explicit hits — no sentinel Pad indices).
    Mute,
    Speaker,
    DtmfToggle,
    /// Inside a scrollable list viewport (drag scroll; tap = no action).
    List,
}

pub struct DialerApp {
    pub(super) backend: Backend,
    pub(super) tab: Tab,
    pub(super) number: String,
    pub(super) query: String,
    pub(super) search_focus: bool,
    pub(super) missed_only: bool,
    pub(super) recents: Recents,
    pub(super) recents_dirty: bool,
    /// Resolved contact list (shared store or built-in demo) — loaded once
    /// at startup; every contacts-tab/lookup surface reads this.
    pub(super) contacts: Vec<Contact>,
    pub(super) call: Option<CallSession>,
    /// Keypad bottom-sheet overlay (opened by the dialpad FAB).
    pub(super) keypad_open: bool,
    /// Sheet slide-in progress 0→1 (spring-ish ease-out over ~240 ms).
    pub(super) sheet_anim: f32,
    pub(super) pressed: Option<Hit>,
    /// Release "pop" animation: (key index, elapsed s).
    pub(super) pop: Option<(usize, f32)>,
    /// List scroll offset px; drag = (start_y, scroll_at_start, moved).
    pub(super) scroll: i32,
    pub(super) max_scroll: i32,
    pub(super) drag: Option<(i32, i32, bool)>,
    /// Long-press backspace repeat: seconds the bksp key has been held;
    /// after the 0.5 s delay digits delete every 60 ms (AOSP behavior).
    pub(super) bksp_hold: f32,
    pub(super) status: String,
    /// Wall-clock anchor: the kit only delivers `Tick` while `animating`
    /// was set, so call progression, the elapsed timer and pop animation
    /// are advanced by real elapsed time in `pump()` instead.
    last_instant: Option<Instant>,
    /// Last rendered size — hit-testing uses it between frames.
    pub(super) vp: (i32, i32),
}

impl DialerApp {
    pub fn new(initial_number: Option<String>) -> Self {
        let backend = Backend::detect();
        let recents = Self::load_recents();
        // `--dial <number>` hand-off: prefill the keypad, never auto-call.
        let number = initial_number.map_or_else(String::new, |n| model::digits_only(&n));
        let keypad_open = !number.is_empty();
        Self {
            backend,
            tab: Tab::Recents,
            number,
            query: String::new(),
            search_focus: false,
            missed_only: false,
            recents,
            recents_dirty: false,
            contacts: contacts::resolved(),
            call: None,
            keypad_open,
            // Start settled at the initial state: cold-start ticks are not
            // guaranteed before the first input event, so a prefilled
            // `--dial` sheet must not depend on the animation to appear.
            sheet_anim: if keypad_open { 1.0 } else { 0.0 },
            pressed: None,
            pop: None,
            scroll: 0,
            max_scroll: 0,
            drag: None,
            bksp_hold: 0.0,
            status: String::new(),
            last_instant: None,
            vp: (720, 1600),
        }
    }

    // ---------------- persistence ----------------

    pub(crate) fn data_dir() -> PathBuf {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join("rabbiz-dialer")
    }

    /// Load the persisted call log, falling back to the demo seed — the
    /// same source the headless CLI/MCP surfaces read.
    pub(crate) fn load_recents() -> Recents {
        let p = Self::data_dir().join("recents.json");
        std::fs::read_to_string(&p)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_else(|| Recents::demo_seed(now_unix()))
    }

    /// Write `recents` to `recents.json` (shared with the headless CLI/MCP
    /// dial path, which has no `DialerApp` instance).
    pub(crate) fn persist_recents(recents: &Recents) {
        let dir = Self::data_dir();
        let _ = std::fs::create_dir_all(&dir);
        if let Ok(s) = serde_json::to_string(recents) {
            let _ = std::fs::write(dir.join("recents.json"), s);
        }
    }

    fn save_recents(&mut self) {
        Self::persist_recents(&self.recents);
        self.recents_dirty = false;
    }

    // ---------------- wall-clock pump ----------------

    /// Advance time-driven state by real elapsed time. Called from both
    /// `handle_event` and `render` so the call timer, phase progression and
    /// pop animation move even when no `Tick` events arrive.
    fn pump(&mut self) {
        let now = Instant::now();
        let dt = self
            .last_instant
            .map_or(0.0, |t| now.saturating_duration_since(t).as_secs_f32());
        self.last_instant = Some(now);
        if dt <= 0.0 {
            return;
        }
        if let Some(c) = self.call.as_mut() {
            c.tick(dt);
            if c.incoming_missed() {
                self.end_call();
                return;
            }
        }
        if let Some((_, t)) = self.pop.as_mut() {
            *t += dt;
            if *t >= POP_S {
                self.pop = None;
            }
        }
        // Sheet slide: ease-out toward open/closed; small snap window.
        let target = if self.keypad_open { 1.0 } else { 0.0 };
        if (self.sheet_anim - target).abs() > 0.001 {
            let dir = if target > self.sheet_anim { 1.0 } else { -1.0 };
            self.sheet_anim = (self.sheet_anim + dir * dt / 0.24).clamp(0.0, 1.0);
        }
        // Long-press backspace: after the initial hold delay, delete one
        // digit per repeat interval until the key is released.
        if self.pressed == Some(Hit::Bksp) && self.call.is_none() && self.keypad_open {
            self.bksp_hold += dt;
            if self.bksp_hold > 0.5 {
                let reps = ((self.bksp_hold - 0.5) / 0.06) as usize;
                let prev = ((self.bksp_hold - dt - 0.5).max(0.0) / 0.06) as usize;
                for _ in prev..reps {
                    self.number.pop();
                }
            }
        } else {
            self.bksp_hold = 0.0;
        }
    }

    /// Ease-out cubic for the sheet travel (fast start, soft landing).
    pub(super) fn sheet_ease(&self) -> f32 {
        let x = self.sheet_anim.clamp(0.0, 1.0);
        1.0 - (1.0 - x).powi(3)
    }

    // ---------------- actions ----------------

    pub(super) fn push_digit(&mut self, d: &str) {
        if !self.status.is_empty() {
            self.status.clear(); // typing dismisses the last error/status
        }
        if self.number.chars().count() < MAX_DIGITS {
            self.number.push_str(d);
        }
    }

    /// Place a call; `number` is sanitized with `digits_only`.
    pub(super) fn place_call(&mut self, number: &str) {
        if self.call.is_some() {
            return;
        }
        let n = model::digits_only(number);
        if n.is_empty() {
            self.status = "Enter a number".to_string();
            return;
        }
        match self.backend.kind {
            BackendKind::Mmcli { modem } => match backend::mmcli_create_call(modem, &n) {
                Ok(path) => {
                    self.call = Some(CallSession::outgoing(n, (!path.is_empty()).then_some(path)));
                    self.status.clear();
                }
                Err(e) => {
                    self.status = if e.is_empty() {
                        "Call failed".to_string()
                    } else {
                        e
                    };
                }
            },
            BackendKind::Simulated => {
                self.call = Some(CallSession::outgoing(n, None));
                self.status.clear();
            }
        }
    }

    pub(super) fn simulate_incoming(&mut self) {
        if self.call.is_some() || !self.backend.can_simulate_incoming() {
            return;
        }
        // Same demo number the shell's "Ring" button uses.
        self.call = Some(CallSession::incoming("0812345678".to_string()));
    }

    pub(super) fn end_call(&mut self) {
        if let Some(c) = self.call.take() {
            let n = model::digits_only(&c.number);
            if !n.is_empty() {
                let name = model::lookup_in(&self.contacts, &n).map_or("", |ct| ct.name.as_str());
                self.recents.add(&n, name, c.log_dir(), now_unix());
                self.recents_dirty = true;
            }
            if let Some(path) = c.backend_path {
                let _ = backend::mmcli_hangup(&path);
            }
        }
    }

    pub(super) fn answer_call(&mut self) {
        if let Some(c) = self.call.as_mut() {
            c.answer();
        }
    }

    pub(super) fn send_dtmf(&mut self, d: &str) {
        if let Some(c) = self.call.as_mut() {
            c.last_dtmf.push_str(d);
            if let Some(path) = c.backend_path.clone() {
                let tones = c.last_dtmf.clone();
                let _ = backend::mmcli_send_dtmf(&path, &tones);
            }
        }
    }

    pub(super) fn leave_search(&mut self, ti: &TextInputCtl) {
        if self.search_focus {
            self.search_focus = false;
            ti.deactivate();
        }
    }

    /// Append committed text to whichever field has focus. Digits typed
    /// while no call is up open the keypad sheet — same as tapping a digit
    /// in the AOSP Phone app.
    fn commit_text(&mut self, s: &str) {
        if self.search_focus {
            self.query.push_str(s);
            return;
        }
        if self.call.is_some() {
            return;
        }
        for ch in s.chars() {
            let d = model::digits_only(&ch.to_string());
            if !d.is_empty() {
                self.keypad_open = true;
                self.push_digit(&d);
            }
        }
    }

    fn backspace_chars(&mut self, n: usize) {
        if self.search_focus {
            for _ in 0..n {
                self.query.pop();
            }
        } else if self.call.is_none() && self.keypad_open {
            for _ in 0..n {
                self.number.pop();
            }
        }
    }

    /// Search filters both pages: name or number substring against the log.
    pub(super) fn filtered_recents(&self) -> Vec<usize> {
        let q = self.query.trim().to_lowercase();
        self.recents
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| !self.missed_only || e.dir == CallDir::Missed)
            .filter(|(_, e)| {
                q.is_empty()
                    || e.name.to_lowercase().contains(&q)
                    || e.number.contains(self.query.trim())
            })
            .map(|(i, _)| i)
            .collect()
    }

    fn update_scroll_limits(&mut self, w: i32, h: i32) {
        let (vp_h, content) = match self.tab {
            Tab::Favorites => {
                let top = self.content_top();
                let vp_h = self.content_bottom(w, h) - top;
                (vp_h, self.favorites_height(self.favorite_indices().len()))
            }
            Tab::Recents => {
                let vp = self.recents_viewport(w, h);
                (vp.h, Self::items_height(&self.recents_items()))
            }
            Tab::Contacts => {
                let vp = self.contacts_viewport(w, h);
                (vp.h, Self::items_height(&self.contacts_items()))
            }
        };
        // +FAB clearance: the last row scrolls fully above the dialpad FAB.
        self.max_scroll = (content + 108 - vp_h).max(0);
        self.scroll = self.scroll.clamp(0, self.max_scroll);
    }
}

impl AppState for DialerApp {
    fn app_id(&self) -> &'static str {
        "rabbiz-dialer"
    }
    fn title(&self) -> &'static str {
        "Phone"
    }

    fn handle_event(&mut self, event: AppEvent, ti: &TextInputCtl) -> AppFrame {
        self.pump();
        let (w, h) = self.vp;
        let mut dirty = false;

        match event {
            AppEvent::TouchDown { pos, .. } => {
                let hit = self.hit_at(w, h, pos);
                self.pressed = hit;
                // Any touch inside a list viewport can start a scroll drag;
                // a tap without movement still dispatches the row action.
                let in_list = !self.keypad_open
                    && self.call.is_none()
                    && match self.tab {
                        Tab::Favorites => {
                            let top = self.content_top();
                            Rect::new(0, top, w, self.content_bottom(w, h) - top).contains(pos)
                        }
                        Tab::Recents => self.recents_viewport(w, h).contains(pos),
                        Tab::Contacts => self.contacts_viewport(w, h).contains(pos),
                    };
                if in_list {
                    self.drag = Some((pos.y, self.scroll, false));
                }
                dirty = true;
            }
            AppEvent::TouchMove { pos, .. } => {
                if let Some((sy, ss, moved)) = self.drag.as_mut() {
                    if (*sy - pos.y).abs() > 10 {
                        *moved = true;
                    }
                    self.scroll = (*ss + *sy - pos.y).clamp(0, self.max_scroll);
                    self.pressed = None;
                    dirty = true;
                }
            }
            AppEvent::TouchUp { pos, .. } => {
                let drag_moved = self.drag.take().is_some_and(|(_, _, m)| m);
                let hit = self.pressed.take();
                self.pressed = None;
                if !drag_moved {
                    if let Some(hit) = hit {
                        if self.hit_at(w, h, pos) == Some(hit) {
                            self.dispatch(hit, ti);
                        }
                    }
                }
                dirty = true;
            }
            AppEvent::Key { keysym, utf8 } => {
                match keysym {
                    0xff08 => self.backspace_chars(1), // BackSpace
                    0xff0d => {
                        // Return places the call when the keypad is up.
                        if self.call.is_none() && self.keypad_open && !self.number.is_empty() {
                            let n = self.number.clone();
                            self.place_call(&n);
                        }
                    }
                    0xff1b => self.leave_search(ti), // Escape
                    _ => {
                        if let Some(s) = utf8 {
                            self.commit_text(&s);
                        }
                    }
                }
                dirty = true;
            }
            AppEvent::TextCommit(s) => {
                self.commit_text(&s);
                dirty = true;
            }
            AppEvent::TextBackspace(n) => {
                self.backspace_chars(n);
                dirty = true;
            }
            AppEvent::Scroll { dy, .. } => {
                // Mouse wheel/trackpad: scroll whichever list is visible.
                // `dy` follows the wl_pointer axis sign (positive = scroll
                // down → larger offset); flip if on-device feel disagrees.
                if !self.keypad_open && self.call.is_none() {
                    let next = (self.scroll as f32 + dy)
                        .round()
                        .clamp(0.0, self.max_scroll as f32) as i32;
                    if next != self.scroll {
                        self.scroll = next;
                        dirty = true;
                    }
                }
            }
            // Left-button presses already arrive as Touch*; hover and
            // non-primary buttons carry no actionable state here.
            AppEvent::PointerMove { .. } | AppEvent::PointerButton { .. } => {}
            AppEvent::Tick(_) => dirty = true,
        }

        if self.recents_dirty {
            self.save_recents();
        }
        let sheet_moving = (self.sheet_anim - if self.keypad_open { 1.0 } else { 0.0 }).abs() > 0.001;
        let bksp_held = self.pressed == Some(Hit::Bksp) && self.keypad_open;
        AppFrame {
            dirty,
            animating: self.call.is_some() || self.pop.is_some() || sheet_moving || bksp_held,
            quit: false,
        }
    }

    fn render(&mut self, canvas: &mut Canvas, theme: &Theme) {
        self.pump();
        let size = canvas.size();
        let (w, h) = (size.w, size.h);
        self.vp = (w, h);
        self.update_scroll_limits(w, h);

        if self.call.is_some() {
            self.render_call(canvas, theme, w, h);
            return;
        }

        self.render_header(canvas, theme, w);
        self.render_search(canvas, theme, w);
        match self.tab {
            Tab::Favorites => self.render_favorites(canvas, theme, w, h),
            Tab::Recents => self.render_recents(canvas, theme, w, h),
            Tab::Contacts => self.render_contacts(canvas, theme, w, h),
        }
        self.render_navbar(canvas, theme, w, h);
        // The sheet is a modal overlay: it slides over the nav bar and
        // hides the FAB (its own action row replaces it).
        if self.sheet_anim > 0.001 {
            self.render_keypad_sheet(canvas, theme, w, h);
        } else {
            self.render_fab(canvas, theme, w, h);
        }
    }
}

pub(crate) fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

pub fn run(initial_number: Option<String>) {
    rabbiz_app_kit::run_app(DialerApp::new(initial_number));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::CallPhase;
    use rabbiz_app_kit::TestHarness;
    use std::path::PathBuf;

    fn shot_dir() -> PathBuf {
        let d = PathBuf::from("/tmp/rabbiz-shots");
        let _ = std::fs::create_dir_all(&d);
        d
    }

    /// Drives the app through its main screens and saves PNGs (the
    /// before/after evidence contract). Deterministic data is injected so
    /// the shots are stable regardless of the host's store files.
    #[test]
    fn renders_main_screens_to_png() {
        let dir = shot_dir();
        let mut h = TestHarness::new(DialerApp::new(None));
        h.state_mut().recents = Recents::demo_seed(now_unix());

        h.state_mut().tab = Tab::Favorites;
        h.save_png(&dir.join("dialer-after-favorites.png"))
            .expect("save favorites shot");

        h.state_mut().tab = Tab::Recents;
        h.save_png(&dir.join("dialer-after-recents.png"))
            .expect("save recents shot");

        h.state_mut().tab = Tab::Contacts;
        h.save_png(&dir.join("dialer-after-contacts.png"))
            .expect("save contacts shot");

        h.state_mut().keypad_open = true;
        h.state_mut().sheet_anim = 1.0;
        h.state_mut().number = "0812345678".to_string();
        h.save_png(&dir.join("dialer-after-keypad.png"))
            .expect("save keypad shot");

        // Incoming ring + active outgoing call.
        h.state_mut().keypad_open = false;
        h.state_mut().call = Some(CallSession::incoming("0812345678".to_string()));
        h.save_png(&dir.join("dialer-after-incoming.png"))
            .expect("save incoming shot");

        let mut active = CallSession::outgoing("0865551234".to_string(), None);
        active.phase = CallPhase::Active;
        active.secs = 74.0;
        h.state_mut().call = Some(active);
        h.save_png(&dir.join("dialer-after-incall.png"))
            .expect("save in-call shot");

        // Dark plum — the on-device product theme.
        h.set_theme(de_design::tokens::DARK);
        h.state_mut().call = None;
        h.state_mut().tab = Tab::Recents;
        h.save_png(&dir.join("dialer-dark-recents.png"))
            .expect("save dark recents shot");
        h.state_mut().tab = Tab::Favorites;
        h.save_png(&dir.join("dialer-dark-favorites.png"))
            .expect("save dark favorites shot");
        h.state_mut().keypad_open = true;
        h.state_mut().sheet_anim = 1.0;
        h.save_png(&dir.join("dialer-dark-keypad.png"))
            .expect("save dark keypad shot");
        h.state_mut().keypad_open = false;
        h.state_mut().sheet_anim = 0.0;
        let mut dark_call = CallSession::outgoing("0865551234".to_string(), None);
        dark_call.phase = CallPhase::Active;
        dark_call.secs = 74.0;
        h.state_mut().call = Some(dark_call);
        h.save_png(&dir.join("dialer-dark-incall.png"))
            .expect("save dark in-call shot");
    }
}
