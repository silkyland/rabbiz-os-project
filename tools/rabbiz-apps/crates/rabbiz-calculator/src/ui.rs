//! Calculator layout, hit targets, and canvas rendering.
//!
//! AOSP target: Google Calculator / Material You — a borderless display pane
//! (expression above, big right-aligned result) over a 4-column keypad of
//! CIRCULAR keys. Row 1 + the operator column get a darker tonal fill than
//! the digits, `=` is the only accent key, and the pending operator keeps the
//! filled-accent treatment. History is a slide-down sheet of fixed-height
//! right-aligned rows (scrollable), opened from a top-right icon button.

use de_api::Pixels;
use de_design::motion::{bezier_eval, BEZIER_ENTER};
use de_design::{tokens, type_scale};
use rabbiz_app_kit::{icons, text, ui, widgets, Canvas, Point, Rect, Size, Theme};

use crate::model::{pending_op, KeyKind, Op};
use crate::CalcApp;

// ---------------------------------------------------------------------------
// Layout (pure, unit-tested). All geometry derives from canvas size.
// ---------------------------------------------------------------------------

const HIST_BTN: i32 = 48;
const HEADER_H: i32 = 72;
const KEY_GAP: i32 = 8;
const DISPLAY_MIN_H: i32 = 200;

/// Fixed history row height (px) — rows pack from the top of the sheet.
pub(crate) const HIST_ROW_H: i32 = 96;

pub(crate) const PRESS_FADE_S: f32 = 0.09;
pub(crate) const HIST_OPEN_S: f32 = 0.24;
pub(crate) const HIST_CLOSE_S: f32 = 0.18;

