//! Layout geometry, hit-testing, and action dispatch for `DialerApp`.

use rabbiz_app_kit::{text, ui, widgets, Point, Rect, Size, TextInputCtl};
use wayland_protocols::wp::text_input::zv3::client::zwp_text_input_v3::ContentPurpose;

use crate::model::{self, CallDir, CallPhase, KEYS};

use super::chrome::ring_rect;
use super::{DialerApp, Hit, Tab, CON_PITCH, CON_ROW_H, REC_PITCH, REC_ROW_H};

/// Circular dialpad-key diameter.
pub(super) const KEY_D: i32 = 88;
/// Horizontal centre-to-centre spacing of pad columns.
const KEY_COL_PITCH: i32 = 140;
/// Vertical pitch of pad rows (circle + breathing room).
const KEY_ROW_PITCH: i32 = 104;
/// Call FAB diameter on the keypad sheet / end-call circle in-call.
const CALL_BTN_D: i32 = 96;
/// In-call toggle circle diameter.
const TOGGLE_D: i32 = 84;
/// Favorites card geometry (fixed 3-column strip).
const FAV_CARD_H: i32 = 104;

/// Chip label width mirroring `widgets::chip_width` (theme-free — the
/// label style is color-independent, so a dummy color measures the same).
fn chip_w(label: &str) -> i32 {
    text::label(label, ui::label([0, 0, 0, 255]), None).map_or(48, |px| px.width() as i32) + 32
}

/// 48px call-back button on the right edge of a list row.
pub(super) fn row_call_rect(row: Rect) -> Rect {
    Rect::new(
        row.right() - ui::MARGIN - 48,
        row.y + (row.h - 48) / 2,
        48,
        48,
    )
}

impl DialerApp {
    // ---------------- page chrome geometry ----------------

    /// Shared SearchPill rect under the header.
    pub(super) fn search_rect(&self, w: i32) -> Rect {
        Rect::new(
            ui::MARGIN,
            ui::TOP_SAFE + 76,
            w - ui::MARGIN * 2,
            widgets::SearchPill::HEIGHT,
        )
    }

    /// Recents|Contacts segmented bar.
    pub(super) fn tabs_rect(&self, w: i32) -> Rect {
        let s = self.search_rect(w);
        Rect::new(ui::MARGIN, s.bottom() + 12, w - ui::MARGIN * 2, 48)
    }

    /// Top of the per-page content area (chips on Recents, favorites on
    /// Contacts). Y-only; independent of `w`.
    pub(super) fn content_top(&self) -> i32 {
        // TOP_SAFE + 76 (search y) + 52 (pill) + 12 + 48 (tabs) + 12
        ui::TOP_SAFE + 200
    }

    /// Dialpad FAB aligned to the trailing content edge.
    pub(super) fn fab_rect(&self, w: i32, h: i32) -> Rect {
        let d = 84;
        Rect::new(w - ui::MARGIN - d, h - ui::BOTTOM_SAFE - 16 - d, d, d)
    }

    // ---------------- recents ----------------

    /// `(All, Missed, Clear?)` chip rects — the Clear chip only exists when
    /// the log is non-empty. Measured widths, no fixed x.
    pub(super) fn recents_chip_rects(&self, w: i32) -> (Rect, Rect, Option<Rect>) {
        let y = self.content_top();
        let missed_n = self
            .recents
            .entries
            .iter()
            .filter(|e| e.dir == CallDir::Missed)
            .count();
        let all_w = chip_w("All");
        let mis_w = chip_w(&format!("Missed ({missed_n})"));
        let all = Rect::new(ui::MARGIN, y, all_w, widgets::CHIP_H);
        let mis = Rect::new(all.right() + 8, y, mis_w, widgets::CHIP_H);
        let clear = (!self.recents.entries.is_empty()).then(|| {
            let cw = chip_w("Clear");
            Rect::new(w - ui::MARGIN - cw, y, cw, widgets::CHIP_H)
        });
        (all, mis, clear)
    }

