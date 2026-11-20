//! Canvas rendering for the Music GUI — header controls, status banners,
//! scope chips, track list rows, empty states, and the now-playing card.

use de_api::Pixels;
use rabbiz_app_kit::{text, ui, Canvas, Point, Rect, Size, Theme};

use crate::app::{rubber_frac, Hit, Layout, MusicApp, ROW_H, ROW_PITCH, SEEK_THUMB, SEEK_TRACK_H};
use crate::library::{self, format_time, Scope};

// Material Icons Round codepoints (verified against the bundled TTF cmap).
const IC_PLAY: &str = "\u{e037}";
const IC_PAUSE: &str = "\u{e034}";
const IC_STOP: &str = "\u{e047}";
const IC_PREV: &str = "\u{e045}";
const IC_NEXT: &str = "\u{e044}";
const IC_NOTE: &str = "\u{e405}";
const IC_MUSIC: &str = "\u{e030}";
const IC_EQ: &str = "\u{e1b8}";
const IC_MUSIC_OFF: &str = "\u{e440}";
const IC_REFRESH: &str = "\u{e5d5}";
const IC_ERROR: &str = "\u{e001}";

/// Single-line ellipsized text (the kit's `draw_text` can wrap; list rows
/// and the now-playing card need one line).
fn draw_line(c: &mut Canvas, s: &str, st: &text::LabelStyle, x: i32, y: i32, max_w: i32) {
    if s.is_empty() {
        return;
    }
    if let Some(px) = text::label_clamped(s, *st, max_w.max(8), 1) {
        c.blit(
            &Pixels::new(
                Size::new(px.width() as i32, px.height() as i32),
                px.data().to_vec(),
            ),
            Point::new(x, y),
        );
    }
}

/// Rasterize + blit a Material icon with ink bounds centered on (cx, cy).
fn draw_icon(c: &mut Canvas, codepoint: &str, size: f32, color: [u8; 4], cx: i32, cy: i32) {
    if let Some(px) = text::icon(codepoint, size, color) {
        let (x0, y0, x1, y1) =
            text::ink_bounds(&px).unwrap_or((0, 0, px.width() as i32, px.height() as i32));
        c.blit(
            &Pixels::new(
                Size::new(px.width() as i32, px.height() as i32),
                px.data().to_vec(),
            ),
            Point::new(cx - (x0 + x1) / 2, cy - (y0 + y1) / 2),
        );
    }
}

/// Circular icon button (transport / rescan / stop).
fn draw_circle_button(
    c: &mut Canvas,
    r: Rect,
    icon: &str,
    icon_size: f32,
    bg: [u8; 4],
    fg: [u8; 4],
    pressed: bool,
) {
    let rr = if pressed {
        Rect::new(r.x + 2, r.y + 2, (r.w - 4).max(1), (r.h - 4).max(1))
    } else {
        r
    };
    let rad = (rr.h as f32) / 2.0;
    c.fill_rounded_rect(rr, rad, bg);
    if pressed {
        c.fill_rounded_rect(rr, rad, [255, 255, 255, 28]);
    }
    draw_icon(c, icon, icon_size, fg, rr.x + rr.w / 2, rr.y + rr.h / 2);
}

impl MusicApp {
    pub(crate) fn draw_rescan(&self, c: &mut Canvas, t: &Theme, r: Rect) {
        draw_circle_button(
            c,
            r,
            IC_REFRESH,
            24.0,
            t.surface2,
            t.text,
            self.pressed == Some(Hit::Rescan),
        );
    }