/// Hold the backspace key this long (ms) to clear the whole expression.
pub(crate) const BACKSPACE_HOLD_MS: f32 = 550.0;

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
    // Reserve a standard-height keypad before giving the readout extra room.
    // Compact windows can shrink both regions while keeping every key visible.
    let header_bottom = ui::TOP_SAFE + HEADER_H;
    let room = (bottom_edge - header_bottom - KEY_GAP * 5).max(5);
    let min_key_h = (room / 5).clamp(1, 56);
    let display_h = (room - min_key_h * 5).clamp(0, DISPLAY_MIN_H);
    let btn_h = ((room - display_h) / 5).clamp(min_key_h, btn_w.max(min_key_h));
    let grid_h = btn_h * 5 + KEY_GAP * 4;
    let grid_top = bottom_edge - grid_h;
    let display = Rect::new(
        m,
        ui::TOP_SAFE + HEADER_H,
        avail_w,
        (grid_top - KEY_GAP) - (ui::TOP_SAFE + HEADER_H),
    );

    // kind order per row; labels are display glyphs. '-' is ASCII on purpose:
    // the bundled face has no U+2212, so the model's '−' is also mapped for
    // display (see draw_readout).
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
            (KeyKind::Op(Op::Sub), "-"),
        ],
        [
            (KeyKind::Digit('1'), "1"),
            (KeyKind::Digit('2'), "2"),
            (KeyKind::Digit('3'), "3"),
            (KeyKind::Op(Op::Add), "+"),
        ],
        [
            (KeyKind::Backspace, icons::BACKSPACE),
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

/// Top-right Material icon button that opens the history sheet (AOSP calc
/// keeps the same affordance in the display pane's top corner).
pub(crate) fn history_btn_rect(w: i32) -> Rect {
    Rect::new(w - ui::MARGIN - HIST_BTN, ui::TOP_SAFE + 12, HIST_BTN, HIST_BTN)
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

/// Sheet chrome + list geometry for [`history_panel`]. `rows` are in CONTENT
/// space (row `i` at `y = i * HIST_ROW_H`, `x` = screen x); add
/// `viewport.y - scroll` for screen coordinates.
#[derive(Debug, Clone)]
pub(crate) struct HistoryLayout {
    pub(crate) header: Rect,
    pub(crate) clear: Rect,
    pub(crate) viewport: Rect,
    pub(crate) rows: Vec<Rect>,
    pub(crate) content_h: i32,
}

/// Header (title + clear), list viewport and row rects for `count` entries.
pub(crate) fn history_layout(panel: Rect, count: usize) -> HistoryLayout {
    let m = ui::MARGIN;
    let header = Rect::new(m, panel.y + 12, panel.w - 2 * m, 56);
    let clear = Rect::new(header.right() - 96, header.y + 4, 96, 48);
    let viewport = Rect::new(
        0,
        header.bottom() + 8,
        panel.w,
        panel.bottom() - header.bottom() - 8 - 12,
    );
    let content_h = count as i32 * HIST_ROW_H;
    let rows = (0..count)
        .map(|i| {
            Rect::new(
                panel.x + m,
                i as i32 * HIST_ROW_H,
                panel.w - 2 * m,
                HIST_ROW_H,
            )
        })
        .collect();
    HistoryLayout {
        header,
        clear,
        viewport,
        rows,
        content_h,
    }
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
// Type ladder — autoshrinking readout text, anchored to the type_scale tokens
// (with() only tunes size/leading, never colors).
// ---------------------------------------------------------------------------

/// Preview or error line beneath the formula.
fn expr_style(len: usize, color: [u8; 4]) -> text::LabelStyle {
    let ts = if len <= 14 {
        type_scale::H2 // 32
    } else if len <= 22 {
        type_scale::BODY // 26
    } else if len <= 34 {
        type_scale::CAPTION // 20
    } else {
        type_scale::MICRO.with(16.0, 20.0, 400)
    };
    text::LabelStyle::from_type(ts, color)
}

/// Primary formula, scaled down for longer expressions.
fn result_style(len: usize, color: [u8; 4]) -> text::LabelStyle {
    let ts = if len <= 10 {
        type_scale::DISPLAY.with(88.0, 96.0, 600)
    } else if len <= 16 {
        type_scale::DISPLAY.with(56.0, 64.0, 600)
    } else if len <= 24 {
        type_scale::H1 // 40
    } else {
        type_scale::TITLE // 28
    };
    text::LabelStyle::from_type(ts, color)
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

    let history_button = history_btn_rect(w);
    ui::draw_text(
        canvas,
        "Calculator",
        &ui::title(theme.text),
        ui::MARGIN,
        ui::TOP_SAFE + 16,
        Some(history_button.x - 2 * ui::MARGIN),
    );
    draw_readout(app, canvas, theme, &layout);

    for (i, key) in layout.keys.iter().enumerate() {
        draw_key(app, canvas, theme, key, i);
    }

    // The history sheet slides down OVER the keypad (scrim + sheet last).
    if hist_active {
        draw_history(app, canvas, theme, w, h, hist_p);
    } else {
        draw_history_button(app, canvas, theme, w);
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
    widgets::icon_btn(
        canvas,
        theme,
        r,
        icons::HISTORY,
        widgets::IconBtnStyle::Tonal,
        false,
        false,
    );
    if alpha > 0.0 {
        ui::press_overlay(canvas, theme, r, (r.h as f32) / 2.0, alpha * 0.14);
    }
}

fn draw_readout(app: &CalcApp, canvas: &mut Canvas, theme: &Theme, layout: &KeypadLayout) {
    let d = layout.display;
    if d.h < 40 {
        return;
    }
    let padding = 8;
    let width = d.w - padding * 2;
    let expr = app.expr.replace('−', "-");
    let has_preview = app.error.is_some() || (!expr.is_empty() && expr != app.preview);
    let preview_h = if has_preview { 46.min(d.h / 3) } else { 0 };
    let formula_h = (d.h - preview_h).min(108);
    let extra = d.h - preview_h - formula_h;
    let formula = Rect::new(d.x, d.y + extra * 2 / 3, d.w, formula_h);
    let primary = if expr.is_empty() { "0" } else { &expr };
    let mut style = result_style(primary.chars().count(), theme.text);
    if let Some(label) = text::label(primary, style, None) {
        let scale = (width.max(1) as f32 / label.width().max(1) as f32)
            .min(formula_h.max(1) as f32 / style.line_height);
        if scale < 1.0 {
            style = ui::styled((style.size * scale).max(16.0), 600, theme.text);
        }
    }
    ui::draw_text_right(canvas, primary, &style, formula, padding);
    if has_preview {
        let (value, color) = match app.error {
            Some(err) => (err.message(), theme.danger),
            None => (app.preview.as_str(), theme.text2),
        };
        let preview = Rect::new(d.x, formula.bottom(), d.w, preview_h);
        ui::draw_text_right(
            canvas,
            value,
            &expr_style(value.chars().count(), color),
            preview,
            padding,
        );
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

    canvas.fill_rounded_rect(panel, tokens::RADIUS_SHEET, theme.surface1);
    let count = app.history.len();
    let hl = history_layout(panel, count);
    let scroll = app
        .hist_scroll
        .offset_i32()
        .clamp(0, (hl.content_h - hl.viewport.h).max(0));

    // "History" title (top-left) + "Clear" text button (top-right).
    ui::draw_text(
        canvas,
        "History",
        &ui::title(theme.text),
        hl.header.x,
        hl.header.y + (hl.header.h - 40) / 2,
        Some(hl.clear.x - hl.header.x - 16),
    );
    let clear_pressed = app
        .pressed
        .map(|(_, hh, inside)| inside && hh == Hit::HistClear)
        .unwrap_or(false);
    if clear_pressed {
        ui::press_overlay(canvas, theme, hl.clear, hl.clear.h as f32 / 2.0, 0.10);
    }
    ui::draw_centered(canvas, "Clear", &ui::label(theme.danger), hl.clear);

    if count == 0 {
        widgets::EmptyState::render(
            canvas,
            hl.viewport,
            theme,
            icons::HISTORY,
            "No calculations yet",
            "Results you compute will show up here",
        );
        return;
    }

    // Rows render into an offscreen canvas in content space, then the visible
    // slice is blitted — that is the clip (Canvas has no scissor).
    let mut off = Canvas::new(Size::new(w, hl.content_h));
    for (i, rr) in hl.rows.iter().enumerate() {
        let Some(entry) = app.history.iter().rev().nth(i) else {
            break;
        };
        let rr = *rr;
        let pressed = app
            .pressed
            .map(|(_, hh, inside)| inside && hh == Hit::HistRow(i))
            .unwrap_or(false);
        if pressed {
            ui::press_overlay(&mut off, theme, rr, 0.0, 0.10);
        }
        // Expression (caption, dim) over result (title-ish), right-aligned.
        if !entry.expr.is_empty() {
            let expr = entry.expr.replace('−', "-");
            let expr_r = Rect::new(rr.x + 24, rr.y + 14, rr.w - 48, 26);
            ui::draw_text_right(&mut off, &expr, &ui::caption(theme.text3), expr_r, 0);
        }
        let res_r = Rect::new(rr.x + 24, rr.y + 42, rr.w - 48, 42);
        let res_st = text::LabelStyle::from_type(type_scale::TITLE, theme.text);
        ui::draw_text_right(&mut off, &entry.result, &res_st, res_r, 0);
        if i + 1 < count {
            ui::divider_inset(&mut off, theme, rr.x + 24, rr.right() - 24, rr.bottom());
        }
    }
    let px = off.into_pixels();
    let region = Rect::new(hl.viewport.x, scroll, hl.viewport.w, hl.viewport.h);
    if let Some(r) = region.intersect(Rect::from_origin_size(Point::ZERO, px.size)) {
        canvas.blit_region(
            &px,
            r,
            Point::new(
                hl.viewport.x + (r.x - region.x),
                hl.viewport.y + (r.y - region.y),
            ),
        );
    }
    ui::scrollbar(
        canvas,
        theme,
        Rect::new(hl.viewport.right() - 8, hl.viewport.y, 6, hl.viewport.h),
        scroll,
        hl.content_h,
    );
}

fn draw_key(app: &CalcApp, canvas: &mut Canvas, theme: &Theme, key: &Key, idx: usize) {
    // Circle that fills the cell (AOSP calc keys are true circles).
    let d = key.rect.w.min(key.rect.h);
    let r = Rect::new(
        key.rect.x + (key.rect.w - d) / 2,
        key.rect.y + (key.rect.h - d) / 2,
        d,
        d,
    );
    // Container color by role: digits get the surface tint, the function row
    // and operator column get the darker tonal fill, `=` is the only accent
    // key (the pending operator keeps the same accent treatment).
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
            (theme.surface3, theme.text)
        }
        KeyKind::Digit(_) | KeyKind::Dot => (theme.surface1, theme.text),
    };
    let rad = (d as f32) / 2.0;
    canvas.fill_rounded_rect(r, rad, bg);

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
        canvas.fill_rounded_rect(r, rad, ui::state_layer(theme, alpha));
    }

    // Glyph: icon font for backspace, text otherwise.
    if key.kind == KeyKind::Backspace {
        let st = text::LabelStyle::icon((d as f32 * 0.32).min(44.0), fg);
        if let Some(px) = text::label(key.label, st, None) {
            let iw = px.width() as i32;
            let ih = px.height() as i32;
            canvas.blit(
                &Pixels::new(Size::new(iw, ih), px.data().to_vec()),
                Point::new(r.x + (r.w - iw) / 2, r.y + (r.h - ih) / 2),
            );
        }
    } else {
        let size = if d >= 120 { 40.0 } else { 34.0 };
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
        for (w, h) in [(720, 1600), (720, 1280), (400, 800), (720, 480), (1080, 2400)] {
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
            // First key row starts below the display pane.
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
        let hl = history_layout(open, 5);
        assert_eq!(hl.rows.len(), 5);
        // Fixed-height rows packed from the top of the viewport.
        for (i, r) in hl.rows.iter().enumerate() {
            assert_eq!(r.h, HIST_ROW_H);
            assert_eq!(r.y, i as i32 * HIST_ROW_H);
            assert!(r.x >= 0 && r.right() <= open.w);
        }
        assert_eq!(hl.content_h, 5 * HIST_ROW_H);
        assert!(hl.viewport.y >= hl.header.bottom());
        assert!(hl.viewport.bottom() <= open.bottom());
    }

    #[test]
    fn history_button_is_top_right_icon_target() {
        let r = history_btn_rect(720);
        assert!(r.w >= 48 && r.h >= 48, "touch target");
        assert_eq!(r.right(), 720 - ui::MARGIN);
        assert!(r.y >= ui::TOP_SAFE);
    }
}
