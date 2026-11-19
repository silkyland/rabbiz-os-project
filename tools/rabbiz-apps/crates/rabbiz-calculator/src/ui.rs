//! Calculator layout, hit targets, and canvas rendering.

use de_api::Pixels;
use de_design::motion::{bezier_eval, BEZIER_ENTER};
use rabbiz_app_kit::{text, ui, Canvas, Point, Rect, Size, Theme};

use crate::model::{pending_op, KeyKind, Op};
use crate::CalcApp;

// ---------------------------------------------------------------------------
// Layout (pure, unit-tested). All geometry derives from canvas size.
// ---------------------------------------------------------------------------

const HISTORY_BTN_W: i32 = 168;
const HISTORY_BTN_H: i32 = 48;
const HEADER_H: i32 = 72;
const KEY_GAP: i32 = 12;
const KEY_RADIUS: f32 = 24.0;
const DISPLAY_MIN_H: i32 = 200;

pub(crate) const PRESS_FADE_S: f32 = 0.09;
pub(crate) const HIST_OPEN_S: f32 = 0.24;
pub(crate) const HIST_CLOSE_S: f32 = 0.18;

#[derive(Debug, Clone, Copy)]
pub(crate) struct Key {
    pub(crate) rect: Rect,
    pub(crate) kind: KeyKind,
    pub(crate) label: &'static str,
}

#[derive(Debug, Clone)]
pub(crate) struct KeypadLayout {
    pub(crate) display: Rect,
    pub(crate) keys: Vec<Key>,
}

