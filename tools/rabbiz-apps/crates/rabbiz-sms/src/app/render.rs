use de_api::Pixels;
use rabbiz_app_kit::text::{self, LabelStyle};
use rabbiz_app_kit::{icons, ui, widgets};
use rabbiz_app_kit::{Canvas, Point, Rect, Size, Theme};

use crate::model::{
    day_label, format_time, local_time, same_run, state_label, thread_key, Message,
};

use super::{
    Focus, Hit, SmsApp, View, BUBBLE_MAX_PCT, BUBBLE_PAD_X, BUBBLE_PAD_Y, DAY_SEP_H, FIELD_LINE_H,
    GUTTER, ROW_H, RUN_GAP, RUN_SEP, SEND_W, STATE_H, TIME_CAPTION_H,
};

// ---- Cached text rasterization ----

/// Rasterize `s` through the shared LRU label cache. `max_lines ==
/// usize::MAX` means "wrap, don't clamp".
fn label_px(s: &str, st: LabelStyle, max_w: Option<i32>, max_lines: usize) -> Option<Pixels> {
    use text::cache::{to_pixels, with_cache, LabelKey};
    let key = LabelKey {
        text: s.to_string(),
        family: st.family,
        size_px: st.size as u32,
        line_height_px: st.line_height as u32,
        weight: st.weight,
        color: st.color,
        max_width_px: max_w.map(|w| w.max(1) as u32),
        max_lines,
    };
    with_cache(|c| {
        c.get_or_insert_with(key, || match (max_w, max_lines) {
            (Some(w), n) if n != usize::MAX => text::label_clamped(s, st, w, n),
            (Some(w), _) => text::label(s, st, Some(w)),
            (None, _) => text::label(s, st, None),
        })
    })
    .map(|px| to_pixels(&px))
}

fn blit_label(
    c: &mut Canvas,
    s: &str,
    st: LabelStyle,
    x: i32,
    y: i32,
    max_w: Option<i32>,
    max_lines: usize,
) {
    if let Some(px) = label_px(s, st, max_w, max_lines) {
        c.blit(&px, Point::new(x, y));
    }
}

// ---- Pure layout math ----

fn clamp_scroll(scroll: i32, max_scroll: i32) -> i32 {
    scroll.clamp(0, max_scroll.max(0))
}

fn bubble_width(text_w: i32, max_bubble_w: i32) -> i32 {
    (text_w + BUBBLE_PAD_X * 2).min(max_bubble_w).max(48)
}

/// Compose field height for `lines` of wrapped text (1..=3).
fn field_height(lines: i32) -> i32 {
    (lines * (FIELD_LINE_H + 6) + 16).clamp(56, 140)
}

// ---- Thread layout types ----

/// A laid-out element in thread content coordinates (y measured from the top
/// of the scrollable content; the viewport transform happens at draw time).
enum Elem {
    Day {
        y: i32,
        label: String,
    },
    Time {
        y: i32,
        right: bool,
        label: String,
    },
    Bubble {
        y: i32,
        msg: usize,
        right: bool,
        w: i32,
        h: i32,
    },
}

struct ThreadLayout {
    elems: Vec<Elem>,
    content_h: i32,
}

fn elem_y(e: &Elem) -> i32 {
    match e {
        Elem::Day { y, .. } | Elem::Time { y, .. } | Elem::Bubble { y, .. } => *y,
    }
}

fn elem_h(e: &Elem) -> i32 {
    match e {
        Elem::Day { .. } => DAY_SEP_H,
        Elem::Time { .. } => TIME_CAPTION_H,
        Elem::Bubble { h, .. } => *h,
    }
}

