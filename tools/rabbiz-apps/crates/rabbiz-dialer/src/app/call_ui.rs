//! In-call screen renderer — a full-height gradient tinted by the
//! caller's contact color (dark plum at the bottom, tint bleeding in from
//! the status bar), the identity in open air (no containing card), the
//! 2x3 tonal control grid, incoming answer/decline circles, DTMF pad.

use rabbiz_app_kit::{icons, ui, Canvas, Rect, Theme};

use crate::model::{self, CallDir, CallPhase, SubLabel, KEYS};

use super::chrome::{draw_avatar, draw_icon, hash_color};
use super::{DialerApp, Hit, PRESS_SHRINK};
use rabbiz_app_kit::ui::{mix, with_alpha as alpha};

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

        // Full-height tinted gradient: the contact color bleeds in from
        // the top edge and dissolves into the page background by ~60% of
        // the screen height. Drawn as horizontal strips (fill_rect only).
        let zone = self.caller_zone(w);
        let top_col = mix(t.bg, tint, 0.42);
        // One strip per ~4px row — fine enough that the falloff reads as a
        // real gradient, not banding (scanline fills are nearly free).
        let strips = (h / 4).max(64);
        for i in 0..strips {
            let f0 = i as f32 / strips as f32;
            // quadratic falloff — strong tint at the very top, gone early.
            let m = (1.0 - f0 / 0.62).max(0.0);
            let col = mix(t.bg, top_col, m * m);
            let y0 = h * i / strips;
            let y1 = h * (i + 1) / strips;
            c.fill_rect(Rect::new(0, y0, w, (y1 - y0).max(1)), col);
        }

        // Big centered avatar with a soft tonal halo ring.
        let av = Rect::new(cx - 75, zone.y + 28, 150, 150);
        c.fill_rounded_rect(
            Rect::new(av.x - 8, av.y - 8, av.w + 16, av.h + 16),
            (av.h + 16) as f32 / 2.0,
            alpha(tint, 0x30),
        );
        draw_avatar(c, av, &name, tint);

        let display_name = if name.is_empty() {
            model::format_number(&number)
        } else {
            name.clone()
        };
        ui::draw_centered(
            c,
            &display_name,
            &ui::styled(34.0, 600, t.text),
            Rect::new(zone.x + 8, av.bottom() + 22, zone.w - 16, 48),
        );
        if !name.is_empty() {
            ui::draw_centered(
                c,
                &model::format_number(&number),
                &ui::styled(18.0, 400, t.text2),
                Rect::new(zone.x + 8, av.bottom() + 74, zone.w - 16, 28),
            );
        }

        let active = phase == CallPhase::Active;
        let st_col = if active { t.success } else { t.text2 };
        ui::draw_centered(
            c,
            &status,
            &ui::styled(24.0, if active { 600 } else { 400 }, st_col),
            Rect::new(zone.x + 8, zone.bottom() - 48, zone.w - 16, 40),
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
            // Decline (left, danger) / Answer (right, success) — labeled
            // circles like the AOSP incoming-call screen.
            let (dec, ans, dec_lbl, ans_lbl) = self.incoming_rects(w, h);
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
                icons::CALL_END,
                44.0,
                t.text_on_accent,
                dec.x + 60,
                dec.y + 60,
            );
            ui::draw_centered(c, "Decline", &ui::label(t.text), dec_lbl);
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
                icons::CALL,
                44.0,
                t.text_on_accent,
                ans.x + 60,
                ans.y + 60,
            );
            ui::draw_centered(c, "Answer", &ui::label(t.text), ans_lbl);
            return;
        }

        // 2×3 tonal-circle grid. Row 0 wires the real toggles; row 1 keeps
        // the AOSP six-slot anatomy with the unwired slots drawn disabled.
        let grid = self.in_call_grid(w, h);
        let real: [(Hit, &str, &str, bool); 3] = [
            (
                Hit::Mute,
                if muted { icons::MIC_OFF } else { icons::MIC },
                if muted { "Unmute" } else { "Mute" },
                muted,
            ),
            (Hit::DtmfToggle, icons::DIALPAD, "Keypad", dtmf),
            (
                Hit::Speaker,
                icons::VOLUME_UP,
                if speaker { "Earpiece" } else { "Speaker" },
                speaker,
            ),
        ];
        for (i, (hit, ic, label, on)) in real.iter().enumerate() {
            let r = grid[0][i];
            let pressed = self.pressed == Some(*hit);
            c.fill_rounded_rect(
                r,
                r.h as f32 / 2.0,
                if *on {
                    alpha(t.accent, 0x40)
                } else {
                    t.surface2
                },
            );
            if pressed {
                ui::press_overlay(c, t, r, r.h as f32 / 2.0, 0.10);
            }
            draw_icon(
                c,
                ic,
                32.0,
                if *on { t.accent } else { t.text },
                r.x + r.w / 2,
                r.y + r.h / 2,
            );
            ui::draw_centered(
                c,
                label,
                &ui::styled(14.0, 400, if *on { t.accent } else { t.text2 }),
                Rect::new(r.x - 22, r.y + r.h + 6, r.w + 44, 22),
            );
        }
        // Reserved slots (hold / add call / video) — visibly disabled.
        let stubs: [(&str, &str); 3] = [
            (icons::PAUSE, "Hold"),
            (icons::ADD, "Add call"),
            (icons::VIDEOCAM, "Video"),
        ];
        for (i, (ic, label)) in stubs.iter().enumerate() {
            let r = grid[1][i];
            c.fill_rounded_rect(r, r.h as f32 / 2.0, t.surface1);
            draw_icon(
                c,
                ic,
                32.0,
                alpha(t.text3, 0x80),
                r.x + r.w / 2,
                r.y + r.h / 2,
            );
            ui::draw_centered(
                c,
                label,
                &ui::styled(14.0, 400, alpha(t.text3, 0x80)),
                Rect::new(r.x - 22, r.y + r.h + 6, r.w + 44, 22),
            );
        }

        // Big red end-call circle, bottom center.
        let end = self.end_call_rect(w, h);
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
            icons::CALL_END,
            48.0,
            t.text_on_accent,
            end.x + 60,
            end.y + 60,
        );
    }

    fn render_dtmf(&mut self, c: &mut Canvas, t: &Theme, w: i32, h: i32) {
        // Panel behind the pad so keys read over the caller card.
        let sheet = self.keypad_sheet(w, h);
        c.fill_rounded_rect(sheet, 36.0, t.surface1);
        let r = (36.0f32) as i32;
        if sheet.h > r {
            c.fill_rect(
                Rect::new(sheet.x, sheet.y + r, sheet.w, sheet.h - r),
                t.surface1,
            );
        }

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
            let radius = rr.h as f32 / 2.0;
            c.fill_rounded_rect(
                rr,
                radius,
                if pressed {
                    alpha(t.accent, 0x30)
                } else {
                    t.surface2
                },
            );
            ui::draw_centered(
                c,
                digit,
                &ui::styled(34.0, 600, t.text),
                Rect::new(rr.x, rr.y + rr.h / 2 - 26, rr.w, 40),
            );
            // Letters under digits 2-9 (indices 1-8; same T9 map as the pad).
            if (1..=8).contains(&i) {
                if let SubLabel::Letters(s) = KEYS[i].sub {
                    ui::draw_centered(
                        c,
                        s,
                        &ui::styled(12.0, 600, t.text3),
                        Rect::new(rr.x, rr.y + rr.h / 2 + 10, rr.w, 18),
                    );
                }
            }
        }
        let hide = self.dtmf_hide_rect(w, h);
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
