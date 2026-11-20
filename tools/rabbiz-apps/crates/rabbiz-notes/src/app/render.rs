use de_api::Pixels;
use de_paint::fonts::font_system;
use rabbiz_app_kit::text;
use rabbiz_app_kit::text::shape::Shaped;
use rabbiz_app_kit::{ui, Canvas, Point, Rect, Theme};

use crate::model::{format_edited, matching_indices, now_unix};

use super::{Focus, Geom, Hit, NotesApp, ROW_GAP, ROW_H, SEARCH_H};

// ---------------------------------------------------------------------------
// Drawing helpers
// ---------------------------------------------------------------------------

/// Blit `px` at (dx,dy) clipped to `clip` — Canvas has no scissor, so we
/// intersect and use `blit_region` for the visible slice.
fn blit_clipped(canvas: &mut Canvas, px: &Pixels, dx: i32, dy: i32, clip: Rect) {
    let dst = Rect::new(dx, dy, px.size.w, px.size.h);
    let Some(r) = dst.intersect(clip) else { return };
    let region = Rect::new(r.x - dx, r.y - dy, r.w, r.h);
    canvas.blit_region(px, region, Point::new(r.x, r.y));
}

/// Single-line clamped label, right-aligned at `right_x`, vertically centred
/// on `cy`, clipped to `clip`.
fn draw_text_right(
    canvas: &mut Canvas,
    s: &str,
    style: &text::LabelStyle,
    right_x: i32,
    cy: i32,
    max_w: i32,
    clip: Rect,
) {
    let Some(px) = text::label_clamped(s, *style, max_w.max(16), 1) else {
        return;
    };
    let pixels = text::cache::to_pixels(&px);
    let x = right_x - pixels.size.w;
    let y = cy - pixels.size.h / 2;
    blit_clipped(canvas, &pixels, x, y, clip);
}

/// Caret rectangle clipped to `clip`.
fn draw_caret(canvas: &mut Canvas, x: i32, y: i32, h: i32, color: [u8; 4], clip: Rect) {
    if let Some(r) = Rect::new(x, y, 3, h).intersect(clip) {
        canvas.fill_rect(r, color);
    }
}

/// Caret geometry for `line`/`index` (cosmic-text byte index inside the
/// line) → (x, line_top, line_height) in content coords.
pub(super) fn caret_geometry(sh: &Shaped, line: usize, index: usize) -> Option<(f32, f32, f32)> {
    let mut empty_fallback: Option<(f32, f32, f32)> = None;
    let mut tail: Option<(f32, f32, f32)> = None;
    for run in sh.buf.layout_runs() {
        if run.line_i != line {
            continue;
        }
        let (top, lh) = (run.line_top, run.line_height);
        let end_x = run.glyphs.last().map(|g| g.x + g.w).unwrap_or(0.0);
        // Fallback for `index` beyond every glyph (trailing whitespace or an
        // empty line): caret after the last glyph of the last visual line.
        tail = Some((end_x, top, lh));
        let Some(first) = run.glyphs.first() else {
            if empty_fallback.is_none() {
                empty_fallback = Some((0.0, top, lh));
            }
            continue;
        };
        let last = run.glyphs.last().unwrap();
        // The first visual line whose last glyph ends at/after `index` owns
        // the caret; glyph byte ranges are contiguous across wraps.
        if index > last.end {
            continue;
        }
        let x = if index <= first.start {
            first.x
        } else {
            let mut x = end_x;
            for g in run.glyphs {
                if index <= g.start {
                    x = g.x;
                    break;
                }
                if index < g.end {
                    let span = (g.end - g.start).max(1) as f32;
                    x = g.x + g.w * ((index - g.start) as f32 / span);
                    break;
                }
                if index == g.end {
                    x = g.x + g.w;
                    break;
                }
            }
            x
        };
        return Some((x, top, lh));
    }
    empty_fallback.or(tail)
}

/// First-line preview for a list row: body minus its title line, collapsed
/// to one clamped line's worth of text.
fn preview_text(body: &str) -> String {
    let mut lines = body.lines();
    let _title = lines.next();
    let rest = lines.collect::<Vec<_>>().join(" ");
    let collapsed = rest.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() > 140 {
        let mut s: String = collapsed.chars().take(140).collect();
        s.push('…');
        s
    } else {
        collapsed
    }
}