    /// Recents list viewport (below the filter chips).
    pub(super) fn recents_viewport(&self, w: i32, h: i32) -> Rect {
        let top = self.content_top() + widgets::CHIP_H + 12;
        Rect::new(0, top, w, h - ui::BOTTOM_SAFE - 16 - top)
    }

    pub(super) fn recents_row_rect(&self, w: i32, i: usize) -> Rect {
        let top = self.recents_viewport(w, 0).y;
        Rect::new(0, top + i as i32 * REC_PITCH - self.scroll, w, REC_ROW_H)
    }

    // ---------------- contacts ----------------

    /// Favorites strip is only shown when not searching and favorites exist.
    pub(super) fn show_favorites(&self) -> bool {
        self.query.trim().is_empty() && self.contacts.iter().any(|c| c.favorite)
    }

    /// One favorites-strip card — the strip is capped at 3 columns, so the
    /// 4th+ favorite no longer paints off the edge.
    pub(super) fn fav_card_rect(&self, w: i32, i: usize) -> Rect {
        let card_w = (w - ui::MARGIN * 2 - 24) / 3;
        Rect::new(
            ui::MARGIN + i as i32 * (card_w + 12),
            self.content_top() + 30,
            card_w,
            FAV_CARD_H,
        )
    }

    /// Y of the "ALL CONTACTS"/"RESULTS" caption.
    pub(super) fn contacts_rows_label_y(&self) -> i32 {
        let top = self.content_top();
        if self.show_favorites() {
            top + 30 + FAV_CARD_H + 16
        } else {
            top + 4
        }
    }

    /// Top of the contacts rows list (below the fixed favorites strip).
    pub(super) fn contacts_rows_top(&self) -> i32 {
        self.contacts_rows_label_y() + 26
    }

    /// Contacts list viewport = rows area only (favorites are fixed).
    pub(super) fn contacts_viewport(&self, w: i32, h: i32) -> Rect {
        let top = self.contacts_rows_top();
        Rect::new(0, top, w, h - ui::BOTTOM_SAFE - 16 - top)
    }

    pub(super) fn contact_row_rect(&self, w: i32, i: usize) -> Rect {
        let top = self.contacts_rows_top();
        Rect::new(0, top + i as i32 * CON_PITCH - self.scroll, w, CON_ROW_H)
    }

    // ---------------- keypad sheet ----------------

    /// Bottom-sheet rect covering the lower screen when the keypad is open.
    pub(super) fn keypad_sheet(&self, w: i32, h: i32) -> Rect {
        // handle+display area (148) + 4 key rows + action row + bottom safe.
        let need = 148 + 3 * KEY_ROW_PITCH + KEY_D + 132 + ui::BOTTOM_SAFE;
        let sh = need.min(h - ui::TOP_SAFE - 24);
        widgets::BottomSheet::sheet_rect(Size::new(w, h), sh)
    }

    /// 12 circular key rects inside the keypad sheet.
    pub(super) fn pad_key_rects(&self, w: i32, h: i32) -> Vec<Rect> {
        let sheet = self.keypad_sheet(w, h);
        let grid_top = sheet.y + 148;
        (0..KEYS.len())
            .map(|i| {
                let r = i as i32 / 3;
                let col = i as i32 % 3;
                let cx = w / 2 + (col - 1) * KEY_COL_PITCH;
                Rect::new(cx - KEY_D / 2, grid_top + r * KEY_ROW_PITCH, KEY_D, KEY_D)
            })
            .collect()
    }

    /// In-call DTMF pad — same footprint as the dial pad.
    pub(super) fn dtmf_key_rects(&self, w: i32, h: i32) -> Vec<Rect> {
        self.pad_key_rects(w, h)
    }