pub(crate) fn keypad_layout(w: i32, h: i32) -> KeypadLayout {
    let m = ui::MARGIN;
    let bottom_edge = h - ui::BOTTOM_SAFE - 16;
    let avail_w = (w - 2 * m).max(0);
    let btn_w = (avail_w - KEY_GAP * 3) / 4;
    // Buttons aim for ≥56 px; on the 720x1600 panel they land ~180 px.
    let room = bottom_edge - (ui::TOP_SAFE + HEADER_H) - KEY_GAP * 4 - DISPLAY_MIN_H;
    let btn_h = (room / 5).clamp(56, (btn_w * 6 / 5).max(56));
    let grid_h = btn_h * 5 + KEY_GAP * 4;
    let grid_top = (bottom_edge - grid_h).max(ui::TOP_SAFE + HEADER_H + DISPLAY_MIN_H);
    let display = Rect::new(
        m,
        ui::TOP_SAFE + HEADER_H,
        avail_w,
        (grid_top - KEY_GAP) - (ui::TOP_SAFE + HEADER_H),
    );

    // kind order per row; labels are display glyphs.
    let rows: [[(KeyKind, &'static str); 4]; 5] = [
        [
            (KeyKind::Clear, "AC"),
            (KeyKind::Negate, "±"),
            (KeyKind::Percent, "%"),
            (KeyKind::Op(Op::Div), "÷"),
        ],
        [
            (KeyKind::Digit('7'), "7"),
            (KeyKind::Digit('8'), "8"),
            (KeyKind::Digit('9'), "9"),
            (KeyKind::Op(Op::Mul), "×"),
        ],
        [
            (KeyKind::Digit('4'), "4"),
            (KeyKind::Digit('5'), "5"),
            (KeyKind::Digit('6'), "6"),
            (KeyKind::Op(Op::Sub), "−"),
        ],
        [
            (KeyKind::Digit('1'), "1"),
            (KeyKind::Digit('2'), "2"),
            (KeyKind::Digit('3'), "3"),
            (KeyKind::Op(Op::Add), "+"),
        ],
        [
            (KeyKind::Backspace, "\u{e14a}"),
            (KeyKind::Digit('0'), "0"),
            (KeyKind::Dot, "."),
            (KeyKind::Equals, "="),
        ],
    ];
    let mut keys = Vec::with_capacity(20);
    for (r, row) in rows.iter().enumerate() {
        for (c, (kind, label)) in row.iter().enumerate() {
            let x = m + c as i32 * (btn_w + KEY_GAP);
            let y = grid_top + r as i32 * (btn_h + KEY_GAP);
            keys.push(Key {
                rect: Rect::new(x, y, btn_w, btn_h),
                kind: *kind,
                label,
            });
        }
    }
    KeypadLayout { display, keys }
}

pub(crate) fn history_btn_rect(_w: i32) -> Rect {
    Rect::new(ui::MARGIN, ui::TOP_SAFE + 10, HISTORY_BTN_W, HISTORY_BTN_H)
}

/// Panel sheet geometry. `p` is the eased open progress (0 closed, 1 open);
/// the sheet slides down from above the top safe area. Returns the on-screen
/// rect (already offset) — all children are positioned relative to it.
pub(crate) fn history_panel(w: i32, h: i32, p: f32) -> Rect {
    let top = ui::TOP_SAFE;
    let ph = h - top - ui::BOTTOM_SAFE - 8;
    let dy = ((1.0 - p) * -(ph as f32 + top as f32)) as i32;
    Rect::new(0, top + dy, w, ph)
}

/// Header (title + clear) and entry row rects, absolute coords matching
/// `history_panel`'s offset rect. Rows are newest-first.
pub(crate) fn history_rows(panel: Rect, count: usize) -> (Rect, Rect, Vec<Rect>) {
    let m = 16;
    let header = Rect::new(m, panel.y + 12, panel.w - 2 * m, 56);
    let clear = Rect::new(header.right() - 96, header.y + 8, 96, 40);
    let avail = panel.bottom() - header.bottom() - 16;
    let n = count.max(1) as i32;
    let pitch = (avail / n).max(1);
    let row_h = (pitch - 8).clamp(36, 72);
    let mut rows = Vec::with_capacity(count);
    for i in 0..count {
        let y = header.bottom() + 8 + i as i32 * pitch;
        rows.push(Rect::new(m, y, panel.w - 2 * m, row_h));
    }
    (header, clear, rows)
}

// ---------------------------------------------------------------------------
// Hit targets.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Hit {
    Key(usize),
    HistRow(usize), // index into history entries, newest-first
    HistClear,
    HistToggle,
    Scrim,
    Panel,
}

impl Hit {
    /// Hits without a meaningful rect are "sticky": the finger counts as
    /// inside for the whole press (scrim/panel background taps).
    pub(crate) fn sticky(self) -> bool {
        matches!(self, Self::Scrim | Self::Panel)
    }
}

// ---------------------------------------------------------------------------
// Rendering.
// ---------------------------------------------------------------------------

pub(crate) fn render(app: &CalcApp, canvas: &mut Canvas, theme: &Theme) {
    let w = canvas.size().w;
    let h = canvas.size().h;
    let layout = keypad_layout(w, h);
    let hist_p = app.history_progress();
    let hist_active = hist_p > 0.001 || app.history_open;

    draw_readout(app, canvas, theme, &layout);

    if hist_active {
        draw_history(app, canvas, theme, w, h, hist_p);
    } else {
        draw_history_button(app, canvas, theme, w);
    }

    for (i, key) in layout.keys.iter().enumerate() {
        draw_key(app, canvas, theme, key, i);
    }
}

fn draw_history_button(app: &CalcApp, canvas: &mut Canvas, theme: &Theme, w: i32) {
    let r = history_btn_rect(w);
    let pressed = app
        .pressed
        .map(|(_, h, inside)| inside && h == Hit::HistToggle)
        .unwrap_or(false)
        || app
            .flash
            .map(|(h, t)| h == Hit::HistToggle && t < 1.0)
            .unwrap_or(false);
    let alpha = if pressed {
        if app.pressed.is_some() {
            ease_out(app.press_t)
        } else {
            1.0 - app.flash.map_or(0.0, |(_, t)| t.min(1.0))
        }
    } else {
        0.0
    };
    canvas.fill_rounded_rect(r, (r.h as f32) / 2.0, theme.surface2);
    if alpha > 0.0 {
        canvas.fill_rounded_rect(r, (r.h as f32) / 2.0, ui::state_layer(theme, alpha * 0.14));
    }
    // History icon + label.
    let icon_st = text::LabelStyle::icon(24.0, theme.text2);
    if let Some(px) = text::label("\u{e8b6}", icon_st, None) {
        let iw = px.width() as i32;
        let ih = px.height() as i32;
        canvas.blit(
            &Pixels::new(Size::new(iw, ih), px.data().to_vec()),
            Point::new(r.x + 16, r.y + (r.h - ih) / 2),
        );
    }
    ui::draw_text(
        canvas,
        "History",
        &ui::label(theme.text),
        r.x + 48,
        r.y + (r.h as f32 / 2.0 - 14.0) as i32,
        Some(r.w - 56),
    );
}

fn draw_readout(app: &CalcApp, canvas: &mut Canvas, theme: &Theme, layout: &KeypadLayout) {
    let d = layout.display;
    if d.h < 40 {
        return;
    }
    let pad = 8;
    // Expression line: small, secondary, right-aligned in the top half,
    // shrinks with length.
    let expr = if app.expr.is_empty() {
        ""
    } else {
        app.expr.as_str()
    };
    if !expr.is_empty() {
        let n = expr.chars().count();
        let size = if n <= 14 {
            30.0
        } else if n <= 22 {
            24.0
        } else if n <= 34 {
            19.0
        } else {
            16.0
        };
        let st = ui::styled(size, 400, theme.text2);
        let expr_r = Rect::new(d.x, d.y, d.w, d.h / 2);
        ui::draw_text_right(canvas, expr, &st, expr_r, pad);
    }
    // Result line: large, right-aligned, shrinks with length.
    let result_rect = Rect::new(d.x, d.y + d.h / 2, d.w, d.h / 2 - pad);
    if let Some(err) = app.error {
        let st = ui::styled(30.0, 600, theme.danger);
        ui::draw_text_right(canvas, err.message(), &st, result_rect, pad);
    } else {
        let n = app.preview.chars().count();
        let size = if n <= 10 {
            58.0
        } else if n <= 16 {
            46.0
        } else if n <= 24 {
            36.0
        } else {
            28.0
        };
        let color = if app.expr.is_empty() {
            theme.text3
        } else {
            theme.text
        };
        let st = ui::styled(size, 600, color);
        ui::draw_text_right(canvas, &app.preview, &st, result_rect, pad);
    }
}

fn draw_history(app: &CalcApp, canvas: &mut Canvas, theme: &Theme, w: i32, h: i32, p: f32) {
    let panel = history_panel(w, h, p);
    // Scrim over everything else, fading with progress.
    let a = (theme.scrim[3] as f32 * p) as u8;
    canvas.fill_rect(
        Rect::new(0, 0, w, h),
        [theme.scrim[0], theme.scrim[1], theme.scrim[2], a],
    );

    canvas.fill_rounded_rect(panel, 28.0, theme.surface1);
    let count = app.history.len();
    let (_, clear, rows) = history_rows(panel, count);

    // "History" title.
    ui::draw_text(
        canvas,
        "History",
        &ui::title(theme.text),
        clear.x - 220,
        clear.y + 2,
        Some(200),
    );
    // Clear pill.
    let clear_pressed = app
        .pressed
        .map(|(_, hh, inside)| inside && hh == Hit::HistClear)
        .unwrap_or(false);
    canvas.fill_rounded_rect(clear, (clear.h as f32) / 2.0, theme.surface2);
    if clear_pressed {
        canvas.fill_rounded_rect(clear, (clear.h as f32) / 2.0, ui::state_layer(theme, 0.14));
    }
    ui::draw_centered(canvas, "Clear", &ui::label(theme.danger), clear);

    if count == 0 {
        // Honest empty state.
        let cy = panel.y + panel.h / 2 - 60;
        let icon_st = text::LabelStyle::icon(56.0, theme.text3);
        if let Some(px) = text::label("\u{e8b6}", icon_st, None) {
            let iw = px.width() as i32;
            let ih = px.height() as i32;
            canvas.blit(
                &Pixels::new(Size::new(iw, ih), px.data().to_vec()),
                Point::new(panel.x + (panel.w - iw) / 2, cy),
            );
        }
        ui::draw_centered(
            canvas,
            "No calculations yet",
            &ui::label(theme.text2),
            Rect::new(panel.x, cy + 76, panel.w, 40),
        );
        ui::draw_centered(
            canvas,
            "Results you compute will show up here",
            &ui::caption(theme.text3),
            Rect::new(panel.x, cy + 118, panel.w, 32),
        );
        return;
    }

    // Rows, newest first (absolute coords, already slide-offset).
    for (i, r) in rows.iter().enumerate() {
        let Some(entry) = app.history.iter().rev().nth(i) else {
            break;
        };
        let rr = *r;
        let pressed = app
            .pressed
            .map(|(_, hh, inside)| inside && hh == Hit::HistRow(i))
            .unwrap_or(false);
        canvas.fill_rounded_rect(rr, 20.0, theme.surface2);
        if pressed {
            canvas.fill_rounded_rect(rr, 20.0, ui::state_layer(theme, 0.12));
        }
        // Expression (small, dim, right aligned) then result (bold).
        let inner = rr.inset(16);
        let expr_r = Rect::new(inner.x, inner.y + 6, inner.w, 26);
        let res_r = Rect::new(inner.x, inner.y + inner.h / 2, inner.w, inner.h / 2 - 4);
        if !entry.expr.is_empty() {
            ui::draw_text_right(
                canvas,
                &entry.expr,
                &ui::styled(19.0, 400, theme.text3),
                expr_r,
                0,
            );
        }
        ui::draw_text_right(
            canvas,
            &entry.result,
            &ui::styled(26.0, 600, theme.text),
            res_r,
            0,
        );
    }
}

fn draw_key(app: &CalcApp, canvas: &mut Canvas, theme: &Theme, key: &Key, idx: usize) {
    let r = key.rect;
    // Container color by role.
    let (bg, fg) = match key.kind {
        KeyKind::Equals => (theme.accent, theme.text_on_accent),
        KeyKind::Op(_) => {
            let pending = pending_op(&app.expr) == Some(key.kind);
            if pending {
                (theme.accent, theme.text_on_accent)
            } else {
                (theme.surface3, theme.accent)
            }
        }
        KeyKind::Clear | KeyKind::Negate | KeyKind::Percent | KeyKind::Backspace => {
            (theme.surface2, theme.text2)
        }
        KeyKind::Digit(_) | KeyKind::Dot => (theme.surface1, theme.text),
    };
    canvas.fill_rounded_rect(r, KEY_RADIUS.min(r.h as f32 / 2.0), bg);

    // Press / release-flash state layer.
    let mut alpha = 0.0f32;
    if let Some((_, hit, inside)) = app.pressed {
        if inside && hit == Hit::Key(idx) {
            alpha = ease_out(app.press_t) * 0.16;
        }
    }
    if let Some((hit, t)) = app.flash {
        if hit == Hit::Key(idx) && t < 1.0 {
            alpha = alpha.max((1.0 - ease_out(t)) * 0.16);
        }
    }
    if alpha > 0.0 {
        canvas.fill_rounded_rect(
            r,
            KEY_RADIUS.min(r.h as f32 / 2.0),
            ui::state_layer(theme, alpha),
        );
    }

    // Glyph: icon font for backspace, text otherwise.
    if key.kind == KeyKind::Backspace {
        let st = text::LabelStyle::icon((r.h as f32 * 0.42).min(44.0), fg);
        if let Some(px) = text::label(key.label, st, None) {
            let iw = px.width() as i32;
            let ih = px.height() as i32;
            canvas.blit(
                &Pixels::new(Size::new(iw, ih), px.data().to_vec()),
                Point::new(r.x + (r.w - iw) / 2, r.y + (r.h - ih) / 2),
            );
        }
    } else {
        let size = if r.h >= 150 { 40.0 } else { 34.0 };
        let st = ui::styled(size, 600, fg);
        ui::draw_centered(canvas, key.label, &st, r);
    }
}

fn ease_out(t: f32) -> f32 {
    bezier_eval(BEZIER_ENTER, t.clamp(0.0, 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_fits_panel() {
        for (w, h) in [(720, 1600), (720, 1280), (400, 800), (1080, 2400)] {
            let l = keypad_layout(w, h);
            assert_eq!(l.keys.len(), 20);
            let bottom = l.keys.iter().map(|k| k.rect.bottom()).max().unwrap();
            assert!(
                bottom <= h - ui::BOTTOM_SAFE,
                "{w}x{h}: keys overflow bottom"
            );
            for k in &l.keys {
                assert!(k.rect.w >= 40, "key too narrow: {}", k.rect.w);
                assert!(k.rect.h >= 48, "key too short: {} @{}x{}", k.rect.h, w, h);
                assert!(k.rect.x >= 0 && k.rect.right() <= w);
            }
            // No overlaps.
            for (i, a) in l.keys.iter().enumerate() {
                for b in &l.keys[i + 1..] {
                    assert!(a.rect.intersect(b.rect).is_none(), "overlap {a:?} {b:?}");
                }
            }
            // First key row starts at grid_top.
            assert!(l.display.bottom() <= l.keys[0].rect.y);
        }
    }

    #[test]
    fn layout_reference_size() {
        let l = keypad_layout(720, 1600);
        // Digit keys should be well above the 56 px minimum on the A21s panel.
        let seven = l.keys.iter().find(|k| k.label == "7").unwrap();
        assert!(seven.rect.h >= 56, "digit height {}", seven.rect.h);
        assert!(seven.rect.w >= 56);
        // '=' is the accent-filled bottom-right key.
        let eq = l.keys.iter().find(|k| k.kind == KeyKind::Equals).unwrap();
        assert_eq!(eq.rect, l.keys[19].rect);
    }

    #[test]
    fn history_panel_geometry() {
        let closed = history_panel(720, 1600, 0.0);
        let open = history_panel(720, 1600, 1.0);
        assert_eq!(open.y, ui::TOP_SAFE);
        assert!(closed.y < open.y, "closed panel should sit above");
        assert!(
            closed.bottom() <= open.y + 1,
            "closed panel mostly offscreen"
        );
        let (_, _, rows) = history_rows(open, 5);
        assert_eq!(rows.len(), 5);
        for r in &rows {
            assert!(r.x >= 0 && r.right() <= open.w);
            assert!(r.h >= 40);
        }
    }
}
