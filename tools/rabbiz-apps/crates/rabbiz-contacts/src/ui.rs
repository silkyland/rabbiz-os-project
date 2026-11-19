//! Rendering and drawing primitives for `rabbiz-contacts`.

use de_api::Pixels;
use rabbiz_app_kit::text;
use rabbiz_app_kit::ui as kit_ui;
use rabbiz_app_kit::{Canvas, Point, Rect, Theme};

use crate::model::{self, TextBuf};
use crate::views::{
    save_bar, DetailItem, FormItem, ListGeom, FAV_CARD_H, HEADER_H, ROW_H, ROW_PITCH, SEARCH_H,
};
use crate::{ContactsApp, Focus, Hit, Screen};

// ---- Material Icons Round codepoints (verified against the bundled TTF) ----
const IC_PERSON: &str = "\u{e7fd}";
const IC_PERSON_ADD: &str = "\u{e7fe}";
const IC_SEARCH: &str = "\u{e8b6}";
const IC_CLOSE: &str = "\u{e5cd}";
const IC_STAR: &str = "\u{e838}";
const IC_STAR_BORDER: &str = "\u{e83a}";
const IC_DELETE: &str = "\u{e872}";
const IC_ADD: &str = "\u{e145}";
const IC_MAIL: &str = "\u{e158}";
const IC_SMS: &str = "\u{e625}";
const IC_CALL: &str = "\u{e0b0}";

// ---- small draw primitives ----

fn alpha(c: [u8; 4], a: u8) -> [u8; 4] {
    [c[0], c[1], c[2], a]
}

fn mix(a: [u8; 4], b: [u8; 4], t: f32) -> [u8; 4] {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t) as u8;
    [l(a[0], b[0]), l(a[1], b[1]), l(a[2], b[2]), l(a[3], b[3])]
}

/// Rasterize + blit a Material icon with ink bounds centered on (cx, cy).
fn draw_icon(c: &mut Canvas, codepoint: &str, size: f32, color: [u8; 4], cx: i32, cy: i32) {
    if let Some(px) = text::icon(codepoint, size, color) {
        let (x0, y0, x1, y1) =
            text::ink_bounds(&px).unwrap_or((0, 0, px.width() as i32, px.height() as i32));
        let ox = cx - (x0 + x1) / 2;
        let oy = cy - (y0 + y1) / 2;
        c.blit(
            &Pixels::new(
                de_api::Size::new(px.width() as i32, px.height() as i32),
                px.data().to_vec(),
            ),
            Point::new(ox, oy),
        );
    }
}

/// Avatar circle: tinted bg + initial (or person glyph), same look as the
/// dialer's avatars (same palette via `model::hash_color`).
fn draw_avatar(c: &mut Canvas, r: Rect, name: &str, tint: [u8; 4]) {
    c.fill_rounded_rect(r, (r.h as f32) / 2.0, alpha(tint, 0x2E));
    if let Some(ch) = name.chars().next() {
        let s: String = ch.to_uppercase().collect();
        let st = kit_ui::styled(r.h as f32 * 0.44, 600, tint);
        kit_ui::draw_centered(c, &s, &st, r);
    } else {
        draw_icon(
            c,
            IC_PERSON,
            r.h as f32 * 0.52,
            tint,
            r.x + r.w / 2,
            r.y + r.h / 2,
        );
    }
}

/// Blit `px` clipped to `clip` (Canvas has no scissor — same trick as notes).
fn blit_clipped(canvas: &mut Canvas, px: &Pixels, dx: i32, dy: i32, clip: Rect) {
    let dst = Rect::new(dx, dy, px.size.w, px.size.h);
    let Some(r) = dst.intersect(clip) else { return };
    let region = Rect::new(r.x - dx, r.y - dy, r.w, r.h);
    canvas.blit_region(px, region, Point::new(r.x, r.y));
}

/// Caption/text/focus/placeholder bundle for `draw_field`.
#[derive(Clone, Copy)]
struct FieldSpec<'a> {
    caption: &'a str,
    buf: &'a TextBuf,
    focused: bool,
    placeholder: &'a str,
}

