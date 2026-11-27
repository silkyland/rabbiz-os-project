//! Recents and Contacts list renderers — flat M3 rows + inset dividers.

use de_api::Pixels;
use rabbiz_app_kit::{icons, text, ui, widgets, Canvas, Point, Rect, Size, Theme};

use crate::model::{self, CallDir};

use super::chrome::{draw_avatar, draw_icon, entry_tint};
use super::input::row_call_rect;
use super::{now_unix, DialerApp, Hit};

/// Blit `px` clipped to `clip` (Canvas has no scissor — same trick as
/// contacts/notes): a half-scrolled row still reads correctly at the
/// viewport edge.
fn blit_clip(c: &mut Canvas, px: &Pixels, dx: i32, dy: i32, clip: Rect) {
    let dst = Rect::new(dx, dy, px.size.w, px.size.h);
    let Some(r) = dst.intersect(clip) else {
        return;
    };
    c.blit_region(
        px,
        Rect::new(r.x - dx, r.y - dy, r.w, r.h),
        Point::new(r.x, r.y),
    );
}

/// Single-line clipped label at (x, y).
fn label_clip(
    c: &mut Canvas,
    s: &str,
    st: text::LabelStyle,
    x: i32,
    y: i32,
    max_w: i32,
    clip: Rect,
) {
    if s.is_empty() {
        return;
    }
    if let Some(px) = text::label_clamped(s, st, max_w.max(8), 1) {
        let p = Pixels::new(
            Size::new(px.width() as i32, px.height() as i32),
            px.data().to_vec(),
        );
        blit_clip(c, &p, x, y, clip);
    }
}

/// Flat list row shared by Recents + Contacts: leading 48px avatar,
/// title + caption, trailing call-back icon button, inset divider.
#[allow(clippy::too_many_arguments)]
fn flat_row(
    c: &mut Canvas,
    t: &Theme,
    row: Rect,
    vp: Rect,
    name: &str,
    tint: [u8; 4],
    name_col: [u8; 4],
    caption_icon: Option<(&str, [u8; 4])>,
    caption: &str,
    caption_col: [u8; 4],
    trailing_text: &str,
    pressed_row: bool,
    pressed_call: bool,
) {
    let Some(vr) = row.intersect(vp) else {
        return;
    };
    if vr.h < 8 {
        return;
    }
    if pressed_row {
        c.fill_rect(vr, ui::state_layer(t, 0.10));
    }
    let cy = row.y + row.h / 2;
    // Leading avatar (kit widget; skipped when the band is clipped away).
    let av = Rect::new(row.x + 16, cy - 24, 48, 48);
    if vp.contains(Point::new(av.x, av.y))
        && vp.contains(Point::new(av.right() - 1, av.bottom() - 1))
    {
        draw_avatar(c, av, name, tint);
    }
    let tx = row.x + 80;
    let call_r = row_call_rect(row);
    let text_right = call_r.x - 8;
    // Title line + trailing time caption (AOSP recents row anatomy).
    label_clip(
        c,
        name,
        ui::styled(22.0, 600, name_col),
        tx,
        row.y + 16,
        text_right - tx - 90,
        vp,
    );
    label_clip(
        c,
        trailing_text,
        ui::caption(t.text3),
        text_right - 90,
        row.y + 20,
        84,
        vp,
    );
    // Caption line: call-type icon + meta.
    let mut cap_x = tx;
    if let Some((ic, col)) = caption_icon {
        if vp.contains(Point::new(cap_x + 8, cy + 10)) {
            draw_icon(c, ic, 18.0, col, cap_x + 8, cy + 10);
        }
        cap_x += 26;
    }
    label_clip(
        c,
        caption,
        ui::caption(caption_col),
        cap_x,
        row.y + 52,
        text_right - cap_x,
        vp,
    );
    // Trailing call-back tonal icon button.
    if vp.contains(Point::new(call_r.x, call_r.y))
        && vp.contains(Point::new(call_r.right() - 1, call_r.bottom() - 1))
    {
        widgets::icon_btn(
            c,
            t,
            call_r,
            icons::CALL,
            widgets::IconBtnStyle::Tonal,
            false,
            pressed_call,
        );
    }
    // Inset divider (from the text column to the right edge).
    let dy = row.bottom() - 1;
    if dy >= vp.y && dy < vp.bottom() {
        ui::divider_inset(c, t, tx, row.right() - ui::MARGIN, dy);
    }
}

