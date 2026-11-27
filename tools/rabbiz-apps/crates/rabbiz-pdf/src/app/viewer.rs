//! Full-page PDF viewer renderer (open/slide motion, page blitting, top/bottom chrome).

use de_api::Rect;
use de_design::motion::{bezier_eval, BEZIER_ENTER, BEZIER_EXIT, BEZIER_STANDARD};
use rabbiz_app_kit::ui;
use rabbiz_app_kit::widgets::{self, IconBtnStyle};
use rabbiz_app_kit::{icons, Canvas, Theme};

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

        // Keep an opaque reading surface behind the title and float navigation
        // above the document where it can be hidden with a tap.
        let opacity = self.chrome_alpha() * e;
        if opacity == 0.0 {
            return;
        }
        let fade = |color: [u8; 4]| ui::with_alpha(color, (f32::from(color[3]) * opacity) as u8);
        let chrome_theme = Theme {
            text: fade(theme.text),
            text2: fade(theme.text2),
            surface2: fade(theme.surface2),
            ..*theme
        };
        let ry = rise as i32;
        let back = ch.back.translate(0, ry);
        let grid = ch.grid_btn.translate(0, ry);
        canvas.fill_rect(
            Rect::new(0, ry, self.cur_w, back.bottom() - ry + 8),
            fade(theme.bg),
        );
        widgets::icon_btn(
            canvas,
            &chrome_theme,
            back,
            icons::BACK,
            IconBtnStyle::Plain,
            false,
            self.back_pressed,
        );
        widgets::icon_btn(
            canvas,
            &chrome_theme,
            grid,
            icons::GRID_VIEW,
            IconBtnStyle::Plain,
            false,
            self.grid_pressed,
        );
        let title_x = back.right() + 12;
        let title_w = (grid.x - title_x - 8).max(0);
        if let Some(doc) = &self.doc {
            ui::draw_text(
                canvas,
                &doc.name,
                &ui::label(chrome_theme.text),
                title_x,
                back.y,
                Some(title_w),
            );
            ui::draw_text(
                canvas,
                &format!("Page {} of {}", self.page + 1, n),
                &ui::caption(chrome_theme.text2),
                title_x,
                back.y + 24,
                Some(title_w),
            );
        }
        let pill = ch.pill.translate(0, ry);
        canvas.fill_rounded_rect(pill, pill.h as f32 / 2.0, chrome_theme.surface2);
        for (r, icon, button) in [
            (ch.prev, icons::CHEVRON_LEFT, NavBtn::Prev),
            (ch.zminus, icons::ZOOM_OUT, NavBtn::ZoomMinus),
            (ch.zplus, icons::ZOOM_IN, NavBtn::ZoomPlus),
            (ch.next, icons::CHEVRON_RIGHT, NavBtn::Next),
        ] {
            widgets::icon_btn(
                canvas,
                &chrome_theme,
                r.translate(0, ry),
                icon,
                IconBtnStyle::Plain,
                false,
                self.nav_pressed == Some(button),
            );
        }
        ui::draw_centered(
            canvas,
            &format!("{}%", (self.zoom * 100.0) as i32),
            &ui::label(chrome_theme.text),
            ch.zlabel.translate(0, ry),
        );
    }
}
