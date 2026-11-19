//! Geometry calculation, scroll bounds, and hit-testing for `rabbiz-contacts`.

use de_design::motion::{bezier_eval, BEZIER_ENTER, STAGGER_MAX, STAGGER_MS};
use rabbiz_app_kit::ui as kit_ui;
use rabbiz_app_kit::{Point, Rect};

use crate::{model, ContactsApp, Focus, Hit, Screen};

// ---- Layout constants (px; all geometry derives from the live canvas size) ----
pub(crate) const SEARCH_H: i32 = 56;
pub(crate) const ROW_PITCH: i32 = 100;
pub(crate) const ROW_H: i32 = 88;
pub(crate) const FAV_CARD_H: i32 = 104;
pub(crate) const FAB_D: i32 = 56;
pub(crate) const FIELD_H: i32 = 76;
pub(crate) const PHONE_CARD_H: i32 = 128;
pub(crate) const EMAIL_CARD_H: i32 = 128;
pub(crate) const NOTE_H: i32 = 104;
pub(crate) const SECTION_GAP: i32 = 18;
pub(crate) const HEADER_H: i32 = 104;
pub(crate) const PHONE_ROW_H: i32 = 84;
pub(crate) const EMAIL_ROW_H: i32 = 76;
/// First-paint stagger budget (wall-clock; no Tick needed — see
/// `stagger_offset`). Matches notes' intro feel.
const INTRO_TOTAL_MS: f32 = STAGGER_MAX as f32 * STAGGER_MS as f32 + 200.0;

// ---- Layout structs (computed identically for render and hit-test) ----

