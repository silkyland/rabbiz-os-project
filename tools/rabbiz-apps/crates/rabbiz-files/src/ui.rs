//! Rendering and drawing primitives for `rabbiz-files`.

use std::path::{Path, PathBuf};

use de_api::{Point, Rect};
use rabbiz_app_kit::text;
use rabbiz_app_kit::ui as kit_ui;
use rabbiz_app_kit::{Canvas, Theme};

use crate::layout as lay;
use crate::model::{self, FileKind, SortKey};
use crate::{Dialog, FilesApp, Hit, Item, SelAct};

const PATH_BAR_H: i32 = 46;
const SEARCH_H: i32 = 54;
const BANNER_H: i32 = 58;
const FOOTER_H: i32 = 40;
const ACTBAR_H: i32 = 68;
/// Cap on recursive work shown in details (mirrors model::WALK_CAP).
const DETAIL_ITEM_CAP: usize = model::WALK_CAP;

fn alpha(c: [u8; 4], a: u8) -> [u8; 4] {
    [c[0], c[1], c[2], a]
}

/// Rasterize + blit a Material icon with ink bounds centered on (cx, cy).
fn draw_icon(c: &mut Canvas, codepoint: &str, size: f32, color: [u8; 4], cx: i32, cy: i32) {
    if let Some(px) = text::icon(codepoint, size, color) {
        let (x0, y0, x1, y1) =
            text::ink_bounds(&px).unwrap_or((0, 0, px.width() as i32, px.height() as i32));
        c.blit(
            &de_api::Pixels::new(
                de_api::Size::new(px.width() as i32, px.height() as i32),
                px.data().to_vec(),
            ),
            Point::new(cx - (x0 + x1) / 2, cy - (y0 + y1) / 2),
        );
    }
}

fn kind_icon(kind: FileKind) -> &'static str {
    match kind {
        FileKind::Folder => "\u{e2c7}",
        FileKind::Image => "\u{e3f4}",
        FileKind::Audio => "\u{e3a1}",
        FileKind::Video => "\u{e04b}",
        FileKind::Archive => "\u{e149}",
        FileKind::Code => "\u{e86f}",
        FileKind::Document => "\u{e873}",
        FileKind::Generic => "\u{e24d}",
    }
}

fn hit_rect(hits: &mut Vec<(Rect, Hit)>, r: Rect, h: Hit) {
    if r.w > 0 && r.h > 0 {
        hits.push((r, h));
    }
}

fn icon_button(
    c: &mut Canvas,
    t: &Theme,
    hits: &mut Vec<(Rect, Hit)>,
    r: Rect,
    codepoint: &str,
    hit: Hit,
    active: bool,
) {
    c.fill_rounded_rect(r, (r.h as f32) / 2.0, t.surface1);
    let col = if active { t.accent } else { t.text };
    draw_icon(c, codepoint, 26.0, col, r.x + r.w / 2, r.y + r.h / 2);
    hit_rect(hits, r, hit);
}

/// (height, title) — dialog card metrics.
fn dialog_metrics(d: &Dialog) -> (i32, &'static str) {
    match d {
        Dialog::NewFolder => (300, "New folder"),
        Dialog::Rename(_) => (300, "Rename"),
        Dialog::Delete { .. } => (300, "Delete"),
        Dialog::Details(_) => (560, "Details"),
    }
}

impl model::FileKind {
    fn label(self) -> &'static str {
        match self {
            FileKind::Folder => "Folder",
            FileKind::Image => "Image",
            FileKind::Audio => "Audio",
            FileKind::Video => "Video",
            FileKind::Archive => "Archive",
            FileKind::Code => "Code",
            FileKind::Document => "Document",
            FileKind::Generic => "File",
        }
    }
}

impl FilesApp {
    // ---------------- render ----------------

    pub(crate) fn render_app(&mut self, c: &mut Canvas, t: &Theme) {
        let size = c.size();
        self.cur_w = size.w;
        self.cur_h = size.h;
        self.sync();
        self.hits.clear();
        self.crumbs.clear();
        self.crumb_targets.clear();

        let mut hits = std::mem::take(&mut self.hits);
        self.render_browser(c, t, &mut hits);
        if self.menu_open {
            self.render_menu(c, t, &mut hits);
        }
        if let Some(d) = self.dialog.clone() {
            self.render_dialog(c, t, &mut hits, &d);
        }
        self.hits = hits;
    }

