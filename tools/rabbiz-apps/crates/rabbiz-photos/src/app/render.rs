//! Grid and full-screen viewer rendering for `rabbiz-photos`.
//!
//! The viewer is immersive: the photo covers the whole surface on a dark
//! (`tokens::DARK`) backdrop regardless of the app theme, and the chrome is
//! an overlay that fades in/out on single-tap via `chrome_alpha`.

use de_api::Rect;
use de_design::motion::{bezier_eval, BEZIER_ENTER, BEZIER_EXIT, BEZIER_STANDARD};
use rabbiz_app_kit::ui;
use rabbiz_app_kit::widgets::{self, IconBtnStyle};
use rabbiz_app_kit::{icons, tokens, Canvas, Theme};

use crate::layout::{self, HEADER_H};
use crate::photos;
use crate::viewer;

use super::PhotosApp;

// ---------------------------------------------------------------------------
// Shared geometry (single source for render + input hit-testing)
// ---------------------------------------------------------------------------

/// Grid header trailing action (refresh / rescan).
pub(super) fn header_action_rect(w: i32) -> Rect {
    Rect::new(w - ui::MARGIN - 48, ui::TOP_SAFE + 16, 48, 48)
}

/// "Rescan" pill drawn below the empty-state block inside `area`.
pub(super) fn rescan_rect(area: Rect) -> Rect {
    let y = (area.y + area.h / 2 + 104).min(area.bottom() - 52);
    Rect::new(area.x + area.w / 2 - 90, y, 180, 52)
}

/// Interactive viewer chrome rects + the two scrim bands. `rise` is the
/// open/close vertical offset the chrome rides together with the content.
#[derive(Debug, Clone, Copy)]
pub(super) struct ViewerChrome {
    pub top_band: Rect,
    pub bottom_band: Rect,
    pub back: Rect,
    pub info: Rect,
    pub delete: Rect,
}

pub(super) fn viewer_chrome(w: i32, h: i32, rise: i32) -> ViewerChrome {
    let top_band = Rect::new(0, rise - 48, w, ui::TOP_SAFE + 140);
    let back = Rect::new(ui::MARGIN, ui::TOP_SAFE + 12 + rise, 48, 48);
    let strip_h = 140;
    let bottom_band = Rect::new(
        0,
        h - ui::BOTTOM_SAFE - strip_h + rise,
        w.max(0),
        strip_h + ui::BOTTOM_SAFE,
    );
    let ay = bottom_band.y + 76;
    ViewerChrome {
        top_band,
        bottom_band,
        back,
        info: Rect::new(w / 2 - 60, ay, 48, 48),
        delete: Rect::new(w / 2 + 12, ay, 48, 48),
    }
}

/// `color` at fraction `a` of its alpha — used to fade chrome as one piece.
fn fade(c: [u8; 4], a: f32) -> [u8; 4] {
    [c[0], c[1], c[2], (c[3] as f32 * a.clamp(0.0, 1.0)) as u8]
}