    /// Bottom action row of the keypad sheet: (hide, call FAB, backspace).
    pub(super) fn action_row(&self, w: i32, h: i32) -> (Rect, Rect, Rect) {
        let sheet = self.keypad_sheet(w, h);
        let y = sheet.bottom() - ui::BOTTOM_SAFE - 124;
        let call = Rect::new(w / 2 - CALL_BTN_D / 2, y, CALL_BTN_D, CALL_BTN_D);
        let hide = Rect::new(w / 2 - 156, y + 24, 48, 48);
        let bksp = Rect::new(w / 2 + 108, y + 24, 48, 48);
        (hide, call, bksp)
    }

    // ---------------- in-call geometry ----------------

    /// Caller card (avatar + name + number + status) — a surface panel so
    /// the avatar sits on a real container, not the flat page bg.
    pub(super) fn caller_card(&self, w: i32) -> Rect {
        Rect::new(ui::MARGIN, ui::TOP_SAFE + 40, w - ui::MARGIN * 2, 420)
    }

    /// 2×3 toggle grid: row 0 = the real toggles (mute/keypad/speaker),
    /// row 1 = reserved slots (drawn disabled). Centers at w/2 ±128.
    pub(super) fn in_call_grid(&self, w: i32, h: i32) -> [[Rect; 3]; 2] {
        let end = self.end_call_rect(w, h);
        let row2_y = end.y - 32 - TOGGLE_D - 32;
        let row1_y = row2_y - TOGGLE_D - 44;
        let mk = |col: i32, y: i32| Rect::new(w / 2 + col - TOGGLE_D / 2, y, TOGGLE_D, TOGGLE_D);
        [
            [mk(-128, row1_y), mk(0, row1_y), mk(128, row1_y)],
            [mk(-128, row2_y), mk(0, row2_y), mk(128, row2_y)],
        ]
    }

    /// Big red end-call circle, bottom center.
    pub(super) fn end_call_rect(&self, w: i32, h: i32) -> Rect {
        Rect::new(w / 2 - 60, h - ui::BOTTOM_SAFE - 20 - 120, 120, 120)
    }

    /// Incoming-call (decline, answer) circles + caption bands under them.
    pub(super) fn incoming_rects(&self, w: i32, h: i32) -> (Rect, Rect, Rect, Rect) {
        let by = h - ui::BOTTOM_SAFE - 300;
        let dec = Rect::new(w / 2 - 190, by, 120, 120);
        let ans = Rect::new(w / 2 + 70, by, 120, 120);
        let dec_lbl = Rect::new(dec.x - 20, by + 128, 160, 30);
        let ans_lbl = Rect::new(ans.x - 20, by + 128, 160, 30);
        (dec, ans, dec_lbl, ans_lbl)
    }

    /// "Hide keypad" pill under the DTMF pad.
    pub(super) fn dtmf_hide_rect(&self, w: i32, h: i32) -> Rect {
        Rect::new(w / 2 - 90, h - ui::BOTTOM_SAFE - 16 - 52, 180, 52)
    }

    // ---------------- hit testing ----------------

    pub(super) fn hit_at(&self, w: i32, h: i32, pos: Point) -> Option<Hit> {
        if self.call.is_some() {
            return self.call_hit(w, h, pos);
        }
        if self.keypad_open {
            return self.keypad_hit(w, h, pos);
        }
        if self.backend.can_simulate_incoming() && ring_rect(w).contains(pos) {
            return Some(Hit::RingSim);
        }
        if let Some(h) = widgets::SearchPill::hit(self.search_rect(w), pos, !self.query.is_empty())
        {
            return Some(match h {
                widgets::SearchHit::Clear => Hit::SearchClear,
                widgets::SearchHit::Field => Hit::Search,
            });
        }
        if let Some(i) = widgets::segmented_hit(self.tabs_rect(w), pos, Tab::ALL.len()) {
            return Some(Hit::Tab(Tab::ALL[i]));
        }
        if self.fab_rect(w, h).contains(pos) {
            return Some(Hit::KeypadFab);
        }
        match self.tab {
            Tab::Recents => self.recents_hit(w, h, pos),
            Tab::Contacts => self.contacts_hit(w, h, pos),
        }
    }