    fn render_browser(&mut self, c: &mut Canvas, t: &Theme, hits: &mut Vec<(Rect, Hit)>) {
        let w = self.cur_w;
        let h = self.cur_h;

        // ---- path bar ----
        let bar_y = kit_ui::TOP_SAFE + 8;
        let mut x = kit_ui::MARGIN;
        if self.can_back() {
            let r = Rect::new(x, bar_y, 44, PATH_BAR_H);
            icon_button(c, t, hits, r, "\u{e5c4}", Hit::Back, false);
            x += 48;
        }
        {
            let r = Rect::new(x, bar_y, 44, PATH_BAR_H);
            icon_button(c, t, hits, r, "\u{e88a}", Hit::Home, false);
            x += 48;
        }
        // Breadcrumb segments.
        let segs = self.crumb_segments();
        let sep_w = 14;
        let avail = w - kit_ui::MARGIN - x - 3 * 52 - 16;
        let mut cx = x;
        let n = segs.len();
        for (i, (label, target)) in segs.iter().enumerate() {
            let st = kit_ui::styled(19.0, if i + 1 == n { 600 } else { 400 }, t.text);
            let max_w = if i + 1 == n {
                (avail - cx + x).max(40)
            } else {
                180
            };
            let Some(px) = text::label(label, st, Some(max_w)) else {
                continue;
            };
            let tw = px.width() as i32;
            if cx + tw > w - kit_ui::MARGIN - 3 * 52 {
                break;
            }
            let r = Rect::new(cx - 4, bar_y, tw + 8, PATH_BAR_H);
            self.crumbs.push((label.clone(), r));
            self.crumb_targets.push(target.clone());
            c.blit(
                &de_api::Pixels::new(
                    de_api::Size::new(tw, px.height() as i32),
                    px.data().to_vec(),
                ),
                Point::new(cx, bar_y + (PATH_BAR_H - px.height() as i32) / 2),
            );
            hit_rect(hits, r, Hit::Crumb(self.crumbs.len() - 1));
            cx += tw;
            if i + 1 < n {
                let st_sep = kit_ui::caption(t.text3);
                let sep = " / ";
                if let Some(sp) = text::label(sep, st_sep, None) {
                    c.blit(
                        &de_api::Pixels::new(
                            de_api::Size::new(sp.width() as i32, sp.height() as i32),
                            sp.data().to_vec(),
                        ),
                        Point::new(cx, bar_y + (PATH_BAR_H - sp.height() as i32) / 2),
                    );
                }
                cx += sep_w;
            }
        }
        // Right-side icon buttons.
        let mut bx = w - kit_ui::MARGIN - 48;
        icon_button(
            c,
            t,
            hits,
            Rect::new(bx, bar_y, 44, PATH_BAR_H),
            if self.grid { "\u{e8ef}" } else { "\u{e9b0}" },
            Hit::ViewBtn,
            false,
        );
        bx -= 52;
        icon_button(
            c,
            t,
            hits,
            Rect::new(bx, bar_y, 44, PATH_BAR_H),
            "\u{e164}",
            Hit::SortBtn,
            self.menu_open,
        );
        bx -= 52;
        icon_button(
            c,
            t,
            hits,
            Rect::new(bx, bar_y, 44, PATH_BAR_H),
            "\u{e8b6}",
            Hit::SearchBtn,
            self.search_open || !self.search.s.is_empty(),
        );

        // ---- search row ----
        let mut top = bar_y + PATH_BAR_H + 8;
        if self.search_open {
            let r = Rect::new(
                kit_ui::MARGIN,
                top,
                w - kit_ui::MARGIN * 2 - 44,
                SEARCH_H - 8,
            );
            c.fill_rounded_rect(r, (r.h as f32) / 2.0, t.surface1);
            draw_icon(c, "\u{e8b6}", 22.0, t.text3, r.x + 26, r.y + r.h / 2);
            let text = if self.search.s.is_empty() {
                "Search files".to_string()
            } else {
                self.search.s.clone()
            };
            let col = if self.search.s.is_empty() {
                t.text3
            } else {
                t.text
            };
            kit_ui::draw_text(
                c,
                &text,
                &kit_ui::styled(20.0, 400, col),
                r.x + 50,
                r.y + (r.h - 26).max(0) / 2,
                Some(r.w - 90),
            );
            hit_rect(hits, r, Hit::SearchField);
            if !self.search.s.is_empty() {
                let cr = Rect::new(r.right() - 40, r.y + 4, 36, r.h - 8);
                c.fill_rounded_rect(cr, (cr.h as f32) / 2.0, t.surface2);
                draw_icon(
                    c,
                    "\u{e5cd}",
                    18.0,
                    t.text,
                    cr.x + cr.w / 2,
                    cr.y + cr.h / 2,
                );
                hit_rect(hits, cr, Hit::SearchClear);
            }
            top += SEARCH_H;
        }

        // ---- picker banner ----
        if let Some((kind, srcs)) = &self.pending_op {
            let r = Rect::new(kit_ui::MARGIN, top, w - kit_ui::MARGIN * 2, BANNER_H - 8);
            c.fill_rounded_rect(r, 18.0, t.surface1);
            kit_ui::draw_text(
                c,
                &format!(
                    "{} {} item{} — pick a folder",
                    kind.verb(),
                    srcs.len(),
                    if srcs.len() == 1 { "" } else { "s" }
                ),
                &kit_ui::label(t.text),
                r.x + 20,
                r.y + 12,
                Some(r.w - 320),
            );
            kit_ui::draw_text(
                c,
                &self.cwd.display().to_string(),
                &kit_ui::caption(t.text3),
                r.x + 20,
                r.y + 34,
                Some(r.w - 320),
            );
            let bw = 120;
            let paste = Rect::new(r.right() - bw - 16 - 96, r.y + 10, bw, r.h - 20);
            kit_ui::draw_pill_button(
                c,
                t,
                paste,
                "Paste here",
                true,
                self.pressed_hit.as_ref() == Some(&Hit::Paste),
            );
            hit_rect(hits, paste, Hit::Paste);
            let cancel = Rect::new(paste.x - 104, r.y + 10, 96, r.h - 20);
            kit_ui::draw_pill_button(c, t, cancel, "Cancel", false, false);
            hit_rect(hits, cancel, Hit::PickerCancel);
            top += BANNER_H;
        }

        // ---- content viewport ----
        self.view_top = top + 8;
        let bottom_reserved =
            kit_ui::BOTTOM_SAFE + FOOTER_H + if self.select_mode { ACTBAR_H + 8 } else { 0 };
        self.view_h = (h - bottom_reserved - self.view_top).max(0);
        self.clamp_scroll();

        // Edge-swipe parallax: content slides right with the finger.
        let ox = self.nav_dx;

        if self.load_error.is_some() {
            self.render_error(c, t, hits);
        } else if self.items.is_empty() {
            self.render_empty(c, t);
        } else {
            self.render_items(c, t, ox);
            self.render_scrollbar(c, t);
        }

        // ---- storage footer ----
        self.render_footer(c, t);

        // ---- FAB ----
        if !self.select_mode && self.pending_op.is_none() && self.dialog.is_none() {
            let d = 56;
            let r = Rect::new(
                w - kit_ui::MARGIN - d,
                h - kit_ui::BOTTOM_SAFE - FOOTER_H - d - 16,
                d,
                d,
            );
            c.fill_rounded_rect(r, (d as f32) / 2.0, t.accent);
            draw_icon(
                c,
                "\u{e2cc}",
                28.0,
                t.text_on_accent,
                r.x + d / 2,
                r.y + d / 2,
            );
            hit_rect(hits, r, Hit::Fab);
        }

        // ---- selection action bar ----
        if self.select_mode {
            self.render_action_bar(c, t, hits);
        }

        // ---- status toast ----
        if let Some((msg, is_err)) = &self.status {
            let col = if *is_err { t.danger } else { t.success };
            let st = kit_ui::styled(18.0, 500, col);
            let max_w = w - kit_ui::MARGIN * 2 - 40;
            let Some(px) = text::label(msg, st, Some(max_w)) else {
                return;
            };
            let tw = px.width() as i32 + 40;
            let th = 44;
            let r = Rect::new(
                (w - tw) / 2,
                h - kit_ui::BOTTOM_SAFE - FOOTER_H - th - 76,
                tw,
                th,
            );
            c.fill_rounded_rect(r, 22.0, t.surface1);
            c.blit(
                &de_api::Pixels::new(
                    de_api::Size::new(px.width() as i32, px.height() as i32),
                    px.data().to_vec(),
                ),
                Point::new(r.x + 20, r.y + (th - px.height() as i32) / 2),
            );
        }
    }

