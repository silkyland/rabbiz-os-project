//! Full-page PDF viewer renderer (open/slide motion, page blitting, top/bottom chrome).

use de_api::Rect;
use de_design::motion::{bezier_eval, BEZIER_ENTER, BEZIER_EXIT, BEZIER_STANDARD};
use rabbiz_app_kit::ui;
use rabbiz_app_kit::{Canvas, Theme};

use crate::view;

use super::{chrome, NavBtn, PageDraw, PdfApp};

impl PdfApp {
    fn viewer_e(&self) -> f32 {
        match &self.anim {
            Some(a) => {
                let t = a.t.clamp(0.0, 1.0);
                if a.open {
                    bezier_eval(BEZIER_ENTER, t)
                } else {
                    1.0 - bezier_eval(BEZIER_EXIT, t)
                }
            }
            None => 1.0,
        }
    }

    /// Draw one page (or placeholder) into the viewer.
    /// `apply_transform` is true only for the settled current page — slide
    /// neighbours always draw at their own fit/zoom-1 layout.
    fn draw_page(&mut self, canvas: &mut Canvas, theme: &Theme, p: PageDraw) {
        let ch = chrome(self.cur_w, self.cur_h);
        let (w_pt, h_pt) = self.page_pt(p.idx);
        let base = view::fit_width_scale(ch.content.w, w_pt);
        let (zoom, pan) = if p.apply_transform {
            (self.zoom, self.pan)
        } else {
            let z = view::ZOOM_STEPS[p.z.min(view::ZOOM_STEPS.len() - 1)];
            // Slide neighbours show their own home position.
            (z, view::home_pan(ch.content.h, w_pt, h_pt, base, z))
        };
        let key = (p.idx, p.z);
        match self.pages.get(&key) {
            Some(Some(px)) => {
                let r = view::page_rect(ch.content, w_pt, h_pt, base, zoom, pan);
                let r = Rect::new(r.x + p.dx as i32, r.y + p.rise as i32, r.w, r.h);
                canvas.blit_scaled(px, r, 1.0);
            }
            Some(None) => {
                let r = ch.content;
                canvas.fill_rect(r, theme.surface1);
                self.page_placeholder(canvas, theme, p.idx, "Can't render this page");
            }
            None => {
                // Not yet rendered: page-shaped placeholder.
                let r = view::page_rect(ch.content, w_pt, h_pt, base, zoom, pan);
                let r = Rect::new(r.x + p.dx as i32, r.y + p.rise as i32, r.w, r.h);
                canvas.fill_rect(r, theme.surface1);
                canvas.fill_rect(
                    Rect::new(r.x, r.y, r.w, (r.h * 18 / 100).max(24)),
                    theme.surface2,
                );
                ui::draw_centered(
                    canvas,
                    &format!("Page {}", p.idx + 1),
                    &ui::caption(theme.text3),
                    Rect::new(r.x, r.y + r.h / 2 - 16, r.w, 32),
                );
            }
        }
    }

    fn page_placeholder(&self, canvas: &mut Canvas, theme: &Theme, idx: usize, msg: &str) {
        let cy = self.cur_h / 2;
        ui::draw_centered(
            canvas,
            &format!("Page {}", idx + 1),
            &ui::body(theme.text),
            Rect::new(0, cy - 60, self.cur_w, 44),
        );
        ui::draw_centered(
            canvas,
            msg,
            &ui::caption(theme.text3),
            Rect::new(0, cy - 8, self.cur_w, 32),
        );
    }

    pub(super) fn render_viewer(&mut self, canvas: &mut Canvas, theme: &Theme) {
        // Keep pan valid across window resizes and the pre-first-frame open.
        self.clamp_pan_now();
        let e = self.viewer_e();
        let n = self.page_count();
        let vw = self.cur_w as f32;
        let ch = chrome(self.cur_w, self.cur_h);

        let bg = theme.bg;
        let alpha = (e * 255.0) as u8;
        canvas.fill_rect(
            Rect::new(0, 0, self.cur_w, self.cur_h),
            [bg[0], bg[1], bg[2], alpha],
        );

        let rise = (1.0 - e) * 48.0;
        if let Some(s) = self.slide {
            let k = bezier_eval(BEZIER_STANDARD, s.t.clamp(0.0, 1.0));
            let dir = s.u.signum();
            let old_x = s.u * (1.0 - k) + vw * dir * k;
            let new_x = (s.u - vw * dir) * (1.0 - k);
            // Neighbours draw at zoom bucket 0 (fit) — a page turn resets zoom.
            self.draw_page(
                canvas,
                theme,
                PageDraw {
                    idx: s.from,
                    z: 0,
                    dx: old_x,
                    rise,
                    apply_transform: false,
                },
            );
            self.draw_page(
                canvas,
                theme,
                PageDraw {
                    idx: s.to,
                    z: self.zoom_i,
                    dx: new_x,
                    rise,
                    apply_transform: false,
                },
            );
        } else {
            self.draw_page(
                canvas,
                theme,
                PageDraw {
                    idx: self.page,
                    z: self.zoom_i,
                    dx: self.drag_dx,
                    rise,
                    apply_transform: true,
                },
            );
        }

        // ---- chrome (rides the same rise) ----
        let ry = rise as i32;
        let back_r = Rect::new(ch.back.x, ch.back.y + ry, ch.back.w, ch.back.h);
        ui::draw_pill_button(canvas, theme, back_r, "‹ Files", false, self.back_pressed);
        let grid_r = Rect::new(
            ch.grid_btn.x,
            ch.grid_btn.y + ry,
            ch.grid_btn.w,
            ch.grid_btn.h,
        );
        ui::draw_pill_button(canvas, theme, grid_r, "Pages", false, self.grid_pressed);
        if n > 0 {
            let counter = format!("{} / {}", self.page + 1, n);
            let mid_w = (grid_r.x - ch.back.right() - 16).max(40);
            let mid = Rect::new(ch.back.right() + 8, ch.back.y + ry, mid_w, ch.back.h);
            ui::draw_centered(canvas, &counter, &ui::label(theme.text), mid);
        }

        // ---- bottom nav strip ----
        let strip_y = ch.prev.y - 20 + ry;
        canvas.fill_rect(
            Rect::new(
                0,
                strip_y - 8,
                self.cur_w,
                self.cur_h - strip_y + ui::BOTTOM_SAFE,
            ),
            theme.scrim,
        );
        let zlabel = format!("{}%", (self.zoom * 100.0) as i32);
        let btns: [(Rect, &str, NavBtn, bool); 5] = [
            (ch.prev, "‹", NavBtn::Prev, true),
            (ch.zminus, "−", NavBtn::ZoomMinus, false),
            (ch.zlabel, zlabel.as_str(), NavBtn::ZoomLabel, false),
            (ch.zplus, "+", NavBtn::ZoomPlus, false),
            (ch.next, "›", NavBtn::Next, true),
        ];
        for (r, label, btn, primary) in btns {
            let rr = Rect::new(r.x, r.y + ry, r.w, r.h);
            ui::draw_pill_button(
                canvas,
                theme,
                rr,
                label,
                primary,
                self.nav_pressed == Some(btn),
            );
        }
        if let Some(d) = self.doc.as_ref() {
            ui::draw_text(
                canvas,
                &d.name,
                &ui::caption(theme.text3),
                ui::MARGIN,
                strip_y - 26 + ry,
                Some(self.cur_w - ui::MARGIN * 2),
            );
        }
    }
}
