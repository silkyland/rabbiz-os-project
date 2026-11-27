//! Canvas rendering for the Music GUI — Material You library screen
//! (TopAppBar + scope chips + flat track rows), status banners, the docked
//! mini-player, and the fullscreen player (art, slider seek, 5-way transport).

use de_api::Pixels;
use rabbiz_app_kit::widgets::{self, IconBtnStyle, TopAppBar};
use rabbiz_app_kit::{icons, text, ui, Canvas, Point, Rect, Size, Theme};

use crate::app::{rubber_frac, Hit, Layout, MusicApp, PlayerGeom, BANNER_H, ROW_H};
use crate::library::{format_time, Repeat, Scope};

// ---------------------------------------------------------------------------
// Clipped drawing helpers — Canvas has no scissor, so every label/icon that
// can cross the list viewport edge is rasterized then `blit_region`ed to its
// visible slice (the contacts/notes convention). This is what keeps a
// half-scrolled row from re-laying out and jumping.
// ---------------------------------------------------------------------------

/// Rasterize a Material icon into Pixels with its ink bounds.
fn icon_px(codepoint: &str, size: f32, color: [u8; 4]) -> Option<(Pixels, i32, i32)> {
    let px = text::icon(codepoint, size, color)?;
    let (x0, y0, x1, y1) =
        text::ink_bounds(&px).unwrap_or((0, 0, px.width() as i32, px.height() as i32));
    Some((
        Pixels::new(
            Size::new(px.width() as i32, px.height() as i32),
            px.data().to_vec(),
        ),
        (x0 + x1) / 2,
        (y0 + y1) / 2,
    ))
}

/// Blit `px` at (dx,dy) clipped to `clip` via `blit_region`.
fn blit_clipped(c: &mut Canvas, px: &Pixels, dx: i32, dy: i32, clip: Rect) {
    let dst = Rect::new(dx, dy, px.size.w, px.size.h);
    let Some(r) = dst.intersect(clip) else { return };
    let region = Rect::new(r.x - dx, r.y - dy, r.w, r.h);
    c.blit_region(px, region, Point::new(r.x, r.y));
}

/// Material icon with ink bounds centered on (cx, cy), clipped to `clip`.
fn draw_icon_clipped(
    c: &mut Canvas,
    codepoint: &str,
    size: f32,
    color: [u8; 4],
    cx: i32,
    cy: i32,
    clip: Rect,
) {
    if let Some((px, ink_x, ink_y)) = icon_px(codepoint, size, color) {
        blit_clipped(c, &px, cx - ink_x, cy - ink_y, clip);
    }
}

/// Same, unclipped (canvas edge is the only bound).
fn draw_icon(c: &mut Canvas, codepoint: &str, size: f32, color: [u8; 4], cx: i32, cy: i32) {
    let full = Rect::from_origin_size(Point::ZERO, c.size());
    draw_icon_clipped(c, codepoint, size, color, cx, cy, full);
}

/// Single-line ellipsized label at (x, y), clipped.
fn draw_line_clipped(
    c: &mut Canvas,
    s: &str,
    st: &text::LabelStyle,
    x: i32,
    y: i32,
    max_w: i32,
    clip: Rect,
) {
    if s.is_empty() {
        return;
    }
    if let Some(px) = text::label_clamped(s, *st, max_w.max(8), 1) {
        let p = Pixels::new(
            Size::new(px.width() as i32, px.height() as i32),
            px.data().to_vec(),
        );
        blit_clipped(c, &p, x, y, clip);
    }
}

/// Single-line label right-aligned at `right_x`, vertically centered on
/// `cy`, clipped.
fn draw_line_right_clipped(
    c: &mut Canvas,
    s: &str,
    st: &text::LabelStyle,
    right_x: i32,
    cy: i32,
    max_w: i32,
    clip: Rect,
) {
    if s.is_empty() {
        return;
    }
    if let Some(px) = text::label_clamped(s, *st, max_w.max(8), 1) {
        let w = px.width() as i32;
        let h = px.height() as i32;
        let p = Pixels::new(Size::new(w, h), px.data().to_vec());
        blit_clipped(c, &p, right_x - w, cy - h / 2, clip);
    }
}