impl SmsApp {
    pub(super) fn render_frame(&mut self, canvas: &mut Canvas, theme: &Theme) {
        let size = canvas.size();
        let w = size.w;
        let h = size.h;

        canvas.fill_rect(Rect::new(0, 0, w, h), theme.bg);
        if self.view == View::List {
            self.render_list(canvas, theme, w, h);
            return;
        }
        match self.transition.as_ref().map(|t| (t.progress(), t.exit)) {
            None => self.render_thread(canvas, theme, w, h),
            Some((p, exit)) => {
                // Slide distance in px; 0 means "settled, draw directly".
                let off = if exit {
                    (p * w as f32) as i32
                } else {
                    ((1.0 - p) * w as f32) as i32
                };
                if off <= 0 {
                    self.render_thread(canvas, theme, w, h);
                } else {
                    self.render_list(canvas, theme, w, h);
                    let mut tc = Canvas::filled(Size::new(w, h), theme.bg);
                    self.render_thread(&mut tc, theme, w, h);
                    canvas.blit(&tc.to_pixels(), Point::new(off, 0));
                }
            }
        }
    }

    // ================= thread layout =================

    /// Lay out the open thread's messages into content coordinates.
    /// `view_w`/`view_h` describe the message viewport in px.
    fn layout_thread(&self, theme: &Theme, view_w: i32, view_h: i32) -> ThreadLayout {
        let content_w = view_w - GUTTER * 2;
        let max_bubble_w = content_w * BUBBLE_MAX_PCT / 100;
        let max_text_w = max_bubble_w - BUBBLE_PAD_X * 2;
        // Measure with the incoming-bubble color; metrics are color-independent.
        let body_style = ui::styled(23.0, 400, theme.text);
        let now = Self::now_ts();

        let mut elems: Vec<Elem> = Vec::new();
        let mut y = 8;
        let mut prev: Option<&Message> = None;
        // Index of the next message in this thread (for run-gap decisions).
        let mut iter = self
            .msgs
            .iter()
            .enumerate()
            .filter(|(_, m)| thread_key(&m.peer) == self.peer_key)
            .peekable();

        while let Some((mi, m)) = iter.next() {
            let new_day = prev.is_none_or(|p| {
                let (a, b) = (local_time(p.ts), local_time(m.ts));
                a.year != b.year || a.yday != b.yday
            });
            if new_day {
                elems.push(Elem::Day {
                    y,
                    label: day_label(m.ts, now),
                });
                y += DAY_SEP_H;
            }
            let new_run = prev.is_none_or(|p| !same_run(p, m));
            if new_run {
                elems.push(Elem::Time {
                    y,
                    right: m.outgoing,
                    label: format_time(m.ts),
                });
                y += TIME_CAPTION_H;
            }
            let (tw, th) = label_px(&m.body, body_style, Some(max_text_w), usize::MAX)
                .map(|px| (px.size.w, px.size.h))
                .unwrap_or((max_text_w / 2, body_style.line_height as i32));
            let bw = bubble_width(tw, max_bubble_w);
            let extra = if m.outgoing { STATE_H } else { 0 };
            let bh = th + BUBBLE_PAD_Y * 2 + extra;
            elems.push(Elem::Bubble {
                y,
                msg: mi,
                right: m.outgoing,
                w: bw,
                h: bh,
            });
            y += bh;
            let next_same_run = iter.peek().is_some_and(|(_, n)| same_run(m, n));
            y += if next_same_run { RUN_GAP } else { RUN_SEP };
            prev = Some(m);
        }
        let _ = view_h;

        ThreadLayout {
            elems,
            content_h: y,
        }
    }

    // ================= list view =================

