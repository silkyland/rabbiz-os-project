//! Shared dialer chrome (header, tab strip) and low-level paint primitives.

use de_api::Pixels;
use rabbiz_app_kit::{icons, text, ui, widgets, Canvas, Point, Rect, Size, Theme};

use crate::backend::BackendKind;
use crate::model::{self, Contact};

use super::{DialerApp, Hit, Tab};

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

/// Avatar circle: tinted bg + initial letter or person icon — the shared
/// kit widget ([`widgets::avatar`]) with the tint as glyph color.
pub(super) fn draw_avatar(c: &mut Canvas, r: Rect, name: &str, tint: [u8; 4]) {
    widgets::avatar(c, name, r, tint, tint);
}

/// Stable avatar tint for unknown numbers — the shared kit palette
/// ([`ui::avatar_tint`]), so a peer matches contacts/sms.
pub(super) fn hash_color(seed: &str) -> [u8; 4] {
    ui::avatar_tint(seed)
}

pub(super) fn entry_tint(contacts: &[Contact], name: &str, number: &str) -> [u8; 4] {
    if let Some(ct) = model::lookup_in(contacts, number) {
        if name.is_empty() || ct.name == name {
            return ct.color;
        }
    }
    hash_color(if name.is_empty() { number } else { name })
}

/// Rect of the simulated-backend "Ring" pill in the header (same geometry
/// for render and hit-test).
pub(super) fn ring_rect(w: i32) -> Rect {
    Rect::new(w - ui::MARGIN - 96, ui::TOP_SAFE + 12, 96, 48)
}

/// Rect of the backend-status chip in the header.
pub(super) fn chip_rect(w: i32, ring_visible: bool) -> Rect {
    let right = if ring_visible {
        ring_rect(w).x - 10
    } else {
        w - ui::MARGIN
    };
    Rect::new(right - 104, ui::TOP_SAFE + 12, 104, 48)
}

impl DialerApp {
    /// Header: "Phone" + backend status caption, status chip, and (simulated
    /// backends only) the Ring button. The right-side reservation is
    /// computed from the actual chip/ring rects — no fixed 320px carve-out.
    pub(super) fn render_header(&mut self, c: &mut Canvas, t: &Theme, w: i32) {
        let ring_visible = self.backend.can_simulate_incoming();
        let chip = chip_rect(w, ring_visible);
        let text_right = chip.x - 12;
        let y = ui::TOP_SAFE + 12;

        let tx = ui::MARGIN;
        ui::draw_text(
            c,
            "Phone",
            &ui::title(t.text),
            tx,
            y - 2,
            Some((text_right - tx).max(40)),
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
            tx,
            y + 42,
            Some((text_right - tx).max(40)),
        );

        // Status chip (modem/sim/demo) + optional Ring simulator.
        let modem_ok = self.backend.kind != BackendKind::Simulated || self.backend.modem_node;
        let chip_col = if modem_ok { t.success } else { t.warning };
        c.fill_rounded_rect(chip, chip.h as f32 / 2.0, t.surface2);
        draw_icon(
            c,
            if modem_ok {
                icons::SIGNAL_CELLULAR
            } else {
                "\u{e624}" // signal_cellular_off (kept from the old header)
            },
            20.0,
            chip_col,
            chip.x + 22,
            chip.y + chip.h / 2,
        );
        ui::draw_centered(
            c,
            self.backend.chip(),
            &ui::styled(15.0, 600, t.text),
            Rect::new(chip.x + 32, chip.y, chip.w - 36, chip.h),
        );

        if ring_visible {
            let ring = ring_rect(w);
            let pressed = self.pressed == Some(Hit::RingSim);
            ui::draw_pill_button(c, t, ring, "Ring", false, pressed);
        }
    }

    /// Shared search pill above both pages — filters recents and contacts.
    pub(super) fn render_search(&mut self, c: &mut Canvas, t: &Theme, w: i32) {
        let r = self.search_rect(w);
        widgets::SearchPill::render(
            c,
            r,
            t,
            &self.query,
            "Search contacts or numbers",
            self.search_focus,
        );
    }

    /// Page switcher: M3 segmented control (Recents | Contacts).
    pub(super) fn render_tabs(&mut self, c: &mut Canvas, t: &Theme, w: i32) {
        let bar = self.tabs_rect(w);
        let labels: Vec<&str> = Tab::ALL.iter().map(|t| t.label()).collect();
        let active = Tab::ALL.iter().position(|x| *x == self.tab).unwrap_or(0);
        widgets::segmented(c, t, bar, &labels, active);
    }

    /// Trailing dialpad FAB opening the keypad sheet.
    pub(super) fn render_fab(&mut self, c: &mut Canvas, t: &Theme, w: i32, h: i32) {
        let r = self.fab_rect(w, h);
        widgets::fab(
            c,
            t,
            r,
            icons::DIALPAD,
            self.pressed == Some(Hit::KeypadFab),
        );
    }
}
