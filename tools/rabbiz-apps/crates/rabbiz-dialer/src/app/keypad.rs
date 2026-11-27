//! Keypad sheet renderer — M3 bottom-sheet overlay with circular
//! borderless T9 keys, a centered call FAB, backspace and a hide chevron.

use de_api::Size;
use rabbiz_app_kit::{icons, text, ui, widgets, Canvas, Rect, Theme};

use crate::model::{self, DialKey, SubLabel, KEYS};

use super::chrome::{draw_icon, fit_style};
use super::{DialerApp, Hit, POP_S, PRESS_SHRINK};
use rabbiz_app_kit::ui::{mix, with_alpha as alpha};

impl DialerApp {
    /// The keypad bottom sheet: scrim + rounded sheet + drag handle, then
    /// number display, caller-ID pill, 12 circular keys and the action row.
    pub(super) fn render_keypad_sheet(&mut self, c: &mut Canvas, t: &Theme, w: i32, h: i32) {
        let sheet = self.keypad_sheet_anim(w, h);
        let ease = self.sheet_ease();
        // Scrim fades in with the sheet travel; the sheet body itself is
        // the widget's rounded-top, squared-bottom shape.
        let scrim_a = (t.scrim[3] as f32 * ease).round() as u8;
        c.fill_rect(
            Rect::new(0, 0, w, h),
            [t.scrim[0], t.scrim[1], t.scrim[2], scrim_a],
        );
        c.fill_rounded_rect(sheet, 36.0, t.surface1);
        let r_int = 36.min(sheet.h / 2);
        if sheet.h > r_int {
            c.fill_rect(
                Rect::new(sheet.x, sheet.y + r_int, sheet.w, sheet.h - r_int),
                t.surface1,
            );
        }
        let handle = Rect::new(sheet.x + (sheet.w - 44) / 2, sheet.y + 12, 44, 5);
        c.fill_rounded_rect(handle, 2.5, ui::state_layer(t, 0.35));
        let _ = Size::new(w, h);

        // Number display (big, fit-shrunk), vertically centered in the
        // display band under the handle.
        let disp = Rect::new(ui::MARGIN, sheet.y + 36, w - ui::MARGIN * 2, 88);
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
            &[48.0, 42.0, 36.0, 30.0, 26.0],
            600,
            col,
            disp.w - 16,
        );
        ui::draw_centered(c, &shown, &st, disp);

        // Caller-ID pill under the display — measured width, never fixed.
        if !self.number.is_empty() {
            if let Some(ct) = model::lookup_in(&self.contacts, &self.number) {
                let txt = format!("{} · {}", ct.name, ct.label.as_str());
                let st = ui::styled(16.0, 600, t.text);
                let tw = text::label(&txt, st, None).map_or(120, |px| px.width() as i32);
                let pw = (tw + 60).clamp(120, w - ui::MARGIN * 2);
                let pill = Rect::new(w / 2 - pw / 2, sheet.y + 108, pw, 40);
                c.fill_rounded_rect(pill, 20.0, t.surface2);
                draw_icon(c, icons::PERSON, 16.0, ct.color, pill.x + 24, pill.y + 20);
                ui::draw_text(
                    c,
                    &txt,
                    &st,
                    pill.x + 44,
                    pill.y + (pill.h - 24) / 2,
                    Some(pill.w - 52),
                );
            }
        }

        for (i, r) in self.pad_key_rects(w, h).iter().enumerate() {
            self.draw_pad_key(c, t, *r, KEYS[i], i);
        }

        // Action row: hide chevron | centered call FAB | backspace.
        let (hide, call, bksp) = self.action_row(w, h);
        widgets::icon_btn(
            c,
            t,
            hide,
            icons::EXPAND_MORE,
            widgets::IconBtnStyle::Plain,
            false,
            self.pressed == Some(Hit::KeypadHide),
        );
        widgets::icon_btn(
            c,
            t,
            bksp,
            icons::BACKSPACE,
            widgets::IconBtnStyle::Plain,
            false,
            self.pressed == Some(Hit::Bksp),
        );

        // Centered accent call FAB (round, ~96px).
        let call_pressed = self.pressed == Some(Hit::Call);
        let call_col = if call_pressed {
            mix(t.success, [0, 0, 0, 255], 0.15)
        } else {
            t.success
        };
        c.fill_rounded_rect(call.translate(0, 4), call.h as f32 / 2.0, t.shadow);
        c.fill_rounded_rect(call, call.h as f32 / 2.0, call_col);
        draw_icon(
            c,
            icons::CALL,
            44.0,
            t.text_on_accent,
            call.x + call.w / 2,
            call.y + call.h / 2,
        );
    }

    /// One pad key: a circular borderless target — press = tonal state
    /// layer, release = the existing decaying "pop" scale.
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
        let radius = rr.h as f32 / 2.0;
        // Tonal circles: a quiet surface2 disc at rest, accent tonal wash
        // while pressed — the M3 dialer key.
        if pressed {
            c.fill_rounded_rect(rr, radius, alpha(t.accent, 0x38));
        } else {
            c.fill_rounded_rect(rr, radius, t.surface2);
        }
        ui::draw_centered(
            c,
            key.digit,
            &ui::styled(36.0, 500, if pressed { t.accent } else { t.text }),
            Rect::new(rr.x, rr.y + rr.h / 2 - 28, rr.w, 42),
        );
        match key.sub {
            SubLabel::None => {}
            SubLabel::Letters(s) => {
                ui::draw_centered(
                    c,
                    s,
                    &ui::styled(13.0, 600, t.text3),
                    Rect::new(rr.x, rr.y + rr.h / 2 + 10, rr.w, 20),
                );
            }
            SubLabel::Icon(cp) => {
                draw_icon(c, cp, 15.0, t.text3, rr.x + rr.w / 2, rr.y + rr.h / 2 + 22);
            }
        }
    }
}
