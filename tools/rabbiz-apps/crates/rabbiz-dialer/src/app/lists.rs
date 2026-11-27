//! Favorites/Recents/Contacts list renderers — sectioned M3 rows over the
//! shared item model (day/letter headers interleaved with data rows).

use de_api::Pixels;
use rabbiz_app_kit::{icons, text, ui, widgets, Canvas, Point, Rect, Size, Theme};

use crate::model::{self, CallDir};

use super::chrome::{draw_avatar, draw_icon, entry_tint};
use super::input::row_call_rect;
use super::{now_unix, DialerApp, Hit, Row};

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

/// Section header band ("Today", "Yesterday", "A", "B", …) — small caps,
/// dim accent ink, inset to the text column like the Google Phone log.
fn section_header(c: &mut Canvas, t: &Theme, band: Rect, vp: Rect, label: &str) {
    let Some(vr) = band.intersect(vp) else {
        return;
    };
    if vr.h < 6 {
        return;
    }
    label_clip(
        c,
        &label.to_uppercase(),
        ui::styled(14.0, 700, t.accent_dim),
        ui::MARGIN + 4,
        band.y + 14,
        band.w - ui::MARGIN * 2,
        vp,
    );
}

/// Flat list row shared by Recents + Contacts: leading 48px avatar,
/// title + caption, trailing call-back tonal icon button, inset divider.
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
    let av = Rect::new(row.x + ui::MARGIN, cy - 24, 48, 48);
    if vp.contains(Point::new(av.x, av.y))
        && vp.contains(Point::new(av.right() - 1, av.bottom() - 1))
    {
        draw_avatar(c, av, name, tint);
    }
    let tx = row.x + ui::MARGIN + 64;
    let call_r = row_call_rect(row);
    let text_right = call_r.x - 12;
    // Title line + trailing time caption (AOSP recents row anatomy).
    label_clip(
        c,
        name,
        ui::styled(20.0, 500, name_col),
        tx,
        row.y + 14,
        text_right - tx - 92,
        vp,
    );
    label_clip(
        c,
        trailing_text,
        ui::caption(t.text3),
        text_right - 88,
        row.y + 18,
        84,
        vp,
    );
    // Caption line: call-type icon + meta.
    let mut cap_x = tx;
    if let Some((ic, col)) = caption_icon {
        if vp.contains(Point::new(cap_x + 8, cy + 12)) {
            draw_icon(c, ic, 18.0, col, cap_x + 8, cy + 12);
        }
        cap_x += 26;
    }
    label_clip(
        c,
        caption,
        ui::caption(caption_col),
        cap_x,
        row.y + 50,
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
    let dy = row.bottom() + 2;
    if dy >= vp.y && dy < vp.bottom() {
        ui::divider_inset(c, t, tx, row.right() - ui::MARGIN, dy);
    }
}

impl DialerApp {
    // ---------------- favorites tab ----------------

    /// Favorite contact grid: 3-column tiles with a large avatar over a
    /// tonal card, tap = quick call — the speed-dial surface.
    pub(super) fn render_favorites(&mut self, c: &mut Canvas, t: &Theme, w: i32, h: i32) {
        let favs = self.favorite_indices();
        let vp = Rect::new(
            0,
            self.content_top(),
            w,
            self.content_bottom(w, h) - self.content_top(),
        );
        for (vis, &real) in favs.iter().enumerate() {
            let ct = &self.contacts[real];
            let card = self.fav_card_rect(w, h, vis);
            let Some(vr) = card.intersect(vp) else {
                continue;
            };
            if vr.h < 8 {
                continue;
            }
            let pressed = self.pressed == Some(Hit::ConCall(real));
            c.fill_rounded_rect(
                card,
                28.0,
                if pressed { t.surface3 } else { t.surface1 },
            );
            if pressed {
                ui::press_overlay(c, t, card, 28.0, 0.08);
            }
            // Avatar with a thin surface ring so the tile reads as one unit.
            let av = Rect::new(card.x + card.w / 2 - 36, card.y + 22, 72, 72);
            draw_avatar(c, av, &ct.name, ct.color);
            ui::draw_centered(
                c,
                &ct.name,
                &ui::styled(16.0, 600, t.text),
                Rect::new(card.x + 8, card.y + 104, card.w - 16, 24),
            );
            ui::draw_centered(
                c,
                if ct.label.is_empty() { "Mobile" } else { &ct.label },
                &ui::caption(t.text3),
                Rect::new(card.x + 8, card.y + 130, card.w - 16, 20),
            );
        }
        let content_h = self.favorites_height(favs.len());
        ui::scrollbar(
            c,
            t,
            Rect::new(w - 4, vp.y, 4, vp.h),
            self.scroll,
            content_h,
        );
        if favs.is_empty() {
            widgets::EmptyState::render(
                c,
                vp,
                t,
                icons::STAR_OUTLINE,
                "No favorites yet",
                "Star contacts to pin them here for quick calling",
            );
        }
    }

    // ---------------- recents tab ----------------

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

        let vp = self.recents_viewport(w, h);
        let items = self.recents_items();
        let now = now_unix();
        for (vis, item) in items.iter().enumerate() {
            let r = self.item_rect(w, &items, vis, vp.y);
            match item {
                Row::Header(label) => section_header(c, t, r, vp, label),
                Row::Recent(real) => {
                    let e = &self.recents.entries[*real];
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
                        self.pressed == Some(Hit::RecRow(*real)),
                        self.pressed == Some(Hit::RecCall(*real)),
                    );
                }
                Row::Contact(_) => {}
            }
        }

        // Scrollbar + empty state.
        ui::scrollbar(
            c,
            t,
            Rect::new(w - 4, vp.y, 4, vp.h),
            self.scroll,
            Self::items_height(&items),
        );
        if items.is_empty() {
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

    // ---------------- contacts tab ----------------

    pub(super) fn render_contacts(&mut self, c: &mut Canvas, t: &Theme, w: i32, h: i32) {
        let vp = self.contacts_viewport(w, h);
        let items = self.contacts_items();
        for (vis, item) in items.iter().enumerate() {
            let r = self.item_rect(w, &items, vis, vp.y);
            match item {
                Row::Header(label) => section_header(c, t, r, vp, label),
                Row::Contact(real) => {
                    let ct = &self.contacts[*real];
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
                        self.pressed == Some(Hit::ConRow(*real)),
                        self.pressed == Some(Hit::ConCall(*real)),
                    );
                }
                Row::Recent(_) => {}
            }
        }

        ui::scrollbar(
            c,
            t,
            Rect::new(w - 4, vp.y, 4, vp.h),
            self.scroll,
            Self::items_height(&items),
        );
        if items.is_empty() {
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
