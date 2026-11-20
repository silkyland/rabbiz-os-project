//! Secondary screen renderers: file picker, page overview grid, and error screen.

use de_api::Rect;
use rabbiz_app_kit::ui;
use rabbiz_app_kit::{Canvas, Theme};

use crate::{doc, view};

use super::{chrome, overview_view, PdfApp, LIST_TOP, ROW_H};

impl PdfApp {
    // ---------------- picker ----------------

    pub(super) fn render_picker(&mut self, canvas: &mut Canvas, theme: &Theme) {
        let w = canvas.size().w;
        let h = canvas.size().h;
        let after = ui::draw_header(
            canvas,
            theme,
            "PDF",
            &format!(
                "{} document{}",
                self.files.len(),
                if self.files.len() == 1 { "" } else { "s" }
            ),
        );
        let list_top = after + 8;
        debug_assert_eq!(list_top, LIST_TOP);
        self.picker_scroll = view::clamp_scroll(self.picker_scroll, self.picker_max_scroll());

        if self.files.is_empty() {
            ui::draw_centered(
                canvas,
                "No PDFs found",
                &ui::body(theme.text3),
                Rect::new(0, h / 2 - 40, w, 80),
            );
            ui::draw_centered(
                canvas,
                "Drop a .pdf into ~/Documents or ~/Downloads,\nor open one from a file manager",
                &ui::caption(theme.text3),
                Rect::new(0, h / 2 + 40, w, 64),
            );
            return;
        }

        for (i, f) in self.files.iter().enumerate() {
            let sy = list_top + i as i32 * ROW_H - self.picker_scroll as i32;
            if sy + ROW_H < list_top || sy > h {
                continue;
            }
            let r = Rect::new(ui::MARGIN, sy, w - ui::MARGIN * 2, ROW_H - 12);
            ui::draw_card(canvas, theme, r, self.pressed_file == Some(i));
            let name = ui::body(theme.text);
            ui::draw_text(canvas, &f.name, &name, r.x + 20, r.y + 16, Some(r.w - 40));
            let sub = format!(
                "{}  ·  {}",
                f.path
                    .parent()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default(),
                doc::format_bytes(f.bytes)
            );
            ui::draw_text(
                canvas,
                &sub,
                &ui::caption(theme.text3),
                r.x + 20,
                r.y + 48,
                Some(r.w - 40),
            );
        }
    }

    // ---------------- overview ----------------

    pub(super) fn render_overview(&mut self, canvas: &mut Canvas, theme: &Theme) {
        let w = canvas.size().w;
        let h = canvas.size().h;
        let ch = chrome(w, h);
        let view = overview_view(w, h);
        self.ov_scroll = view::clamp_scroll(self.ov_scroll, self.ov_max_scroll());

        let back_r = Rect::new(ch.back.x, ch.back.y, ch.back.w, ch.back.h);
        ui::draw_pill_button(canvas, theme, back_r, "‹ Back", false, self.back_pressed);
        // Compact title beside the back button (no draw_header — it would
        // collide with the pill).
        ui::draw_text(
            canvas,
            "Pages",
            &ui::title(theme.text),
            back_r.right() + 16,
            back_r.y + 2,
            Some((ch.grid_btn.x - back_r.right() - 32).max(60)),
        );
        ui::draw_text(
            canvas,
            &format!("{} pages", self.page_count()),
            &ui::caption(theme.text3),
            back_r.right() + 16,
            back_r.y + 34,
            Some((ch.grid_btn.x - back_r.right() - 32).max(60)),
        );

        if self.page_count() == 0 {
            ui::draw_centered(
                canvas,
                "No pages",
                &ui::body(theme.text3),
                Rect::new(0, h / 2 - 40, w, 80),
            );
            return;
        }

        // Queue thumbs for visible cells (same metrics as view::grid_cell).
        let cols = view::GRID_COLS.min(view.w / 80).max(1);
        let cell_w = (view.w - view::GRID_GAP * (cols - 1)) / cols;
        let row_h = cell_w * 4 / 3 + view::GRID_GAP;
        let first_row = (self.ov_scroll as i32 / row_h.max(1))
            .saturating_sub(1)
            .max(0) as usize;
        let last_row = ((self.ov_scroll as i32 + view.h) / row_h.max(1) + 2) as usize;
        let first = first_row * cols as usize;
        let last = ((last_row * cols as usize) + cols as usize).min(self.page_count());
        for i in first..last {
            self.request_thumb(i);
        }

        for i in 0..self.page_count() {
            let Some(cell) = view::grid_cell(view, i, view::GRID_COLS, self.ov_scroll) else {
                continue;
            };
            match self.thumbs.get(&i) {
                Some(Some(px)) => {
                    let inner = view::contain_rect(cell, px.size.w, px.size.h);
                    canvas.blit_scaled(px, inner, 1.0);
                }
                Some(None) => {
                    canvas.fill_rect(cell, theme.surface1);
                    ui::draw_centered(
                        canvas,
                        &format!("{}", i + 1),
                        &ui::caption(theme.text3),
                        cell,
                    );
                }
                None => {
                    canvas.fill_rect(cell, theme.surface1);
                    canvas.fill_rect(
                        Rect::new(cell.x, cell.y, cell.w, cell.h / 5),
                        theme.surface2,
                    );
                }
            }
            // Page number badge.
            ui::draw_text(
                canvas,
                &format!("{}", i + 1),
                &ui::caption(theme.text3),
                cell.x + 8,
                cell.y + cell.h - 26,
                Some(cell.w - 16),
            );
            if i == self.page {
                // Accent ring on the current page.
                canvas.fill_rect(
                    Rect::new(cell.x - 2, cell.y - 2, cell.w + 4, 3),
                    theme.accent,
                );
            }
        }
    }

    // ---------------- error ----------------

    pub(super) fn render_error(&mut self, canvas: &mut Canvas, theme: &Theme) {
        let w = canvas.size().w;
        let h = canvas.size().h;
        let ch = chrome(w, h);
        let back_r = Rect::new(ch.back.x, ch.back.y, ch.back.w, ch.back.h);
        ui::draw_pill_button(canvas, theme, back_r, "‹ Files", false, self.back_pressed);
        let cy = h / 2 - 40;
        ui::draw_centered(
            canvas,
            "Can't open PDF",
            &ui::title(theme.text),
            Rect::new(0, cy, w, 60),
        );
        ui::draw_centered(
            canvas,
            &self.error,
            &ui::body(theme.text2),
            Rect::new(ui::MARGIN * 2, cy + 70, w - ui::MARGIN * 4, 80),
        );
        ui::draw_centered(
            canvas,
            "The file may be corrupt or password-protected.",
            &ui::caption(theme.text3),
            Rect::new(0, cy + 160, w, 40),
        );
    }
}
