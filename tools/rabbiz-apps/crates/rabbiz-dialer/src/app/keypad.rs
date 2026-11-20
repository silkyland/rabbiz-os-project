//! Keypad tab renderer and per-key press/pop animation.

use rabbiz_app_kit::{ui, Canvas, Rect, Theme};

use crate::model::{self, DialKey, SubLabel, KEYS};

use super::chrome::{alpha, draw_icon, fit_style, mix};
use super::{DialerApp, Hit, POP_S, PRESS_SHRINK};

impl DialerApp {
    pub(super) fn render_keypad(&mut self, c: &mut Canvas, t: &Theme, w: i32, h: i32) {
        let top = self.content_top();

        // Caller-ID pill.
        if !self.number.is_empty() {
            if let Some(ct) = model::lookup_in(&self.contacts, &self.number) {
                let pill = Rect::new(w / 2 - 180, top, 360, 40);
                c.fill_rounded_rect(pill, 20.0, t.surface1);
                draw_icon(c, "\u{e7fd}", 16.0, ct.color, pill.x + 22, pill.y + 20);
                let txt = format!("{} · {}", ct.name, ct.label.as_str());
                ui::draw_centered(
                    c,
                    &txt,
                    &ui::styled(16.0, 600, t.text),
                    Rect::new(pill.x + 38, pill.y, pill.w - 44, pill.h),
                );
            }
        }

        let keys = self.pad_key_rects(w, h);
        let grid_top = keys.first().map_or(top + 200, |r| r.y);
        let disp = Rect::new(
            ui::MARGIN,
            top + 56,
            w - ui::MARGIN * 2,
            (grid_top - top - 72).max(80),
        );
        let shown = if self.number.is_empty() {
            "Enter number".to_string()
        } else {
            model::format_number(&self.number)
        };
        let col = if self.number.is_empty() {
            t.text3
        } else {
            t.text
        };
        let st = fit_style(
            &shown,
            &[52.0, 46.0, 40.0, 34.0, 28.0],
            600,
            col,
            disp.w - 16,
        );
        ui::draw_centered(c, &shown, &st, disp);

        for (i, r) in keys.iter().enumerate() {
            self.draw_pad_key(c, t, *r, KEYS[i], i);
        }

        // Action row: backspace + call FAB.
        let (bk, call) = self.action_row(w, h);
        let bk_pressed = self.pressed == Some(Hit::Bksp);
        c.fill_rounded_rect(bk, 26.0, if bk_pressed { t.surface3 } else { t.surface1 });
        draw_icon(
            c,
            "\u{e14a}",
            32.0,
            t.text,
            bk.x + bk.w / 2,
            bk.y + bk.h / 2,
        );

        let call_pressed = self.pressed == Some(Hit::Call);
        let call_col = if call_pressed {
            mix(t.success, [0, 0, 0, 255], 0.15)
        } else {
            t.success
        };
        c.fill_rounded_rect(call, call.h as f32 / 2.0, call_col);
        draw_icon(
            c,
            "\u{e0b0}",
            46.0,
            t.text_on_accent,
            call.x + call.w / 2,
            call.y + call.h / 2,
        );
    }

    /// One pad key: surface card, accent state-layer when pressed, and a
    /// decaying scale "pop" for ~POP_S after release.
    fn draw_pad_key(&self, c: &mut Canvas, t: &Theme, r: Rect, key: DialKey, idx: usize) {
        let mut scale = 1.0f32;
        if let Some((pi, pt)) = self.pop {
            if pi == idx {
                let p = (pt / POP_S).min(1.0);
                scale = 1.0 + 0.07 * (1.0 - p) * (1.0 - p);
            }
        }
        let pressed = self.pressed == Some(Hit::Pad(idx));
        let mut rr = r;
        if pressed {
            let dw = (r.w as f32 * PRESS_SHRINK) as i32;
            let dh = (r.h as f32 * PRESS_SHRINK) as i32;
            rr = Rect::new(
                r.x + dw / 2,
                r.y + dh / 2,
                (r.w - dw).max(1),
                (r.h - dh).max(1),
            );
        } else if scale != 1.0 {
            let dw = (r.w as f32 * (scale - 1.0)) as i32;
            let dh = (r.h as f32 * (scale - 1.0)) as i32;
            rr = Rect::new(r.x - dw / 2, r.y - dh / 2, r.w + dw, r.h + dh);
        }
        let radius = (rr.h as f32 / 2.0).min(30.0);
        c.fill_rounded_rect(rr, radius, t.surface1);
        if pressed {
            c.fill_rounded_rect(rr, radius, alpha(t.accent, 0x30));
        }
        let digit_st = ui::styled(36.0, 600, t.text);
        ui::draw_centered(
            c,
            key.digit,
            &digit_st,
            Rect::new(rr.x, rr.y + rr.h / 2 - 32, rr.w, 46),
        );
        match key.sub {
            SubLabel::None => {}
            SubLabel::Letters(s) => {
                ui::draw_centered(
                    c,
                    s,
                    &ui::styled(14.0, 600, t.text3),
                    Rect::new(rr.x, rr.y + rr.h / 2 + 6, rr.w, 22),
                );
            }
            SubLabel::Icon(cp) => {
                draw_icon(c, cp, 16.0, t.text3, rr.x + rr.w / 2, rr.y + rr.h / 2 + 20);
            }
        }
    }
}