    fn render_list(&mut self, canvas: &mut Canvas, theme: &Theme, w: i32, h: i32) {
        let header_end = ui::draw_header(canvas, theme, "Messages", "");
        ui::divider_inset(canvas, theme, ui::MARGIN, w - ui::MARGIN, header_end);

        let bottom = h - ui::BOTTOM_SAFE;
        let control_height = 56;
        let compose = Rect::new(
            w - ui::MARGIN - 180,
            bottom - control_height - 12,
            180,
            control_height,
        );
        self.new_msg_rect = compose;
        let area = Rect::new(0, header_end + 8, w, (compose.y - 20 - header_end).max(0));
        self.list_area = area;
        self.row_rects.clear();

        let threads = self.threads();
        let content_h = threads.len() as i32 * ROW_H;
        self.list_scroll = clamp_scroll(self.list_scroll, content_h - area.h);
        if !area.is_empty() && threads.is_empty() {
            widgets::EmptyState::render(
                canvas,
                area,
                theme,
                icons::MESSAGE,
                "No conversations",
                "Start a conversation to send an SMS.",
            );
        } else if !area.is_empty() {
            // A viewport-sized canvas clips rows at the toolbar and compose action.
            let mut list = Canvas::filled(Size::new(area.w, area.h), theme.bg);
            let name_style = ui::styled(23.0, 600, theme.text);
            let preview_style = ui::caption(theme.text2);
            let time_style = ui::caption(theme.text2);
            let now = Self::now_ts();
            let today = local_time(now);
            let viewport = Rect::new(0, 0, area.w, area.h);
            let first = (self.list_scroll / ROW_H) as usize;
            for (i, (_, peer, last_idx)) in threads.iter().enumerate().skip(first) {
                let y = i as i32 * ROW_H - self.list_scroll;
                if y >= area.h {
                    break;
                }
                let row = Rect::new(0, y, area.w, ROW_H);
                let Some(visible) = row.intersect(viewport) else {
                    continue;
                };
                self.row_rects.push((i, visible.translate(area.x, area.y)));
                if self.pressed == Some(Hit::Row(i)) {
                    ui::press_overlay(&mut list, theme, visible, 0.0, 0.10);
                }

                let m = &self.msgs[*last_idx];
                let name = self.display_peer(peer);
                let avatar = Rect::new(ui::MARGIN, y + 20, 52, 52);
                let known = self.contacts.name_for(peer);
                widgets::avatar(
                    &mut list,
                    known.unwrap_or(""),
                    avatar,
                    ui::avatar_tint(known.unwrap_or(peer)),
                    theme.text,
                );
                let text_x = avatar.right() + 18;
                let text_right = row.right() - ui::MARGIN;
                let date = local_time(m.ts);
                let when = if date.year == today.year && date.yday == today.yday {
                    format_time(m.ts)
                } else {
                    day_label(m.ts, now)
                };
                let mut name_right = text_right;
                if let Some(px) = label_px(&when, time_style, Some((w / 4).max(1)), 1) {
                    name_right -= px.size.w + 12;
                    list.blit(&px, Point::new(text_right - px.size.w, y + 18));
                }
                blit_label(
                    &mut list,
                    &name,
                    name_style,
                    text_x,
                    y + 14,
                    Some((name_right - text_x).max(1)),
                    1,
                );
                blit_label(
                    &mut list,
                    &m.body,
                    preview_style,
                    text_x,
                    y + 48,
                    Some((text_right - text_x).max(1)),
                    1,
                );
                ui::divider_inset(&mut list, theme, text_x, text_right, row.bottom() - 1);
            }
            canvas.blit(&list.to_pixels(), Point::new(area.x, area.y));
            ui::scrollbar(
                canvas,
                theme,
                Rect::new(area.right() - 8, area.y, 6, area.h),
                self.list_scroll,
                content_h,
            );
        }

        ui::draw_pill_button(
            canvas,
            theme,
            compose,
            "New message",
            true,
            self.pressed == Some(Hit::NewMsg),
        );
        widgets::Toast::render(
            canvas,
            Size::new(w, h),
            ui::BOTTOM_SAFE + control_height + 24,
            theme,
            &self.status,
        );
    }

    // ================= thread view =================