// ---------------------------------------------------------------------------
// List screen
// ---------------------------------------------------------------------------

impl NotesApp {
    pub(super) fn render_list(&mut self, canvas: &mut Canvas, theme: &Theme, g: Geom) {
        let w = canvas.size().w;
        ui::draw_header(canvas, theme, "Notes", "");

        // -- search field ----------------------------------------------------
        canvas.fill_rounded_rect(g.search, SEARCH_H as f32 / 2.0, theme.surface1);
        // Material "search" icon glyph.
        if let Some(px) = text::icon("\u{e8b6}", 22.0, theme.text3) {
            let p = text::cache::to_pixels(&px);
            let iy = g.search.y + (g.search.h - p.size.h) / 2;
            blit_clipped(canvas, &p, g.search.x + 16, iy, g.search);
        }
        let qstyle = ui::body(theme.text);
        let text_x = g.search.x + 52;
        let text_w = (g.search.w - 52 - 16).max(16);
        let clip = g.search;
        let cy = g.search.y + g.search.h / 2;
        if self.search.text.is_empty() {
            ui::draw_text(
                canvas,
                "Search notes",
                &ui::body(theme.text3),
                text_x,
                cy - 14,
                Some(text_w),
            );
        } else {
            if let Some(px) = text::label_clamped(&self.search.text, qstyle, text_w, 1) {
                let p = text::cache::to_pixels(&px);
                blit_clipped(canvas, &p, text_x, cy - p.size.h / 2, clip);
                if self.focus == Focus::Search {
                    draw_caret(
                        canvas,
                        text_x + p.size.w + 2,
                        cy - 12,
                        24,
                        theme.accent,
                        clip,
                    );
                }
            }
        }
        if self.focus == Focus::Search && self.search.text.is_empty() {
            draw_caret(canvas, text_x, cy - 12, 24, theme.accent, clip);
        }

        // -- note rows ---------------------------------------------------------
        let ids = matching_indices(&self.notes, &self.search.text);
        let vp = g.list_vp;
        let content_h = ids.len() as i32 * (ROW_H + ROW_GAP) + 16;
        let max_scroll = (content_h - vp.h).max(0);
        self.scroll = self.scroll.clamp(0, max_scroll);

        if self.notes.is_empty() {
            let r = Rect::new(0, vp.y + vp.h / 2 - 60, w, 140);
            ui::draw_centered(canvas, "No notes yet", &ui::title(theme.text2), r);
            ui::draw_centered(
                canvas,
                "Tap + to write your first note",
                &ui::body(theme.text3),
                Rect::new(0, r.y + 52, w, 40),
            );
        } else if ids.is_empty() {
            ui::draw_centered(
                canvas,
                &format!("No notes match \"{}\"", self.search.text.trim()),
                &ui::body(theme.text3),
                Rect::new(0, vp.y + vp.h / 2 - 20, w, 40),
            );
        }

        let title_style = ui::label(theme.text);
        let prev_style = ui::caption(theme.text2);
        let time_style = ui::caption(theme.text3);
        let now = now_unix();
        let row_h = ROW_H + ROW_GAP;

        for (vis_i, &note_i) in ids.iter().enumerate() {
            let note = &self.notes[note_i];
            let mut y = vp.y + vis_i as i32 * row_h - self.scroll;
            y += self.stagger_offset(vis_i);
            let row = Rect::new(vp.x, y, vp.w, ROW_H);
            let Some(vis) = row.intersect(vp) else {
                continue;
            };
            let pressed = self.pressed == Some(Hit::Row(note.id));
            ui::draw_card(canvas, theme, vis, pressed);

            let inner = Rect::new(row.x + 20, row.y, row.w - 40, row.h);
            let Some(tclip) = inner.intersect(vp) else {
                continue;
            };

            // Title (one line, ellipsized).
            if let Some(px) = text::label_clamped(&note.title, title_style, inner.w, 1) {
                let p = text::cache::to_pixels(&px);
                blit_clipped(canvas, &p, inner.x, inner.y + 14, tclip);
            }
            // Preview (one line).
            let prev = preview_text(&note.body);
            if !prev.is_empty() {
                if let Some(px) = text::label_clamped(&prev, prev_style, inner.w, 1) {
                    let p = text::cache::to_pixels(&px);
                    blit_clipped(canvas, &p, inner.x, inner.y + 48, tclip);
                }
            }
            // Edited time, right-aligned near the top.
            let when = format_edited(note.updated, now);
            draw_text_right(
                canvas,
                &when,
                &time_style,
                inner.x + inner.w,
                inner.y + 26,
                150,
                tclip,
            );
        }

        // -- FAB ---------------------------------------------------------------
        // Hidden while the search OSK is up: it would sit under the keyboard.
        if self.focus != Focus::Search {
            let fab = g.fab;
            let fab_bg = if self.pressed == Some(Hit::New) {
                theme.accent_dim
            } else {
                theme.accent
            };
            canvas.fill_rounded_rect(fab, (fab.h / 2) as f32, fab_bg);
            let plus = ui::styled(30.0, 600, theme.text_on_accent);
            ui::draw_centered(canvas, "+", &plus, fab);
        }
    }
}