    /// Breadcrumb labels + jump targets, root→leaf.
    fn crumb_segments(&self) -> Vec<(String, PathBuf)> {
        let mut segs: Vec<(String, PathBuf)> = Vec::new();
        let mut cur = PathBuf::from("/");
        segs.push(("/".to_string(), cur.clone()));
        for comp in self.cwd.components() {
            use std::path::Component;
            match comp {
                Component::RootDir => {}
                Component::Normal(os) => {
                    cur.push(os);
                    let label = if cur == self.home {
                        "Home".to_string()
                    } else {
                        os.to_string_lossy().into_owned()
                    };
                    segs.push((label, cur.clone()));
                }
                _ => {}
            }
        }
        // De-duplicate the leading "/" when cwd == "/".
        if self.cwd == Path::new("/") {
            segs.truncate(1);
        } else if segs.len() > 1 && segs[0].1 == Path::new("/") {
            // keep "/" crumb + each component
        }
        segs
    }

    fn render_error(&self, c: &mut Canvas, t: &Theme, hits: &mut Vec<(Rect, Hit)>) {
        let cx = self.cur_w / 2;
        let cy = self.view_top + self.view_h / 2;
        draw_icon(c, "\u{e002}", 56.0, t.danger, cx, cy - 60);
        kit_ui::draw_centered(
            c,
            "Can't open this folder",
            &kit_ui::label(t.text),
            Rect::new(0, cy, self.cur_w, 40),
        );
        let msg = self.load_error.clone().unwrap_or_default();
        kit_ui::draw_centered(
            c,
            &msg,
            &kit_ui::caption(t.text3),
            Rect::new(0, cy + 44, self.cur_w, 40),
        );
        let bw = 160;
        let r = Rect::new(cx - bw / 2, cy + 96, bw, 52);
        kit_ui::draw_pill_button(c, t, r, "Retry", true, false);
        hit_rect(hits, r, Hit::Retry);
    }