    /// Compose-bar rects (absolute coords). Returns
    /// `(to_rect, body_rect, send_rect, msg_area)`.
    fn compose_layout(&self, theme: &Theme, w: i32, h: i32) -> (Rect, Rect, Rect, Rect) {
        let field_w = w - GUTTER * 2 - SEND_W - 12;
        let body_style = ui::styled(22.0, 400, theme.text);
        let lines = if self.body_draft.trim().is_empty() {
            1
        } else {
            label_px(&self.body_draft, body_style, Some(field_w - 40), 3)
                .map(|px| ((px.size.h + FIELD_LINE_H - 1) / FIELD_LINE_H).clamp(1, 3))
                .unwrap_or(1)
        };
        let field_h = field_height(lines);

        let bottom = h - ui::BOTTOM_SAFE - 12; // bottom edge of the compose block
        let send_r = Rect::new(w - GUTTER - SEND_W, bottom - field_h, SEND_W, field_h);
        let body_r = Rect::new(GUTTER, bottom - field_h, field_w, field_h);
        let header_end = ui::TOP_SAFE + widgets::TopAppBar::HEIGHT;
        let to_r = if self.is_new {
            Rect::new(GUTTER, header_end + 12, w - GUTTER * 2, 52)
        } else {
            Rect::new(0, 0, 0, 0)
        };
        let msg_top = header_end.max(to_r.bottom()) + 12;
        let msg_area = Rect::new(0, msg_top, w, (body_r.y - 18 - msg_top).max(0));
        (to_r, body_r, send_r, msg_area)
    }

    fn render_thread(&mut self, canvas: &mut Canvas, theme: &Theme, w: i32, h: i32) {
        let title = if self.is_new {
            // Compose for a known contact shows their name; otherwise the
            // generic "New message" caption.
            self.contacts
                .name_for(&self.to_draft)
                .map_or_else(|| "New message".to_string(), str::to_string)
        } else {
            self.display_peer(&self.peer_label)
        };
        let bar = widgets::TopAppBar::layout(Size::new(w, h), ui::TOP_SAFE);
        self.back_rect = widgets::TopAppBar::back_rect(bar);
        widgets::TopAppBar::render(canvas, bar, theme, &title, true, None);
        if self.pressed == Some(Hit::Back) {
            ui::press_overlay(canvas, theme, self.back_rect, 24.0, 0.10);
        }

        let (to_r, body_r, send_r, msg_area) = self.compose_layout(theme, w, h);
        self.to_rect = to_r;
        self.body_rect = body_r;
        self.send_rect = send_r;
        self.msg_area = msg_area;

        self.draw_messages(canvas, theme, w, msg_area);
        self.draw_compose(canvas, theme, to_r, body_r, send_r);
        widgets::Toast::render(
            canvas,
            Size::new(w, h),
            h - msg_area.bottom(),
            theme,
            &self.status,
        );
    }

    fn draw_messages(&mut self, canvas: &mut Canvas, theme: &Theme, w: i32, area: Rect) {
        let layout = self.layout_thread(theme, w, area.h);
        if layout.elems.is_empty() || area.h <= 0 {
            return;
        }
        let view_h = area.h;
        let max_scroll = (layout.content_h - view_h).max(0);
        self.scroll = clamp_scroll(self.scroll, max_scroll);
        // Content bottom-anchored when scroll == 0: an element at content
        // y = content_h lands on the viewport bottom.
        let top_idx = layout.content_h - view_h - self.scroll;

        let mut messages = Canvas::filled(Size::new(area.w, area.h), theme.bg);
        let cap_style = ui::caption(theme.text2);
        let state_style = ui::styled(16.0, 400, theme.text_on_accent);

        for elem in &layout.elems {
            let ey = elem_y(elem) - top_idx;
            if ey + elem_h(elem) <= 0 || ey >= area.h {
                continue; // culled
            }
            match elem {
                Elem::Day { label, .. } => {
                    if let Some(px) = label_px(label, cap_style, None, 1) {
                        messages.blit(&px, Point::new((w - px.size.w) / 2, ey + 4));
                    }
                }
                Elem::Time { right, label, .. } => {
                    if let Some(px) = label_px(label, cap_style, None, 1) {
                        let ex = if *right {
                            w - GUTTER - px.size.w
                        } else {
                            GUTTER
                        };
                        messages.blit(&px, Point::new(ex, ey));
                    }
                }
                Elem::Bubble {
                    msg,
                    right,
                    w: bw,
                    h: bh,
                    ..
                } => {
                    let m = &self.msgs[*msg];
                    let bx = if *right { w - GUTTER - bw } else { GUTTER };
                    let br = Rect::new(bx, ey, *bw, *bh);
                    let bg = if *right { theme.accent } else { theme.surface1 };
                    messages.fill_rounded_rect(br, 20.0, bg);
                    let fg = if *right {
                        theme.text_on_accent
                    } else {
                        theme.text
                    };
                    let max_tw = (w - GUTTER * 2) * BUBBLE_MAX_PCT / 100 - BUBBLE_PAD_X * 2;
                    let style = ui::styled(23.0, 400, fg);
                    if let Some(px) = label_px(&m.body, style, Some(max_tw), usize::MAX) {
                        messages.blit(&px, Point::new(bx + BUBBLE_PAD_X, ey + BUBBLE_PAD_Y));
                    }
                    if m.outgoing {
                        let sl = state_label(m.state);
                        if let Some(px) = label_px(sl, state_style, None, 1) {
                            messages.blit(
                                &px,
                                Point::new(
                                    bx + bw - BUBBLE_PAD_X - px.size.w,
                                    ey + bh - BUBBLE_PAD_Y - 18,
                                ),
                            );
                        }
                    }
                }
            }
        }
        canvas.blit(&messages.to_pixels(), Point::new(area.x, area.y));
    }