/// Draw one form field: card + small-caps caption + text + caret when
/// focused. `card` may extend past `clip_vp` while scrolling — the card bg
/// is drawn on the intersection and all text is blit-clipped.
fn draw_field(c: &mut Canvas, t: &Theme, card: Rect, spec: FieldSpec, clip_vp: Rect) {
    let FieldSpec {
        caption,
        buf,
        focused,
        placeholder,
    } = spec;
    let Some(vis) = card.intersect(clip_vp) else {
        return;
    };
    c.fill_rounded_rect(vis, 20.0, t.surface1);
    let clip = vis;
    // Caption.
    let cap = Rect::new(card.x + 20, card.y + 10, card.w - 40, 20);
    if let Some(px) = text::label_clamped(caption, kit_ui::styled(13.0, 600, t.text3), cap.w, 1) {
        let p = text::cache::to_pixels(&px);
        blit_clipped(c, &p, cap.x, cap.y, clip);
    }
    let text_r = Rect::new(card.x + 20, card.y + 34, card.w - 40, card.h - 48);
    let tclip = text_r.intersect(clip).unwrap_or(text_r);
    if buf.text.is_empty() {
        if let Some(px) = text::label_clamped(placeholder, kit_ui::body(t.text3), text_r.w, 1) {
            let p = text::cache::to_pixels(&px);
            blit_clipped(c, &p, text_r.x, text_r.y + 2, tclip);
        }
    } else {
        let style = kit_ui::body(t.text);
        // Note field may contain newlines — draw up to 3 lines.
        let n = if buf.text.contains('\n') {
            buf.text.lines().count().min(3)
        } else {
            1
        };
        for (i, line) in buf.text.lines().take(n).enumerate() {
            if let Some(px) = text::label_clamped(line, style, text_r.w, 1) {
                let p = text::cache::to_pixels(&px);
                blit_clipped(c, &p, text_r.x, text_r.y + i as i32 * 30, tclip);
            }
        }
    }
    if focused {
        // Caret after the last visible line's text (approx end position).
        let style = kit_ui::body(t.text);
        let last = buf.text.lines().last().unwrap_or("");
        let wpx = text::label(last, style, None)
            .map(|px| px.width() as i32)
            .unwrap_or(0);
        let line_i = if buf.text.contains('\n') {
            buf.text.lines().count().saturating_sub(1).min(2)
        } else {
            0
        };
        let cx = (text_r.x + wpx + 2).min(text_r.x + text_r.w - 4);
        let cy = text_r.y + line_i as i32 * 30;
        if let Some(r) = Rect::new(cx, cy + 2, 3, 26).intersect(tclip) {
            c.fill_rect(r, t.accent);
        }
    }
}

/// Compact single-line field used inside phone/email cards (no card chrome —
/// the parent card provides it).
fn draw_mini_field(
    c: &mut Canvas,
    t: &Theme,
    r: Rect,
    caption: &str,
    buf: &TextBuf,
    focused: bool,
    placeholder: &str,
) {
    let _ = caption;
    let text_r = Rect::new(r.x, r.y + 4, r.w, r.h - 4);
    if buf.text.is_empty() {
        kit_ui::draw_text(
            c,
            placeholder,
            &kit_ui::styled(17.0, 400, t.text3),
            text_r.x,
            text_r.y + 2,
            Some(text_r.w),
        );
    } else if let Some(px) = text::label_clamped(&buf.text, kit_ui::body(t.text), text_r.w, 1) {
        let p = text::cache::to_pixels(&px);
        blit_clipped(c, &p, text_r.x, text_r.y + 2, text_r);
    }
    if focused {
        let style = kit_ui::body(t.text);
        let wpx = text::label(&buf.text, style, None)
            .map(|px| px.width() as i32)
            .unwrap_or(0);
        let cx = (text_r.x + wpx + 2).min(text_r.x + text_r.w - 4);
        let rc = Rect::new(cx, text_r.y, 3, text_r.h - 6);
        if let Some(rc) = rc.intersect(text_r) {
            c.fill_rect(rc, t.accent);
        }
    }
}

/// Section caption ("PHONES", "ALL CONTACTS", …).
fn draw_section(c: &mut Canvas, t: &Theme, s: &str, x: i32, y: i32) {
    kit_ui::draw_text(c, s, &kit_ui::styled(13.0, 600, t.text3), x, y, None);
}

impl ContactsApp {
    // ---------------- list screen ----------------

