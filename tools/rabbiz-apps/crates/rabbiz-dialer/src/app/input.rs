//! Layout geometry, hit-testing, and action dispatch for `DialerApp`.

use rabbiz_app_kit::{ui, Point, Rect, TextInputCtl};
use wayland_protocols::wp::text_input::zv3::client::zwp_text_input_v3::ContentPurpose;

use crate::model::{self, CallDir, CallPhase, Contact, KEYS};

use super::{DialerApp, Hit, Tab, CON_PITCH, CON_ROW_H, REC_PITCH, REC_ROW_H};

impl DialerApp {
    // ---------------- geometry ----------------

    pub(super) fn tabs_rect(&self, w: i32) -> Rect {
        Rect::new(ui::MARGIN, ui::TOP_SAFE + 92, w - ui::MARGIN * 2, 46)
    }

    pub(super) fn content_top(&self) -> i32 {
        ui::TOP_SAFE + 92 + 46 + 16
    }

    /// i-th segment rect inside the tab pill.
    pub(super) fn tab_rect(&self, w: i32, idx: usize) -> Rect {
        let t = self.tabs_rect(w);
        let n = Tab::ALL.len() as i32;
        let gap = 8;
        let bw = (t.w - gap * (n - 1)) / n;
        Rect::new(t.x + idx as i32 * (bw + gap), t.y, bw, t.h)
    }

    /// Bottom action row on the keypad page: (backspace, call FAB).
    pub(super) fn action_row(&self, w: i32, h: i32) -> (Rect, Rect) {
        let y = h - ui::BOTTOM_SAFE - 108;
        let bk = Rect::new(ui::MARGIN, y + 12, 72, 72);
        let call = Rect::new(w - ui::MARGIN - 108, y, 108, 108);
        (bk, call)
    }

    /// Keypad grid rects (12 keys), bottom-anchored above the action row.
    pub(super) fn pad_key_rects(&self, w: i32, h: i32) -> Vec<Rect> {
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
    pub(super) fn dtmf_key_rects(&self, w: i32, h: i32) -> Vec<Rect> {
        self.pad_key_rects(w, h)
            .iter()
            .map(|r| Rect::new(r.x, r.y + r.h / 6, r.w, r.h * 2 / 3))
            .collect()
    }

    /// Top of the contacts rows list (below the fixed favorites strip when
    /// no search query is active).
    pub(super) fn contacts_rows_top(&self) -> i32 {
        let top = self.content_top();
        if self.query.trim().is_empty() {
            top + 72 + 22 + 108 + 14 + 22 // label + fav cards + gap + section label
        } else {
            top + 72 + 14
        }
    }

    /// Contacts list viewport = rows area only (favorites/search are fixed).
    pub(super) fn contacts_viewport(&self, w: i32, h: i32) -> Rect {
        let top = self.contacts_rows_top();
        Rect::new(
            ui::MARGIN,
            top,
            w - ui::MARGIN * 2,
            h - ui::BOTTOM_SAFE - 16 - top,
        )
    }

    /// Recents list viewport (below the filter chips).
    pub(super) fn recents_viewport(&self, w: i32, h: i32) -> Rect {
        let top = self.content_top() + 52;
        Rect::new(
            ui::MARGIN,
            top,
            w - ui::MARGIN * 2,
            h - ui::BOTTOM_SAFE - 16 - top,
        )
    }

    pub(super) fn contact_row_rect(&self, w: i32, i: usize) -> Rect {
        let top = self.contacts_rows_top();
        Rect::new(
            ui::MARGIN,
            top + i as i32 * CON_PITCH - self.scroll,
            w - ui::MARGIN * 2,
            CON_ROW_H,
        )
    }

    pub(super) fn recents_row_rect(&self, w: i32, i: usize) -> Rect {
        let top = self.recents_viewport(w, 0).y;
        Rect::new(
            ui::MARGIN,
            top + i as i32 * REC_PITCH - self.scroll,
            w - ui::MARGIN * 2,
            REC_ROW_H,
        )
    }

    // ---------------- hit testing ----------------

    pub(super) fn hit_at(&self, w: i32, h: i32, pos: Point) -> Option<Hit> {
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

    pub(super) fn dispatch(&mut self, hit: Hit, ti: &TextInputCtl) {
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
