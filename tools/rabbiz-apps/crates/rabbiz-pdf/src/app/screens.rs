//! Secondary screen renderers: file picker, page overview grid, and error screen.

use de_api::Rect;
use rabbiz_app_kit::ui;
use rabbiz_app_kit::widgets::{self, IconBtnStyle};
use rabbiz_app_kit::{icons, Canvas, Theme};

use crate::{doc, view};

use super::{chrome, overview_view, PdfApp, LIST_TOP, ROW_H};

pub(super) fn refresh_rect(w: i32) -> Rect {
    Rect::new(w - ui::MARGIN - 48, ui::TOP_SAFE + 16, 48, 48)
}

pub(super) fn state_action_rect(w: i32, h: i32) -> Rect {
    let y = (h / 2 + 128).min(h - ui::BOTTOM_SAFE - 56);
    Rect::new((w - 144) / 2, y, 144, 48)
}

impl PdfApp {
    // ---------------- picker ----------------

    pub(super) fn render_picker(&mut self, canvas: &mut Canvas, theme: &Theme) {
        let w = canvas.size().w;
        let h = canvas.size().h;
        let list_top = LIST_TOP;
        self.picker_scroll = view::clamp_scroll(self.picker_scroll, self.picker_max_scroll());

        if self.files.is_empty() {
            self.render_picker_header(canvas, theme);
            widgets::EmptyState::render(
                canvas,
                Rect::new(0, list_top, w, (h - list_top - ui::BOTTOM_SAFE).max(0)),
                theme,
                icons::DESCRIPTION,
                "No PDFs found",
                "Add a PDF to Documents or Downloads",
            );
            ui::draw_pill_button(
                canvas,
                theme,
                state_action_rect(w, h),
                "Rescan",
                false,
                self.rescan_pressed,
            );
            return;
        }

        for (i, f) in self.files.iter().enumerate() {
            let sy = list_top + i as i32 * ROW_H - self.picker_scroll as i32;
            if sy + ROW_H <= list_top || sy >= h {
                continue;
            }
            let r = Rect::new(ui::MARGIN, sy, w - ui::MARGIN * 2, ROW_H);
            if self.pressed_file == Some(i) {
                canvas.fill_rounded_rect(r, 12.0, theme.surface2);
            }
            widgets::icon_btn(
                canvas,
                theme,
                Rect::new(r.x, r.y + 12, 48, 48),
                icons::DESCRIPTION,
                IconBtnStyle::Tonal,
                false,
                false,
            );
            let x = r.x + 64;
            let text_w = (r.right() - x).max(0);
            ui::draw_text(
                canvas,
                &f.name,
                &ui::body(theme.text),
                x,
                r.y + 10,
                Some(text_w),
            );
            let directory = f
                .path
                .parent()
                .and_then(|parent| parent.file_name())
                .and_then(|name| name.to_str())
                .unwrap_or("Files");
            let details = format!("{directory} · {}", doc::format_bytes(f.bytes));
            ui::draw_text(
                canvas,
                &details,
                &ui::caption(theme.text2),
                x,
                r.y + 39,
                Some(text_w),
            );
            canvas.fill_rect(Rect::new(x, r.bottom() - 1, text_w, 1), theme.line);
        }

        canvas.fill_rect(Rect::new(0, 0, w, list_top), theme.bg);
        canvas.fill_rect(
            Rect::new(0, h - ui::BOTTOM_SAFE, w, ui::BOTTOM_SAFE),
            theme.bg,
        );
        self.render_picker_header(canvas, theme);
    }

    fn render_picker_header(&self, canvas: &mut Canvas, theme: &Theme) {
        let w = canvas.size().w;
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

        widgets::icon_btn(
            canvas,
            theme,
            refresh_rect(w),
            icons::REFRESH,
            IconBtnStyle::Plain,
            false,
            self.picker_action_pressed,
        );
    }

    // ---------------- overview ----------------

    pub(super) fn render_overview(&mut self, canvas: &mut Canvas, theme: &Theme) {
        let w = canvas.size().w;
        let h = canvas.size().h;
        let ch = chrome(w, h);
        let view = overview_view(w, h);
        self.ov_scroll = view::clamp_scroll(self.ov_scroll, self.ov_max_scroll());

        let back_r = Rect::new(ch.back.x, ch.back.y, ch.back.w, ch.back.h);
        widgets::icon_btn(
            canvas,
            theme,
            back_r,
            icons::BACK,
            IconBtnStyle::Plain,
            false,
            self.back_pressed,
        );
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
        widgets::icon_btn(
            canvas,
            theme,
            back_r,
            icons::BACK,
            IconBtnStyle::Plain,
            false,
            self.back_pressed,
        );
        widgets::EmptyState::render(
            canvas,
            Rect::new(0, LIST_TOP, w, (h - LIST_TOP - ui::BOTTOM_SAFE).max(0)),
            theme,
            icons::DESCRIPTION,
            "Can't open PDF",
            &self.error,
        );
        if self.last_path.is_some() {
            ui::draw_pill_button(
                canvas,
                theme,
                state_action_rect(w, h),
                "Retry",
                false,
                self.retry_pressed,
            );
        }
    }
}