/// `tokens::DARK` with every color's alpha scaled by `a`. Overlay chrome on
/// a photo always comes from the dark variant: light text/icons stay
/// legible over bright shots and the whole overlay fades out together.
fn dark_faded(a: f32) -> Theme {
    let t = tokens::DARK;
    Theme {
        bg: fade(t.bg, a),
        canvas: fade(t.canvas, a),
        surface1: fade(t.surface1, a),
        surface2: fade(t.surface2, a),
        surface3: fade(t.surface3, a),
        text: fade(t.text, a),
        text2: fade(t.text2, a),
        text3: fade(t.text3, a),
        text_on_accent: fade(t.text_on_accent, a),
        accent: fade(t.accent, a),
        accent_hi: fade(t.accent_hi, a),
        accent_dim: fade(t.accent_dim, a),
        success: fade(t.success, a),
        warning: fade(t.warning, a),
        danger: fade(t.danger, a),
        line: fade(t.line, a),
        scrim: fade(t.scrim, a),
        glass: fade(t.glass, a),
        glass_hi: fade(t.glass_hi, a),
        shadow: fade(t.shadow, a),
        icon_tile: fade(t.icon_tile, a),
        icon_glyph: fade(t.icon_glyph, a),
    }
}

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
        widgets::icon_btn(
            canvas,
            theme,
            header_action_rect(w),
            icons::REFRESH,
            IconBtnStyle::Plain,
            false,
            self.refresh_pressed,
        );
        self.view_top = after_header + 8;
        self.view_h = (h - ui::BOTTOM_SAFE - self.view_top).max(0);
        self.layout = layout::build(w, &self.groups);
        self.sv
            .set_bounds(self.layout.content_h as f32, self.view_h as f32);
        let scroll = self.sv.offset();
        self.rebuild_pending();

        if self.photos.is_empty() {
            let area = Rect::new(0, self.view_top, w, self.view_h);
            widgets::EmptyState::render(
                canvas,
                area,
                theme,
                icons::PHOTO_LIBRARY,
                "No photos yet",
                "Add images to Pictures or Downloads",
            );
            ui::draw_pill_button(
                canvas,
                theme,
                rescan_rect(area),
                "Rescan",
                false,
                self.rescan_pressed,
            );
            return;
        }

        let view_bottom = self.view_top + self.view_h;
        let scroll_i = scroll as i32;

        // Cells first — the sticky section header overlays them.
        for cell in &self.layout.cells {
            let sy = self.view_top + cell.rect.y - scroll_i;
            if sy + cell.rect.h < self.view_top || sy > view_bottom {
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
                ui::press_overlay(canvas, theme, r, 0.0, 0.12);
            }
        }

        // Section headers at their natural positions (label-weight text,
        // 24 px indent — AOSP gallery style).
        let header_style = ui::styled(19.0, 600, theme.text2);
        let header_ty = (HEADER_H - 26) / 2;
        for hd in &self.layout.headers {
            let g = &self.groups[hd.group];
            let sy = self.view_top + hd.rect.y - scroll_i;
            if sy + hd.rect.h <= self.view_top || sy >= view_bottom {
                continue;
            }
            ui::draw_text(
                canvas,
                &g.label,
                &header_style,
                hd.rect.x,
                sy + header_ty,
                Some(hd.rect.w),
            );
        }

        // Sticky header: the section the user is inside pins to the view top
        // and slides away as the next section's header reaches it.
        if let Some(pi) = self
            .layout
            .headers
            .iter()
            .rposition(|hd| (hd.rect.y as f32) <= scroll)
        {
            let hd = &self.layout.headers[pi];
            let mut pin_y = self.view_top;
            if let Some(nxt) = self.layout.headers.get(pi + 1) {
                let nxt_top = self.view_top + nxt.rect.y - scroll_i;
                pin_y = pin_y.min(nxt_top - HEADER_H);
            }
            let band = Rect::new(0, pin_y, w, HEADER_H);
            canvas.fill_rect(band, theme.bg);
            ui::draw_text(
                canvas,
                &self.groups[hd.group].label,
                &header_style,
                hd.rect.x,
                pin_y + header_ty,
                Some(hd.rect.w),
            );
        }

        ui::scrollbar(
            canvas,
            theme,
            Rect::new(w - 8, self.view_top, 8, self.view_h),
            scroll_i,
            self.layout.content_h,
        );
    }
}

// ---------------------------------------------------------------------------
// Viewer
// ---------------------------------------------------------------------------

