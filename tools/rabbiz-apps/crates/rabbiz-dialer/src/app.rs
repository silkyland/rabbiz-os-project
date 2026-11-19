//! rabbiz-dialer — Phone app for RabbizOS (Rust/native Wayland build).
//!
//! Layout and behaviour mirror the shell-owned QML dialer
//! (`shell/apps/dialer/Dialer.qml`): Keypad / Recents / Contacts tabs,
//! T9 sub-labels, `3-3-4` number grouping, caller-ID matching, and the
//! same simulated call progression until a real telephony backend exists
//! (`backend.rs` documents the ModemManager/`mmcli` path).

use std::path::PathBuf;
use std::time::Instant;

use de_api::Pixels;
use rabbiz_app_kit::{
    text, ui, AppEvent, AppFrame, AppState, Canvas, Point, Rect, Size, TextInputCtl, Theme,
};
use wayland_protocols::wp::text_input::zv3::client::zwp_text_input_v3::ContentPurpose;

use crate::backend::{self, Backend, BackendKind};
use crate::contacts;
use crate::model::{
    self, CallDir, CallPhase, CallSession, Contact, DialKey, Recents, SubLabel, KEYS,
};

const MAX_DIGITS: usize = 24;
/// Release "pop" duration — inside the ≤600 ms motion budget, near the
/// `de_design::motion::DURATIONS_MS` 160/240 ladder.
const POP_S: f32 = 0.18;
/// Pressed-state shrink derived from `de_design::motion::PRESS_SCALE`.
const PRESS_SHRINK: f32 = 1.0 - de_design::motion::PRESS_SCALE;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Keypad,
    Recents,
    Contacts,
}

impl Tab {
    const ALL: [Tab; 3] = [Tab::Keypad, Tab::Recents, Tab::Contacts];
    fn label(self) -> &'static str {
        match self {
            Tab::Keypad => "Keypad",
            Tab::Recents => "Recents",
            Tab::Contacts => "Contacts",
        }
    }
    fn icon(self) -> &'static str {
        match self {
            Tab::Keypad => "\u{e0bc}",   // dialpad
            Tab::Recents => "\u{e889}",  // history
            Tab::Contacts => "\u{e7fd}", // person
        }
    }
}

/// Everything the finger can land on. Copy so `pressed` can snapshot it.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Hit {
    Pad(usize),
    Dtmf(usize),
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
    /// Inside a scrollable list viewport (drag scroll; tap = no action).
    List,
}

pub struct DialerApp {
    backend: Backend,
    tab: Tab,
    number: String,
    query: String,
    search_focus: bool,
    missed_only: bool,
    recents: Recents,
    recents_dirty: bool,
    /// Resolved contact list (shared store or built-in demo) — loaded once
    /// at startup; every contacts-tab/lookup surface reads this.
    contacts: Vec<Contact>,
    call: Option<CallSession>,
    pressed: Option<Hit>,
    /// Release "pop" animation: (key index, elapsed s).
    pop: Option<(usize, f32)>,
    /// List scroll offset px; drag = (start_y, scroll_at_start, moved).
    scroll: i32,
    max_scroll: i32,
    drag: Option<(i32, i32, bool)>,
    status: String,
    /// Wall-clock anchor: the kit only delivers `Tick` while `animating`
    /// was set, so call progression, the elapsed timer and pop animation
    /// are advanced by real elapsed time in `pump()` instead.
    last_instant: Option<Instant>,
    /// Last rendered size — hit-testing uses it between frames.
    vp: (i32, i32),
}

// Row metrics (pitch includes the gap).
const REC_PITCH: i32 = 106;
const REC_ROW_H: i32 = 94;
const CON_PITCH: i32 = 100;
const CON_ROW_H: i32 = 88;

impl DialerApp {
    pub fn new(initial_number: Option<String>) -> Self {
        let backend = Backend::detect();
        let recents = Self::load_recents();
        // `--dial <number>` hand-off: prefill the keypad, never auto-call.
        let number = initial_number.map_or_else(String::new, |n| model::digits_only(&n));
        Self {
            backend,
            tab: Tab::Keypad,
            number,
            query: String::new(),
            search_focus: false,
            missed_only: false,
            recents,
            recents_dirty: false,
            contacts: contacts::resolved(),
            call: None,
            pressed: None,
            pop: None,
            scroll: 0,
            max_scroll: 0,
            drag: None,
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
    }

    // ---------------- actions ----------------

    fn push_digit(&mut self, d: &str) {
        if !self.status.is_empty() {
            self.status.clear(); // typing dismisses the last error/status
        }
        if self.number.chars().count() < MAX_DIGITS {
            self.number.push_str(d);
        }
    }

    /// Place a call; `number` is sanitized with `digits_only`.
    fn place_call(&mut self, number: &str) {
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

    fn simulate_incoming(&mut self) {
        if self.call.is_some() || !self.backend.can_simulate_incoming() {
            return;
        }
        // Same demo number the shell's "Ring" button uses.
        self.call = Some(CallSession::incoming("0812345678".to_string()));
    }

    fn end_call(&mut self) {
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

    fn answer_call(&mut self) {
        if let Some(c) = self.call.as_mut() {
            c.answer();
        }
    }

    fn send_dtmf(&mut self, d: &str) {
        if let Some(c) = self.call.as_mut() {
            c.last_dtmf.push_str(d);
            if let Some(path) = c.backend_path.clone() {
                let tones = c.last_dtmf.clone();
                let _ = backend::mmcli_send_dtmf(&path, &tones);
            }
        }
    }

    fn leave_search(&mut self, ti: &TextInputCtl) {
        if self.search_focus {
            self.search_focus = false;
            ti.deactivate();
        }
    }

    /// Append committed text to whichever field has focus.
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
                self.push_digit(&d);
            }
        }
    }

    fn backspace_chars(&mut self, n: usize) {
        if self.search_focus {
            for _ in 0..n {
                self.query.pop();
            }
        } else if self.call.is_none() {
            for _ in 0..n {
                self.number.pop();
            }
        }
    }

    // ---------------- geometry ----------------

    fn tabs_rect(&self, w: i32) -> Rect {
        Rect::new(ui::MARGIN, ui::TOP_SAFE + 92, w - ui::MARGIN * 2, 46)
    }

    fn content_top(&self) -> i32 {
        ui::TOP_SAFE + 92 + 46 + 16
    }

