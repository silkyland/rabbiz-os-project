//! Recents and Contacts list tab renderers.

use rabbiz_app_kit::{ui, Canvas, Rect, Theme};

use crate::model::{self, CallDir, Contact};

use super::chrome::{alpha, draw_avatar, draw_icon, entry_tint, fit_style};
use super::{now_unix, DialerApp, Hit};

impl DialerApp {
    pub(super) fn render_recents(&mut self, c: &mut Canvas, t: &Theme, w: i32, h: i32) {
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

    pub(super) fn render_contacts(&mut self, c: &mut Canvas, t: &Theme, w: i32, h: i32) {
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
}
