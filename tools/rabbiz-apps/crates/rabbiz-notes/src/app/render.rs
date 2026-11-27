use de_api::Pixels;
use de_paint::fonts::font_system;
use rabbiz_app_kit::text;
use rabbiz_app_kit::text::shape::Shaped;
use rabbiz_app_kit::text::LabelStyle;
use rabbiz_app_kit::widgets::{self, SearchPill, TopAppBar};
use rabbiz_app_kit::{icons, ui, Canvas, Point, Rect, Theme};

use crate::model::{format_edited, matching_indices, now_unix};

use super::{
    Focus, Geom, Hit, NotesApp, BODY_LH, BODY_SIZE, ROW_GAP, ROW_H, TITLE_BODY_GAP, TITLE_LH,
    TITLE_SIZE,
};

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

/// Single-line ellipsized label at (x, y), clipped.
fn draw_line(canvas: &mut Canvas, s: &str, st: &LabelStyle, x: i32, y: i32, max_w: i32, clip: Rect) {
    if s.is_empty() {
        return;
    }
    if let Some(px) = text::label_clamped(s, *st, max_w.max(8), 1) {
        let p = text::cache::to_pixels(&px);
        blit_clipped(canvas, &p, x, y, clip);
    }
}

/// Material icon with ink bounds centered on (cx, cy), clipped.
fn draw_icon_clipped(
    canvas: &mut Canvas,
    codepoint: &str,
    size: f32,
    color: [u8; 4],
    cx: i32,
    cy: i32,
    clip: Rect,
) {
    if let Some(px) = text::icon(codepoint, size, color) {
        let p = text::cache::to_pixels(&px);
        let (x0, y0, x1, y1) = text::ink_bounds(&px).unwrap_or((0, 0, p.size.w, p.size.h));
        blit_clipped(canvas, &p, cx - (x0 + x1) / 2, cy - (y0 + y1) / 2, clip);
    }
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

/// Preview body for a list row: body minus its title line, whitespace-
/// collapsed, char-safe truncated for the two clamped lines it renders into.
fn preview_text(body: &str) -> String {
    let mut lines = body.lines();
    let _title = lines.next();
    let rest = lines.collect::<Vec<_>>().join(" ");
    let collapsed = rest.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() > 240 {
        let mut s: String = collapsed.chars().take(240).collect();
        s.push('…');
        s
    } else {
        collapsed
    }
}

fn title_ph_style(t: &Theme) -> LabelStyle {
    let mut st = ui::styled(TITLE_SIZE, 600, t.text3);
    st.line_height = TITLE_LH;
    st
}

fn body_ph_style(t: &Theme) -> LabelStyle {
    let mut st = ui::styled(BODY_SIZE, 400, t.text3);
    st.line_height = BODY_LH;
    st
}

/// Rasterize a `Shaped` block and blit it clipped (title/body editor text).
fn blit_shaped(canvas: &mut Canvas, sh: &Shaped, style: &LabelStyle, x: i32, y: i32, clip: Rect) {
    if let Ok(mut fs) = font_system().lock() {
        if let Some(px) = sh.rasterize(&mut fs, style.color) {
            let p = text::cache::to_pixels(&px);
            blit_clipped(canvas, &p, x, y, clip);
        }
    }
}

// ---------------------------------------------------------------------------
// List screen: searchable notes with a preview and edited time.
// ---------------------------------------------------------------------------

impl NotesApp {
    pub(super) fn render_list(&mut self, canvas: &mut Canvas, theme: &Theme, g: Geom) {
        ui::draw_header(canvas, theme, "Notes", "");

        // -- search pill (kit: focus ring + clear-×) --------------------------
        SearchPill::render(
            canvas,
            g.search,
            theme,
            &self.search.text,
            "Search notes",
            self.focus == Focus::Search,
        );
        if self.focus == Focus::Search {
            // Caret AT the cursor — arrows/Home/End move it mid-text.
            let style = ui::body(theme.text);
            let cur = self.search.cursor.min(self.search.text.len());
            let prefix_w = text::label(&self.search.text[..cur], style, None)
                .map(|px| px.width() as i32)
                .unwrap_or(0);
            let cy = g.search.y + g.search.h / 2;
            draw_caret(
                canvas,
                g.search.x + 48 + prefix_w + 2,
                cy - 12,
                24,
                theme.accent,
                g.search,
            );
        }

        // -- note rows ---------------------------------------------------------
        let ids = matching_indices(&self.notes, &self.search.text);
        let vp = g.list_vp;
        let bottom_pad = if self.focus == Focus::Search {
            16
        } else {
            (vp.bottom() - g.fab.y).max(0) + 16
        };
        let content_h = ids.len() as i32 * (ROW_H + ROW_GAP) + bottom_pad;
        self.list_sv.set_bounds(content_h as f32, vp.h as f32);
        let scroll = self.list_sv.offset_i32();

        if self.notes.is_empty() {
            widgets::EmptyState::render(
                canvas,
                vp,
                theme,
                icons::DESCRIPTION,
                "No notes yet",
                "Tap + to write your first note",
            );
        } else if ids.is_empty() {
            widgets::EmptyState::render(
                canvas,
                vp,
                theme,
                icons::SEARCH,
                &format!("No notes match \"{}\"", self.search.text.trim()),
                "",
            );
        }

        let title_style = ui::styled(24.0, 600, theme.text);
        let prev_style = ui::body(theme.text2);
        let time_style = ui::caption(theme.text2);
        let now = now_unix();
        let row_h = ROW_H + ROW_GAP;

        for (vis_i, &note_i) in ids.iter().enumerate() {
            let note = &self.notes[note_i];
            let mut y = vp.y + vis_i as i32 * row_h - scroll;
            y += self.stagger_offset(vis_i);
            let row = Rect::new(vp.x, y, vp.w, ROW_H);
            let Some(vis) = row.intersect(vp) else {
                continue;
            };
            let pressed = self.pressed == Some(Hit::Row(note.id));
            if pressed {
                ui::press_overlay(canvas, theme, vis, 0.0, 0.08);
            }

            let inner = Rect::new(row.x + 12, row.y, row.w - 24, row.h);
            let Some(tclip) = inner.intersect(vp) else {
                continue;
            };

            let when = format_edited(note.updated, now);
            let time_w = text::label(&when, time_style, None)
                .map_or(0, |px| px.width() as i32)
                .min(inner.w / 3);
            let time_x = inner.right() - time_w;
            draw_line(canvas, &when, &time_style, time_x, inner.y + 17, time_w, tclip);
            // The timestamp stays in the title row, leaving two lines for content.
            draw_line(
                canvas,
                &note.title,
                &title_style,
                inner.x,
                inner.y + 13,
                time_x - inner.x - 16,
                tclip,
            );
            // Preview (two lines, ellipsized).
            let prev = preview_text(&note.body);
            if !prev.is_empty() {
                if let Some(px) = text::label_clamped(&prev, prev_style, inner.w, 2) {
                    let p = text::cache::to_pixels(&px);
                    blit_clipped(canvas, &p, inner.x, inner.y + 42, tclip);
                }
            }
            if vis_i + 1 < ids.len() && (vp.y..vp.bottom()).contains(&(row.bottom() - 1)) {
                ui::divider_inset(canvas, theme, inner.x, inner.right(), row.bottom() - 1);
            }
        }

        ui::scrollbar(
            canvas,
            theme,
            Rect::new(vp.right() - 5, vp.y, 5, vp.h),
            scroll,
            content_h,
        );

        // -- FAB (hidden while the search OSK is up: it would sit under it) ---
        if self.focus != Focus::Search {
            widgets::fab(
                canvas,
                theme,
                g.fab,
                icons::ADD,
                self.pressed == Some(Hit::New),
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Editor screen — Keep-style chromeless: icon bar + borderless fields.
// ---------------------------------------------------------------------------

impl NotesApp {
    pub(super) fn render_editor(&mut self, canvas: &mut Canvas, theme: &Theme, g: Geom) {
        // Top bar: back + delete icons (kit bar draws bg + hairline + glyphs).
        TopAppBar::render(canvas, g.bar, theme, "", true, Some(icons::DELETE));
        if self.pressed == Some(Hit::Back) {
            ui::press_overlay(canvas, theme, g.back, g.back.h as f32 / 2.0, 0.10);
        }
        if self.pressed == Some(Hit::Delete) {
            ui::press_overlay(canvas, theme, g.del, g.del.h as f32 / 2.0, 0.10);
        }
        // Armed delete (first tap): danger circle behind the glyph.
        if self.delete_armed {
            let r = g.del.inset(2);
            canvas.fill_rounded_rect(r, r.h as f32 / 2.0, theme.danger);
            let c = r.center();
            draw_icon_clipped(canvas, icons::DELETE, 24.0, theme.text_on_accent, c.x, c.y, r);
        }

        // Borderless fields on the surface — title block + body block,
        // shared scroll + caret-follows-scroll (contract unchanged).
        let area = g.text_area;
        self.ensure_shaped();
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

        let (title_txt, body_txt) = Self::title_body_split(&self.edit.text);
        let th = self.title_block_h();
        let scroll = self.edit_scroll;

        // Title block — TITLE size, "Title" placeholder when empty.
        let title_style = self.title_style();
        if title_txt.is_empty() {
            draw_line(
                canvas,
                "Title",
                &title_ph_style(theme),
                area.x,
                area.y + 4 - scroll,
                area.w,
                area,
            );
        } else if let Some(sh) = &self.title_shaped {
            blit_shaped(canvas, sh, &title_style, area.x, area.y - scroll, area);
        } else if let Some(px) = text::label(title_txt, title_style, Some(area.w)) {
            let p = text::cache::to_pixels(&px);
            blit_clipped(canvas, &p, area.x, area.y - scroll, area);
        }

        // Body block — body size, "Note" placeholder when empty.
        let body_y = area.y + th + TITLE_BODY_GAP - scroll;
        let body_style = self.body_style();
        if body_txt.is_empty() {
            draw_line(
                canvas,
                "Note",
                &body_ph_style(theme),
                area.x,
                body_y + 4,
                area.w,
                area,
            );
        } else if let Some(sh) = &self.body_shaped {
            blit_shaped(canvas, sh, &body_style, area.x, body_y, area);
        } else if let Some(px) = text::label(body_txt, body_style, Some(area.w)) {
            let p = text::cache::to_pixels(&px);
            blit_clipped(canvas, &p, area.x, body_y, area);
        }

        // Caret — title block caret is taller, matching the larger type.
        if let Some((cx, ctop, ch)) = self.caret_pos() {
            let sx = area.x + cx.round() as i32 - 1;
            let sy = area.y + ctop.round() as i32 - scroll;
            let sh_h = (ch * 0.72).round() as i32;
            draw_caret(canvas, sx, sy + 4, sh_h.max(8), theme.accent, area);
        }
    }
}