    pub(crate) fn render_list(&mut self, c: &mut Canvas, t: &Theme, w: i32, h: i32) {
        let g = self.list_geom(w, h);
        // Header: title + count subtitle. A single space keeps draw_header's
        // two-line layout (and therefore the search field position) stable
        // when the list is empty.
        let subtitle = if self.contacts.is_empty() {
            " ".to_string()
        } else {
            format!(
                "{} contact{}",
                self.contacts.len(),
                if self.contacts.len() == 1 { "" } else { "s" }
            )
        };
        kit_ui::draw_header(c, t, "Contacts", &subtitle);

        // -- search field ----------------------------------------------------
        c.fill_rounded_rect(g.search, SEARCH_H as f32 / 2.0, t.surface1);
        draw_icon(
            c,
            IC_SEARCH,
            22.0,
            t.accent,
            g.search.x + 30,
            g.search.y + g.search.h / 2,
        );
        let text_x = g.search.x + 52;
        let avail =
            g.search.x + g.search.w - text_x - if self.search.text.is_empty() { 16 } else { 48 };
        if self.search.text.is_empty() {
            kit_ui::draw_text(
                c,
                "Search name, number or email",
                &kit_ui::body(t.text3),
                text_x,
                g.search.y + 17,
                Some(avail),
            );
        } else {
            let st = kit_ui::body(t.text);
            if let Some(px) = text::label_clamped(&self.search.text, st, avail, 1) {
                let p = text::cache::to_pixels(&px);
                blit_clipped(
                    c,
                    &p,
                    text_x,
                    g.search.y + g.search.h / 2 - p.size.h / 2,
                    g.search,
                );
                if self.search_focus {
                    let cx = (text_x + p.size.w + 2).min(g.search.x + g.search.w - 12);
                    let r = Rect::new(cx, g.search.y + 16, 3, g.search.h - 32);
                    if let Some(r) = r.intersect(g.search) {
                        c.fill_rect(r, t.accent);
                    }
                }
            }
        }
        if self.search.text.is_empty() && self.search_focus {
            let r = Rect::new(text_x, g.search.y + 16, 3, g.search.h - 32);
            if let Some(r) = r.intersect(g.search) {
                c.fill_rect(r, t.accent);
            }
        }
        if !self.search.text.is_empty() {
            let xr = Rect::new(g.search.x + g.search.w - 46, g.search.y + 14, 30, 30);
            draw_icon(c, IC_CLOSE, 20.0, t.text3, xr.x + 15, xr.y + 15);
        }

        // -- favorites strip (hidden while searching) ------------------------
        let show_favs = self.search.text.trim().is_empty() && !self.favorites().is_empty();
        if show_favs {
            draw_section(c, t, "FAVORITES", kit_ui::MARGIN, g.favs_label_y);
            let favs = self.favorites();
            let n = favs.len() as i32;
            let card_w = (w - kit_ui::MARGIN * 2 - 12 * (n - 1)) / n;
            for (i, &ci) in favs.iter().enumerate() {
                let cx = kit_ui::MARGIN + i as i32 * (card_w + 12);
                let card = Rect::new(cx, g.favs_top, card_w, FAV_CARD_H);
                let pressed = self.pressed == Some(Hit::FavCard(i));
                c.fill_rounded_rect(card, 24.0, if pressed { t.surface3 } else { t.surface1 });
                let ct = &self.contacts[ci];
                draw_avatar(
                    c,
                    Rect::new(card.x + card.w / 2 - 26, card.y + 12, 52, 52),
                    &ct.name,
                    model::avatar_tint(ct),
                );
                kit_ui::draw_centered(
                    c,
                    &ct.name,
                    &kit_ui::styled(15.0, 600, t.text),
                    Rect::new(card.x, card.y + 72, card.w, 20),
                );
                if let Some(p) = ct.primary_phone() {
                    kit_ui::draw_centered(
                        c,
                        &model::format_number(&p.number),
                        &kit_ui::styled(12.0, 400, t.text3),
                        Rect::new(card.x, card.y + 92, card.w, 16),
                    );
                }
            }
        }

        // -- section label + rows ---------------------------------------------
        draw_section(
            c,
            t,
            if self.search.text.trim().is_empty() {
                "ALL CONTACTS"
            } else {
                "RESULTS"
            },
            kit_ui::MARGIN,
            g.section_y,
        );

        let ids = self.filtered();
        if self.contacts.is_empty() {
            self.render_empty(c, t, w, h, &g);
        } else if ids.is_empty() {
            kit_ui::draw_centered(
                c,
                &format!("No contacts match \"{}\"", self.search.text.trim()),
                &kit_ui::styled(20.0, 400, t.text3),
                Rect::new(g.vp.x, g.vp.y + g.vp.h / 2 - 20, g.vp.w, 40),
            );
        }

        for (vis, &ci) in ids.iter().enumerate() {
            let ct = &self.contacts[ci];
            let mut y = g.vp.y + vis as i32 * ROW_PITCH - self.scroll;
            y += self.stagger_offset(vis);
            let row = Rect::new(g.vp.x, y, g.vp.w, ROW_H);
            let Some(vis_row) = row.intersect(g.vp) else {
                continue;
            };
            let pressed = self.pressed == Some(Hit::Row(ci));
            kit_ui::draw_card(c, t, vis_row, pressed);
            // M3 state-layer on press (90 ms-class feedback, static frame).
            if pressed {
                c.fill_rounded_rect(vis_row, 24.0, kit_ui::state_layer(t, 0.10));
            }

            let inner = Rect::new(row.x + 14, row.y, row.w - 14 - 90, row.h);
            let Some(tclip) = inner.intersect(g.vp) else {
                continue;
            };
            draw_avatar(
                c,
                Rect::new(row.x + 14, row.y + (row.h - 60) / 2, 60, 60),
                &ct.name,
                model::avatar_tint(ct),
            );
            // Name (+ favorite star).
            let name_w = inner.w - 30;
            if let Some(px) = text::label_clamped(&ct.name, kit_ui::label(t.text), name_w, 1) {
                let p = text::cache::to_pixels(&px);
                blit_clipped(c, &p, inner.x, inner.y + 12, tclip);
            }
            if ct.favorite {
                draw_icon(
                    c,
                    IC_STAR,
                    18.0,
                    t.warning,
                    inner.x + inner.w - 8,
                    inner.y + 20,
                );
            }
            // Primary number (+ label).
            if let Some(p) = ct.primary_phone() {
                let meta = if p.label.trim().is_empty() {
                    model::format_number(&p.number)
                } else {
                    format!("{} · {}", p.label, model::format_number(&p.number))
                };
                kit_ui::draw_text(
                    c,
                    &meta,
                    &kit_ui::styled(16.0, 400, t.text3),
                    inner.x,
                    inner.y + 48,
                    Some(inner.w),
                );
            } else if !ct.emails.is_empty() {
                kit_ui::draw_text(
                    c,
                    &ct.emails[0].address,
                    &kit_ui::styled(16.0, 400, t.text3),
                    inner.x,
                    inner.y + 48,
                    Some(inner.w),
                );
            }
            // Quick-dial button.
            let cb = Rect::new(row.x + row.w - 72, row.y + (row.h - 56) / 2, 56, 56);
            let cb_vis = cb.intersect(g.vp).unwrap_or(cb);
            let cbp = self.pressed == Some(Hit::DialRow(ci));
            c.fill_rounded_rect(
                cb_vis,
                28.0,
                alpha(t.success, if cbp { 0x66 } else { 0x24 }),
            );
            draw_icon(c, IC_CALL, 26.0, t.success, cb.x + 28, cb.y + 28);
        }

        // -- FAB ---------------------------------------------------------------
        if !self.search_focus && !self.contacts.is_empty() {
            let fab = g.fab;
            let fab_bg = if self.pressed == Some(Hit::Fab) {
                t.accent_dim
            } else {
                t.accent
            };
            c.fill_rounded_rect(fab, (fab.h / 2) as f32, fab_bg);
            draw_icon(
                c,
                IC_ADD,
                30.0,
                t.text_on_accent,
                fab.x + fab.w / 2,
                fab.y + fab.h / 2,
            );
        }
    }

