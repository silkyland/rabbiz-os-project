//! Layout geometry, hit-testing, and action dispatch for `DialerApp`.

use rabbiz_app_kit::{text, ui, widgets, Point, Rect, Size, TextInputCtl};
use wayland_protocols::wp::text_input::zv3::client::zwp_text_input_v3::ContentPurpose;

use crate::model::{self, CallDir, CallPhase, KEYS};

use super::chrome::ring_rect;
use super::{now_unix, DialerApp, Hit, Row, Tab, HEADER_H, ROW_H, ROW_PITCH};

/// Circular dialpad-key diameter.
pub(super) const KEY_D: i32 = 96;
/// Horizontal centre-to-centre spacing of pad columns.
const KEY_COL_PITCH: i32 = 148;
/// Vertical pitch of pad rows (circle + breathing room).
const KEY_ROW_PITCH: i32 = 108;
/// Call FAB diameter on the keypad sheet / end-call circle in-call.
const CALL_BTN_D: i32 = 96;
/// In-call control circle diameter.
const TOGGLE_D: i32 = 76;
/// In-call control grid cell pitch (button + label breathing room).
const TOGGLE_CELL: i32 = 128;

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
    // ---------------- list item model ----------------

    /// Recents flattened into scrollable rows: a "Today/Yesterday/…" header
    /// before each new day bucket, then the call entries under it.
    pub(super) fn recents_items(&self) -> Vec<Row> {
        let now = now_unix();
        let mut out = Vec::new();
        let mut last: Option<String> = None;
        for &real in &self.filtered_recents() {
            let e = &self.recents.entries[real];
            let label = model::day_section_label(now, e.unix);
            if last.as_ref() != Some(&label) {
                out.push(Row::Header(label.clone()));
                last = Some(label);
            }
            out.push(Row::Recent(real));
        }
        out
    }

    /// Contacts flattened into scrollable rows: alphabetical letter
    /// headers over the name-sorted, query-filtered list.
    pub(super) fn contacts_items(&self) -> Vec<Row> {
        let mut idx: Vec<usize> = self
            .contacts
            .iter()
            .enumerate()
            .filter(|(_, c)| model::contact_matches(c, &self.query))
            .map(|(i, _)| i)
            .collect();
        idx.sort_by(|&a, &b| {
            self.contacts[a]
                .name
                .to_lowercase()
                .cmp(&self.contacts[b].name.to_lowercase())
        });
        let mut out = Vec::new();
        let mut last: Option<String> = None;
        for i in idx {
            let letter = self.contacts[i]
                .name
                .chars()
                .next()
                .unwrap_or('#')
                .to_uppercase()
                .to_string();
            if last.as_ref() != Some(&letter) {
                out.push(Row::Header(letter.clone()));
                last = Some(letter);
            }
            out.push(Row::Contact(i));
        }
        out
    }

    /// Favorites shown on the Favorites tab (contact indices, stable order).
    pub(super) fn favorite_indices(&self) -> Vec<usize> {
        self.contacts
            .iter()
            .enumerate()
            .filter(|(_, c)| c.favorite)
            .filter(|(_, c)| model::contact_matches(c, &self.query))
            .map(|(i, _)| i)
            .collect()
    }

    /// Y of item `vis` in a list viewport, or the item's height band.
    /// Items walk cumulative heights: headers are `HEADER_H`, rows `ROW_PITCH`.
    pub(super) fn item_y(items: &[Row], vis: usize) -> (i32, i32) {
        let mut y = 0;
        for (i, item) in items.iter().enumerate() {
            let h = match item {
                Row::Header(_) => HEADER_H,
                _ => ROW_PITCH,
            };
            if i == vis {
                return (y, h);
            }
            y += h;
        }
        (y, 0)
    }

    pub(super) fn items_height(items: &[Row]) -> i32 {
        items
            .iter()
            .map(|item| match item {
                Row::Header(_) => HEADER_H,
                _ => ROW_PITCH,
            })
            .sum()
    }

    // ---------------- page chrome geometry ----------------

    /// Bottom navigation bar rect.
    pub(super) fn navbar_rect(&self, w: i32, h: i32) -> Rect {
        widgets::BottomTabBar::layout(Size::new(w, h), ui::BOTTOM_SAFE)
    }

    /// Shared SearchPill rect under the header.
    pub(super) fn search_rect(&self, w: i32) -> Rect {
        Rect::new(
            ui::MARGIN,
            ui::TOP_SAFE + 76,
            w - ui::MARGIN * 2,
            widgets::SearchPill::HEIGHT,
        )
    }

    /// Top of the per-page content area. Y-only; independent of `w`.
    pub(super) fn content_top(&self) -> i32 {
        // TOP_SAFE + 76 (search y) + 52 (pill) + 16 gap
        ui::TOP_SAFE + 144
    }

    /// Bottom edge usable by scrolling content (above the nav bar).
    pub(super) fn content_bottom(&self, w: i32, h: i32) -> i32 {
        self.navbar_rect(w, h).y - 8
    }

    /// Dialpad FAB floating above the nav bar, trailing edge.
    pub(super) fn fab_rect(&self, w: i32, h: i32) -> Rect {
        let d = 88;
        let nav = self.navbar_rect(w, h);
        Rect::new(w - ui::MARGIN - d, nav.y - 20 - d, d, d)
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

    /// Recents list viewport (below the filter chips, above the nav bar).
    pub(super) fn recents_viewport(&self, w: i32, h: i32) -> Rect {
        let top = self.content_top() + widgets::CHIP_H + 12;
        Rect::new(0, top, w, self.content_bottom(w, h) - top)
    }

    // ---------------- contacts ----------------

    /// Contacts list viewport (below the search pill, above the nav bar).
    pub(super) fn contacts_viewport(&self, w: i32, h: i32) -> Rect {
        let top = self.content_top();
        Rect::new(0, top, w, self.content_bottom(w, h) - top)
    }

    /// Rect of item `vis` (scrolled) within a viewport-topped list.
    pub(super) fn item_rect(&self, w: i32, items: &[Row], vis: usize, vp_top: i32) -> Rect {
        let (y, h) = Self::item_y(items, vis);
        let band = match items.get(vis) {
            Some(Row::Header(_)) => h,
            _ => ROW_H,
        };
        let _ = h;
        Rect::new(0, vp_top + y - self.scroll, w, band)
    }

    // ---------------- favorites grid ----------------

    /// One favorites-grid card: 3 columns, avatar+name tile.
    pub(super) fn fav_card_rect(&self, w: i32, h: i32, i: usize) -> Rect {
        let cols = 3;
        let gap = 16;
        let card_w = (w - ui::MARGIN * 2 - gap * (cols - 1)) / cols;
        let card_h = 168;
        let vp_top = self.content_top();
        let row = i as i32 / cols;
        let col = i as i32 % cols;
        let _ = h;
        Rect::new(
            ui::MARGIN + col * (card_w + gap),
            vp_top + row * (card_h + gap) - self.scroll,
            card_w,
            card_h,
        )
    }

    pub(super) fn favorites_height(&self, count: usize) -> i32 {
        let rows = (count as i32 + 2) / 3;
        rows * (168 + 16)
    }

    // ---------------- keypad sheet ----------------

    /// Bottom-sheet rect covering the lower screen when the keypad is open.
    pub(super) fn keypad_sheet(&self, w: i32, h: i32) -> Rect {
        // handle+display area (148) + 4 key rows + action row + bottom safe.
        let need = 148 + 3 * KEY_ROW_PITCH + KEY_D + 132 + ui::BOTTOM_SAFE;
        let sh = need.min(h - ui::TOP_SAFE - 24);
        widgets::BottomSheet::sheet_rect(Size::new(w, h), sh)
    }

    /// The sheet slid up by the open animation (0 ease = parked off-screen).
    pub(super) fn keypad_sheet_anim(&self, w: i32, h: i32) -> Rect {
        let mut r = self.keypad_sheet(w, h);
        let travel = ((1.0 - self.sheet_ease()) * (r.h as f32 + 48.0)).round() as i32;
        r.y += travel;
        r
    }

    /// 12 circular key rects for a sheet rect (shared by dial pad + DTMF).
    pub(super) fn key_rects_in(w: i32, sheet: Rect) -> Vec<Rect> {
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

    /// 12 circular key rects inside the (animated) keypad sheet.
    pub(super) fn pad_key_rects(&self, w: i32, h: i32) -> Vec<Rect> {
        Self::key_rects_in(w, self.keypad_sheet_anim(w, h))
    }

    /// In-call DTMF pad — same footprint, but the sheet is static in-call
    /// (the keypad-sheet open animation only applies to the dial pad).
    pub(super) fn dtmf_key_rects(&self, w: i32, h: i32) -> Vec<Rect> {
        Self::key_rects_in(w, self.keypad_sheet(w, h))
    }

    /// Bottom action row of the keypad sheet: (hide, call FAB, backspace).
    pub(super) fn action_row(&self, w: i32, h: i32) -> (Rect, Rect, Rect) {
        let sheet = self.keypad_sheet_anim(w, h);
        let y = sheet.bottom() - ui::BOTTOM_SAFE - 124;
        let call = Rect::new(w / 2 - CALL_BTN_D / 2, y, CALL_BTN_D, CALL_BTN_D);
        let hide = Rect::new(w / 2 - 156, y + 24, 48, 48);
        let bksp = Rect::new(w / 2 + 108, y + 24, 48, 48);
        (hide, call, bksp)
    }

    // ---------------- in-call geometry ----------------

    /// Caller identity zone (avatar + name + number + status) — open air
    /// over the tinted gradient, no containing card.
    pub(super) fn caller_zone(&self, w: i32) -> Rect {
        Rect::new(ui::MARGIN, ui::TOP_SAFE + 24, w - ui::MARGIN * 2, 400)
    }

    /// 2×3 control grid with labels: row 0 = (mute, keypad, speaker),
    /// row 1 = reserved slots (drawn disabled). Centres at w/2 ±128.
    pub(super) fn in_call_grid(&self, w: i32, h: i32) -> [[Rect; 3]; 2] {
        let end = self.end_call_rect(w, h);
        let row2_y = end.y - 24 - TOGGLE_CELL;
        let row1_y = row2_y - TOGGLE_CELL;
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
        let by = h - ui::BOTTOM_SAFE - 280;
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
        if let Some(i) = widgets::BottomTabBar::hit(self.navbar_rect(w, h), pos, Tab::ALL.len()) {
            return Some(Hit::Tab(Tab::ALL[i]));
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
        if self.fab_rect(w, h).contains(pos) {
            return Some(Hit::KeypadFab);
        }
        match self.tab {
            Tab::Favorites => self.favorites_hit(w, h, pos),
            Tab::Recents => self.recents_hit(w, h, pos),
            Tab::Contacts => self.contacts_hit(w, h, pos),
        }
    }

    fn keypad_hit(&self, w: i32, h: i32, pos: Point) -> Option<Hit> {
        let sheet = self.keypad_sheet_anim(w, h);
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

    fn favorites_hit(&self, w: i32, h: i32, pos: Point) -> Option<Hit> {
        let vp = Rect::new(0, self.content_top(), w, self.content_bottom(w, h) - self.content_top());
        if !vp.contains(pos) {
            return None;
        }
        for (vis, &real) in self.favorite_indices().iter().enumerate() {
            if self.fav_card_rect(w, h, vis).contains(pos) {
                return Some(Hit::ConCall(real));
            }
        }
        Some(Hit::List)
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
        let items = self.recents_items();
        let vp_top = vp.y;
        for (vis, item) in items.iter().enumerate() {
            let Row::Recent(real) = item else {
                continue;
            };
            let r = self.item_rect(w, &items, vis, vp_top);
            if !r.contains(pos) {
                continue;
            }
            if row_call_rect(r).contains(pos) {
                return Some(Hit::RecCall(*real));
            }
            return Some(Hit::RecRow(*real));
        }
        Some(Hit::List)
    }

    fn contacts_hit(&self, w: i32, h: i32, pos: Point) -> Option<Hit> {
        let vp = self.contacts_viewport(w, h);
        if !vp.contains(pos) {
            return None;
        }
        let items = self.contacts_items();
        let vp_top = vp.y;
        for (vis, item) in items.iter().enumerate() {
            let Row::Contact(real) = item else {
                continue;
            };
            let r = self.item_rect(w, &items, vis, vp_top);
            if !r.contains(pos) {
                continue;
            }
            if row_call_rect(r).contains(pos) {
                return Some(Hit::ConCall(*real));
            }
            return Some(Hit::ConRow(*real));
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