    fn render_empty(&self, c: &mut Canvas, t: &Theme) {
        let cx = self.cur_w / 2;
        let cy = self.view_top + self.view_h / 2;
        draw_icon(c, "\u{e2c7}", 56.0, t.text3, cx, cy - 60);
        let (title, sub) = if !self.search.s.is_empty() {
            (
                "No matches".to_string(),
                format!("Nothing matches “{}”", self.search.s),
            )
        } else if !self.show_hidden {
            (
                "Folder is empty".to_string(),
                "Hidden files are not shown".to_string(),
            )
        } else {
            ("Folder is empty".to_string(), String::new())
        };
        kit_ui::draw_centered(
            c,
            &title,
            &kit_ui::label(t.text),
            Rect::new(0, cy, self.cur_w, 40),
        );
        if !sub.is_empty() {
            kit_ui::draw_centered(
                c,
                &sub,
                &kit_ui::caption(t.text3),
                Rect::new(0, cy + 44, self.cur_w, 40),
            );
        }
    }

    fn render_items(&self, c: &mut Canvas, t: &Theme, ox: i32) {
        let view_top = self.view_top as f32;
        let view_h = self.view_h as f32;
        let scroll = self.scroll;
        // Visible index range.
        let first = if self.grid {
            let row = (scroll / (lay::CELL_H + lay::GAP) as f32) as usize;
            row.saturating_sub(1) * lay::GRID_COLS
        } else {
            ((scroll / (lay::ROW_H + lay::GAP) as f32) as usize).saturating_sub(1)
        };
        let last = if self.grid {
            (((scroll + view_h) / (lay::CELL_H + lay::GAP) as f32) as usize + 1) * lay::GRID_COLS
        } else {
            ((scroll + view_h) / (lay::ROW_H + lay::GAP) as f32) as usize + 1
        };
        for idx in first..last.min(self.items.len()) {
            let (y, h_cell) = if self.grid {
                (lay::cell_y(idx, scroll) + view_top, lay::CELL_H as f32)
            } else {
                (lay::row_y(idx, scroll) + view_top, lay::ROW_H as f32)
            };
            if y + h_cell < view_top || y > view_top + view_h {
                continue;
            }
            if self.grid {
                self.render_cell(c, t, idx, y as i32, ox);
            } else {
                self.render_row(c, t, idx, y as i32, ox);
            }
        }
    }

