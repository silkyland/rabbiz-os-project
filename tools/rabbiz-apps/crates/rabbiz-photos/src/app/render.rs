//! Grid and full-screen viewer rendering for `rabbiz-photos`.

use de_api::Rect;
use de_design::motion::{bezier_eval, BEZIER_ENTER, BEZIER_EXIT, BEZIER_STANDARD};
use rabbiz_app_kit::ui;
use rabbiz_app_kit::{Canvas, Theme};

use crate::layout;
use crate::photos;
use crate::viewer;

use super::PhotosApp;

// ---------------------------------------------------------------------------
// Grid
// ---------------------------------------------------------------------------

impl PhotosApp {
    pub(super) fn render_grid(&mut self, canvas: &mut Canvas, theme: &Theme) {
        let w = canvas.size().w;
        let h = canvas.size().h;
        let after_header = ui::draw_header(
            canvas,
            theme,
            "Photos",
            &format!("{} items", self.photos.len()),
        );
        self.view_top = after_header + 8;
        self.view_h = (h - ui::BOTTOM_SAFE - self.view_top).max(0);
        self.layout = layout::build(w, &self.groups);
        self.scroll = layout::clamp_scroll(self.scroll, self.max_scroll());
        self.rebuild_pending();

        if self.photos.is_empty() {
            ui::draw_centered(
                canvas,
                "No photos in ~/Pictures or ~/Downloads",
                &ui::body(theme.text3),
                Rect::new(0, h / 2 - 40, w, 80),
            );
            return;
        }

        // Group headers (content coords → screen coords).
        for hd in &self.layout.headers {
            let g = &self.groups[hd.group];
            let sy = self.view_top + hd.rect.y - self.scroll as i32;
            if sy + hd.rect.h < self.view_top || sy > h {
                continue;
            }
            ui::draw_text(
                canvas,
                &g.label,
                &ui::caption(theme.text3),
                hd.rect.x,
                sy + 10,
                Some(hd.rect.w),
            );
        }

        for cell in &self.layout.cells {
            let sy = self.view_top + cell.rect.y - self.scroll as i32;
            if sy + cell.rect.h < self.view_top || sy > h {
                continue;
            }
            let r = Rect::new(cell.rect.x, sy, cell.rect.w, cell.rect.h);
            let idx = cell.photo;
            if let Some(px) = self.thumbs.get(&idx) {
                canvas.blit_scaled(px, r, 1.0);
            } else {
                // Placeholder: surface tile + format hint, like Android's
                // grey thumb while the decode is in flight.
                canvas.fill_rect(r, theme.surface1);
                let name = self.photos.get(idx).map(|p| p.name.as_str()).unwrap_or("");
                let ext = name.rsplit('.').next().unwrap_or("").to_uppercase();
                if self.thumb_failed.contains(&idx) {
                    ui::draw_centered(canvas, "!", &ui::title(theme.text3), r);
                } else {
                    ui::draw_centered(canvas, &ext, &ui::caption(theme.text3), r);
                }
            }
            if self.pressed_cell == Some(idx) {
                canvas.fill_rect(r, [255, 255, 255, 18]);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Viewer
// ---------------------------------------------------------------------------

impl PhotosApp {
    /// Progress of the open/close animation in 0..=1 (1 = fully shown).
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

    /// Draw one decoded (or placeholder) photo into the viewer at `dx` extra
    /// horizontal offset and `rise` vertical offset (open/close motion).
    /// `apply_transform` is true only for the settled current image —
    /// sliding neighbours always draw at fit/zoom 1.
    fn draw_photo(
        &self,
        canvas: &mut Canvas,
        theme: &Theme,
        idx: usize,
        dx: f32,
        rise: f32,
        apply_transform: bool,
    ) {
        match self.v_px.get(&idx) {
            Some(Some(px)) => {
                let (zoom, pan) = if apply_transform {
                    (self.v_zoom, self.v_pan)
                } else {
                    (1.0, (0.0, 0.0))
                };
                let base = viewer::fit_scale(self.cur_w, self.cur_h, px.size.w, px.size.h);
                let (x, y, dw, dh) = viewer::image_rect(
                    self.cur_w, self.cur_h, px.size.w, px.size.h, base, zoom, pan,
                );
                let r = Rect::new((x + dx) as i32, (y + rise) as i32, dw as i32, dh as i32);
                canvas.blit_scaled(px, r, 1.0);
            }
            Some(None) => {
                let r = Rect::new(0, 0, self.cur_w, self.cur_h);
                canvas.fill_rect(r, theme.surface1);
                self.draw_placeholder(canvas, theme, idx, "Can't decode this image");
            }
            None => {
                let r = Rect::new(0, 0, self.cur_w, self.cur_h);
                canvas.fill_rect(r, theme.surface1);
                self.draw_placeholder(canvas, theme, idx, "Loading…");
            }
        }
    }

    fn draw_placeholder(&self, canvas: &mut Canvas, theme: &Theme, idx: usize, msg: &str) {
        let name = self
            .photos
            .get(idx)
            .map(|p| p.name.as_str())
            .unwrap_or("Photo");
        let cy = self.cur_h / 2;
        ui::draw_centered(
            canvas,
            name,
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
        let e = self.viewer_e();
        let n = self.photos.len();
        let vw = self.cur_w as f32;

        // Background: translucent while the open/close anim plays.
        let bg = theme.bg;
        let alpha = (e * 255.0) as u8;
        canvas.fill_rect(
            Rect::new(0, 0, self.cur_w, self.cur_h),
            [bg[0], bg[1], bg[2], alpha],
        );

        // Slide offsets: during a committed page turn both images travel;
        // otherwise the current image follows drag_dx / settle.
        // Open/close motion: content rises ~48 px while fading in.
        let rise = (1.0 - e) * 48.0;
        if let Some(s) = self.slide {
            let k = bezier_eval(BEZIER_STANDARD, s.t.clamp(0.0, 1.0));
            // Content-space model: the outgoing image travels from the
            // release offset `u` to one viewport in the swipe direction;
            // the incoming image starts one viewport past `u` and lands at 0.
            let dir = s.u.signum();
            let old_x = s.u * (1.0 - k) + vw * dir * k;
            let new_x = (s.u - vw * dir) * (1.0 - k);
            self.draw_photo(canvas, theme, s.from, old_x, rise, false);
            self.draw_photo(canvas, theme, s.to, new_x, rise, false);
        } else {
            let dx = self.drag_dx;
            self.draw_photo(canvas, theme, self.v_index, dx, rise, true);
        }

        // ---- chrome (rides the same rise as the content) ----
        let ry = rise as i32;
        let back_r = Rect::new(ui::MARGIN, ui::TOP_SAFE + 12 + ry, 120, 48);
        ui::draw_pill_button(canvas, theme, back_r, "‹ Back", false, self.back_pressed);
        if n > 0 {
            let counter = format!("{} / {}", self.v_index + 1, n);
            ui::draw_text(
                canvas,
                &counter,
                &ui::caption(theme.text2),
                self.cur_w - ui::MARGIN - 80,
                ui::TOP_SAFE + 24 + ry,
                Some(80),
            );
        }

        // Bottom metadata: file name + date on a translucent strip.
        let strip_h = 76;
        let strip_y = self.cur_h - ui::BOTTOM_SAFE - strip_h + ry;
        canvas.fill_rect(
            Rect::new(0, strip_y, self.cur_w, strip_h + ui::BOTTOM_SAFE),
            theme.scrim,
        );
        if let Some(p) = self.photos.get(self.v_index) {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            let meta = format!(
                "{}  ·  {}  ·  {}",
                p.name,
                photos::format_day(p.mtime, now),
                photos::format_bytes(p.bytes)
            );
            ui::draw_text(
                canvas,
                &meta,
                &ui::caption(theme.text),
                ui::MARGIN,
                strip_y + 14,
                Some(self.cur_w - ui::MARGIN * 2),
            );
            ui::draw_text(
                canvas,
                "Double-tap to zoom · swipe to turn pages",
                &ui::caption(theme.text3),
                ui::MARGIN,
                strip_y + 42,
                Some(self.cur_w - ui::MARGIN * 2),
            );
        }
    }
}
