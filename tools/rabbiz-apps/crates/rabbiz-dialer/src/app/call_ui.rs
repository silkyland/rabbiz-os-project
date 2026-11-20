//! In-call screen renderer (caller banner, call controls, incoming actions, DTMF pad).

use rabbiz_app_kit::{ui, Canvas, Rect, Theme};

use crate::model::{self, CallDir, CallPhase, SubLabel, KEYS};

use super::chrome::{alpha, draw_icon, hash_color, mix};
use super::{DialerApp, Hit, PRESS_SHRINK};

impl DialerApp {
    pub(super) fn render_call(&mut self, c: &mut Canvas, t: &Theme, w: i32, h: i32) {
        // Snapshot everything needed, then drop the borrow before the
        // &mut self sub-renderers.
        let (phase, dtmf_open, number, status, name, tint) = {
            let call = self.call.as_ref().expect("call screen without call");
            let contact = model::lookup_in(&self.contacts, &call.number);
            let name = contact.map_or(String::new(), |ct| ct.name.clone());
            let tint = contact.map_or_else(|| hash_color(&call.number), |ct| ct.color);
            let status = match (call.dir, call.phase) {
                (CallDir::In, CallPhase::Ringing) if !call.answered => "Incoming call".to_string(),
                (CallDir::In, CallPhase::Ringing) => "Ringing…".to_string(),
                (CallDir::Out, CallPhase::Dialing) => "Dialing…".to_string(),
                (CallDir::Out, CallPhase::Ringing) => "Ringing…".to_string(),
                (_, CallPhase::Active) => call.elapsed_label(),
                // Defensive: sessions are only ever In+Ringing or Out+{Dialing,Ringing,Active}.
                (CallDir::Missed, _) | (CallDir::In, CallPhase::Dialing) => {
                    "Call ended".to_string()
                }
            };
            (
                call.phase,
                call.dtmf_open,
                call.number.clone(),
                status,
                name,
                tint,
            )
        };
        let cx = w / 2;

        // Avatar.
        let av = Rect::new(cx - 66, ui::TOP_SAFE + 150, 132, 132);
        c.fill_rounded_rect(av, 66.0, alpha(tint, 0x30));
        if name.is_empty() {
            draw_icon(c, "\u{e7fd}", 56.0, tint, cx, av.y + av.h / 2);
        } else {
            let initial: String = name.chars().next().unwrap_or('?').to_uppercase().collect();
            ui::draw_centered(c, &initial, &ui::styled(56.0, 600, tint), av);
        }

        let display_name = if name.is_empty() {
            model::format_number(&number)
        } else {
            name.clone()
        };
        ui::draw_centered(
            c,
            &display_name,
            &ui::styled(32.0, 600, t.text),
            Rect::new(ui::MARGIN, av.y + av.h + 26, w - ui::MARGIN * 2, 48),
        );
        if !name.is_empty() {
            ui::draw_centered(
                c,
                &model::format_number(&number),
                &ui::caption(t.text3),
                Rect::new(ui::MARGIN, av.y + av.h + 76, w - ui::MARGIN * 2, 30),
            );
        }

        let active = phase == CallPhase::Active;
        let st_col = if active { t.success } else { t.text2 };
        ui::draw_centered(
            c,
            &status,
            &ui::styled(24.0, if active { 600 } else { 400 }, st_col),
            Rect::new(ui::MARGIN, av.y + av.h + 116, w - ui::MARGIN * 2, 40),
        );

        if dtmf_open {
            self.render_dtmf(c, t, w, h);
        } else {
            self.render_call_controls(c, t, w, h);
        }
    }