impl MusicApp {
    // ---------------- library chrome ---------------------------------------

    pub(crate) fn draw_bar(&self, c: &mut Canvas, t: &Theme, l: &Layout) {
        TopAppBar::render(c, l.bar, t, "Music", false, Some(icons::REFRESH));
        if self.pressed == Some(Hit::Rescan) {
            ui::press_overlay(c, t, l.rescan, l.rescan.h as f32 / 2.0, 0.12);
        }
    }

    /// Status banners stacked from `top`, BANNER_H tall, BANNER_GAP apart —
    /// exactly the space `layout()` reserves.
    pub(crate) fn draw_banners(&self, c: &mut Canvas, t: &Theme, top: i32) {
        let w = c.size().w;
        let mut y = top;
        if !self.player.mpv_ok {
            let r = Rect::new(ui::MARGIN, y, w - ui::MARGIN * 2, BANNER_H);
            ui::draw_card(c, t, r, false);
            draw_icon(c, icons::WARNING, 22.0, t.danger, r.x + 36, r.y + r.h / 2);
            draw_line_clipped(
                c,
                "mpv isn't installed, so nothing can play yet.",
                &ui::styled(17.0, 600, t.danger),
                r.x + 64,
                r.y + 10,
                r.w - 84,
                r,
            );
            draw_line_clipped(
                c,
                "Install it (pacman -S mpv), then tap the refresh button.",
                &ui::caption(t.text3),
                r.x + 64,
                r.y + 34,
                r.w - 84,
                r,
            );
            y += BANNER_H + 8;
        }
        if let Some(err) = &self.player.error {
            let r = Rect::new(ui::MARGIN, y, w - ui::MARGIN * 2, BANNER_H);
            ui::draw_card(c, t, r, false);
            draw_icon(c, icons::WARNING, 22.0, t.danger, r.x + 36, r.y + r.h / 2);
            draw_line_clipped(
                c,
                err,
                &ui::styled(17.0, 400, t.danger),
                r.x + 64,
                r.y + r.h / 2 - 11,
                r.w - 84,
                r,
            );
        }
    }

    /// Scope chips — kit `chips_row` measures content width; the returned
    /// rects are stored for hit-testing so taps match pixels exactly.
    pub(crate) fn draw_chips(&mut self, c: &mut Canvas, t: &Theme, l: &Layout) {
        self.chip_rects.clear();
        if self.tracks.is_empty() {
            return;
        }
        let items: Vec<(String, bool)> = Scope::ALL
            .iter()
            .map(|s| {
                let count = match s.folder() {
                    None => self.tracks.len(),
                    Some(f) => crate::library::count_folder(&self.tracks, f),
                };
                (format!("{} · {count}", s.label()), self.scope == *s)
            })
            .collect();
        let pressed = Scope::ALL
            .iter()
            .position(|s| self.pressed == Some(Hit::Chip(*s)));
        let refs: Vec<(&str, bool)> = items.iter().map(|(s, b)| (s.as_str(), *b)).collect();
        self.chip_rects = widgets::chips_row(c, t, ui::MARGIN, l.chips_y, &refs, 12, pressed);
    }

    pub(crate) fn draw_section_label(&self, c: &mut Canvas, t: &Theme, l: &Layout) {
        if self.tracks.is_empty() {
            return;
        }
        let full = Rect::from_origin_size(Point::ZERO, c.size());
        draw_line_clipped(
            c,
            "Library",
            &ui::styled(16.0, 600, t.text3),
            ui::MARGIN,
            l.label_y,
            200,
            full,
        );
        let n = self.visible().len();
        let right = format!("{n} track{} · newest first", if n == 1 { "" } else { "s" });
        draw_line_right_clipped(
            c,
            &right,
            &ui::caption(t.text3),
            c.size().w - ui::MARGIN,
            l.label_y + 11,
            320,
            full,
        );
    }