// ---------------------------------------------------------------------------
// Editor screen
// ---------------------------------------------------------------------------

impl NotesApp {
    pub(super) fn render_editor(&mut self, canvas: &mut Canvas, theme: &Theme, g: Geom) {
        // Top bar.
        ui::draw_pill_button(
            canvas,
            theme,
            g.back,
            "‹ Notes",
            false,
            self.pressed == Some(Hit::Back),
        );
        // Delete pill: danger-filled when armed for the second tap.
        let del_bg = if self.delete_armed {
            theme.danger
        } else {
            theme.surface2
        };
        let del_fg = if self.delete_armed {
            theme.text_on_accent
        } else {
            theme.text2
        };
        let dr = if self.pressed == Some(Hit::Delete) {
            Rect::new(g.del.x + 2, g.del.y + 2, g.del.w - 4, g.del.h - 4)
        } else {
            g.del
        };
        canvas.fill_rounded_rect(dr, (dr.h / 2) as f32, del_bg);
        ui::draw_centered(canvas, "Delete", &ui::label(del_fg), dr);

        // Card + clipped text area.
        ui::draw_card(canvas, theme, g.editor_card, false);
        let area = g.text_area;

        self.ensure_shaped();
        // Clamp scroll and keep the caret visible (auto-follow on every
        // render — cheap and correct after taps, inserts and resizes).
        let max_scroll = self.edit_max_scroll();
        if let Some((_cx, ctop, ch)) = self.caret_pos() {
            let caret_bottom = ctop + ch;
            if ctop - 12.0 < self.edit_scroll as f32 {
                self.edit_scroll = (ctop as i32 - 12).max(0);
            }
            if caret_bottom + 12.0 > self.edit_scroll as f32 + area.h as f32 {
                self.edit_scroll = (caret_bottom as i32 + 12 - area.h).clamp(0, max_scroll);
            }
        }
        self.edit_scroll = self.edit_scroll.clamp(0, max_scroll);

        if self.edit.text.is_empty() {
            ui::draw_text(
                canvas,
                "Start typing…",
                &ui::body(theme.text3),
                area.x,
                area.y + 4,
                Some(area.w),
            );
        } else if let Some(sh) = &self.shaped {
            let style = self.body_style();
            if let Ok(mut fs) = font_system().lock() {
                if let Some(px) = sh.rasterize(&mut fs, style.color) {
                    let p = text::cache::to_pixels(&px);
                    blit_clipped(canvas, &p, area.x, area.y - self.edit_scroll, area);
                }
            }
            // Caret.
            if let Some((cx, ctop, ch)) = self.caret_pos() {
                let sx = area.x + cx.round() as i32 - 1;
                let sy = area.y + ctop.round() as i32 - self.edit_scroll;
                let sh_h = (ch * 0.72).round() as i32;
                draw_caret(canvas, sx, sy + 4, sh_h.max(8), theme.accent, area);
            }
        } else {
            // Shaping unavailable — unwrapped fallback, still clipped.
            for (ln, line) in self.edit.text.lines().take(40).enumerate() {
                ui::draw_text(
                    canvas,
                    line,
                    &ui::body(theme.text),
                    area.x,
                    area.y + ln as i32 * 32 - self.edit_scroll,
                    Some(area.w),
                );
            }
        }
    }
}