#[derive(Debug, Clone, Copy)]
pub(crate) struct ListGeom {
    pub(crate) search: Rect,
    pub(crate) favs_label_y: i32, // y of the "FAVORITES" caption (0 when hidden)
    pub(crate) favs_top: i32,     // y of favorites cards (0 when hidden)
    pub(crate) section_y: i32,    // y of "ALL CONTACTS"/"RESULTS" caption
    pub(crate) vp: Rect,
    pub(crate) fab: Rect,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct DetailGeom {
    pub(crate) back: Rect,
    pub(crate) edit: Rect,
    pub(crate) del: Rect,
    pub(crate) fav: Rect,
    pub(crate) call: Rect,
    pub(crate) message: Rect,
    pub(crate) vp: Rect,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct FormGeom {
    pub(crate) back: Rect,
    pub(crate) vp: Rect,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum FormItem {
    Label(&'static str, i32),
    Name {
        card: Rect,
    },
    Phone {
        idx: usize,
        card: Rect,
        label: Rect,
        number: Rect,
        remove: Rect,
    },
    AddPhone {
        rect: Rect,
    },
    Email {
        idx: usize,
        card: Rect,
        label: Rect,
        address: Rect,
        remove: Rect,
    },
    AddEmail {
        rect: Rect,
    },
    Note {
        card: Rect,
    },
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum DetailItem {
    /// Contact header block (avatar + name) at content-y.
    Header {
        y: i32,
    },
    Label(&'static str, i32), // section caption at content-y
    Phone {
        idx: usize,
        y: i32,
        h: i32,
        call: Rect,
        sms: Rect,
    },
    Email {
        idx: usize,
        y: i32,
        h: i32,
    },
    NoteCard {
        y: i32,
        h: i32,
    },
}

pub(crate) fn save_bar(w: i32, h: i32) -> Rect {
    Rect::new(
        kit_ui::MARGIN,
        h - kit_ui::BOTTOM_SAFE - 16 - 72,
        w - kit_ui::MARGIN * 2,
        72,
    )
}

pub(crate) fn call_bar(w: i32, h: i32) -> Rect {
    Rect::new(
        kit_ui::MARGIN,
        h - kit_ui::BOTTOM_SAFE - 16 - 72,
        w - kit_ui::MARGIN * 2 - 140,
        72,
    )
}

pub(crate) fn message_bar(w: i32, h: i32) -> Rect {
    Rect::new(
        w - kit_ui::MARGIN - 132,
        h - kit_ui::BOTTOM_SAFE - 16 - 72,
        132,
        72,
    )
}

impl ContactsApp {
    // ---------------- geometry ----------------

    pub(crate) fn list_geom(&self, w: i32, h: i32) -> ListGeom {
        let m = kit_ui::MARGIN;
        let header_bottom = kit_ui::TOP_SAFE + 16 + 80; // title + optional subtitle
        let search = Rect::new(m, header_bottom + 8, w - m * 2, SEARCH_H);
        let show_favs = self.search.text.trim().is_empty() && !self.favorites().is_empty();
        // Cursor walk: search → [FAVORITES label + cards] → section label → vp.
        let mut y = search.bottom() + 12;
        let mut favs_label_y = 0;
        let mut favs_top = 0;
        if show_favs {
            favs_label_y = y;
            y += 24;
            favs_top = y;
            y += FAV_CARD_H + 14;
        }
        let section_y = y;
        y += 26; // caption + gap before the rows
        let vp = Rect::new(m, y, w - m * 2, (h - kit_ui::BOTTOM_SAFE - 16 - y).max(120));
        let fab = Rect::new(
            w - m - FAB_D,
            h - kit_ui::BOTTOM_SAFE - 16 - FAB_D,
            FAB_D,
            FAB_D,
        );
        ListGeom {
            search,
            favs_label_y,
            favs_top,
            section_y,
            vp,
            fab,
        }
    }

    pub(crate) fn detail_geom(&self, w: i32, h: i32) -> DetailGeom {
        let m = kit_ui::MARGIN;
        let y = kit_ui::TOP_SAFE + 12;
        let back = Rect::new(m, y, 120, 48);
        let del = Rect::new(w - m - 104, y, 104, 48);
        let edit = Rect::new(del.x - 92 - 8, y, 92, 48);
        let fav = Rect::new(edit.x - 52 - 8, y, 52, 48);
        let vp_top = y + 48 + 12;
        let vp = Rect::new(m, vp_top, w - m * 2, h - kit_ui::BOTTOM_SAFE - 16 - vp_top);
        DetailGeom {
            back,
            edit,
            del,
            fav,
            call: call_bar(w, h),
            message: message_bar(w, h),
            vp,
        }
    }

    pub(crate) fn form_geom(&self, w: i32, h: i32) -> FormGeom {
        let m = kit_ui::MARGIN;
        let y = kit_ui::TOP_SAFE + 12;
        let back = Rect::new(m, y, 110, 48);
        let vp_top = y + 48 + 12;
        // Leave room for the fixed Save bar at the bottom.
        let vp = Rect::new(
            m,
            vp_top,
            w - m * 2,
            h - kit_ui::BOTTOM_SAFE - 16 - 84 - vp_top,
        );
        FormGeom { back, vp }
    }

    /// Contact index under `pos` in the list viewport, honoring scroll and
    /// the first-paint stagger offset.
    pub(crate) fn list_row_at(&self, g: &ListGeom, pos: Point) -> Option<usize> {
        if !g.vp.contains(pos) {
            return None;
        }
        let ids = self.filtered();
        let dy = pos.y - g.vp.y + self.scroll;
        if dy < 0 {
            return None;
        }
        let vis = dy / ROW_PITCH;
        let staggered_top =
            g.vp.y + vis * ROW_PITCH - self.scroll + self.stagger_offset(vis as usize);
        if pos.y < staggered_top || pos.y >= staggered_top + ROW_H {
            return None;
        }
        ids.get(vis as usize).copied()
    }

    // ---------------- detail items (shared by render + hit-test) -----------

    /// Ordered content items for the detail screen in *screen* coords
    /// (scroll applied). `content_h` is the un-scrolled height.
    pub(crate) fn detail_items(&self, w: i32) -> (Vec<DetailItem>, i32) {
        let m = kit_ui::MARGIN;
        let mut items = Vec::new();
        let mut y = 0i32;
        let Some(c) = self.open_contact() else {
            return (items, 0);
        };

        // Header block: avatar + name (+ primary number caption).
        items.push(DetailItem::Header { y: 0 });
        y += HEADER_H + 8;

        // Phones.
        let phones: Vec<(usize, &model::Phone)> = c
            .phones
            .iter()
            .enumerate()
            .filter(|(_, p)| !p.number.trim().is_empty())
            .collect();
        if !phones.is_empty() {
            items.push(DetailItem::Label("PHONES", y));
            y += 26;
            for (idx, _) in &phones {
                let row_y = y;
                let call = Rect::new(w - m - 120, row_y + 12, 56, 56);
                let sms = Rect::new(w - m - 184, row_y + 12, 56, 56);
                items.push(DetailItem::Phone {
                    idx: *idx,
                    y: row_y,
                    h: PHONE_ROW_H,
                    call,
                    sms,
                });
                y += PHONE_ROW_H + 10;
            }
            y += SECTION_GAP;
        }

        // Emails.
        let emails: Vec<(usize, &model::Email)> = c
            .emails
            .iter()
            .enumerate()
            .filter(|(_, e)| !e.address.trim().is_empty())
            .collect();
        if !emails.is_empty() {
            items.push(DetailItem::Label("EMAILS", y));
            y += 26;
            for (idx, _) in &emails {
                items.push(DetailItem::Email {
                    idx: *idx,
                    y,
                    h: EMAIL_ROW_H,
                });
                y += EMAIL_ROW_H + 10;
            }
            y += SECTION_GAP;
        }

        // Note.
        if !c.note.trim().is_empty() {
            items.push(DetailItem::Label("NOTE", y));
            y += 26;
            let lines = c.note.lines().count().max(1) as i32;
            let h = (40 + lines * 30).clamp(72, 220);
            items.push(DetailItem::NoteCard { y, h });
            y += h + SECTION_GAP;
        }

        // Nothing else? Show a hint line.
        if phones.is_empty() && emails.is_empty() && c.note.trim().is_empty() {
            items.push(DetailItem::Label(
                "No phone, email or note yet — tap Edit to add.",
                y,
            ));
            y += 40;
        }

        (items, y + 16)
    }

    // ---------------- form items (shared by render + hit-test) -------------

    pub(crate) fn form_items(&self, w: i32) -> (Vec<FormItem>, i32) {
        let m = kit_ui::MARGIN;
        let inner_w = w - m * 2;
        let mut items = Vec::new();
        let mut y = 0i32;

        // Name card.
        items.push(FormItem::Name {
            card: Rect::new(0, y, inner_w, FIELD_H),
        });
        y += FIELD_H + SECTION_GAP;

        // Phones.
        items.push(FormItem::Label("PHONES", y));
        y += 26;
        for i in 0..self.form.phones.len() {
            let card = Rect::new(0, y, inner_w, PHONE_CARD_H);
            let label = Rect::new(20, y + 12, inner_w - 80, 30);
            let number = Rect::new(20, y + 66, inner_w - 80, 40);
            let remove = Rect::new(inner_w - 56, y + 12, 44, 44);
            items.push(FormItem::Phone {
                idx: i,
                card,
                label,
                number,
                remove,
            });
            y += PHONE_CARD_H + 10;
        }
        items.push(FormItem::AddPhone {
            rect: Rect::new(0, y, inner_w, 48),
        });
        y += 48 + SECTION_GAP;

        // Emails (label always shown so the first address can be added).
        items.push(FormItem::Label("EMAILS", y));
        y += 26;
        for i in 0..self.form.emails.len() {
            let card = Rect::new(0, y, inner_w, EMAIL_CARD_H);
            let label = Rect::new(20, y + 12, inner_w - 80, 30);
            let address = Rect::new(20, y + 66, inner_w - 80, 40);
            let remove = Rect::new(inner_w - 56, y + 12, 44, 44);
            items.push(FormItem::Email {
                idx: i,
                card,
                label,
                address,
                remove,
            });
            y += EMAIL_CARD_H + 10;
        }
        items.push(FormItem::AddEmail {
            rect: Rect::new(0, y, inner_w, 48),
        });
        y += 48 + SECTION_GAP;

        // Note.
        items.push(FormItem::Label("NOTE", y));
        y += 26;
        items.push(FormItem::Note {
            card: Rect::new(0, y, inner_w, NOTE_H),
        });
        y += NOTE_H + 16;

        (items, y)
    }

    // ---------------- hit testing ----------------

    pub(crate) fn hit_at(&self, pos: Point) -> Option<Hit> {
        let (w, h) = self.vp;
        match self.screen {
            Screen::List => self.list_hit(w, h, pos),
            Screen::Detail => self.detail_hit(w, h, pos),
            Screen::Form => self.form_hit(w, h, pos),
        }
    }

    fn list_hit(&self, w: i32, h: i32, pos: Point) -> Option<Hit> {
        let g = self.list_geom(w, h);
        let search = g.search;
        if search.contains(pos) {
            if !self.search.text.is_empty()
                && Rect::new(search.x + search.w - 46, search.y + 14, 30, 30).contains(pos)
            {
                return Some(Hit::SearchClear);
            }
            return Some(Hit::Search);
        }
        if g.favs_top != 0 {
            let favs = self.favorites();
            let n = favs.len() as i32;
            let card_w = (w - kit_ui::MARGIN * 2 - 12 * (n - 1)) / n;
            for (i, _) in favs.iter().enumerate() {
                let cx = kit_ui::MARGIN + i as i32 * (card_w + 12);
                if Rect::new(cx, g.favs_top, card_w, FAV_CARD_H).contains(pos) {
                    return Some(Hit::FavCard(i));
                }
            }
        }
        // Empty-state "Add contact" button — inside the viewport, so it must
        // be tested before the generic list-area hit.
        if self.contacts.is_empty() {
            let btn = Rect::new(w / 2 - 110, g.vp.y + g.vp.h / 2 + 78, 220, 56);
            if btn.contains(pos) {
                return Some(Hit::EmptyAdd);
            }
        }
        if g.vp.contains(pos) {
            if let Some(ci) = self.list_row_at(&g, pos) {
                // Quick-dial button on the row's right edge.
                let row_top = g.vp.y
                    + self
                        .filtered()
                        .iter()
                        .position(|&x| x == ci)
                        .map(|v| v as i32)
                        .unwrap_or(0)
                        * ROW_PITCH
                    - self.scroll;
                let call_btn = Rect::new(g.vp.x + g.vp.w - 72, row_top + (ROW_H - 56) / 2, 56, 56);
                if call_btn.contains(pos) {
                    return Some(Hit::DialRow(ci));
                }
                return Some(Hit::Row(ci));
            }
            return Some(Hit::List);
        }
        if g.fab.contains(pos) && !self.search_focus && !self.contacts.is_empty() {
            return Some(Hit::Fab);
        }
        None
    }

    fn detail_hit(&self, w: i32, h: i32, pos: Point) -> Option<Hit> {
        let g = self.detail_geom(w, h);
        if g.back.contains(pos) {
            return Some(Hit::Back);
        }
        if g.edit.contains(pos) {
            return Some(Hit::Edit);
        }
        if g.del.contains(pos) {
            return Some(Hit::Delete);
        }
        if g.fav.contains(pos) {
            return Some(Hit::FavToggle);
        }
        // Fixed call/message bar (bottom of screen, above gesture area).
        if g.call.contains(pos) {
            return Some(Hit::Call);
        }
        if g.message.contains(pos) {
            return Some(Hit::Message);
        }
        // Scrollable content.
        if !g.vp.contains(pos) {
            return None;
        }
        let (items, _) = self.detail_items(w);
        for it in items {
            match it {
                DetailItem::Phone {
                    idx,
                    y,
                    h: rh,
                    call,
                    sms,
                } => {
                    let card = Rect::new(kit_ui::MARGIN, y, w - kit_ui::MARGIN * 2, rh);
                    let row_s = Rect::new(card.x, card.y + g.vp.y - self.scroll, card.w, rh);
                    let call_s = call.translate(0, g.vp.y - self.scroll);
                    let sms_s = sms.translate(0, g.vp.y - self.scroll);
                    // Sub-buttons first (they sit inside the row's right edge).
                    if call_s.contains(pos) {
                        return Some(Hit::Dial(idx));
                    }
                    if sms_s.contains(pos) {
                        return Some(Hit::Sms(idx));
                    }
                    // Tapping the row itself dials — fast path for the
                    // common "tap a number to call" gesture.
                    if row_s.contains(pos) {
                        return Some(Hit::Dial(idx));
                    }
                }
                DetailItem::Email { y, h: rh, .. } => {
                    let row_s = Rect::new(
                        kit_ui::MARGIN,
                        y + g.vp.y - self.scroll,
                        w - kit_ui::MARGIN * 2,
                        rh,
                    );
                    if row_s.contains(pos) {
                        return Some(Hit::Detail); // emails are display-only
                    }
                }
                _ => {}
            }
        }
        Some(Hit::Detail)
    }

    fn form_hit(&self, w: i32, h: i32, pos: Point) -> Option<Hit> {
        let g = self.form_geom(w, h);
        if g.back.contains(pos) {
            return Some(Hit::Cancel);
        }
        if save_bar(w, h).contains(pos) {
            return Some(Hit::Save);
        }
        if !g.vp.contains(pos) {
            return None;
        }
        let (items, _) = self.form_items(w);
        let dy = g.vp.y - self.scroll; // content→screen offset
        for it in items {
            let shift = |r: Rect| r.translate(kit_ui::MARGIN, dy);
            match it {
                FormItem::Name { card } => {
                    if shift(card).contains(pos) {
                        return Some(Hit::Field(Focus::Name));
                    }
                }
                FormItem::Phone {
                    idx,
                    label,
                    number,
                    remove,
                    ..
                } => {
                    if shift(remove).contains(pos) {
                        return Some(Hit::RemovePhone(idx));
                    }
                    if shift(number).contains(pos) {
                        return Some(Hit::Field(Focus::PhoneNum(idx)));
                    }
                    if shift(label).contains(pos) {
                        return Some(Hit::Field(Focus::PhoneLabel(idx)));
                    }
                }
                FormItem::AddPhone { rect } => {
                    if shift(rect).contains(pos) {
                        return Some(Hit::AddPhone);
                    }
                }
                FormItem::Email {
                    idx,
                    label,
                    address,
                    remove,
                    ..
                } => {
                    if shift(remove).contains(pos) {
                        return Some(Hit::RemoveEmail(idx));
                    }
                    if shift(address).contains(pos) {
                        return Some(Hit::Field(Focus::EmailAddr(idx)));
                    }
                    if shift(label).contains(pos) {
                        return Some(Hit::Field(Focus::EmailLabel(idx)));
                    }
                }
                FormItem::AddEmail { rect } => {
                    if shift(rect).contains(pos) {
                        return Some(Hit::AddEmail);
                    }
                }
                FormItem::Note { card } => {
                    if shift(card).contains(pos) {
                        return Some(Hit::Field(Focus::Note));
                    }
                }
                FormItem::Label(..) => {}
            }
        }
        Some(Hit::Form)
    }

    // ---------------- scroll limits ----------------

    pub(crate) fn max_scroll(&self, w: i32, h: i32) -> i32 {
        match self.screen {
            Screen::List => {
                let g = self.list_geom(w, h);
                let n = self.filtered().len() as i32;
                let content = n * ROW_PITCH + 16;
                (content - g.vp.h).max(0)
            }
            Screen::Detail => {
                let g = self.detail_geom(w, h);
                let (_, ch) = self.detail_items(w);
                // Reserve space for the fixed call/message bar when present.
                let bar = if self
                    .open_contact()
                    .is_some_and(|c| c.primary_phone().is_some())
                {
                    84
                } else {
                    0
                };
                (ch - (g.vp.h - bar).max(0)).max(0)
            }
            Screen::Form => {
                let g = self.form_geom(w, h);
                let (_, ch) = self.form_items(w);
                (ch - g.vp.h).max(0)
            }
        }
    }

    pub(crate) fn viewport(&self, w: i32, h: i32) -> Rect {
        match self.screen {
            Screen::List => self.list_geom(w, h).vp,
            Screen::Detail => self.detail_geom(w, h).vp,
            Screen::Form => self.form_geom(w, h).vp,
        }
    }

    // ---------------- intro stagger (wall-clock, no Tick needed) ----------

    pub(crate) fn stagger_offset(&self, i: usize) -> i32 {
        let ms = self.intro_start.elapsed().as_secs_f32() * 1000.0;
        if ms >= INTRO_TOTAL_MS || i >= STAGGER_MAX {
            return 0;
        }
        let start = i as f32 * STAGGER_MS as f32;
        let t = ((ms - start).max(0.0)) / 160.0;
        if t >= 1.0 {
            0
        } else {
            ((1.0 - bezier_eval(BEZIER_ENTER, t)) * 14.0).round() as i32
        }
    }
}
