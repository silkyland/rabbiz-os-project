use de_api::Pixels;
use rabbiz_app_kit::text::{self, LabelStyle};
use rabbiz_app_kit::{icons, ui, widgets};
use rabbiz_app_kit::{Canvas, Point, Rect, Size, Theme};

use crate::model::{
    day_label, format_time, local_time, same_run, state_label, thread_key, Message, MsgState,
};

use super::{
    Focus, Hit, SmsApp, View, BUBBLE_MAX_PCT, BUBBLE_PAD_X, BUBBLE_PAD_Y, DAY_SEP_H, FIELD_LINE_H,
    GUTTER, ROW_H, RUN_GAP, RUN_SEP, TIME_CAPTION_H,
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

/// Bubble corner radius — large, M3 conversation shape.
const BUBBLE_R: f32 = 24.0;

/// Square off one corner of an already-rounded bubble by overpainting a
/// same-color patch — the canvas has one radius per rect, so the M3
/// "tail" corners (small/sharp) are faked by flattening the curve.
fn square_corner(c: &mut Canvas, r: Rect, col: [u8; 4], right: bool, bottom: bool) {
    const PATCH: i32 = 16;
    let x = if right { r.right() - PATCH } else { r.x };
    let y = if bottom { r.bottom() - PATCH } else { r.y };
    c.fill_rect(Rect::new(x, y, PATCH, PATCH), col);
}

fn draw_icon_centered(c: &mut Canvas, icon: &str, size: f32, col: [u8; 4], cx: i32, cy: i32) {
    if let Some(px) = text::icon(icon, size, col) {
        let w = px.width() as i32;
        let h = px.height() as i32;
        let p = de_api::Pixels::new(Size::new(w, h), px.data().to_vec());
        c.blit(&p, Point::new(cx - w / 2, cy - h / 2));
    }
}

// ---- Thread layout types ----

/// A laid-out element in thread content coordinates (y measured from the top
/// of the scrollable content; the viewport transform happens at draw time).
enum Elem {
    Day {
        y: i32,
        label: String,
    },
    /// Time+state caption under the LAST bubble of a run — replaces the
    /// detached floating timestamps of the old design.
    Caption {
        y: i32,
        right: bool,
        label: String,
        failed: bool,
    },
    Bubble {
        y: i32,
        msg: usize,
        right: bool,
        w: i32,
        h: i32,
        first_in_run: bool,
    },
}

struct ThreadLayout {
    elems: Vec<Elem>,
    content_h: i32,
}

fn elem_y(e: &Elem) -> i32 {
    match e {
        Elem::Day { y, .. } | Elem::Caption { y, .. } | Elem::Bubble { y, .. } => *y,
    }
}

fn elem_h(e: &Elem) -> i32 {
    match e {
        Elem::Day { .. } => DAY_SEP_H,
        Elem::Caption { .. } => TIME_CAPTION_H,
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
        // Collect this thread's messages once so next-in-run lookups are easy.
        let thread: Vec<(usize, &Message)> = self
            .msgs
            .iter()
            .enumerate()
            .filter(|(_, m)| thread_key(&m.peer) == self.peer_key)
            .collect();

        for (pos, (mi, m)) in thread.iter().enumerate() {
            let prev = pos.checked_sub(1).and_then(|p| thread.get(p).map(|t| t.1));
            let next = thread.get(pos + 1).map(|t| t.1);

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
            let first_in_run = prev.is_none_or(|p| !same_run(p, m));
            let last_in_run = next.is_none_or(|n| !same_run(m, n));
            if first_in_run && !new_day {
                y += RUN_SEP; // breathing room between runs
            }

            let (tw, th) = label_px(&m.body, body_style, Some(max_text_w), usize::MAX)
                .map(|px| (px.size.w, px.size.h))
                .unwrap_or((max_text_w, 28));
            let bw = bubble_width(tw, max_bubble_w);
            let bh = th + BUBBLE_PAD_Y * 2;
            elems.push(Elem::Bubble {
                y,
                msg: *mi,
                right: m.outgoing,
                w: bw,
                h: bh,
                first_in_run,
            });
            y += bh;

            if last_in_run {
                // Caption under the run: time for incoming, state + time for
                // outgoing — honest delivery states stay visible.
                let label = if m.outgoing {
                    format!("{} · {}", state_label(m.state), format_time(m.ts))
                } else {
                    format_time(m.ts)
                };
                y += 4;
                elems.push(Elem::Caption {
                    y,
                    right: m.outgoing,
                    label,
                    failed: m.outgoing && m.state == MsgState::Failed,
                });
                y += TIME_CAPTION_H;
            } else {
                y += RUN_GAP;
            }
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

        // SearchPill under the header — filters by name/number/body text.
        let pill = Rect::new(
            ui::MARGIN,
            header_end + 12,
            w - ui::MARGIN * 2,
            widgets::SearchPill::HEIGHT,
        );
        self.search_rect = pill;
        let clear_r = widgets::SearchPill::clear_rect(pill);
        self.search_clear_rect = clear_r;
        widgets::SearchPill::render(
            canvas,
            pill,
            theme,
            &self.query,
            "Search conversations",
            self.focus == Focus::Search,
        );

        let bottom = h - ui::BOTTOM_SAFE;
        // Circular compose FAB with a soft shadow — same family as the
        // dialer's keypad FAB.
        let fab_d = 88;
        let fab = Rect::new(
            w - ui::MARGIN - fab_d,
            bottom - fab_d - 20,
            fab_d,
            fab_d,
        );
        self.new_msg_rect = fab;
        let area = Rect::new(0, pill.bottom() + 8, w, (fab.y - 20 - pill.bottom() - 8).max(0));
        self.list_area = area;
        self.row_rects.clear();

        let threads = self.filtered_threads();
        let content_h = threads.len() as i32 * ROW_H;
        self.list_scroll = clamp_scroll(self.list_scroll, content_h - area.h);
        if !area.is_empty() && threads.is_empty() {
            let searching = !self.query.trim().is_empty();
            widgets::EmptyState::render(
                canvas,
                area,
                theme,
                icons::MESSAGE,
                if searching {
                    "No matches"
                } else {
                    "No conversations"
                },
                if searching {
                    "Try a different name, number or text"
                } else {
                    "Start a conversation to send an SMS."
                },
            );
        } else if !area.is_empty() {
            // A viewport-sized canvas clips rows at the toolbar and compose action.
            let mut list = Canvas::filled(Size::new(area.w, area.h), theme.bg);
            let now = Self::now_ts();
            let today = local_time(now);
            let viewport = Rect::new(0, 0, area.w, area.h);
            let first = (self.list_scroll / ROW_H) as usize;
            for (i, (key, peer, last_idx)) in threads.iter().enumerate().skip(first) {
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
                let unread = self.unread_in(key);
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
                // Unread: name/preview go bold+bright, the timestamp flips
                // to accent ink and a dot sits ahead of the preview.
                let hot = unread > 0;
                let name_style = ui::styled(
                    23.0,
                    if hot { 700 } else { 600 },
                    theme.text,
                );
                let preview_style = ui::caption(if hot { theme.text } else { theme.text2 });
                let mut name_right = text_right;
                let when_style = ui::caption(if hot { theme.accent } else { theme.text2 });
                if let Some(px) = label_px(&when, when_style, Some((w / 4).max(1)), 1) {
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
                // Preview: outgoing messages carry the "You: " prefix like
                // Google Messages; unread threads get an accent dot first.
                let mut preview_x = text_x;
                if hot {
                    let dot = Rect::new(text_x, y + 58, 12, 12);
                    list.fill_rounded_rect(dot, 6.0, theme.accent);
                    preview_x += 22;
                }
                let preview = if m.outgoing {
                    format!("You: {}", m.body)
                } else {
                    m.body.clone()
                };
                blit_label(
                    &mut list,
                    &preview,
                    preview_style,
                    preview_x,
                    y + 48,
                    Some((text_right - preview_x).max(1)),
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

        // Compose FAB: shadow disc + accent circle + edit glyph.
        let pressed_fab = self.pressed == Some(Hit::NewMsg);
        canvas.fill_rounded_rect(
            Rect::new(fab.x + 3, fab.y + 6, fab.w, fab.h),
            fab.h as f32 / 2.0,
            ui::with_alpha(theme.shadow, 0x30),
        );
        canvas.fill_rounded_rect(
            fab,
            fab.h as f32 / 2.0,
            if pressed_fab {
                ui::mix(theme.accent, [0, 0, 0, 255], 0.15)
            } else {
                theme.accent
            },
        );
        draw_icon_centered(
            canvas,
            icons::EDIT,
            34.0,
            theme.text_on_accent,
            fab.x + fab.w / 2,
            fab.y + fab.h / 2,
        );
        widgets::Toast::render(
            canvas,
            Size::new(w, h),
            ui::BOTTOM_SAFE + 112,
            theme,
            &self.status,
        );
    }

    // ================= thread view =================

    /// Compose-bar rects (absolute coords). Returns
    /// `(to_rect, body_rect, send_rect, msg_area)`.
    fn compose_layout(&self, theme: &Theme, w: i32, h: i32) -> (Rect, Rect, Rect, Rect) {
        let send_d = 64;
        let field_w = w - GUTTER * 2 - send_d - 14;
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
        let send_r = Rect::new(w - GUTTER - send_d, bottom - send_d, send_d, send_d);
        let body_r = Rect::new(GUTTER, bottom - field_h, field_w, field_h);
        let header_end = ui::TOP_SAFE + widgets::TopAppBar::HEIGHT;
        let to_r = if self.is_new {
            Rect::new(GUTTER, header_end + 12, w - GUTTER * 2, 56)
        } else {
            Rect::new(0, 0, 0, 0)
        };
        let msg_top = header_end.max(to_r.bottom()) + 12;
        let msg_area = Rect::new(0, msg_top, w, (body_r.y - 18 - msg_top).max(0));
        (to_r, body_r, send_r, msg_area)
    }

    /// Thread header: back arrow, then (for existing threads) the peer's
    /// avatar + name over their number, and a tonal call shortcut trailing
    /// — the same anatomy as the Google Messages conversation app bar.
    fn render_thread_header(&mut self, canvas: &mut Canvas, theme: &Theme, w: i32, h: i32) {
        let bar = widgets::TopAppBar::layout(Size::new(w, h), ui::TOP_SAFE);
        self.back_rect = widgets::TopAppBar::back_rect(bar);

        if self.is_new {
            widgets::TopAppBar::render(canvas, bar, theme, "New message", true, None);
        } else {
            widgets::TopAppBar::render(canvas, bar, theme, "", true, None);
            let peer = self.peer_label.clone();
            let name = self.display_peer(&peer);
            let known = self.contacts.name_for(&peer);
            let tint = ui::avatar_tint(known.unwrap_or(&peer));
            let av = Rect::new(bar.x + 64, bar.y + (bar.h - 48) / 2, 48, 48);
            widgets::avatar(canvas, known.unwrap_or(""), av, tint, theme.text);
            let tx = av.right() + 16;
            blit_label(
                canvas,
                &name,
                ui::styled(23.0, 600, theme.text),
                tx,
                bar.y + 12,
                Some(w - tx - 140),
                1,
            );
            let sub = crate::model::format_phone(&peer);
            if sub != name {
                blit_label(
                    canvas,
                    &sub,
                    ui::caption(theme.text2),
                    tx,
                    bar.y + 46,
                    Some(w - tx - 140),
                    1,
                );
            }
            // Tonal call shortcut — hands the number to the dialer.
            let call = Rect::new(w - ui::MARGIN - 52, bar.y + (bar.h - 52) / 2, 52, 52);
            self.call_rect = call;
            widgets::icon_btn(
                canvas,
                theme,
                call,
                icons::CALL,
                widgets::IconBtnStyle::Tonal,
                false,
                self.pressed == Some(Hit::CallPeer),
            );
        }
        if self.pressed == Some(Hit::Back) {
            ui::press_overlay(canvas, theme, self.back_rect, 24.0, 0.10);
        }
        // Hairline under the bar so the scrolling bubbles read as passing beneath.
        ui::divider_inset(
            canvas,
            theme,
            0,
            w,
            bar.bottom(),
        );
    }

    fn render_thread(&mut self, canvas: &mut Canvas, theme: &Theme, w: i32, h: i32) {
        self.render_thread_header(canvas, theme, w, h);

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

        for elem in &layout.elems {
            let ey = elem_y(elem) - top_idx;
            if ey + elem_h(elem) <= 0 || ey >= area.h {
                continue; // culled
            }
            match elem {
                Elem::Day { label, .. } => {
                    // Centered tonal pill chip — the M3 date separator.
                    if let Some(px) = label_px(label, cap_style, None, 1) {
                        let pw = px.size.w + 32;
                        let pill = Rect::new((w - pw) / 2, ey, pw, DAY_SEP_H - 6);
                        messages.fill_rounded_rect(pill, (DAY_SEP_H - 6) as f32 / 2.0, theme.surface2);
                        messages.blit(&px, Point::new((w - px.size.w) / 2, ey + 3));
                    }
                }
                Elem::Caption {
                    right,
                    label,
                    failed,
                    ..
                } => {
                    let st = if *failed {
                        ui::caption(theme.danger)
                    } else {
                        cap_style
                    };
                    if let Some(px) = label_px(label, st, None, 1) {
                        let ex = if *right {
                            w - GUTTER - px.size.w - 6
                        } else {
                            GUTTER + 6
                        };
                        messages.blit(&px, Point::new(ex, ey));
                    }
                }
                Elem::Bubble {
                    msg,
                    right,
                    w: bw,
                    h: bh,
                    first_in_run,
                    ..
                } => {
                    let m = &self.msgs[*msg];
                    let bx = if *right { w - GUTTER - bw } else { GUTTER };
                    let br = Rect::new(bx, ey, *bw, *bh);
                    let bg = if *right { theme.accent } else { theme.surface1 };
                    messages.fill_rounded_rect(br, BUBBLE_R, bg);
                    // M3 run shaping: the tail corner (bottom on the
                    // outgoing/incoming side) stays sharp on every bubble;
                    // consecutive bubbles in a run also flatten where they
                    // meet, so a run reads as one connected shape.
                    if *right {
                        square_corner(&mut messages, br, bg, true, true);
                        if !first_in_run {
                            square_corner(&mut messages, br, bg, true, false);
                        }
                    } else {
                        square_corner(&mut messages, br, bg, false, true);
                        if !first_in_run {
                            square_corner(&mut messages, br, bg, false, false);
                        }
                    }
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
                28.0,
                if focused {
                    theme.surface2
                } else {
                    theme.surface1
                },
            );
            if focused {
                // Accent focus ring — the M3 text-field cue.
                let fill = theme.surface2;
                canvas.fill_rounded_rect(to_r, 28.0, theme.accent);
                canvas.fill_rounded_rect(to_r.inset(2), 26.0, fill);
            }
            let (shown, col) = if self.to_draft.is_empty() {
                ("To: phone number", theme.text2)
            } else {
                (self.to_draft.as_str(), theme.text)
            };
            blit_label(
                canvas,
                shown,
                ui::styled(22.0, 400, col),
                to_r.x + 22,
                to_r.y + 15,
                Some(to_r.w - 44),
                1,
            );
        }

        let focused = self.focus == Focus::Body;
        let radius = (body_r.h as f32 / 2.0).min(28.0);
        canvas.fill_rounded_rect(
            body_r,
            radius,
            if focused {
                theme.surface2
            } else {
                theme.surface1
            },
        );
        if focused {
            canvas.fill_rounded_rect(body_r, radius, theme.accent);
            canvas.fill_rounded_rect(body_r.inset(2), (radius - 2.0).max(0.0), theme.surface2);
        }
        let max_tw = body_r.w - 44;
        if self.body_draft.trim().is_empty() {
            blit_label(
                canvas,
                "Message",
                ui::styled(22.0, 400, theme.text3),
                body_r.x + 22,
                body_r.y + (body_r.h - 28) / 2,
                Some(max_tw),
                1,
            );
        } else {
            blit_label(
                canvas,
                &self.body_draft,
                ui::styled(22.0, 400, theme.text),
                body_r.x + 22,
                body_r.y + 12,
                Some(max_tw),
                3,
            );
        }

        // Circular send button — accent when armed, dimmed surface when
        // there is nothing to send (the button stays visible either way,
        // like the Messages composer).
        let can_send = !self.body_draft.trim().is_empty()
            && (!self.is_new || !self.to_draft.trim().is_empty());
        let pressed_send = self.pressed == Some(Hit::Send);
        let bg = if !can_send {
            theme.surface2
        } else if pressed_send {
            ui::mix(theme.accent, [0, 0, 0, 255], 0.15)
        } else {
            theme.accent
        };
        canvas.fill_rounded_rect(send_r, send_r.h as f32 / 2.0, bg);
        draw_icon_centered(
            canvas,
            icons::SEND,
            26.0,
            if can_send {
                theme.text_on_accent
            } else {
                theme.text3
            },
            // Optical nudge: the send glyph points up-right, so it sits
            // slightly left-low of the geometric center.
            send_r.x + send_r.w / 2 - 1,
            send_r.y + send_r.h / 2 + 1,
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