    fn draw_compose(
        &mut self,
        canvas: &mut Canvas,
        theme: &Theme,
        to_r: Rect,
        body_r: Rect,
        send_r: Rect,
    ) {
        if self.is_new && !to_r.is_empty() {
            let focused = self.focus == Focus::To;
            canvas.fill_rounded_rect(
                to_r,
                16.0,
                if focused {
                    theme.surface2
                } else {
                    theme.surface1
                },
            );
            let (shown, col) = if self.to_draft.is_empty() {
                ("To: phone number", theme.text2)
            } else {
                (self.to_draft.as_str(), theme.text)
            };
            blit_label(
                canvas,
                shown,
                ui::styled(22.0, 400, col),
                to_r.x + 18,
                to_r.y + 14,
                Some(to_r.w - 36),
                1,
            );
        }

        let focused = self.focus == Focus::Body;
        canvas.fill_rounded_rect(
            body_r,
            24.0,
            if focused {
                theme.surface2
            } else {
                theme.surface1
            },
        );
        let max_tw = body_r.w - 40;
        if self.body_draft.trim().is_empty() {
            blit_label(
                canvas,
                "Message",
                ui::styled(22.0, 400, theme.text3),
                body_r.x + 18,
                body_r.y + (body_r.h - 28) / 2,
                Some(max_tw),
                1,
            );
        } else {
            blit_label(
                canvas,
                &self.body_draft,
                ui::styled(22.0, 400, theme.text),
                body_r.x + 18,
                body_r.y + 12,
                Some(max_tw),
                3,
            );
        }

        let can_send = !self.body_draft.trim().is_empty()
            && (!self.is_new || !self.to_draft.trim().is_empty());
        ui::draw_pill_button(
            canvas,
            theme,
            send_r,
            "Send",
            can_send,
            self.pressed == Some(Hit::Send),
        );
    }
}

// ---- pure-logic tests ----
#[cfg(test)]
mod layout_tests {
    use super::*;

    #[test]
    fn clamp_scroll_bounds() {
        assert_eq!(clamp_scroll(-5, 100), 0);
        assert_eq!(clamp_scroll(50, 100), 50);
        assert_eq!(clamp_scroll(500, 100), 100);
        assert_eq!(clamp_scroll(10, 0), 0);
    }

    #[test]
    fn bubble_width_caps_at_max() {
        let max = 400;
        assert_eq!(bubble_width(1000, max), max);
        assert_eq!(bubble_width(50, max), 50 + BUBBLE_PAD_X * 2);
        assert!(bubble_width(10, max) >= 48);
    }

    #[test]
    fn field_height_grows_and_caps() {
        assert_eq!(field_height(1), 56);
        assert!(field_height(2) > field_height(1));
        assert_eq!(field_height(9), 140);
    }
}
