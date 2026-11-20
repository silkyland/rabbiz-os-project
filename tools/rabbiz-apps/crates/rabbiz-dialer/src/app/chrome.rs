//! Shared dialer chrome (header, tab strip) and low-level paint primitives.

use de_api::Pixels;
use rabbiz_app_kit::{text, ui, Canvas, Point, Rect, Size, Theme};

use crate::backend::BackendKind;
use crate::model::{self, Contact};

use super::{DialerApp, Hit, Tab};

pub(super) fn alpha(c: [u8; 4], a: u8) -> [u8; 4] {
    [c[0], c[1], c[2], a]
}

pub(super) fn mix(a: [u8; 4], b: [u8; 4], t: f32) -> [u8; 4] {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t) as u8;
    [l(a[0], b[0]), l(a[1], b[1]), l(a[2], b[2]), l(a[3], b[3])]
}

/// Rasterize + blit a Material icon with its ink bounds centered on (cx, cy).
pub(super) fn draw_icon(
    c: &mut Canvas,
    codepoint: &str,
    size: f32,
    color: [u8; 4],
    cx: i32,
    cy: i32,
) {
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
pub(super) fn fit_style(
    s: &str,
    sizes: &[f32],
    weight: u16,
    color: [u8; 4],
    max_w: i32,
) -> text::LabelStyle {
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
pub(super) fn draw_avatar(c: &mut Canvas, r: Rect, name: &str, tint: [u8; 4]) {
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
pub(super) fn hash_color(seed: &str) -> [u8; 4] {
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

pub(super) fn entry_tint(contacts: &[Contact], name: &str, number: &str) -> [u8; 4] {
    if let Some(ct) = model::lookup_in(contacts, number) {
        if name.is_empty() || ct.name == name {
            return ct.color;
        }
    }
    hash_color(if name.is_empty() { number } else { name })
}

impl DialerApp {
    pub(super) fn render_header(&mut self, c: &mut Canvas, t: &Theme, w: i32) {
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

    pub(super) fn render_tabs(&mut self, c: &mut Canvas, t: &Theme, w: i32) {
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
}