impl DialerApp {
    pub(super) fn render_recents(&mut self, c: &mut Canvas, t: &Theme, w: i32, h: i32) {
        // Filter chips (measured widths, 48px tall).
        let (all_r, mis_r, clear_r) = self.recents_chip_rects(w);
        let missed_n = self
            .recents
            .entries
            .iter()
            .filter(|e| e.dir == CallDir::Missed)
            .count();
        widgets::chip(
            c,
            t,
            all_r,
            "All",
            !self.missed_only,
            self.pressed == Some(Hit::FilterAll),
        );
        widgets::chip(
            c,
            t,
            mis_r,
            &format!("Missed ({missed_n})"),
            self.missed_only,
            self.pressed == Some(Hit::FilterMissed),
        );
        if let Some(cr) = clear_r {
            widgets::chip(
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
            let e = &self.recents.entries[real];
            let missed = e.dir == CallDir::Missed;
            let name = if e.name.is_empty() {
                model::format_number(&e.number)
            } else {
                e.name.clone()
            };
            let tint = entry_tint(&self.contacts, &e.name, &e.number);
            let meta = format!("{} · {}", e.dir.label(), model::format_number(&e.number));
            let cap_col = if missed { t.danger } else { t.text3 };
            flat_row(
                c,
                t,
                r,
                vp,
                &name,
                tint,
                if missed { t.danger } else { t.text },
                Some((e.dir.icon(), if missed { t.danger } else { t.text3 })),
                &meta,
                cap_col,
                &model::when_label(now, e.unix),
                self.pressed == Some(Hit::RecRow(real)),
                self.pressed == Some(Hit::RecCall(real)),
            );
        }

        // Scrollbar + empty state.
        let content_h = idxs.len() as i32 * super::REC_PITCH;
        ui::scrollbar(
            c,
            t,
            Rect::new(w - 4, vp.y, 4, vp.h),
            self.scroll,
            content_h,
        );
        if idxs.is_empty() {
            widgets::EmptyState::render(
                c,
                vp,
                t,
                if self.missed_only {
                    "\u{e0b4}" // call_missed
                } else {
                    icons::HISTORY
                },
                if self.missed_only {
                    "No missed calls"
                } else {
                    "No recent calls"
                },
                if self.recents.entries.is_empty() {
                    "Calls you make will show up here"
                } else if !self.query.trim().is_empty() {
                    "No matches in the call log"
                } else {
                    ""
                },
            );
        }
    }

    pub(super) fn render_contacts(&mut self, c: &mut Canvas, t: &Theme, w: i32, h: i32) {
        // Favorites strip — capped at 3 fixed cards (no offscreen overflow).
        if self.show_favorites() {
            ui::draw_text(
                c,
                "FAVORITES",
                &ui::styled(14.0, 600, t.text3),
                ui::MARGIN,
                self.content_top() + 4,
                Some(w - ui::MARGIN * 2),
            );
            for (i, ct) in self
                .contacts
                .iter()
                .filter(|ct| ct.favorite)
                .take(3)
                .enumerate()
            {
                let card = self.fav_card_rect(w, i);
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
                    Rect::new(card.x + 4, card.y + 74, card.w - 8, 20),
                );
            }
        }

        ui::draw_text(
            c,
            if self.query.trim().is_empty() {
                "ALL CONTACTS"
            } else {
                "RESULTS"
            },
            &ui::styled(14.0, 600, t.text3),
            ui::MARGIN,
            self.contacts_rows_label_y(),
            Some(w - ui::MARGIN * 2),
        );

        let vp = self.contacts_viewport(w, h);
        let list = self.filtered_contacts();
        for (vis, ct) in list.iter().enumerate() {
            let r = self.contact_row_rect(w, vis);
            let real = self.contact_index(ct);
            let meta = if ct.label.is_empty() {
                model::format_number(&ct.number)
            } else {
                format!("{} · {}", ct.label, model::format_number(&ct.number))
            };
            flat_row(
                c,
                t,
                r,
                vp,
                &ct.name,
                ct.color,
                t.text,
                None,
                &meta,
                t.text3,
                "",
                self.pressed == Some(Hit::ConRow(real)),
                self.pressed == Some(Hit::ConCall(real)),
            );
        }

        let content_h = list.len() as i32 * super::CON_PITCH;
        ui::scrollbar(
            c,
            t,
            Rect::new(w - 4, vp.y, 4, vp.h),
            self.scroll,
            content_h,
        );
        if list.is_empty() {
            widgets::EmptyState::render(
                c,
                vp,
                t,
                icons::PERSON,
                "No contacts match",
                "Try a different name or number",
            );
        }
    }
}