    fn render_empty(&self, c: &mut Canvas, t: &Theme, w: i32, _h: i32, g: &ListGeom) {
        let cy = g.vp.y + g.vp.h / 2 - 90;
        draw_icon(c, IC_PERSON_ADD, 72.0, t.text3, w / 2, cy);
        kit_ui::draw_centered(
            c,
            "No contacts yet",
            &kit_ui::title(t.text2),
            Rect::new(0, cy + 56, w, 44),
        );
        kit_ui::draw_centered(
            c,
            "Add the people you call and text most",
            &kit_ui::body(t.text3),
            Rect::new(0, cy + 108, w, 36),
        );
        // "Add contact" action — same rect hit-tested in list_hit.
        let btn = Rect::new(w / 2 - 110, cy + 168, 220, 56);
        let pressed = self.pressed == Some(Hit::EmptyAdd);
        kit_ui::draw_pill_button(c, t, btn, "Add contact", true, pressed);
    }

    // ---------------- detail screen ----------------

    pub(crate) fn render_detail(&mut self, c: &mut Canvas, t: &Theme, w: i32, h: i32) {
        let g = self.detail_geom(w, h);

        // Top bar pills.
        kit_ui::draw_pill_button(
            c,
            t,
            g.back,
            "‹ Contacts",
            false,
            self.pressed == Some(Hit::Back),
        );
        // Favorite star toggle.
        let fav_on = self.open_contact().is_some_and(|x| x.favorite);
        let fr = if self.pressed == Some(Hit::FavToggle) {
            Rect::new(g.fav.x + 2, g.fav.y + 2, g.fav.w - 4, g.fav.h - 4)
        } else {
            g.fav
        };
        c.fill_rounded_rect(
            fr,
            (fr.h / 2) as f32,
            if fav_on {
                alpha(t.warning, 0x30)
            } else {
                t.surface2
            },
        );
        draw_icon(
            c,
            if fav_on { IC_STAR } else { IC_STAR_BORDER },
            24.0,
            if fav_on { t.warning } else { t.text2 },
            fr.x + fr.w / 2,
            fr.y + fr.h / 2,
        );
        kit_ui::draw_pill_button(c, t, g.edit, "Edit", false, self.pressed == Some(Hit::Edit));
        // Delete pill: danger-filled when armed.
        let del_bg = if self.delete_armed {
            t.danger
        } else {
            t.surface2
        };
        let del_fg = if self.delete_armed {
            t.text_on_accent
        } else {
            t.text2
        };
        let dr = if self.pressed == Some(Hit::Delete) {
            Rect::new(g.del.x + 2, g.del.y + 2, g.del.w - 4, g.del.h - 4)
        } else {
            g.del
        };
        c.fill_rounded_rect(dr, (dr.h / 2) as f32, del_bg);
        kit_ui::draw_centered(
            c,
            if self.delete_armed { "Sure?" } else { "Delete" },
            &kit_ui::label(del_fg),
            dr,
        );

        let Some(ct) = self.open_contact().cloned() else {
            // Contact vanished (deleted elsewhere) — fall back to list.
            self.screen = Screen::List;
            return;
        };

        // Scrollable content.
        let (items, content_h) = self.detail_items(w);
        let bar_h = if ct.primary_phone().is_some() { 84 } else { 0 };
        let max_scroll = (content_h - (g.vp.h - bar_h).max(0)).max(0);
        self.scroll = self.scroll.clamp(0, max_scroll);
        let dy = g.vp.y - self.scroll;

        for it in items {
            match it {
                DetailItem::Header { y } => {
                    // Avatar + name block.
                    let head = Rect::new(kit_ui::MARGIN, y + dy, w - kit_ui::MARGIN * 2, HEADER_H);
                    let Some(hv) = head.intersect(g.vp) else {
                        continue;
                    };
                    let av = Rect::new(head.x, head.y + 8, 84, 84);
                    draw_avatar(c, av, &ct.name, model::avatar_tint(&ct));
                    let tx = head.x + 104;
                    let name_r = Rect::new(tx, head.y + 16, head.w - 104, 44);
                    if let Some(px) =
                        text::label_clamped(&ct.name, kit_ui::title(t.text), name_r.w, 2)
                    {
                        let p = text::cache::to_pixels(&px);
                        blit_clipped(c, &p, tx, name_r.y, hv);
                    }
                    if let Some(p) = ct.primary_phone() {
                        let meta = if p.label.trim().is_empty() {
                            model::format_number(&p.number)
                        } else {
                            format!("{} · {}", p.label, model::format_number(&p.number))
                        };
                        kit_ui::draw_text(
                            c,
                            &meta,
                            &kit_ui::body(t.text3),
                            tx,
                            head.y + 66,
                            Some(name_r.w),
                        );
                    } else if let Some(e) = ct.primary_email() {
                        kit_ui::draw_text(
                            c,
                            &e.address,
                            &kit_ui::body(t.text3),
                            tx,
                            head.y + 66,
                            Some(name_r.w),
                        );
                    }
                }
                DetailItem::Label(s, y) => {
                    if s.is_empty() || y + dy > g.vp.y + g.vp.h || y + dy + 26 < g.vp.y {
                        continue;
                    }
                    draw_section(c, t, s, kit_ui::MARGIN, y + dy);
                }
                DetailItem::Phone {
                    idx,
                    y,
                    h: rh,
                    call,
                    sms,
                } => {
                    let card = Rect::new(kit_ui::MARGIN, y + dy, w - kit_ui::MARGIN * 2, rh);
                    let Some(cv) = card.intersect(g.vp) else {
                        continue;
                    };
                    let pressed_dial = self.pressed == Some(Hit::Dial(idx));
                    let pressed_sms = self.pressed == Some(Hit::Sms(idx));
                    kit_ui::draw_card(c, t, cv, pressed_dial || pressed_sms);
                    let p = &ct.phones[idx];
                    // Label caption + number.
                    if !p.label.trim().is_empty() {
                        draw_section(c, t, &p.label.to_uppercase(), card.x + 20, card.y + 12);
                    }
                    kit_ui::draw_text(
                        c,
                        &model::format_number(&p.number),
                        &kit_ui::styled(24.0, 600, t.text),
                        card.x + 20,
                        card.y + (if p.label.trim().is_empty() { 14 } else { 36 }),
                        Some(card.w - 220),
                    );
                    // Call + SMS buttons.
                    let call_s = call.translate(0, dy);
                    let sms_s = sms.translate(0, dy);
                    let cbg = if pressed_dial {
                        alpha(t.success, 0x55)
                    } else {
                        alpha(t.success, 0x22)
                    };
                    if let Some(r) = call_s.intersect(g.vp) {
                        c.fill_rounded_rect(r, 28.0, cbg);
                    }
                    draw_icon(c, IC_CALL, 26.0, t.success, call_s.x + 28, call_s.y + 28);
                    let sbg = if pressed_sms {
                        alpha(t.accent, 0x55)
                    } else {
                        alpha(t.accent, 0x22)
                    };
                    if let Some(r) = sms_s.intersect(g.vp) {
                        c.fill_rounded_rect(r, 28.0, sbg);
                    }
                    draw_icon(c, IC_SMS, 26.0, t.accent, sms_s.x + 28, sms_s.y + 28);
                }
                DetailItem::Email { idx, y, h: rh } => {
                    let card = Rect::new(kit_ui::MARGIN, y + dy, w - kit_ui::MARGIN * 2, rh);
                    let Some(cv) = card.intersect(g.vp) else {
                        continue;
                    };
                    kit_ui::draw_card(c, t, cv, false);
                    let e = &ct.emails[idx];
                    draw_icon(c, IC_MAIL, 26.0, t.accent, card.x + 34, card.y + rh / 2);
                    let tx = card.x + 64;
                    let label = if e.label.trim().is_empty() {
                        "email".to_string()
                    } else {
                        e.label.clone()
                    };
                    draw_section(c, t, &label.to_uppercase(), tx, card.y + 12);
                    kit_ui::draw_text(
                        c,
                        &e.address,
                        &kit_ui::styled(20.0, 500, t.text),
                        tx,
                        card.y + 32,
                        Some(card.w - 84),
                    );
                }
                DetailItem::NoteCard { y, h: nh } => {
                    let card = Rect::new(kit_ui::MARGIN, y + dy, w - kit_ui::MARGIN * 2, nh);
                    let Some(cv) = card.intersect(g.vp) else {
                        continue;
                    };
                    kit_ui::draw_card(c, t, cv, false);
                    let inner = Rect::new(card.x + 20, card.y + 10, card.w - 40, card.h - 20);
                    let clip = inner.intersect(g.vp).unwrap_or(inner);
                    for (i, line) in ct.note.lines().take(6).enumerate() {
                        kit_ui::draw_text(
                            c,
                            line,
                            &kit_ui::body(t.text2),
                            inner.x,
                            inner.y + i as i32 * 30,
                            Some(inner.w),
                        );
                    }
                    let _ = clip;
                }
            }
        }

        // Fixed call/message bar.
        if ct.primary_phone().is_some() {
            let call = g.call;
            let msg = g.message;
            let cp = self.pressed == Some(Hit::Call);
            let mp = self.pressed == Some(Hit::Message);
            c.fill_rounded_rect(
                call,
                36.0,
                if cp {
                    mix(t.success, [0, 0, 0, 255], 0.15)
                } else {
                    t.success
                },
            );
            draw_icon(
                c,
                IC_CALL,
                30.0,
                t.text_on_accent,
                call.x + 40,
                call.y + call.h / 2,
            );
            kit_ui::draw_text(
                c,
                "Call",
                &kit_ui::styled(22.0, 600, t.text_on_accent),
                call.x + 76,
                call.y + call.h / 2 - 14,
                Some(call.w - 90),
            );
            c.fill_rounded_rect(
                msg,
                36.0,
                if mp {
                    mix(t.accent, [0, 0, 0, 255], 0.15)
                } else {
                    t.accent
                },
            );
            draw_icon(
                c,
                IC_SMS,
                30.0,
                t.text_on_accent,
                msg.x + 40,
                msg.y + msg.h / 2,
            );
            kit_ui::draw_text(
                c,
                "Message",
                &kit_ui::styled(22.0, 600, t.text_on_accent),
                msg.x + 76,
                msg.y + msg.h / 2 - 14,
                Some(msg.w - 90),
            );
        }
    }