    fn render_row(&self, c: &mut Canvas, t: &Theme, idx: usize, y: i32, ox: i32) {
        let x = lay::PAD + ox;
        let w = self.cur_w - lay::PAD * 2;
        let r = Rect::new(x, y, w, lay::ROW_H);
        let item = self.items[idx];
        let (icon_kind, name, sub, is_sel, is_link) = self.item_info(item);
        let pressed = self.pressed_item == Some(idx) && self.pressed_hit.is_none();

        // Row surface.
        c.fill_rounded_rect(r, 16.0, t.surface1);
        if is_sel {
            c.fill_rounded_rect(r, 16.0, alpha(t.accent_dim, 90));
        } else if pressed {
            c.fill_rounded_rect(r, 16.0, kit_ui::state_layer(t, 0.08));
        }

        let mut cx = x + 16;
        // Checkbox in selection mode.
        if self.select_mode {
            let cb = Rect::new(cx, y + (lay::ROW_H - 26) / 2, 26, 26);
            c.fill_rounded_rect(cb, 7.0, if is_sel { t.accent } else { t.surface3 });
            if is_sel {
                draw_icon(c, "\u{e5ca}", 18.0, t.text_on_accent, cb.x + 13, cb.y + 13);
            }
            cx += 40;
        }
        // Kind icon tile.
        let tile = Rect::new(cx, y + 12, 40, lay::ROW_H - 24);
        c.fill_rounded_rect(tile, 12.0, alpha(t.accent_dim, 60));
        let glyph_col = if item == Item::Parent {
            t.text3
        } else {
            t.accent
        };
        draw_icon(
            c,
            if item == Item::Parent {
                "\u{e5d8}"
            } else {
                kind_icon(icon_kind)
            },
            24.0,
            glyph_col,
            tile.x + tile.w / 2,
            tile.y + tile.h / 2,
        );
        cx += tile.w + 16;
        let text_w = r.right() - 16 - cx;
        kit_ui::draw_text(
            c,
            &name,
            &kit_ui::styled(21.0, if item == Item::Parent { 500 } else { 450 }, t.text),
            cx,
            y + 12,
            Some(text_w),
        );
        let sub_line = if is_link {
            format!("{sub} · link")
        } else {
            sub
        };
        kit_ui::draw_text(
            c,
            &sub_line,
            &kit_ui::caption(t.text3),
            cx,
            y + 38,
            Some(text_w),
        );
        // Trailing modified date for files.
        if let Item::Ent(i) = item {
            if let Some(e) = self.vis.get(i) {
                if !e.is_dir {
                    let date = model::format_time(e.mtime);
                    let st = kit_ui::styled(15.0, 400, t.text3);
                    let max_w = 150;
                    if let Some(px) = text::label(&date, st, Some(max_w)) {
                        c.blit(
                            &de_api::Pixels::new(
                                de_api::Size::new(px.width() as i32, px.height() as i32),
                                px.data().to_vec(),
                            ),
                            Point::new(
                                r.right() - 16 - px.width() as i32,
                                y + (lay::ROW_H - px.height() as i32) / 2,
                            ),
                        );
                    }
                }
            }
        }
    }