    // ---------------- track list -------------------------------------------

    /// Flat M3 rows: 56 px art tile, title + caption text, right-aligned
    /// format tag, inset hairline divider. Rows draw at their full content
    /// rect — all ink is clipped to the viewport, so a partially visible row
    /// never re-lays out (the old clamped-rect rows jumped while scrolling).
    pub(crate) fn draw_list(&self, c: &mut Canvas, t: &Theme, l: &Layout) {
        let vis = self.visible();
        if vis.is_empty() {
            return;
        }
        let vp = l.list;
        let scroll = self.list_sv.offset_i32();
        for (vi, &ti) in vis.iter().enumerate() {
            let row = Rect::new(vp.x, vp.y + vi as i32 * ROW_H - scroll, vp.w, ROW_H);
            let Some(vis_r) = row.intersect(vp) else {
                continue;
            };
            let track = &self.tracks[ti];
            let is_current = self.player.current == Some(ti);
            if self.pressed == Some(Hit::Row(ti)) {
                c.fill_rect(vis_r, ui::state_layer(t, 0.10));
            }

            // Art tile: format glyph stands in for artwork (MVP — no art).
            let tile = Rect::new(row.x + ui::MARGIN, row.y + (ROW_H - 56) / 2, 56, 56);
            if let Some(tv) = tile.intersect(vis_r) {
                c.fill_rounded_rect(tv, 12.0, t.surface3);
            }
            let (icon, icol) = if is_current && self.player.is_playing() {
                (icons::MUSIC_NOTE, t.accent)
            } else {
                (icons::MUSIC_NOTE, t.text3)
            };
            draw_icon_clipped(c, icon, 26.0, icol, tile.x + 28, tile.y + 28, vis_r);

            // Text column; the right slot reserves the format tag width.
            let tx = tile.right() + 16;
            let text_right = row.right() - ui::MARGIN - 72;
            let title_col = if is_current { t.accent } else { t.text };
            draw_line_clipped(
                c,
                &track.title,
                &ui::styled(20.0, 600, title_col),
                tx,
                row.y + 18,
                text_right - tx,
                vis_r,
            );
            draw_line_clipped(
                c,
                &track.caption(),
                &ui::caption(t.text3),
                tx,
                row.y + 46,
                text_right - tx,
                vis_r,
            );
            draw_line_right_clipped(
                c,
                &track.ext.to_uppercase(),
                &ui::styled(14.0, 600, t.text3),
                row.right() - ui::MARGIN,
                row.y + ROW_H / 2,
                72,
                vis_r,
            );

            // Inset hairline — only while its line is inside the viewport.
            let dy = row.bottom() - 1;
            if dy < vp.bottom() && vi + 1 < vis.len() {
                ui::divider_inset(c, t, tx, row.right() - 1, dy);
            }
        }

        ui::scrollbar(
            c,
            t,
            Rect::new(vp.right() - 6, vp.y, 6, vp.h),
            scroll,
            vis.len() as i32 * ROW_H,
        );
    }