    fn keypad_hit(&self, w: i32, h: i32, pos: Point) -> Option<Hit> {
        let sheet = self.keypad_sheet(w, h);
        if !sheet.contains(pos) {
            return Some(Hit::KeypadHide); // scrim tap dismisses the sheet
        }
        let (hide, call, bksp) = self.action_row(w, h);
        if call.contains(pos) {
            return Some(Hit::Call);
        }
        if hide.contains(pos) {
            return Some(Hit::KeypadHide);
        }
        if bksp.contains(pos) {
            return Some(Hit::Bksp);
        }
        for (i, r) in self.pad_key_rects(w, h).iter().enumerate() {
            if r.contains(pos) {
                return Some(Hit::Pad(i));
            }
        }
        Some(Hit::List) // inside the sheet: swallow, no fall-through
    }

    fn recents_hit(&self, w: i32, h: i32, pos: Point) -> Option<Hit> {
        let (all, mis, clear) = self.recents_chip_rects(w);
        if all.contains(pos) {
            return Some(Hit::FilterAll);
        }
        if mis.contains(pos) {
            return Some(Hit::FilterMissed);
        }
        if clear.is_some_and(|r| r.contains(pos)) {
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
            if row_call_rect(r).contains(pos) {
                return Some(Hit::RecCall(real));
            }
            return Some(Hit::RecRow(real));
        }
        Some(Hit::List)
    }

    fn contacts_hit(&self, w: i32, h: i32, pos: Point) -> Option<Hit> {
        // Favorites strip (fixed, capped at 3, only when not searching).
        if self.show_favorites() {
            for (i, ct) in self
                .contacts
                .iter()
                .filter(|c| c.favorite)
                .take(3)
                .enumerate()
            {
                if self.fav_card_rect(w, i).contains(pos) {
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
            if row_call_rect(r).contains(pos) {
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
            if self.dtmf_hide_rect(w, h).contains(pos) {
                return Some(Hit::Bksp); // toggles the DTMF pad closed
            }
            return None;
        }
        let incoming = c.dir == CallDir::In && c.phase == CallPhase::Ringing && !c.answered;
        if incoming {
            let (dec, ans, _, _) = self.incoming_rects(w, h);
            if dec.contains(pos) {
                return Some(Hit::Call); // decline → logged as missed
            }
            if ans.contains(pos) {
                return Some(Hit::RingSim); // answer
            }
            return None;
        }
        // Explicit toggle hits — no sentinel Pad indices.
        let grid = self.in_call_grid(w, h);
        let hits = [Hit::Mute, Hit::DtmfToggle, Hit::Speaker];
        for (i, r) in grid[0].iter().enumerate() {
            if r.contains(pos) {
                return Some(hits[i]);
            }
        }
        if self.end_call_rect(w, h).contains(pos) {
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
            Hit::Dtmf(i) => {
                if i < model::DTMF_KEYS.len() {
                    let d = model::DTMF_KEYS[i];
                    self.send_dtmf(d);
                    self.pop = Some((i + 100, 0.0));
                }
            }
            Hit::Mute => {
                if let Some(c) = self.call.as_mut() {
                    c.muted = !c.muted;
                }
            }
            Hit::DtmfToggle => {
                if let Some(c) = self.call.as_mut() {
                    c.dtmf_open = !c.dtmf_open;
                }
            }
            Hit::Speaker => {
                if let Some(c) = self.call.as_mut() {
                    c.speaker = !c.speaker;
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
                } else if self.keypad_open {
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
                    self.keypad_open = true;
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
                    self.keypad_open = true;
                    self.scroll = 0;
                }
            }
            Hit::ConCall(i) => {
                if let Some(ct) = self.contacts.get(i) {
                    let n = ct.number.clone();
                    self.place_call(&n);
                }
            }
            Hit::KeypadFab => {
                self.leave_search(ti);
                self.keypad_open = true;
            }
            Hit::KeypadHide => {
                self.keypad_open = false;
            }
            Hit::List | Hit::Pad(_) => {}
        }
    }
}