    fn render_cell(&self, c: &mut Canvas, t: &Theme, idx: usize, y: i32, ox: i32) {
        let x = lay::cell_x(idx, self.cur_w) + ox;
        let cw = lay::cell_w(self.cur_w);
        let r = Rect::new(x, y, cw, lay::CELL_H);
        let item = self.items[idx];
        let (icon_kind, name, _sub, is_sel, _link) = self.item_info(item);
        let pressed = self.pressed_item == Some(idx) && self.pressed_hit.is_none();

        if is_sel {
            c.fill_rounded_rect(r, 18.0, alpha(t.accent_dim, 90));
        } else if pressed {
            c.fill_rounded_rect(r, 18.0, kit_ui::state_layer(t, 0.08));
        }

        // Checkbox overlay top-left in selection mode.
        if self.select_mode {
            let cb = Rect::new(x + 8, y + 8, 26, 26);
            c.fill_rounded_rect(
                cb,
                8.0,
                if is_sel {
                    t.accent
                } else {
                    alpha(t.surface3, 230)
                },
            );
            if is_sel {
                draw_icon(c, "\u{e5ca}", 18.0, t.text_on_accent, cb.x + 13, cb.y + 13);
            }
        }

        let icon_sz = 52.0;
        let cy = y + 18 + (icon_sz as i32) / 2;
        let glyph = if item == Item::Parent {
            "\u{e5d8}"
        } else {
            kind_icon(icon_kind)
        };
        let col = if item == Item::Parent {
            t.text3
        } else {
            t.text
        };
        draw_icon(c, glyph, icon_sz, col, x + cw / 2, cy);
        kit_ui::draw_centered(
            c,
            &name,
            &kit_ui::styled(17.0, 500, t.text),
            Rect::new(x + 6, y + 84, cw - 12, 40),
        );
        kit_ui::draw_centered(
            c,
            &_sub,
            &kit_ui::caption(t.text3),
            Rect::new(x + 6, y + 122, cw - 12, 30),
        );
    }

    /// Name/subtitle/kind for an item (parent row synthesizes "..").
    fn item_info(&self, item: Item) -> (FileKind, String, String, bool, bool) {
        match item {
            Item::Parent => (
                FileKind::Folder,
                "..".to_string(),
                "Parent folder".to_string(),
                false,
                false,
            ),
            Item::Ent(i) => {
                let e = &self.vis[i];
                let sub = if e.is_dir {
                    "Folder".to_string()
                } else {
                    format!(
                        "{} · {}",
                        model::format_size(e.size),
                        model::format_time(e.mtime)
                    )
                };
                let sel = self.selection.contains(&e.path);
                (e.kind, e.name.clone(), sub, sel, e.is_link)
            }
        }
    }

    fn render_scrollbar(&self, c: &mut Canvas, t: &Theme) {
        let max = self.max_scroll();
        if max <= 0 || self.view_h <= 0 {
            return;
        }
        let content = lay::content_h(self.content_count(), self.grid);
        let track_h = self.view_h;
        let thumb_h = ((track_h as i64 * track_h as i64) / content.max(1) as i64) as i32;
        let thumb_h = thumb_h.clamp(48, track_h);
        let travel = track_h - thumb_h;
        let ty = self.view_top + (travel as f32 * (self.scroll / max as f32)) as i32;
        let r = Rect::new(self.cur_w - 8, ty, 5, thumb_h);
        c.fill_rounded_rect(r, 2.5, alpha(t.text3, 110));
    }

    fn render_footer(&self, c: &mut Canvas, t: &Theme) {
        let y = self.cur_h - kit_ui::BOTTOM_SAFE - FOOTER_H;
        let Some((free, total)) = model::free_space(&self.cwd) else {
            return;
        };
        let used_pct = if total > 0 {
            ((total - free) as f32 / total as f32 * 100.0) as i32
        } else {
            0
        };
        let label = format!(
            "{} free of {} · {}% used",
            model::format_size(free),
            model::format_size(total),
            used_pct
        );
        kit_ui::draw_text(
            c,
            &label,
            &kit_ui::caption(t.text3),
            kit_ui::MARGIN,
            y + (FOOTER_H - 20).max(0) / 2,
            Some(self.cur_w - kit_ui::MARGIN * 2 - 140),
        );
        // Usage bar, right side.
        let bw = 120;
        let bx = self.cur_w - kit_ui::MARGIN - bw;
        let track = Rect::new(bx, y + FOOTER_H / 2 - 4, bw, 8);
        c.fill_rounded_rect(track, 4.0, t.surface3);
        let fillw = (bw as f32 * (used_pct.min(100) as f32 / 100.0)) as i32;
        if fillw > 0 {
            let fr = Rect::new(bx, track.y, fillw, track.h);
            c.fill_rounded_rect(fr, 4.0, if used_pct > 90 { t.danger } else { t.accent });
        }
    }