    pub(crate) fn draw_empty(&self, c: &mut Canvas, t: &Theme, l: &Layout) {
        let Some(btn) = l.empty_btn else { return };
        let area = Rect::new(0, l.list.y, c.size().w, (l.list.h / 2) + 40);
        if self.tracks.is_empty() {
            widgets::EmptyState::render(
                c,
                area,
                t,
                icons::MUSIC_NOTE,
                "No audio files yet",
                "Put audio in ~/Music or ~/Downloads, then tap Scan again.",
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
            widgets::EmptyState::render(
                c,
                area,
                t,
                icons::QUEUE_MUSIC,
                &format!("Nothing in {}", self.scope.label()),
                "Tap All to see every track, or add audio to this folder.",
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

    // ---------------- mini-player ------------------------------------------

    /// Docked mini-player: 3 px accent progress hairline on its top edge,
    /// 56 px art tile, two text lines, next + play/pause. Body taps expand
    /// to the fullscreen player.
    pub(crate) fn draw_mini(&self, c: &mut Canvas, t: &Theme, np: Rect) {
        c.fill_rect(np, t.surface1);
        c.fill_rect(Rect::new(np.x, np.y, np.w, 1), t.line);
        let frac = self.player.progress();
        if frac > 0.0 {
            let w = (np.w as f32 * frac).round().max(4.0) as i32;
            c.fill_rect(Rect::new(np.x, np.y, w.min(np.w), 3), t.accent);
        }

        let tile = Self::mini_tile_rect(np);
        c.fill_rounded_rect(tile, 12.0, t.surface3);
        let (icon, icol) = if self.player.is_playing() {
            (icons::MUSIC_NOTE, t.accent)
        } else {
            (icons::MUSIC_NOTE, t.text3)
        };
        draw_icon(c, icon, 26.0, icol, tile.x + 28, tile.y + 28);

        let txt = Self::mini_text_rect(np);
        let title = self.player.display_title(&self.tracks);
        let folder = self
            .player
            .current
            .and_then(|i| self.tracks.get(i))
            .map(|tr| tr.folder.label())
            .unwrap_or("");
        let state = if self.player.is_playing() {
            "Playing"
        } else if self.player.eof {
            "Finished"
        } else {
            "Paused"
        };
        let caption = [folder, state]
            .iter()
            .filter(|s| !s.is_empty())
            .copied()
            .collect::<Vec<_>>()
            .join(" · ");
        draw_line_clipped(
            c,
            title,
            &ui::styled(20.0, 600, t.text),
            txt.x,
            np.y + 26,
            txt.w,
            np,
        );
        draw_line_clipped(
            c,
            &caption,
            &ui::caption(t.text3),
            txt.x,
            np.y + 54,
            txt.w,
            np,
        );

        widgets::icon_btn(
            c,
            t,
            Self::mini_next_rect(np),
            icons::SKIP_NEXT,
            IconBtnStyle::Plain,
            false,
            self.pressed == Some(Hit::MiniNext),
        );
        widgets::icon_btn(
            c,
            t,
            Self::mini_play_rect(np),
            if self.player.is_playing() {
                icons::PAUSE
            } else {
                icons::PLAY
            },
            IconBtnStyle::Tonal,
            false,
            self.pressed == Some(Hit::MiniToggle),
        );
    }

    // ---------------- fullscreen player ------------------------------------

    /// Material You player: down-chevron bar, big rounded art with a soft
    /// shadow, title + caption, `widgets::slider` seek with rubber-band drag,
    /// elapsed/remaining captions, and a 5-way transport (shuffle, prev,
    /// accent hero play/pause, next, repeat).
    pub(crate) fn draw_player(&self, c: &mut Canvas, t: &Theme, w: i32, h: i32) {
        c.fill_rect(Rect::new(0, 0, w, h), t.bg);
        let g: PlayerGeom = self.player_geom(w, h);

        widgets::icon_btn(
            c,
            t,
            g.collapse,
            icons::EXPAND_MORE,
            IconBtnStyle::Plain,
            false,
            self.pressed == Some(Hit::CollapsePlayer),
        );
        ui::draw_centered(
            c,
            "Now Playing",
            &ui::label(t.text2),
            Rect::new(g.bar.x + 140, g.bar.y, (g.bar.w - 280).max(0), g.bar.h),
        );

        // Art tile — shadow drop, then the placeholder tile + glyph.
        c.fill_rounded_rect(g.art.translate(0, 8), 24.0, t.shadow);
        c.fill_rounded_rect(g.art, 24.0, t.surface3);
        draw_icon(
            c,
            icons::MUSIC_NOTE,
            g.art.w as f32 * 0.28,
            ui::with_alpha(t.text3, 0x99),
            g.art.x + g.art.w / 2,
            g.art.y + g.art.h / 2,
        );

        let title = self.player.display_title(&self.tracks);
        let title = if title.is_empty() {
            "Nothing playing"
        } else {
            title
        };
        ui::draw_centered(c, title, &ui::styled(30.0, 600, t.text), g.title);
        let folder = self
            .player
            .current
            .and_then(|i| self.tracks.get(i))
            .map(|tr| tr.caption())
            .unwrap_or_default();
        let ext = self
            .player
            .current
            .and_then(|i| self.tracks.get(i))
            .map(|tr| tr.ext.to_uppercase())
            .unwrap_or_default();
        let caption = [folder.as_str(), ext.as_str()]
            .iter()
            .filter(|s| !s.is_empty())
            .copied()
            .collect::<Vec<_>>()
            .join(" · ");
        ui::draw_centered(c, &caption, &ui::body(t.text2), g.subtitle);

        // Seek — kit slider look; the drag math (rubber band + commit on
        // release) is unchanged.
        let raw_frac = self
            .seek_drag
            .map(|(_, r)| r)
            .unwrap_or_else(|| self.player.progress());
        let disp = rubber_frac(raw_frac);
        widgets::slider(c, t, g.seek_lane, disp, self.seek_drag.is_some());
        draw_line_clipped(
            c,
            &format_time(self.player.est_pos()),
            &ui::styled(15.0, 400, t.text3),
            g.times.x,
            g.times.y + 3,
            140,
            g.times,
        );
        let dur_s = if self.player.duration > 0.0 {
            format_time(self.player.duration)
        } else {
            "--:--".to_string()
        };
        draw_line_right_clipped(
            c,
            &dur_s,
            &ui::styled(15.0, 400, t.text3),
            g.times.right(),
            g.times.y + g.times.h / 2,
            140,
            g.times,
        );

        // Transport: shuffle / prev / hero / next / repeat.
        let [sh_r, prev_r, hero_r, next_r, rep_r] = g.transport;
        widgets::icon_btn(
            c,
            t,
            sh_r,
            icons::SHUFFLE,
            IconBtnStyle::Plain,
            self.player.shuffle,
            self.pressed == Some(Hit::Shuffle),
        );
        widgets::icon_btn(
            c,
            t,
            prev_r,
            icons::SKIP_PREV,
            IconBtnStyle::Tonal,
            false,
            self.pressed == Some(Hit::Prev),
        );
        // Hero play/pause — 96 px accent circle.
        c.fill_rounded_rect(hero_r, hero_r.h as f32 / 2.0, t.accent);
        if self.pressed == Some(Hit::Toggle) {
            c.fill_rounded_rect(
                hero_r,
                hero_r.h as f32 / 2.0,
                ui::with_alpha(t.text_on_accent, 0x26),
            );
        }
        draw_icon(
            c,
            if self.player.is_playing() {
                icons::PAUSE
            } else {
                icons::PLAY
            },
            44.0,
            t.text_on_accent,
            hero_r.x + hero_r.w / 2,
            hero_r.y + hero_r.h / 2,
        );
        widgets::icon_btn(
            c,
            t,
            next_r,
            icons::SKIP_NEXT,
            IconBtnStyle::Tonal,
            false,
            self.pressed == Some(Hit::Next),
        );
        widgets::icon_btn(
            c,
            t,
            rep_r,
            if self.player.repeat == Repeat::One {
                icons::REPEAT_ONE
            } else {
                icons::REPEAT
            },
            IconBtnStyle::Plain,
            self.player.repeat != Repeat::Off,
            self.pressed == Some(Hit::RepeatBtn),
        );
    }
}