impl PhotosApp {
    /// Progress of the open/close animation in 0..=1 (1 = fully shown).
    pub(super) fn viewer_e(&self) -> f32 {
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

    /// The open/close vertical offset chrome and content ride together.
    pub(super) fn viewer_rise(&self) -> i32 {
        ((1.0 - self.viewer_e()) * 48.0) as i32
    }

    /// Draw one decoded (or placeholder) photo into the viewer at `dx` extra
    /// horizontal offset and `rise` vertical offset (open/close motion).
    /// `apply_transform` is true only for the settled current image —
    /// sliding neighbours always draw at fit/zoom 1.
    fn draw_photo(
        &self,
        canvas: &mut Canvas,
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
                canvas.fill_rect(r, tokens::DARK.surface1);
                self.draw_placeholder(canvas, idx, "Can't decode this image");
            }
            None => {
                let r = Rect::new(0, 0, self.cur_w, self.cur_h);
                canvas.fill_rect(r, tokens::DARK.surface1);
                self.draw_placeholder(canvas, idx, "Loading…");
            }
        }
    }

    fn draw_placeholder(&self, canvas: &mut Canvas, idx: usize, msg: &str) {
        let t = tokens::DARK;
        let name = self
            .photos
            .get(idx)
            .map(|p| p.name.as_str())
            .unwrap_or("Photo");
        let cy = self.cur_h / 2;
        ui::draw_centered(
            canvas,
            name,
            &ui::body(t.text),
            Rect::new(0, cy - 60, self.cur_w, 44),
        );
        ui::draw_centered(
            canvas,
            msg,
            &ui::caption(t.text3),
            Rect::new(0, cy - 8, self.cur_w, 32),
        );
    }

    /// Current photo's display name (shared by the title line and the
    /// delete-confirmation dialog body).
    pub(super) fn current_name(&self) -> String {
        self.photos
            .get(self.v_index)
            .map(|p| p.name.clone())
            .unwrap_or_default()
    }

    pub(super) fn render_viewer(&mut self, canvas: &mut Canvas, theme: &Theme) {
        let e = self.viewer_e();
        let n = self.photos.len();
        let vw = self.cur_w as f32;

        // Full-bleed dark surface — translucent only during the open/close
        // morph so the grid can show through.
        let bg = tokens::DARK.bg;
        let alpha = (e * 255.0) as u8;
        canvas.fill_rect(
            Rect::new(0, 0, self.cur_w, self.cur_h),
            [bg[0], bg[1], bg[2], alpha],
        );

        // Slide offsets: during a committed page turn both images travel;
        // otherwise the current image follows drag_dx / settle.
        let rise = (1.0 - e) * 48.0;
        if let Some(s) = self.slide {
            let k = bezier_eval(BEZIER_STANDARD, s.t.clamp(0.0, 1.0));
            // Content-space model: the outgoing image travels from the
            // release offset `u` to one viewport in the swipe direction;
            // the incoming image starts one viewport past `u` and lands at 0.
            let dir = s.u.signum();
            let old_x = s.u * (1.0 - k) + vw * dir * k;
            let new_x = (s.u - vw * dir) * (1.0 - k);
            self.draw_photo(canvas, s.from, old_x, rise, false);
            self.draw_photo(canvas, s.to, new_x, rise, false);
        } else {
            let dx = self.drag_dx;
            self.draw_photo(canvas, self.v_index, dx, rise, true);
        }

        // ---- overlay chrome (rides the same rise; fades on single-tap) ----
        let a = self.chrome_alpha() * e;
        if a > 0.001 {
            let vc = viewer_chrome(self.cur_w, self.cur_h, rise as i32);
            let vt = dark_faded(a);

            // Scrim bands protect the text/icons on bright photos.
            canvas.fill_rect(vc.top_band, fade(tokens::DARK.scrim, a));
            canvas.fill_rect(vc.bottom_band, fade(tokens::DARK.scrim, a));

            // Back: icon button on a scrim circle.
            canvas.fill_rounded_rect(vc.back, 24.0, fade(tokens::DARK.scrim, a));
            widgets::icon_btn(
                canvas,
                &vt,
                vc.back,
                icons::BACK,
                IconBtnStyle::Plain,
                false,
                self.back_pressed,
            );

            if let Some(p) = self.photos.get(self.v_index) {
                // Title: photo name beside the back button.
                ui::draw_text(
                    canvas,
                    &p.name,
                    &ui::label(vt.text),
                    vc.back.right() + 14,
                    vc.back.y + 11,
                    Some(self.cur_w - ui::MARGIN - (vc.back.right() + 14) - 84),
                );
            }
            if n > 0 {
                let counter = format!("{} / {}", self.v_index + 1, n);
                ui::draw_text_right(
                    canvas,
                    &counter,
                    &ui::caption(vt.text2),
                    Rect::new(self.cur_w - ui::MARGIN - 90, vc.back.y, 90, 48),
                    0,
                );
            }

            // Bottom strip: metadata line + action icon row.
            if let Some(p) = self.photos.get(self.v_index) {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0);
                let mut meta = format!(
                    "{}  ·  {}",
                    photos::format_day(p.mtime, now),
                    photos::format_bytes(p.bytes)
                );
                if self.show_info {
                    if let Some((iw, ih)) = self.info_dimensions {
                        meta = format!("{meta}  ·  {iw}×{ih}");
                    }
                }
                ui::draw_text(
                    canvas,
                    &meta,
                    &ui::caption(vt.text2),
                    ui::MARGIN,
                    vc.bottom_band.y + 22,
                    Some(self.cur_w - ui::MARGIN * 2),
                );
            }

            // Actions: info (selected = dims shown) + delete, on scrim circles.
            canvas.fill_rounded_rect(vc.info, 24.0, fade(tokens::DARK.scrim, a));
            widgets::icon_btn(
                canvas,
                &vt,
                vc.info,
                icons::INFO,
                IconBtnStyle::Plain,
                self.show_info,
                self.info_pressed,
            );
            canvas.fill_rounded_rect(vc.delete, 24.0, fade(tokens::DARK.scrim, a));
            widgets::icon_btn(
                canvas,
                &vt,
                vc.delete,
                icons::DELETE,
                IconBtnStyle::Plain,
                false,
                self.delete_pressed,
            );
        }

        // Transient toast (delete result, rescan notice).
        if let Some((msg, _)) = &self.toast {
            widgets::Toast::render(canvas, canvas.size(), ui::BOTTOM_SAFE, theme, msg);
        }

        // Delete confirmation — a real dialog, above all chrome.
        if self.del_dialog {
            let name = self.current_name();
            widgets::dialog(
                canvas,
                theme,
                canvas.size(),
                "Delete this photo?",
                &[name.as_str()],
                &["Cancel", "Delete"],
                self.del_pressed,
            );
        }
    }
}