    fn render_action_bar(&self, c: &mut Canvas, t: &Theme, hits: &mut Vec<(Rect, Hit)>) {
        let w = self.cur_w;
        let y = self.cur_h - kit_ui::BOTTOM_SAFE - FOOTER_H - ACTBAR_H - 8;
        let r = Rect::new(kit_ui::MARGIN, y, w - kit_ui::MARGIN * 2, ACTBAR_H);
        c.fill_rounded_rect(r, 20.0, t.surface1);
        // Count + Done on the left.
        let n = self.selection.len();
        kit_ui::draw_text(
            c,
            &format!("{n} selected"),
            &kit_ui::styled(18.0, 600, t.text),
            r.x + 20,
            r.y + 14,
            Some(160),
        );
        let single = n == 1;
        // Fixed order left→right after the count.
        let mut ax = r.x + 150;
        for (act, label, enabled) in [
            (SelAct::Open, "Open", single),
            (SelAct::Rename, "Rename", single),
            (SelAct::Copy, "Copy", true),
            (SelAct::Move, "Move", true),
            (SelAct::Delete, "Delete", true),
            (SelAct::Info, "Info", single),
            (SelAct::Done, "Done", true),
        ] {
            let bw = 68;
            let br = Rect::new(ax, r.y + 14, bw, ACTBAR_H - 28);
            c.fill_rounded_rect(br, 12.0, if enabled { t.surface2 } else { t.surface1 });
            let col = if enabled { t.text } else { t.text3 };
            kit_ui::draw_centered(c, label, &kit_ui::styled(16.0, 600, col), br);
            if enabled {
                hit_rect(hits, br, Hit::Sel(act));
            }
            ax += bw + 8;
        }
    }

    fn render_menu(&mut self, c: &mut Canvas, t: &Theme, hits: &mut Vec<(Rect, Hit)>) {
        // Panel under the sort button (top-right area).
        let w = 280;
        let x = self.cur_w - kit_ui::MARGIN - 48 - 3 * 52 - w;
        let y = kit_ui::TOP_SAFE + 8 + PATH_BAR_H + 8;
        let rows = SortKey::ALL.len() + 2; // + hidden toggle + spacer
        let h = rows as i32 * 48 + 16;
        let r = Rect::new(x, y, w, h);
        self.menu_rect = r;
        c.fill_rounded_rect(r, 20.0, t.surface1);
        let mut ry = r.y + 8;
        for k in SortKey::ALL {
            let row = Rect::new(r.x + 8, ry, r.w - 16, 44);
            kit_ui::draw_text(
                c,
                k.label(),
                &kit_ui::styled(19.0, 450, t.text),
                row.x + 16,
                row.y + 11,
                Some(row.w - 60),
            );
            if self.key == k {
                draw_icon(c, "\u{e5ca}", 20.0, t.accent, row.right() - 26, row.y + 22);
            }
            hit_rect(hits, row, Hit::MenuSort(k));
            ry += 48;
        }
        // separator + hidden toggle
        c.fill_rect(Rect::new(r.x + 16, ry + 4, r.w - 32, 1), t.line);
        ry += 8;
        let row = Rect::new(r.x + 8, ry, r.w - 16, 44);
        kit_ui::draw_text(
            c,
            "Show hidden files",
            &kit_ui::styled(19.0, 450, t.text),
            row.x + 16,
            row.y + 11,
            Some(row.w - 80),
        );
        kit_ui::draw_switch(c, t, row.right() - 64, row.y + 6, self.show_hidden);
        hit_rect(hits, row, Hit::MenuHidden);
    }