    // ---------------- form screen ----------------

    pub(crate) fn render_form(&mut self, c: &mut Canvas, t: &Theme, w: i32, h: i32) {
        let g = self.form_geom(w, h);
        let editing = self.editing_id.is_some();

        // Top bar (Cancel only — Save lives in the fixed bottom bar).
        kit_ui::draw_pill_button(
            c,
            t,
            g.back,
            "‹ Cancel",
            false,
            self.pressed == Some(Hit::Cancel),
        );
        let save_pressed = self.pressed == Some(Hit::Save);

        // Fixed Save bar at bottom (always reachable regardless of scroll).
        let bar = save_bar(w, h);
        let bar_bg = if save_pressed { t.accent_dim } else { t.accent };
        c.fill_rounded_rect(bar, 36.0, bar_bg);
        kit_ui::draw_centered(
            c,
            "Save contact",
            &kit_ui::styled(24.0, 600, t.text_on_accent),
            bar,
        );
        if self.form_error {
            kit_ui::draw_centered(
                c,
                "Add a name or a phone number",
                &kit_ui::styled(15.0, 500, t.danger),
                Rect::new(kit_ui::MARGIN, bar.y - 26, w - kit_ui::MARGIN * 2, 22),
            );
        }

        let (items, content_h) = self.form_items(w);
        let max_scroll = (content_h - g.vp.h).max(0);
        self.scroll = self.scroll.clamp(0, max_scroll);
        let dy = g.vp.y - self.scroll;

        for it in items {
            match it {
                FormItem::Label(s, y) => {
                    if y + dy > g.vp.y + g.vp.h || y + dy + 26 < g.vp.y {
                        continue;
                    }
                    draw_section(c, t, s, kit_ui::MARGIN + 4, y + dy);
                }
                FormItem::Name { card } => {
                    let r = card.translate(kit_ui::MARGIN, dy);
                    draw_field(
                        c,
                        t,
                        r,
                        FieldSpec {
                            caption: "NAME",
                            buf: &self.form.name,
                            focused: self.focus == Focus::Name,
                            placeholder: "Full name",
                        },
                        g.vp,
                    );
                }
                FormItem::Phone {
                    idx,
                    card,
                    label,
                    number,
                    remove,
                } => {
                    let r = card.translate(kit_ui::MARGIN, dy);
                    let Some(v) = r.intersect(g.vp) else { continue };
                    c.fill_rounded_rect(v, 20.0, t.surface1);
                    let Some(row) = self.form.phones.get(idx) else {
                        continue;
                    };
                    // Two stacked sub-fields inside the card.
                    let lab = label.translate(kit_ui::MARGIN, dy);
                    let num = number.translate(kit_ui::MARGIN, dy);
                    let rem = remove.translate(kit_ui::MARGIN, dy);
                    // Label field (top strip).
                    if let Some(lv) = Rect::new(lab.x, lab.y, lab.w, lab.h).intersect(g.vp) {
                        draw_mini_field(
                            c,
                            t,
                            lv,
                            "LABEL",
                            &row.label,
                            self.focus == Focus::PhoneLabel(idx),
                            "mobile, work…",
                        );
                    }
                    // Divider.
                    let div_y = lab.y + lab.h + 4;
                    if div_y > g.vp.y && div_y < g.vp.y + g.vp.h {
                        c.fill_rect(Rect::new(v.x + 20, div_y, v.w - 40, 1), t.line);
                    }
                    if let Some(nv) = Rect::new(num.x, num.y, num.w, num.h).intersect(g.vp) {
                        draw_mini_field(
                            c,
                            t,
                            nv,
                            "NUMBER",
                            &row.number,
                            self.focus == Focus::PhoneNum(idx),
                            "081 234 5678",
                        );
                    }
                    // Remove button.
                    let pressed_rm = self.pressed == Some(Hit::RemovePhone(idx));
                    if let Some(rv) = rem.intersect(g.vp) {
                        c.fill_rounded_rect(
                            rv,
                            22.0,
                            if pressed_rm {
                                alpha(t.danger, 0x30)
                            } else {
                                t.surface2
                            },
                        );
                        draw_icon(
                            c,
                            IC_DELETE,
                            20.0,
                            t.danger,
                            rv.x + rv.w / 2,
                            rv.y + rv.h / 2,
                        );
                    }
                }
                FormItem::AddPhone { rect } => {
                    let r = rect.translate(kit_ui::MARGIN, dy);
                    let Some(v) = r.intersect(g.vp) else { continue };
                    let pressed = self.pressed == Some(Hit::AddPhone);
                    c.fill_rounded_rect(v, 24.0, if pressed { t.surface3 } else { t.surface2 });
                    draw_icon(c, IC_ADD, 20.0, t.accent, v.x + 26, v.y + v.h / 2);
                    kit_ui::draw_text(
                        c,
                        "Add phone",
                        &kit_ui::styled(18.0, 600, t.accent),
                        v.x + 48,
                        v.y + v.h / 2 - 12,
                        Some(v.w - 60),
                    );
                }
                FormItem::Email {
                    idx,
                    card,
                    label,
                    address,
                    remove,
                } => {
                    let r = card.translate(kit_ui::MARGIN, dy);
                    let Some(v) = r.intersect(g.vp) else { continue };
                    c.fill_rounded_rect(v, 20.0, t.surface1);
                    let Some(row) = self.form.emails.get(idx) else {
                        continue;
                    };
                    let lab = label.translate(kit_ui::MARGIN, dy);
                    let addr = address.translate(kit_ui::MARGIN, dy);
                    let rem = remove.translate(kit_ui::MARGIN, dy);
                    if let Some(lv) = Rect::new(lab.x, lab.y, lab.w, lab.h).intersect(g.vp) {
                        draw_mini_field(
                            c,
                            t,
                            lv,
                            "LABEL",
                            &row.label,
                            self.focus == Focus::EmailLabel(idx),
                            "home, work…",
                        );
                    }
                    let div_y = lab.y + lab.h + 4;
                    if div_y > g.vp.y && div_y < g.vp.y + g.vp.h {
                        c.fill_rect(Rect::new(v.x + 20, div_y, v.w - 40, 1), t.line);
                    }
                    if let Some(av) = Rect::new(addr.x, addr.y, addr.w, addr.h).intersect(g.vp) {
                        draw_mini_field(
                            c,
                            t,
                            av,
                            "EMAIL",
                            &row.address,
                            self.focus == Focus::EmailAddr(idx),
                            "name@example.com",
                        );
                    }
                    let pressed_rm = self.pressed == Some(Hit::RemoveEmail(idx));
                    if let Some(rv) = rem.intersect(g.vp) {
                        c.fill_rounded_rect(
                            rv,
                            22.0,
                            if pressed_rm {
                                alpha(t.danger, 0x30)
                            } else {
                                t.surface2
                            },
                        );
                        draw_icon(
                            c,
                            IC_DELETE,
                            20.0,
                            t.danger,
                            rv.x + rv.w / 2,
                            rv.y + rv.h / 2,
                        );
                    }
                }
                FormItem::AddEmail { rect } => {
                    let r = rect.translate(kit_ui::MARGIN, dy);
                    let Some(v) = r.intersect(g.vp) else { continue };
                    let pressed = self.pressed == Some(Hit::AddEmail);
                    c.fill_rounded_rect(v, 24.0, if pressed { t.surface3 } else { t.surface2 });
                    draw_icon(c, IC_ADD, 20.0, t.accent, v.x + 26, v.y + v.h / 2);
                    kit_ui::draw_text(
                        c,
                        "Add email",
                        &kit_ui::styled(18.0, 600, t.accent),
                        v.x + 48,
                        v.y + v.h / 2 - 12,
                        Some(v.w - 60),
                    );
                }
                FormItem::Note { card } => {
                    let r = card.translate(kit_ui::MARGIN, dy);
                    draw_field(
                        c,
                        t,
                        r,
                        FieldSpec {
                            caption: "NOTE",
                            buf: &self.form.note,
                            focused: self.focus == Focus::Note,
                            placeholder: "Notes about this contact",
                        },
                        g.vp,
                    );
                }
            }
        }

        // Title hint under the top bar.
        kit_ui::draw_text(
            c,
            if editing {
                "Edit contact"
            } else {
                "New contact"
            },
            &kit_ui::title(t.text),
            kit_ui::MARGIN,
            kit_ui::TOP_SAFE + 68,
            Some(w - kit_ui::MARGIN * 2),
        );
    }
}