    pub(crate) fn draw_banners(&self, c: &mut Canvas, t: &Theme, top: i32) {
        let w = c.size().w;
        let mut y = top;
        if !self.player.mpv_ok {
            let r = Rect::new(ui::MARGIN, y, w - ui::MARGIN * 2, 80);
            ui::draw_card(c, t, r, false);
            draw_icon(c, IC_ERROR, 22.0, t.danger, r.x + 34, r.y + 26);
            draw_line(
                c,
                "mpv is not installed, so nothing can play. Install it (pacman -S mpv), then tap the refresh button.",
                &ui::styled(16.0, 400, t.danger),
                r.x + 60,
                r.y + 12,
                r.w - 84,
            );
            draw_line(
                c,
                "Audio output uses PipeWire/PulseAudio — the system volume keys control it.",
                &ui::caption(t.text3),
                r.x + 60,
                r.y + 40,
                r.w - 84,
            );
            y += 88;
        }
        if let Some(err) = &self.player.error {
            let r = Rect::new(ui::MARGIN, y, w - ui::MARGIN * 2, 80);
            ui::draw_card(c, t, r, false);
            draw_icon(c, IC_ERROR, 22.0, t.danger, r.x + 34, r.y + 26);
            draw_line(
                c,
                err,
                &ui::styled(16.0, 400, t.danger),
                r.x + 60,
                r.y + 22,
                r.w - 84,
            );
        }
    }

    pub(crate) fn draw_chips(&self, c: &mut Canvas, t: &Theme, l: &Layout) {
        if self.tracks.is_empty() {
            return;
        }
        for (i, s) in Scope::ALL.iter().enumerate() {
            let r = l.chips[i];
            let active = self.scope == *s;
            let bg = if active { t.accent } else { t.surface2 };
            let fg = if active { t.text_on_accent } else { t.text };
            c.fill_rounded_rect(r, (r.h as f32) / 2.0, bg);
            if self.pressed == Some(Hit::Chip(*s)) {
                c.fill_rounded_rect(r, (r.h as f32) / 2.0, [255, 255, 255, 28]);
            }
            let count = match s.folder() {
                None => self.tracks.len(),
                Some(f) => library::count_folder(&self.tracks, f),
            };
            let label = format!("{} · {count}", s.label());
            ui::draw_centered(c, &label, &ui::styled(18.0, 600, fg), r);
        }
    }

    pub(crate) fn draw_section_label(&self, c: &mut Canvas, t: &Theme, l: &Layout) {
        if self.tracks.is_empty() {
            return;
        }
        let y = l.chips[0].y + 48 + 8;
        draw_line(
            c,
            "Library",
            &ui::styled(16.0, 600, t.text3),
            ui::MARGIN,
            y,
            200,
        );
        draw_line(
            c,
            "Newest first",
            &ui::caption(t.text3),
            c.size().w - ui::MARGIN - 140,
            y + 2,
            140,
        );
    }

    pub(crate) fn draw_list(&self, c: &mut Canvas, t: &Theme, l: &Layout) {
        let vis = self.visible();
        if vis.is_empty() {
            return;
        }
        for (vi, &ti) in vis.iter().enumerate() {
            let row_top = l.list.y + vi as i32 * ROW_PITCH - self.scroll;
            // Clamp the row to the viewport (no scissor on Canvas — same
            // convention as the dialer app).
            let top = row_top.max(l.list.y);
            let bottom = (row_top + ROW_H).min(l.list.y + l.list.h);
            let rh = bottom - top;
            if rh <= 0 {
                continue;
            }
            let r = Rect::new(l.list.x, top, l.list.w, rh);
            let track = &self.tracks[ti];
            let is_current = self.player.current == Some(ti);
            let pressed = self.pressed == Some(Hit::Row(ti));
            c.fill_rounded_rect(r, 20.0, if pressed { t.surface3 } else { t.surface1 });
            if is_current {
                // Accent spine marks the loaded track.
                c.fill_rounded_rect(
                    Rect::new(r.x + 4, r.y + 14, 4, (r.h - 28).max(4)),
                    2.0,
                    t.accent,
                );
            }
            // Art tile: format glyph stands in for artwork (MVP — no art).
            let tile = Rect::new(r.x + 16, r.y + (rh - 56).max(0) / 2, 56, 56);
            c.fill_rounded_rect(tile, 16.0, t.surface3);
            let (icon, icol) = if is_current && self.player.is_playing() {
                (IC_EQ, t.accent)
            } else if is_current {
                (IC_PAUSE, t.text2)
            } else {
                (IC_NOTE, t.text3)
            };
            draw_icon(c, icon, 26.0, icol, tile.x + 28, tile.y + 28);

            let tx = r.x + 88;
            let title_col = if is_current { t.accent } else { t.text };
            draw_line(
                c,
                &track.title,
                &ui::styled(20.0, 600, title_col),
                tx,
                r.y + 14,
                r.w - 88 - 90,
            );
            draw_line(
                c,
                &track.caption(),
                &ui::caption(t.text3),
                tx,
                r.y + 38,
                r.w - 88 - 90,
            );
            // Format badge, right-aligned in a fixed slot.
            let badge = Rect::new(r.x + r.w - 84, r.y + (rh - 30).max(0) / 2, 68, 30);
            draw_line(
                c,
                &track.ext.to_uppercase(),
                &ui::styled(14.0, 600, t.text3),
                badge.x,
                badge.y + 4,
                badge.w,
            );
        }
    }