    fn render_dialog(&self, c: &mut Canvas, t: &Theme, hits: &mut Vec<(Rect, Hit)>, d: &Dialog) {
        let w = self.cur_w;
        let h = self.cur_h;
        c.fill_rect(Rect::new(0, 0, w, h), t.scrim);
        let cw = (w - 64).min(560);
        let (ch, title) = dialog_metrics(d);
        let r = Rect::new((w - cw) / 2, (h - ch) / 2, cw, ch);
        c.fill_rounded_rect(r, 28.0, t.surface1);

        let mut y = r.y + 24;
        kit_ui::draw_text(c, title, &kit_ui::title(t.text), r.x + 28, y, Some(cw - 56));
        y += 52;
        match d {
            Dialog::NewFolder | Dialog::Rename(_) => {
                let fr = Rect::new(r.x + 24, y, cw - 48, 56);
                c.fill_rounded_rect(fr, 14.0, t.surface2);
                let shown = if self.text.s.is_empty() {
                    "Name".to_string()
                } else {
                    self.text.s.clone()
                };
                let col = if self.text.s.is_empty() {
                    t.text3
                } else {
                    t.text
                };
                kit_ui::draw_text(
                    c,
                    &shown,
                    &kit_ui::styled(22.0, 400, col),
                    fr.x + 18,
                    fr.y + 15,
                    Some(fr.w - 36),
                );
                hit_rect(hits, fr, Hit::DlgField);
            }
            Dialog::Delete { paths, items } => {
                let names: Vec<String> = paths
                    .iter()
                    .take(3)
                    .map(|p| {
                        p.file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_else(|| p.display().to_string())
                    })
                    .collect();
                let mut msg = if paths.len() == 1 {
                    format!("Delete “{}”?", names[0])
                } else {
                    format!("Delete {} items?", paths.len())
                };
                if *items > paths.len() {
                    msg.push_str(&format!(" ({items} files inside)"));
                }
                msg.push_str("\nThis can't be undone.");
                for line in msg.lines() {
                    kit_ui::draw_text(c, line, &kit_ui::body(t.text2), r.x + 28, y, Some(cw - 56));
                    y += 34;
                }
            }
            Dialog::Details(path) => {
                self.render_details_body(c, t, r, path, &mut y);
            }
        }
        // Buttons.
        let by = r.bottom() - 68;
        let bw = 140;
        let cancel = Rect::new(r.right() - 24 - bw * 2 - 12, by, bw, 48);
        let ok_label = match d {
            Dialog::Details(_) => "Close",
            Dialog::Delete { .. } => "Delete",
            _ => "OK",
        };
        let ok = Rect::new(r.right() - 24 - bw, by, bw, 48);
        kit_ui::draw_pill_button(c, t, cancel, "Cancel", false, false);
        hit_rect(hits, cancel, Hit::DlgCancel);
        kit_ui::draw_pill_button(c, t, ok, ok_label, !matches!(d, Dialog::Details(_)), false);
        hit_rect(hits, ok, Hit::DlgOk);
    }

    fn render_details_body(&self, c: &mut Canvas, t: &Theme, card: Rect, path: &Path, y: &mut i32) {
        let meta = std::fs::metadata(path).ok();
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        let is_dir = meta.as_ref().map(|m| m.is_dir()).unwrap_or(false);
        let link = std::fs::symlink_metadata(path)
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false);
        let rows: Vec<(String, String)> = {
            let mut v = vec![
                ("Name".into(), name.clone()),
                ("Path".into(), path.display().to_string()),
            ];
            if let Some(m) = &meta {
                use std::os::unix::fs::MetadataExt;
                v.push((
                    "Type".into(),
                    if is_dir {
                        "Folder".to_string()
                    } else {
                        model::kind_of(&name, false).label().to_string()
                    },
                ));
                if link {
                    if let Ok(target) = std::fs::read_link(path) {
                        v.push(("Link to".into(), target.display().to_string()));
                    }
                }
                if is_dir {
                    match model::dir_stats(path) {
                        Ok((n, bytes)) => {
                            let cnt = if n >= DETAIL_ITEM_CAP {
                                format!("{DETAIL_ITEM_CAP}+")
                            } else {
                                n.to_string()
                            };
                            v.push((
                                "Contents".into(),
                                format!("{cnt} items · {}", model::format_size(bytes)),
                            ));
                        }
                        Err(e) => v.push(("Contents".into(), format!("error: {e}"))),
                    }
                } else {
                    v.push(("Size".into(), model::format_size(m.len())));
                }
                v.push((
                    "Modified".into(),
                    model::format_time(
                        m.modified()
                            .ok()
                            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                            .map(|d| d.as_secs() as i64)
                            .unwrap_or(0),
                    ),
                ));
                v.push((
                    "Permissions".into(),
                    format!(
                        "{} ({:o})",
                        model::mode_string(m.mode() & 0o777),
                        m.mode() & 0o7777
                    ),
                ));
            } else {
                v.push(("Error".into(), "metadata unavailable".into()));
            }
            v
        };
        for (k, val) in rows {
            if *y > card.bottom() - 90 {
                break;
            }
            kit_ui::draw_text(c, &k, &kit_ui::caption(t.text3), card.x + 28, *y, Some(120));
            kit_ui::draw_text(
                c,
                &val,
                &kit_ui::body(t.text),
                card.x + 160,
                *y,
                Some(card.w - 200),
            );
            *y += 36;
        }
    }
}