    /// i-th segment rect inside the tab pill.
    fn tab_rect(&self, w: i32, idx: usize) -> Rect {
        let t = self.tabs_rect(w);
        let n = Tab::ALL.len() as i32;
        let gap = 8;
        let bw = (t.w - gap * (n - 1)) / n;
        Rect::new(t.x + idx as i32 * (bw + gap), t.y, bw, t.h)
    }

    /// Bottom action row on the keypad page: (backspace, call FAB).
    fn action_row(&self, w: i32, h: i32) -> (Rect, Rect) {
        let y = h - ui::BOTTOM_SAFE - 108;
        let bk = Rect::new(ui::MARGIN, y + 12, 72, 72);
        let call = Rect::new(w - ui::MARGIN - 108, y, 108, 108);
        (bk, call)
    }

    /// Keypad grid rects (12 keys), bottom-anchored above the action row.
    fn pad_key_rects(&self, w: i32, h: i32) -> Vec<Rect> {
        let gap = 14;
        let margin = 40;
        let keys_bottom = h - ui::BOTTOM_SAFE - 108 - 24;
        let grid_floor = self.content_top() + 190;
        let max_grid = keys_bottom - grid_floor;
        let bh = ((max_grid - gap * 3) / 4).clamp(62, 96);
        let grid_h = bh * 4 + gap * 3;
        let top = keys_bottom - grid_h;
        let bw = (w - margin * 2 - gap * 2) / 3;
        (0..KEYS.len())
            .map(|i| {
                let r = i as i32 / 3;
                let c = i as i32 % 3;
                Rect::new(margin + c * (bw + gap), top + r * (bh + gap), bw, bh)
            })
            .collect()
    }

    /// In-call DTMF pad — same footprint as the dial pad, shorter keys.
    fn dtmf_key_rects(&self, w: i32, h: i32) -> Vec<Rect> {
        self.pad_key_rects(w, h)
            .iter()
            .map(|r| Rect::new(r.x, r.y + r.h / 6, r.w, r.h * 2 / 3))
            .collect()
    }

    /// Top of the contacts rows list (below the fixed favorites strip when
    /// no search query is active).
    fn contacts_rows_top(&self) -> i32 {
        let top = self.content_top();
        if self.query.trim().is_empty() {
            top + 72 + 22 + 108 + 14 + 22 // label + fav cards + gap + section label
        } else {
            top + 72 + 14
        }
    }

    /// Contacts list viewport = rows area only (favorites/search are fixed).
    fn contacts_viewport(&self, w: i32, h: i32) -> Rect {
        let top = self.contacts_rows_top();
        Rect::new(
            ui::MARGIN,
            top,
            w - ui::MARGIN * 2,
            h - ui::BOTTOM_SAFE - 16 - top,
        )
    }

    /// Recents list viewport (below the filter chips).
    fn recents_viewport(&self, w: i32, h: i32) -> Rect {
        let top = self.content_top() + 52;
        Rect::new(
            ui::MARGIN,
            top,
            w - ui::MARGIN * 2,
            h - ui::BOTTOM_SAFE - 16 - top,
        )
    }

    fn contact_row_rect(&self, w: i32, i: usize) -> Rect {
        let top = self.contacts_rows_top();
        Rect::new(
            ui::MARGIN,
            top + i as i32 * CON_PITCH - self.scroll,
            w - ui::MARGIN * 2,
            CON_ROW_H,
        )
    }

    fn recents_row_rect(&self, w: i32, i: usize) -> Rect {
        let top = self.recents_viewport(w, 0).y;
        Rect::new(
            ui::MARGIN,
            top + i as i32 * REC_PITCH - self.scroll,
            w - ui::MARGIN * 2,
            REC_ROW_H,
        )
    }