    pub(crate) fn draw_empty(&self, c: &mut Canvas, t: &Theme, l: &Layout) {
        let Some(btn) = l.empty_btn else { return };
        let cy = btn.y - 150;
        if self.tracks.is_empty() {
            draw_icon(c, IC_MUSIC_OFF, 64.0, t.text3, c.size().w / 2, cy);
            ui::draw_centered(
                c,
                "No audio files yet",
                &ui::styled(24.0, 600, t.text),
                Rect::new(0, cy + 48, c.size().w, 40),
            );
            ui::draw_centered(
                c,
                "Put audio in ~/Music or ~/Downloads, then tap Scan again.",
                &ui::body(t.text3),
                Rect::new(ui::MARGIN * 2, cy + 92, c.size().w - ui::MARGIN * 4, 60),
            );
            ui::draw_pill_button(
                c,
                t,
                btn,
                "Scan again",
                true,
                self.pressed == Some(Hit::EmptyAction),
            );
        } else {
            // Scope filter produced an empty list.
            draw_icon(c, IC_MUSIC, 56.0, t.text3, c.size().w / 2, cy);
            ui::draw_centered(
                c,
                &format!("Nothing in {}", self.scope.label()),
                &ui::styled(24.0, 600, t.text),
                Rect::new(0, cy + 48, c.size().w, 40),
            );
            ui::draw_centered(
                c,
                "Tap All to see every track, or add audio to this folder.",
                &ui::body(t.text3),
                Rect::new(ui::MARGIN * 2, cy + 92, c.size().w - ui::MARGIN * 4, 60),
            );
            ui::draw_pill_button(
                c,
                t,
                btn,
                "Show all tracks",
                true,
                self.pressed == Some(Hit::EmptyAction),
            );
        }
    }