    fn render_call_controls(&mut self, c: &mut Canvas, t: &Theme, w: i32, h: i32) {
        let incoming = self
            .call
            .as_ref()
            .is_some_and(|c| c.dir == CallDir::In && c.phase == CallPhase::Ringing && !c.answered);
        let (muted, speaker, dtmf) = self
            .call
            .as_ref()
            .map_or((false, false, false), |c| (c.muted, c.speaker, c.dtmf_open));

        if incoming {
            // Decline (left, danger) / Answer (right, success).
            let by = h - ui::BOTTOM_SAFE - 330;
            let dec = Rect::new(w / 2 - 150, by, 120, 120);
            let ans = Rect::new(w / 2 + 30, by, 120, 120);
            c.fill_rounded_rect(
                dec,
                60.0,
                if self.pressed == Some(Hit::Call) {
                    mix(t.danger, [0, 0, 0, 255], 0.15)
                } else {
                    t.danger
                },
            );
            draw_icon(
                c,
                "\u{e0b1}",
                48.0,
                t.text_on_accent,
                dec.x + 60,
                dec.y + 60,
            );
            c.fill_rounded_rect(
                ans,
                60.0,
                if self.pressed == Some(Hit::RingSim) {
                    mix(t.success, [0, 0, 0, 255], 0.15)
                } else {
                    t.success
                },
            );
            draw_icon(
                c,
                "\u{e0b0}",
                48.0,
                t.text_on_accent,
                ans.x + 60,
                ans.y + 60,
            );
            return;
        }

        // Toggle row: mute / keypad / speaker.
        let ty = h - ui::BOTTOM_SAFE - 310;
        let tw = 84;
        let gap = (w - ui::MARGIN * 2 - tw * 3) / 2;
        let toggles: [(Hit, &'static str, &'static str, bool); 3] = [
            (
                Hit::Pad(90),
                if muted { "\u{e02b}" } else { "\u{e029}" },
                if muted { "Unmute" } else { "Mute" },
                muted,
            ),
            (Hit::Pad(91), "\u{e0bc}", "Keypad", dtmf),
            (
                Hit::Pad(92),
                "\u{e32d}",
                if speaker { "Earpiece" } else { "Speaker" },
                speaker,
            ),
        ];
        for (i, (hit, ic, label, on)) in toggles.iter().enumerate() {
            let r = Rect::new(ui::MARGIN + i as i32 * (tw + gap), ty, tw, tw);
            let pressed = self.pressed == Some(*hit);
            c.fill_rounded_rect(
                r,
                42.0,
                if pressed || *on {
                    alpha(t.accent, 0x40)
                } else {
                    t.surface1
                },
            );
            draw_icon(
                c,
                ic,
                34.0,
                if *on { t.accent } else { t.text },
                r.x + r.w / 2,
                r.y + r.w / 2,
            );
            ui::draw_centered(
                c,
                label,
                &ui::styled(14.0, 400, if *on { t.accent } else { t.text2 }),
                Rect::new(r.x, r.y + tw + 8, r.w, 22),
            );
        }

        // End-call FAB.
        let end = Rect::new(w / 2 - 60, h - ui::BOTTOM_SAFE - 170, 120, 120);
        c.fill_rounded_rect(
            end,
            60.0,
            if self.pressed == Some(Hit::Call) {
                mix(t.danger, [0, 0, 0, 255], 0.15)
            } else {
                t.danger
            },
        );
        draw_icon(
            c,
            "\u{e0b1}",
            50.0,
            t.text_on_accent,
            end.x + 60,
            end.y + 60,
        );
    }

    fn render_dtmf(&mut self, c: &mut Canvas, t: &Theme, w: i32, h: i32) {
        for (i, r) in self.dtmf_key_rects(w, h).iter().enumerate() {
            let digit = model::DTMF_KEYS[i];
            let pressed = self.pressed == Some(Hit::Dtmf(i));
            let mut rr = *r;
            if pressed {
                let dw = (r.w as f32 * PRESS_SHRINK) as i32;
                let dh = (r.h as f32 * PRESS_SHRINK) as i32;
                rr = Rect::new(
                    r.x + dw / 2,
                    r.y + dh / 2,
                    (r.w - dw).max(1),
                    (r.h - dh).max(1),
                );
            }
            c.fill_rounded_rect(
                rr,
                (rr.h as f32 / 2.0).min(26.0),
                if pressed { t.surface3 } else { t.surface1 },
            );
            ui::draw_centered(
                c,
                digit,
                &ui::styled(34.0, 600, t.text),
                Rect::new(rr.x, rr.y + rr.h / 2 - 28, rr.w, 42),
            );
            // Letters under digits 2-9 (indices 1-8; same T9 map as the dial pad).
            if (1..=8).contains(&i) {
                if let SubLabel::Letters(s) = KEYS[i].sub {
                    ui::draw_centered(
                        c,
                        s,
                        &ui::styled(13.0, 600, t.text3),
                        Rect::new(rr.x, rr.y + rr.h / 2 + 8, rr.w, 20),
                    );
                }
            }
        }
        let hide = Rect::new(w / 2 - 90, h - ui::BOTTOM_SAFE - 70, 180, 52);
        ui::draw_pill_button(
            c,
            t,
            hide,
            "Hide keypad",
            false,
            self.pressed == Some(Hit::Bksp),
        );
    }
}