    fn filtered_recents(&self) -> Vec<usize> {
        self.recents
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| !self.missed_only || e.dir == CallDir::Missed)
            .map(|(i, _)| i)
            .collect()
    }

    fn filtered_contacts(&self) -> Vec<&Contact> {
        self.contacts
            .iter()
            .filter(|c| model::contact_matches(c, &self.query))
            .collect()
    }

    fn contact_index(&self, ct: &Contact) -> usize {
        self.contacts
            .iter()
            .position(|x| x.number == ct.number && x.name == ct.name)
            .unwrap_or(0)
    }

    fn update_scroll_limits(&mut self, w: i32, h: i32) {
        let (vp_h, count, pitch) = match self.tab {
            Tab::Recents => {
                let vp = self.recents_viewport(w, h);
                (vp.h, self.filtered_recents().len() as i32, REC_PITCH)
            }
            Tab::Contacts => {
                let vp = self.contacts_viewport(w, h);
                (vp.h, self.filtered_contacts().len() as i32, CON_PITCH)
            }
            Tab::Keypad => (0, 0, 1),
        };
        let content = count * pitch;
        self.max_scroll = (content - vp_h).max(0);
        self.scroll = self.scroll.clamp(0, self.max_scroll);
    }

    // ---------------- hit testing ----------------

    fn hit_at(&self, w: i32, h: i32, pos: Point) -> Option<Hit> {
        if self.call.is_some() {
            return self.call_hit(w, h, pos);
        }
        for (i, t) in Tab::ALL.iter().enumerate() {
            if self.tab_rect(w, i).contains(pos) {
                return Some(Hit::Tab(*t));
            }
        }
        match self.tab {
            Tab::Keypad => self.keypad_hit(w, h, pos),
            Tab::Recents => self.recents_hit(w, h, pos),
            Tab::Contacts => self.contacts_hit(w, h, pos),
        }
    }

    fn keypad_hit(&self, w: i32, h: i32, pos: Point) -> Option<Hit> {
        let (bk, call) = self.action_row(w, h);
        if call.contains(pos) {
            return Some(Hit::Call);
        }
        if bk.contains(pos) {
            return Some(Hit::Bksp);
        }
        for (i, r) in self.pad_key_rects(w, h).iter().enumerate() {
            if r.contains(pos) {
                return Some(Hit::Pad(i));
            }
        }
        None
    }

    fn recents_hit(&self, w: i32, h: i32, pos: Point) -> Option<Hit> {
        let top = self.content_top();
        if Rect::new(ui::MARGIN, top, 96, 40).contains(pos) {
            return Some(Hit::FilterAll);
        }
        if Rect::new(ui::MARGIN + 104, top, 160, 40).contains(pos) {
            return Some(Hit::FilterMissed);
        }
        if Rect::new(w - ui::MARGIN - 100, top + 4, 100, 36).contains(pos)
            && !self.recents.entries.is_empty()
        {
            return Some(Hit::RecClear);
        }
        let vp = self.recents_viewport(w, h);
        if !vp.contains(pos) {
            return None;
        }
        for (vis, &real) in self.filtered_recents().iter().enumerate() {
            let r = self.recents_row_rect(w, vis);
            if !r.contains(pos) {
                continue;
            }
            let cb = Rect::new(r.x + r.w - 72, r.y + (r.h - 54) / 2, 54, 54);
            if cb.contains(pos) {
                return Some(Hit::RecCall(real));
            }
            return Some(Hit::RecRow(real));
        }
        Some(Hit::List)
    }

    fn contacts_hit(&self, w: i32, h: i32, pos: Point) -> Option<Hit> {
        let top = self.content_top();
        let search = Rect::new(ui::MARGIN, top, w - ui::MARGIN * 2, 56);
        if search.contains(pos) {
            if !self.query.is_empty()
                && Rect::new(search.x + search.w - 46, search.y + 14, 30, 30).contains(pos)
            {
                return Some(Hit::SearchClear);
            }
            return Some(Hit::Search);
        }
        // Favorites strip (fixed, only when not searching).
        if self.query.trim().is_empty() {
            let fav_top = top + 72 + 22;
            let card_w = (w - ui::MARGIN * 2 - 24) / 3;
            let favs: Vec<&Contact> = self.contacts.iter().filter(|c| c.favorite).collect();
            for (i, ct) in favs.iter().enumerate() {
                let cx = ui::MARGIN + i as i32 * (card_w + 12);
                if Rect::new(cx, fav_top, card_w, 108).contains(pos) {
                    return Some(Hit::ConCall(self.contact_index(ct)));
                }
            }
        }
        let vp = self.contacts_viewport(w, h);
        if !vp.contains(pos) {
            return None;
        }
        let list = self.filtered_contacts();
        for (vis, ct) in list.iter().enumerate() {
            let r = self.contact_row_rect(w, vis);
            if !r.contains(pos) {
                continue;
            }
            let real = self.contact_index(ct);
            let cb = Rect::new(r.x + r.w - 72, r.y + (r.h - 56) / 2, 56, 56);
            if cb.contains(pos) {
                return Some(Hit::ConCall(real));
            }
            return Some(Hit::ConRow(real));
        }
        Some(Hit::List)
    }

    // ---------------- in-call hit testing ----------------

    fn call_hit(&self, w: i32, h: i32, pos: Point) -> Option<Hit> {
        let c = self.call.as_ref()?;
        if c.dtmf_open {
            for (i, r) in self.dtmf_key_rects(w, h).iter().enumerate() {
                if r.contains(pos) {
                    return Some(Hit::Dtmf(i));
                }
            }
            let hide = Rect::new(w / 2 - 90, h - ui::BOTTOM_SAFE - 70, 180, 52);
            if hide.contains(pos) {
                return Some(Hit::Bksp); // toggles the DTMF pad closed
            }
            return None;
        }
        let incoming = c.dir == CallDir::In && c.phase == CallPhase::Ringing && !c.answered;
        if incoming {
            let by = h - ui::BOTTOM_SAFE - 330;
            if Rect::new(w / 2 - 150, by, 120, 120).contains(pos) {
                return Some(Hit::Call); // decline → logged as missed
            }
            if Rect::new(w / 2 + 30, by, 120, 120).contains(pos) {
                return Some(Hit::RingSim); // answer
            }
            return None;
        }
        // Toggle row: mute / keypad / speaker.
        let ty = h - ui::BOTTOM_SAFE - 310;
        let tw = 84;
        let gap = (w - ui::MARGIN * 2 - tw * 3) / 2;
        for i in 0..3 {
            let r = Rect::new(ui::MARGIN + i as i32 * (tw + gap), ty, tw, tw);
            if r.contains(pos) {
                return Some(Hit::Pad(90 + i));
            }
        }
        let end = Rect::new(w / 2 - 60, h - ui::BOTTOM_SAFE - 170, 120, 120);
        if end.contains(pos) {
            return Some(Hit::Call);
        }
        None
    }

    // ---------------- dispatch ----------------

    fn dispatch(&mut self, hit: Hit, ti: &TextInputCtl) {
        match hit {
            Hit::Pad(i) if i < KEYS.len() => {
                self.push_digit(KEYS[i].digit);
                self.pop = Some((i, 0.0));
            }
            Hit::Pad(90) => {
                if let Some(c) = self.call.as_mut() {
                    c.muted = !c.muted;
                }
            }
            Hit::Pad(91) => {
                if let Some(c) = self.call.as_mut() {
                    c.dtmf_open = !c.dtmf_open;
                }
            }
            Hit::Pad(92) => {
                if let Some(c) = self.call.as_mut() {
                    c.speaker = !c.speaker;
                }
            }
            Hit::Dtmf(i) => {
                if i < model::DTMF_KEYS.len() {
                    let d = model::DTMF_KEYS[i];
                    self.send_dtmf(d);
                    self.pop = Some((i + 100, 0.0));
                }
            }
            Hit::Bksp => {
                if self.call.as_ref().is_some_and(|c| c.dtmf_open) {
                    if let Some(c) = self.call.as_mut() {
                        c.dtmf_open = false;
                    }
                } else {
                    self.number.pop();
                }
            }
            Hit::Call => {
                if self.call.is_some() {
                    // End call — or decline an incoming ring (log_dir()
                    // records it as missed because `answered` stays false).
                    self.end_call();
                } else if self.tab == Tab::Keypad {
                    let n = self.number.clone();
                    self.place_call(&n);
                }
            }
            Hit::RingSim => {
                if self.call.is_some() {
                    self.answer_call();
                } else {
                    self.simulate_incoming();
                }
            }
            Hit::Tab(t) => {
                self.leave_search(ti);
                self.tab = t;
                self.scroll = 0;
            }
            Hit::Search => {
                self.search_focus = true;
                ti.activate(ContentPurpose::Normal);
                ti.set_surrounding(&self.query, self.query.len());
            }
            Hit::SearchClear => {
                self.query.clear();
                ti.set_surrounding("", 0);
            }
            Hit::RecClear => {
                self.recents.clear();
                self.recents_dirty = true;
                self.scroll = 0;
            }
            Hit::FilterAll => {
                self.missed_only = false;
                self.scroll = 0;
            }
            Hit::FilterMissed => {
                self.missed_only = true;
                self.scroll = 0;
            }
            Hit::RecRow(i) => {
                if let Some(e) = self.recents.entries.get(i) {
                    self.number = model::digits_only(&e.number);
                    self.tab = Tab::Keypad;
                    self.scroll = 0;
                }
            }
            Hit::RecCall(i) => {
                if let Some(e) = self.recents.entries.get(i) {
                    let n = e.number.clone();
                    self.place_call(&n);
                }
            }
            Hit::ConRow(i) => {
                if let Some(ct) = self.contacts.get(i) {
                    self.number = model::digits_only(&ct.number);
                    self.tab = Tab::Keypad;
                    self.scroll = 0;
                }
            }
            Hit::ConCall(i) => {
                if let Some(ct) = self.contacts.get(i) {
                    let n = ct.number.clone();
                    self.place_call(&n);
                }
            }
            Hit::List | Hit::Pad(_) => {}
        }
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
                let in_list = match self.tab {
                    Tab::Recents => self.recents_viewport(w, h).contains(pos),
                    Tab::Contacts => self.contacts_viewport(w, h).contains(pos),
                    Tab::Keypad => false,
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
                        // Return places the call on the keypad tab.
                        if self.call.is_none() && self.tab == Tab::Keypad && !self.number.is_empty()
                        {
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
                if matches!(self.tab, Tab::Recents | Tab::Contacts) {
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
        AppFrame {
            dirty,
            animating: self.call.is_some() || self.pop.is_some(),
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
        self.render_tabs(canvas, theme, w);
        match self.tab {
            Tab::Keypad => self.render_keypad(canvas, theme, w, h),
            Tab::Recents => self.render_recents(canvas, theme, w, h),
            Tab::Contacts => self.render_contacts(canvas, theme, w, h),
        }
    }
}

// ---------------------------------------------------------------------------
// Drawing helpers
// ---------------------------------------------------------------------------

fn alpha(c: [u8; 4], a: u8) -> [u8; 4] {
    [c[0], c[1], c[2], a]
}

fn mix(a: [u8; 4], b: [u8; 4], t: f32) -> [u8; 4] {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t) as u8;
    [l(a[0], b[0]), l(a[1], b[1]), l(a[2], b[2]), l(a[3], b[3])]
}

/// Rasterize + blit a Material icon with its ink bounds centered on (cx, cy).
fn draw_icon(c: &mut Canvas, codepoint: &str, size: f32, color: [u8; 4], cx: i32, cy: i32) {
    if let Some(px) = text::icon(codepoint, size, color) {
        let (x0, y0, x1, y1) =
            text::ink_bounds(&px).unwrap_or((0, 0, px.width() as i32, px.height() as i32));
        let ox = cx - (x0 + x1) / 2;
        let oy = cy - (y0 + y1) / 2;
        c.blit(
            &Pixels::new(
                Size::new(px.width() as i32, px.height() as i32),
                px.data().to_vec(),
            ),
            Point::new(ox, oy),
        );
    }
}

/// Largest of `sizes` whose rendered width fits `max_w`.
fn fit_style(s: &str, sizes: &[f32], weight: u16, color: [u8; 4], max_w: i32) -> text::LabelStyle {
    for &sz in sizes {
        let st = ui::styled(sz, weight, color);
        if let Some(px) = text::label(s, st, None) {
            if px.width() as i32 <= max_w {
                return st;
            }
        }
    }
    ui::styled(*sizes.last().unwrap_or(&24.0), weight, color)
}

/// Avatar circle: tinted bg + initial letter or person icon.
fn draw_avatar(c: &mut Canvas, r: Rect, name: &str, tint: [u8; 4]) {
    c.fill_rounded_rect(r, (r.h as f32) / 2.0, alpha(tint, 0x2E));
    if let Some(ch) = name.chars().next() {
        let s: String = ch.to_uppercase().collect();
        let st = ui::styled(r.h as f32 * 0.44, 600, tint);
        ui::draw_centered(c, &s, &st, r);
    } else {
        draw_icon(
            c,
            "\u{e7fd}",
            r.h as f32 * 0.52,
            tint,
            r.x + r.w / 2,
            r.y + r.h / 2,
        );
    }
}

/// Stable avatar tint for unknown numbers (port of `avatarColorFor`).
fn hash_color(seed: &str) -> [u8; 4] {
    const PALETTE: [[u8; 4]; 6] = [
        [0x3B, 0x82, 0xF6, 0xFF],
        [0x10, 0xB9, 0x81, 0xFF],
        [0xF5, 0x9E, 0x0B, 0xFF],
        [0xEC, 0x48, 0x99, 0xFF],
        [0x8B, 0x5C, 0xF6, 0xFF],
        [0x06, 0xB6, 0xD4, 0xFF],
    ];
    let mut h: u32 = 0;
    for b in seed.bytes() {
        h = h.wrapping_mul(31).wrapping_add(u32::from(b)) & 0xFFFF;
    }
    PALETTE[(h as usize) % PALETTE.len()]
}

fn entry_tint(contacts: &[Contact], name: &str, number: &str) -> [u8; 4] {
    if let Some(ct) = model::lookup_in(contacts, number) {
        if name.is_empty() || ct.name == name {
            return ct.color;
        }
    }
    hash_color(if name.is_empty() { number } else { name })
}

// ---------------------------------------------------------------------------
// Screen renderers
// ---------------------------------------------------------------------------

impl DialerApp {
    fn render_header(&mut self, c: &mut Canvas, t: &Theme, w: i32) {
        let y = ui::TOP_SAFE + 14;
        let tile = Rect::new(ui::MARGIN, y, 52, 52);
        c.fill_rounded_rect(tile, 26.0, alpha(t.accent, 0x2A));
        draw_icon(c, "\u{e0b0}", 28.0, t.accent, tile.x + 26, tile.y + 26);

        ui::draw_text(
            c,
            "Phone",
            &ui::title(t.text),
            ui::MARGIN + 66,
            y + 2,
            Some(w - ui::MARGIN * 2 - 320),
        );
        let sub = if self.status.is_empty() {
            self.backend.status_line().to_string()
        } else {
            self.status.clone()
        };
        let sub_col = if self.status.is_empty() {
            t.text3
        } else {
            t.warning
        };
        ui::draw_text(
            c,
            &sub,
            &ui::caption(sub_col),
            ui::MARGIN + 66,
            y + 44,
            Some(w - ui::MARGIN * 2 - 320),
        );

        // Right side: mode chip + (simulated backends only) Ring button.
        let chip_w = 76;
        let chip = Rect::new(w - ui::MARGIN - chip_w, y + 6, chip_w, 40);
        c.fill_rounded_rect(chip, 20.0, t.surface2);
        let modem_ok = self.backend.kind != BackendKind::Simulated || self.backend.modem_node;
        let chip_col = if modem_ok { t.success } else { t.warning };
        draw_icon(
            c,
            if modem_ok { "\u{e202}" } else { "\u{e624}" },
            18.0,
            chip_col,
            chip.x + 20,
            chip.y + 20,
        );
        ui::draw_centered(
            c,
            self.backend.chip(),
            &ui::styled(15.0, 600, t.text),
            Rect::new(chip.x + 34, chip.y, chip.w - 36, chip.h),
        );

        if self.backend.can_simulate_incoming() {
            let rw = 92;
            let ring = Rect::new(chip.x - rw - 10, y + 6, rw, 40);
            let pressed = self.pressed == Some(Hit::RingSim);
            ui::draw_pill_button(c, t, ring, "Ring", false, pressed);
            draw_icon(
                c,
                "\u{e0d1}",
                16.0,
                t.accent,
                ring.x + 18,
                ring.y + ring.h / 2,
            );
        }
    }

    fn render_tabs(&mut self, c: &mut Canvas, t: &Theme, w: i32) {
        let tr = self.tabs_rect(w);
        c.fill_rounded_rect(tr, tr.h as f32 / 2.0, t.surface2);
        for (i, tab) in Tab::ALL.iter().enumerate() {
            let r = self.tab_rect(w, i);
            let active = self.tab == *tab;
            if active {
                c.fill_rounded_rect(r, r.h as f32 / 2.0, t.surface1);
            }
            let col = if active { t.text } else { t.text3 };
            draw_icon(c, tab.icon(), 20.0, col, r.x + 28, r.y + r.h / 2);
            ui::draw_text(
                c,
                tab.label(),
                &ui::styled(17.0, if active { 600 } else { 400 }, col),
                r.x + 46,
                r.y + r.h / 2 - 12,
                Some(r.x + r.w - r.x - 50),
            );
        }
    }

    fn render_keypad(&mut self, c: &mut Canvas, t: &Theme, w: i32, h: i32) {
        let top = self.content_top();

        // Caller-ID pill.
        if !self.number.is_empty() {
            if let Some(ct) = model::lookup_in(&self.contacts, &self.number) {
                let pill = Rect::new(w / 2 - 180, top, 360, 40);
                c.fill_rounded_rect(pill, 20.0, t.surface1);
                draw_icon(c, "\u{e7fd}", 16.0, ct.color, pill.x + 22, pill.y + 20);
                let txt = format!("{} · {}", ct.name, ct.label.as_str());
                ui::draw_centered(
                    c,
                    &txt,
                    &ui::styled(16.0, 600, t.text),
                    Rect::new(pill.x + 38, pill.y, pill.w - 44, pill.h),
                );
            }
        }

        let keys = self.pad_key_rects(w, h);
        let grid_top = keys.first().map_or(top + 200, |r| r.y);
        let disp = Rect::new(
            ui::MARGIN,
            top + 56,
            w - ui::MARGIN * 2,
            (grid_top - top - 72).max(80),
        );
        let shown = if self.number.is_empty() {
            "Enter number".to_string()
        } else {
            model::format_number(&self.number)
        };
        let col = if self.number.is_empty() {
            t.text3
        } else {
            t.text
        };
        let st = fit_style(
            &shown,
            &[52.0, 46.0, 40.0, 34.0, 28.0],
            600,
            col,
            disp.w - 16,
        );
        ui::draw_centered(c, &shown, &st, disp);

        for (i, r) in keys.iter().enumerate() {
            self.draw_pad_key(c, t, *r, KEYS[i], i);
        }

        // Action row: backspace + call FAB.
        let (bk, call) = self.action_row(w, h);
        let bk_pressed = self.pressed == Some(Hit::Bksp);
        c.fill_rounded_rect(bk, 26.0, if bk_pressed { t.surface3 } else { t.surface1 });
        draw_icon(
            c,
            "\u{e14a}",
            32.0,
            t.text,
            bk.x + bk.w / 2,
            bk.y + bk.h / 2,
        );

        let call_pressed = self.pressed == Some(Hit::Call);
        let call_col = if call_pressed {
            mix(t.success, [0, 0, 0, 255], 0.15)
        } else {
            t.success
        };
        c.fill_rounded_rect(call, call.h as f32 / 2.0, call_col);
        draw_icon(
            c,
            "\u{e0b0}",
            46.0,
            t.text_on_accent,
            call.x + call.w / 2,
            call.y + call.h / 2,
        );
    }

    /// One pad key: surface card, accent state-layer when pressed, and a
    /// decaying scale "pop" for ~POP_S after release.
    fn draw_pad_key(&self, c: &mut Canvas, t: &Theme, r: Rect, key: DialKey, idx: usize) {
        let mut scale = 1.0f32;
        if let Some((pi, pt)) = self.pop {
            if pi == idx {
                let p = (pt / POP_S).min(1.0);
                scale = 1.0 + 0.07 * (1.0 - p) * (1.0 - p);
            }
        }
        let pressed = self.pressed == Some(Hit::Pad(idx));
        let mut rr = r;
        if pressed {
            let dw = (r.w as f32 * PRESS_SHRINK) as i32;
            let dh = (r.h as f32 * PRESS_SHRINK) as i32;
            rr = Rect::new(
                r.x + dw / 2,
                r.y + dh / 2,
                (r.w - dw).max(1),
                (r.h - dh).max(1),
            );
        } else if scale != 1.0 {
            let dw = (r.w as f32 * (scale - 1.0)) as i32;
            let dh = (r.h as f32 * (scale - 1.0)) as i32;
            rr = Rect::new(r.x - dw / 2, r.y - dh / 2, r.w + dw, r.h + dh);
        }
        let radius = (rr.h as f32 / 2.0).min(30.0);
        c.fill_rounded_rect(rr, radius, t.surface1);
        if pressed {
            c.fill_rounded_rect(rr, radius, alpha(t.accent, 0x30));
        }
        let digit_st = ui::styled(36.0, 600, t.text);
        ui::draw_centered(
            c,
            key.digit,
            &digit_st,
            Rect::new(rr.x, rr.y + rr.h / 2 - 32, rr.w, 46),
        );
        match key.sub {
            SubLabel::None => {}
            SubLabel::Letters(s) => {
                ui::draw_centered(
                    c,
                    s,
                    &ui::styled(14.0, 600, t.text3),
                    Rect::new(rr.x, rr.y + rr.h / 2 + 6, rr.w, 22),
                );
            }
            SubLabel::Icon(cp) => {
                draw_icon(c, cp, 16.0, t.text3, rr.x + rr.w / 2, rr.y + rr.h / 2 + 20);
            }
        }
    }

    fn render_recents(&mut self, c: &mut Canvas, t: &Theme, w: i32, h: i32) {
        let top = self.content_top();
        // Filter chips.
        let all_r = Rect::new(ui::MARGIN, top, 96, 40);
        c.fill_rounded_rect(
            all_r,
            20.0,
            if !self.missed_only {
                t.accent
            } else {
                t.surface2
            },
        );
        ui::draw_centered(
            c,
            "All",
            &ui::styled(
                16.0,
                600,
                if !self.missed_only {
                    t.text_on_accent
                } else {
                    t.text2
                },
            ),
            all_r,
        );

        let missed_n = self
            .recents
            .entries
            .iter()
            .filter(|e| e.dir == CallDir::Missed)
            .count();
        let mis_r = Rect::new(ui::MARGIN + 104, top, 160, 40);
        c.fill_rounded_rect(
            mis_r,
            20.0,
            if self.missed_only {
                t.accent
            } else {
                t.surface2
            },
        );
        ui::draw_centered(
            c,
            &format!("Missed ({missed_n})"),
            &ui::styled(
                16.0,
                600,
                if self.missed_only {
                    t.text_on_accent
                } else {
                    t.text2
                },
            ),
            mis_r,
        );

        if !self.recents.entries.is_empty() {
            let cr = Rect::new(w - ui::MARGIN - 100, top + 4, 100, 36);
            ui::draw_pill_button(
                c,
                t,
                cr,
                "Clear",
                false,
                self.pressed == Some(Hit::RecClear),
            );
        }

        let idxs = self.filtered_recents();
        let vp = self.recents_viewport(w, h);
        let now = now_unix();
        for (vis, &real) in idxs.iter().enumerate() {
            let r = self.recents_row_rect(w, vis);
            if r.y + r.h < vp.y || r.y > vp.y + vp.h {
                continue;
            }
            let e = &self.recents.entries[real];
            let rr = Rect::new(
                r.x,
                r.y.max(vp.y),
                r.w,
                (r.y + r.h).min(vp.y + vp.h) - r.y.max(vp.y),
            );
            if rr.h <= 0 {
                continue;
            }
            let row_pressed = self.pressed == Some(Hit::RecRow(real));
            c.fill_rounded_rect(rr, 24.0, if row_pressed { t.surface3 } else { t.surface1 });
            let av = Rect::new(rr.x + 14, rr.y + (rr.h - 58) / 2, 58, 58);
            let tint = if e.dir == CallDir::Missed {
                t.danger
            } else {
                entry_tint(&self.contacts, &e.name, &e.number)
            };
            draw_avatar(c, av, &e.name, tint);
            let name = if e.name.is_empty() {
                model::format_number(&e.number)
            } else {
                e.name.clone()
            };
            let name_col = if e.dir == CallDir::Missed {
                t.danger
            } else {
                t.text
            };
            ui::draw_text(
                c,
                &name,
                &ui::styled(21.0, 600, name_col),
                rr.x + 86,
                rr.y + 12,
                Some(rr.w - 200),
            );
            let dir_col = match e.dir {
                CallDir::Missed => t.danger,
                CallDir::In => t.success,
                CallDir::Out => t.accent,
            };
            let meta = format!(
                "{} · {} · {}",
                e.dir.label(),
                model::format_number(&e.number),
                model::when_label(now, e.unix)
            );
            draw_icon(c, e.dir.icon(), 18.0, dir_col, rr.x + 94, rr.y + 50);
            ui::draw_text(
                c,
                &meta,
                &ui::styled(15.0, 400, t.text3),
                rr.x + 110,
                rr.y + 42,
                Some(rr.w - 216),
            );
            let cb = Rect::new(rr.x + rr.w - 72, rr.y + (rr.h - 54) / 2, 54, 54);
            let cbp = self.pressed == Some(Hit::RecCall(real));
            c.fill_rounded_rect(cb, 27.0, alpha(t.success, if cbp { 0x66 } else { 0x24 }));
            draw_icon(c, "\u{e0b0}", 26.0, t.success, cb.x + 27, cb.y + 27);
        }

        if idxs.is_empty() {
            let msg = if self.missed_only {
                "No missed calls"
            } else {
                "No recent calls"
            };
            let ic = if self.missed_only {
                "\u{e0b4}"
            } else {
                "\u{e889}"
            };
            draw_icon(c, ic, 56.0, t.text3, vp.x + vp.w / 2, vp.y + vp.h / 2 - 40);
            ui::draw_centered(
                c,
                msg,
                &ui::styled(20.0, 400, t.text3),
                Rect::new(vp.x, vp.y + vp.h / 2 + 16, vp.w, 40),
            );
        }
    }

    fn render_contacts(&mut self, c: &mut Canvas, t: &Theme, w: i32, h: i32) {
        let top = self.content_top();
        let search = Rect::new(ui::MARGIN, top, w - ui::MARGIN * 2, 56);
        c.fill_rounded_rect(
            search,
            28.0,
            if self.search_focus {
                t.surface1
            } else {
                t.surface2
            },
        );
        draw_icon(c, "\u{e8b6}", 24.0, t.accent, search.x + 30, search.y + 28);
        let text_x = search.x + 52;
        let avail = search.x + search.w - text_x - if self.query.is_empty() { 16 } else { 48 };
        if self.query.is_empty() {
            ui::draw_text(
                c,
                "Search contacts or numbers",
                &ui::styled(19.0, 400, t.text3),
                text_x,
                search.y + 17,
                Some(avail),
            );
        } else {
            let st = fit_style(&self.query, &[21.0, 18.0, 16.0], 400, t.text, avail);
            ui::draw_text(c, &self.query, &st, text_x, search.y + 15, Some(avail));
        }
        if !self.query.is_empty() {
            let xr = Rect::new(search.x + search.w - 46, search.y + 14, 30, 30);
            draw_icon(c, "\u{e5cd}", 20.0, t.text3, xr.x + 15, xr.y + 15);
        }

        let showing_favs = self.query.trim().is_empty();
        if showing_favs {
            let y = top + 72 + 22;
            ui::draw_text(
                c,
                "FAVORITES",
                &ui::styled(14.0, 600, t.text3),
                ui::MARGIN,
                top + 72,
                Some(w),
            );
            let favs: Vec<&Contact> = self.contacts.iter().filter(|ct| ct.favorite).collect();
            let card_w = (w - ui::MARGIN * 2 - 24) / 3;
            for (i, ct) in favs.iter().enumerate() {
                let cx = ui::MARGIN + i as i32 * (card_w + 12);
                let card = Rect::new(cx, y, card_w, 108);
                let real = self.contact_index(ct);
                let pressed = self.pressed == Some(Hit::ConCall(real));
                c.fill_rounded_rect(card, 24.0, if pressed { t.surface3 } else { t.surface1 });
                draw_avatar(
                    c,
                    Rect::new(card.x + card.w / 2 - 28, card.y + 12, 56, 56),
                    &ct.name,
                    ct.color,
                );
                ui::draw_centered(
                    c,
                    &ct.name,
                    &ui::styled(15.0, 600, t.text),
                    Rect::new(card.x, card.y + 76, card.w, 20),
                );
                ui::draw_centered(
                    c,
                    "Tap to call",
                    &ui::styled(12.0, 400, t.text3),
                    Rect::new(card.x, card.y + 96, card.w, 16),
                );
            }
        }

        let rows_top = self.contacts_rows_top();
        ui::draw_text(
            c,
            if showing_favs {
                "ALL CONTACTS"
            } else {
                "RESULTS"
            },
            &ui::styled(14.0, 600, t.text3),
            ui::MARGIN,
            rows_top - 22,
            Some(w),
        );

        let vp_bottom = h - ui::BOTTOM_SAFE - 16;
        let list = self.filtered_contacts();
        for (vis, ct) in list.iter().enumerate() {
            let r = self.contact_row_rect(w, vis);
            if r.y + r.h < rows_top || r.y > vp_bottom {
                continue;
            }
            let rr = Rect::new(
                r.x,
                r.y.max(rows_top),
                r.w,
                (r.y + r.h).min(vp_bottom) - r.y.max(rows_top),
            );
            if rr.h <= 0 {
                continue;
            }
            let real = self.contact_index(ct);
            let row_pressed = self.pressed == Some(Hit::ConRow(real));
            c.fill_rounded_rect(rr, 24.0, if row_pressed { t.surface3 } else { t.surface1 });
            draw_avatar(
                c,
                Rect::new(rr.x + 14, rr.y + (rr.h - 56) / 2, 56, 56),
                &ct.name,
                ct.color,
            );
            ui::draw_text(
                c,
                &ct.name,
                &ui::styled(21.0, 600, t.text),
                rr.x + 84,
                rr.y + 14,
                Some(rr.w - 200),
            );
            let meta = format!("{} · {}", ct.label, model::format_number(&ct.number));
            ui::draw_text(
                c,
                &meta,
                &ui::styled(15.0, 400, t.text3),
                rr.x + 84,
                rr.y + 46,
                Some(rr.w - 200),
            );
            let cb = Rect::new(rr.x + rr.w - 72, rr.y + (rr.h - 54) / 2, 54, 54);
            let cbp = self.pressed == Some(Hit::ConCall(real));
            c.fill_rounded_rect(cb, 27.0, alpha(t.success, if cbp { 0x66 } else { 0x24 }));
            draw_icon(c, "\u{e0b0}", 26.0, t.success, cb.x + 27, cb.y + 27);
        }
        if list.is_empty() {
            ui::draw_centered(
                c,
                "No contacts match",
                &ui::styled(20.0, 400, t.text3),
                Rect::new(ui::MARGIN, rows_top + 120, w - ui::MARGIN * 2, 40),
            );
        }
    }

    // ---------------- in-call screen ----------------

    fn render_call(&mut self, c: &mut Canvas, t: &Theme, w: i32, h: i32) {
        // Snapshot everything needed, then drop the borrow before the
        // &mut self sub-renderers.
        let (phase, dtmf_open, number, status, name, tint) = {
            let call = self.call.as_ref().expect("call screen without call");
            let contact = model::lookup_in(&self.contacts, &call.number);
            let name = contact.map_or(String::new(), |ct| ct.name.clone());
            let tint = contact.map_or_else(|| hash_color(&call.number), |ct| ct.color);
            let status = match (call.dir, call.phase) {
                (CallDir::In, CallPhase::Ringing) if !call.answered => "Incoming call".to_string(),
                (CallDir::In, CallPhase::Ringing) => "Ringing…".to_string(),
                (CallDir::Out, CallPhase::Dialing) => "Dialing…".to_string(),
                (CallDir::Out, CallPhase::Ringing) => "Ringing…".to_string(),
                (_, CallPhase::Active) => call.elapsed_label(),
                // Defensive: sessions are only ever In+Ringing or Out+{Dialing,Ringing,Active}.
                (CallDir::Missed, _) | (CallDir::In, CallPhase::Dialing) => {
                    "Call ended".to_string()
                }
            };
            (
                call.phase,
                call.dtmf_open,
                call.number.clone(),
                status,
                name,
                tint,
            )
        };
        let cx = w / 2;

        // Avatar.
        let av = Rect::new(cx - 66, ui::TOP_SAFE + 150, 132, 132);
        c.fill_rounded_rect(av, 66.0, alpha(tint, 0x30));
        if name.is_empty() {
            draw_icon(c, "\u{e7fd}", 56.0, tint, cx, av.y + av.h / 2);
        } else {
            let initial: String = name.chars().next().unwrap_or('?').to_uppercase().collect();
            ui::draw_centered(c, &initial, &ui::styled(56.0, 600, tint), av);
        }

        let display_name = if name.is_empty() {
            model::format_number(&number)
        } else {
            name.clone()
        };
        ui::draw_centered(
            c,
            &display_name,
            &ui::styled(32.0, 600, t.text),
            Rect::new(ui::MARGIN, av.y + av.h + 26, w - ui::MARGIN * 2, 48),
        );
        if !name.is_empty() {
            ui::draw_centered(
                c,
                &model::format_number(&number),
                &ui::caption(t.text3),
                Rect::new(ui::MARGIN, av.y + av.h + 76, w - ui::MARGIN * 2, 30),
            );
        }

        let active = phase == CallPhase::Active;
        let st_col = if active { t.success } else { t.text2 };
        ui::draw_centered(
            c,
            &status,
            &ui::styled(24.0, if active { 600 } else { 400 }, st_col),
            Rect::new(ui::MARGIN, av.y + av.h + 116, w - ui::MARGIN * 2, 40),
        );

        if dtmf_open {
            self.render_dtmf(c, t, w, h);
        } else {
            self.render_call_controls(c, t, w, h);
        }
    }

    fn render_call_controls(&mut self, c: &mut Canvas, t: &Theme, w: i32, h: i32) {
        let incoming = self
            .call
            .as_ref()
            .is_some_and(|c| c.dir == CallDir::In && c.phase == CallPhase::Ringing && !c.answered);
        let (muted, speaker, dtmf) = self
            .call
            .as_ref()
            .map_or((false, false, false), |c| (c.muted, c.speaker, c.dtmf_open));

        if incoming {
            // Decline (left, danger) / Answer (right, success).
            let by = h - ui::BOTTOM_SAFE - 330;
            let dec = Rect::new(w / 2 - 150, by, 120, 120);
            let ans = Rect::new(w / 2 + 30, by, 120, 120);
            c.fill_rounded_rect(
                dec,
                60.0,
                if self.pressed == Some(Hit::Call) {
                    mix(t.danger, [0, 0, 0, 255], 0.15)
                } else {
                    t.danger
                },
            );
            draw_icon(
                c,
                "\u{e0b1}",
                48.0,
                t.text_on_accent,
                dec.x + 60,
                dec.y + 60,
            );
            c.fill_rounded_rect(
                ans,
                60.0,
                if self.pressed == Some(Hit::RingSim) {
                    mix(t.success, [0, 0, 0, 255], 0.15)
                } else {
                    t.success
                },
            );
            draw_icon(
                c,
                "\u{e0b0}",
                48.0,
                t.text_on_accent,
                ans.x + 60,
                ans.y + 60,
            );
            return;
        }

        // Toggle row: mute / keypad / speaker.
        let ty = h - ui::BOTTOM_SAFE - 310;
        let tw = 84;
        let gap = (w - ui::MARGIN * 2 - tw * 3) / 2;
        let toggles: [(Hit, &'static str, &'static str, bool); 3] = [
            (
                Hit::Pad(90),
                if muted { "\u{e02b}" } else { "\u{e029}" },
                if muted { "Unmute" } else { "Mute" },
                muted,
            ),
            (Hit::Pad(91), "\u{e0bc}", "Keypad", dtmf),
            (
                Hit::Pad(92),
                "\u{e32d}",
                if speaker { "Earpiece" } else { "Speaker" },
                speaker,
            ),
        ];
        for (i, (hit, ic, label, on)) in toggles.iter().enumerate() {
            let r = Rect::new(ui::MARGIN + i as i32 * (tw + gap), ty, tw, tw);
            let pressed = self.pressed == Some(*hit);
            c.fill_rounded_rect(
                r,
                42.0,
                if pressed || *on {
                    alpha(t.accent, 0x40)
                } else {
                    t.surface1
                },
            );
            draw_icon(
                c,
                ic,
                34.0,
                if *on { t.accent } else { t.text },
                r.x + r.w / 2,
                r.y + r.w / 2,
            );
            ui::draw_centered(
                c,
                label,
                &ui::styled(14.0, 400, if *on { t.accent } else { t.text2 }),
                Rect::new(r.x, r.y + tw + 8, r.w, 22),
            );
        }

        // End-call FAB.
        let end = Rect::new(w / 2 - 60, h - ui::BOTTOM_SAFE - 170, 120, 120);
        c.fill_rounded_rect(
            end,
            60.0,
            if self.pressed == Some(Hit::Call) {
                mix(t.danger, [0, 0, 0, 255], 0.15)
            } else {
                t.danger
            },
        );
        draw_icon(
            c,
            "\u{e0b1}",
            50.0,
            t.text_on_accent,
            end.x + 60,
            end.y + 60,
        );
    }

    fn render_dtmf(&mut self, c: &mut Canvas, t: &Theme, w: i32, h: i32) {
        for (i, r) in self.dtmf_key_rects(w, h).iter().enumerate() {
            let digit = model::DTMF_KEYS[i];
            let pressed = self.pressed == Some(Hit::Dtmf(i));
            let mut rr = *r;
            if pressed {
                let dw = (r.w as f32 * PRESS_SHRINK) as i32;
                let dh = (r.h as f32 * PRESS_SHRINK) as i32;
                rr = Rect::new(
                    r.x + dw / 2,
                    r.y + dh / 2,
                    (r.w - dw).max(1),
                    (r.h - dh).max(1),
                );
            }
            c.fill_rounded_rect(
                rr,
                (rr.h as f32 / 2.0).min(26.0),
                if pressed { t.surface3 } else { t.surface1 },
            );
            ui::draw_centered(
                c,
                digit,
                &ui::styled(34.0, 600, t.text),
                Rect::new(rr.x, rr.y + rr.h / 2 - 28, rr.w, 42),
            );
            // Letters under digits 2-9 (indices 1-8; same T9 map as the dial pad).
            if (1..=8).contains(&i) {
                if let SubLabel::Letters(s) = KEYS[i].sub {
                    ui::draw_centered(
                        c,
                        s,
                        &ui::styled(13.0, 600, t.text3),
                        Rect::new(rr.x, rr.y + rr.h / 2 + 8, rr.w, 20),
                    );
                }
            }
        }
        let hide = Rect::new(w / 2 - 90, h - ui::BOTTOM_SAFE - 70, 180, 52);
        ui::draw_pill_button(
            c,
            t,
            hide,
            "Hide keypad",
            false,
            self.pressed == Some(Hit::Bksp),
        );
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