    pub(crate) fn draw_now_playing(&self, c: &mut Canvas, t: &Theme, np: Rect) {
        ui::draw_card(c, t, np, false);
        let ix = np.x + 16;

        // Art row: tile + title/caption + stop.
        let tile = Rect::new(ix, np.y + 16, 72, 72);
        c.fill_rounded_rect(tile, 20.0, t.surface3);
        let (icon, icol) = if self.player.is_playing() {
            (IC_EQ, t.accent)
        } else if self.player.eof {
            (IC_MUSIC, t.text3)
        } else {
            (IC_PAUSE, t.text2)
        };
        draw_icon(c, icon, 34.0, icol, tile.x + 36, tile.y + 36);

        let title = self.player.display_title(&self.tracks);
        let folder = self
            .player
            .current
            .and_then(|i| self.tracks.get(i))
            .map(|tr| tr.folder.label())
            .unwrap_or("");
        let ext = self
            .player
            .current
            .and_then(|i| self.tracks.get(i))
            .map(|tr| tr.ext.to_uppercase())
            .unwrap_or_default();
        let state = if self.player.is_playing() {
            "Playing"
        } else if self.player.eof {
            "Finished"
        } else {
            "Paused"
        };
        let caption = [folder, ext.as_str(), state]
            .iter()
            .filter(|s| !s.is_empty())
            .copied()
            .collect::<Vec<_>>()
            .join(" · ");

        draw_line(
            c,
            title,
            &ui::styled(22.0, 600, t.text),
            ix + 88,
            np.y + 26,
            np.w - 88 - 16 - 52,
        );
        draw_line(
            c,
            &caption,
            &ui::caption(t.text3),
            ix + 88,
            np.y + 54,
            np.w - 88 - 16 - 52,
        );
        draw_circle_button(
            c,
            Self::stop_rect(np),
            IC_STOP,
            22.0,
            t.surface2,
            t.text,
            self.pressed == Some(Hit::Stop),
        );

        // Seek bar.
        let st = Self::seek_touch_rect(np);
        let track_r = Self::seek_track_rect(np);
        c.fill_rounded_rect(track_r, (SEEK_TRACK_H as f32) / 2.0, t.surface3);
        // Drag fraction may leave 0..=1 (rubber band); live progress is
        // always in range, so rubber_frac is the identity there.
        let raw_frac = self
            .seek_drag
            .map(|(_, r)| r)
            .unwrap_or_else(|| self.player.progress());
        let disp = rubber_frac(raw_frac);
        let dur_known = self.player.duration > 0.0;
        if dur_known || self.seek_drag.is_some() {
            let fill_w = (track_r.w as f32 * disp.clamp(0.0, 1.0)).round() as i32;
            if fill_w > 0 {
                c.fill_rounded_rect(
                    Rect::new(track_r.x, track_r.y, fill_w, track_r.h),
                    (SEEK_TRACK_H as f32) / 2.0,
                    t.accent,
                );
            }
            if dur_known {
                // Thumb may overshoot with the rubber band but stays on the track.
                let lo = (track_r.x + SEEK_THUMB) as f32;
                let hi = (track_r.x + track_r.w - SEEK_THUMB) as f32;
                let cx = (track_r.x as f32 + track_r.w as f32 * disp).clamp(lo, hi) as i32;
                let thumb = Rect::new(
                    cx - SEEK_THUMB,
                    track_r.y + track_r.h / 2 - SEEK_THUMB,
                    SEEK_THUMB * 2,
                    SEEK_THUMB * 2,
                );
                c.fill_rounded_rect(thumb, SEEK_THUMB as f32, t.accent);
            }
        }
        // Times.
        let times_y = st.y + st.h + 2;
        draw_line(
            c,
            &format_time(self.player.est_pos()),
            &ui::styled(15.0, 400, t.text3),
            ix,
            times_y,
            120,
        );
        let dur_s = if self.player.duration > 0.0 {
            format_time(self.player.duration)
        } else {
            "--:--".to_string()
        };
        // Right-align by measuring is overkill; draw at fixed right slot.
        draw_line(
            c,
            &dur_s,
            &ui::styled(15.0, 400, t.text3),
            np.x + np.w - 16 - 80,
            times_y,
            80,
        );

        // Transport.
        let [prev_r, play_r, next_r] = Self::transport_rects(np);
        draw_circle_button(
            c,
            prev_r,
            IC_PREV,
            26.0,
            t.surface2,
            t.text,
            self.pressed == Some(Hit::Prev),
        );
        draw_circle_button(
            c,
            play_r,
            if self.player.is_playing() {
                IC_PAUSE
            } else {
                IC_PLAY
            },
            34.0,
            t.accent,
            t.text_on_accent,
            self.pressed == Some(Hit::Toggle),
        );
        draw_circle_button(
            c,
            next_r,
            IC_NEXT,
            26.0,
            t.surface2,
            t.text,
            self.pressed == Some(Hit::Next),
        );
    }
}
