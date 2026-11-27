//! RabbizOS Calculator — Material 3 keypad app.
//!
//! Design notes (vs the previous implementation):
//! - Flat M3-Expressive-style readout: small live expression above a large
//!   right-aligned result, no display "container" (Google Calculator 9.x did
//!   the same). Errors are inline in the result slot, tinted `danger`.
//! - Classic 4-column keypad: digits on `surface1`, function keys on
//!   `surface2`, operators tinted `accent` on `surface3`, `=` filled accent.
//!   The pending operator gets the filled-accent "live" treatment.
//! - History is a slide-down sheet (BEZIER_ENTER/EXIT, ≤240 ms) listing the
//!   last 20 calculations; tap an entry to reuse its result, Clear empties.
//! - Press feedback is a state-layer fade (~90 ms), all layout is derived
//!   from the live canvas size — no hardcoded 720x1600 anywhere.

use std::collections::VecDeque;
use std::time::Duration;

use de_design::motion::{BEZIER_ENTER, BEZIER_EXIT};
use rabbiz_app_kit::{
    ui as kit_ui, AppEvent, AppFrame, AppState, Canvas, Point, Rect, Size, TextInputCtl, Theme,
};

mod cli;
mod mcp;
mod model;
mod ui;

#[cfg(test)]
pub(crate) use model::EvalErr;
pub(crate) use model::{eval_expr, format_num, strip_pending_ops};

use model::{push_input, toggle_sign, HistEntry, KeyKind, HISTORY_CAP};
use ui::{
    history_btn_rect, history_layout, history_panel, keypad_layout, Hit, BACKSPACE_HOLD_MS,
    HIST_CLOSE_S, HIST_OPEN_S, PRESS_FADE_S,
};

// ---------------------------------------------------------------------------
// App state.
// ---------------------------------------------------------------------------

pub(crate) struct CalcApp {
    pub(crate) expr: String,
    pub(crate) preview: String,
    pub(crate) error: Option<model::EvalErr>,
    value: Option<f64>,
    pub(crate) history: VecDeque<HistEntry>,

    size: Size,
    pub(crate) pressed: Option<(i32, Hit, bool)>, // touch id, hit, finger-inside flag
    pub(crate) press_t: f32,
    pub(crate) flash: Option<(Hit, f32)>, // released hit fading out
    pub(crate) history_open: bool,
    hist_tween: kit_ui::Tween,
    /// Drag/fling scroll of the history list (content is `history_layout`).
    pub(crate) hist_scroll: rabbiz_app_kit::ScrollView,
    /// Monotonic ms fed by Tick — the clock for `hist_scroll` velocity and
    /// the backspace long-press timer.
    now_ms: u64,
    /// Elapsed ms of the current hold (drives long-press ⌫ = clear all).
    press_ms: f32,
    /// Set when the backspace long-press already fired `clear()`.
    long_fired: bool,
}

impl CalcApp {
    fn new() -> Self {
        Self {
            expr: String::new(),
            preview: "0".into(),
            error: None,
            value: None,
            history: VecDeque::new(),
            size: Size::new(720, 1600),
            pressed: None,
            press_t: 1.0,
            flash: None,
            history_open: false,
            hist_tween: kit_ui::Tween::new(0.0, 0.0, 0.001, BEZIER_ENTER),
            hist_scroll: rabbiz_app_kit::ScrollView::new(),
            now_ms: 0,
            press_ms: 0.0,
            long_fired: false,
        }
    }

    // -- expression mutation -------------------------------------------------

    fn recalc(&mut self) {
        self.error = None;
        self.value = None;
        if self.expr.is_empty() {
            self.preview = "0".into();
            return;
        }
        match eval_expr(&self.expr) {
            Ok(v) => {
                self.value = Some(v);
                self.preview = format_num(v);
            }
            Err(e) => {
                // A trailing pending operator isn't an error — preview the
                // stripped expression instead ("2+" previews "2").
                let stripped = strip_pending_ops(&self.expr);
                if stripped.is_empty() {
                    self.preview = "0".into();
                } else if let Ok(v) = eval_expr(stripped) {
                    self.value = Some(v);
                    self.preview = format_num(v);
                } else {
                    self.error = Some(e);
                    self.preview.clear();
                }
            }
        }
    }

    fn input_char(&mut self, c: char) {
        match c {
            '0'..='9' | '.' | '%' | '+' | '−' | '×' | '÷' => {
                if push_input(&mut self.expr, c) {
                    self.recalc();
                }
            }
            '=' => self.commit(),
            _ => {}
        }
    }

    fn backspace(&mut self) {
        if self.expr.pop().is_some() {
            self.recalc();
        }
    }

    fn clear(&mut self) {
        self.expr.clear();
        self.recalc();
    }

    fn negate(&mut self) {
        if toggle_sign(&mut self.expr) {
            self.recalc();
        }
    }

    /// '=' commits the live value into history and continues from the result.
    fn commit(&mut self) {
        let Some(v) = self.value else { return };
        if self.error.is_some() || self.expr.is_empty() {
            return;
        }
        let result = format_num(v);
        self.history.push_back(HistEntry {
            expr: self.expr.clone(),
            result: result.clone(),
        });
        while self.history.len() > HISTORY_CAP {
            self.history.pop_front();
        }
        self.expr = result;
        self.recalc();
    }

    fn reuse_history(&mut self, idx_newest_first: usize) {
        let Some(entry) = self.history.iter().rev().nth(idx_newest_first) else {
            return;
        };
        self.expr = entry.result.clone();
        self.recalc();
        self.close_history();
    }

    // -- history sheet ---------------------------------------------------------

    fn open_history(&mut self) {
        if self.history_open {
            return;
        }
        self.history_open = true;
        self.hist_scroll.scroll_to(0.0);
        let from = self.hist_tween.value();
        self.hist_tween = kit_ui::Tween::new(from, 1.0, HIST_OPEN_S, BEZIER_ENTER);
    }

    fn close_history(&mut self) {
        if !self.history_open && self.hist_tween.done() {
            return;
        }
        self.history_open = false;
        let from = self.hist_tween.value();
        self.hist_tween = kit_ui::Tween::new(from, 0.0, HIST_CLOSE_S, BEZIER_EXIT);
    }

    pub(crate) fn history_progress(&self) -> f32 {
        self.hist_tween.value()
    }

    // -- hit testing -----------------------------------------------------------

    /// Scroll offset clamped to the current list bounds (content-space rows
    /// shift up by this much on screen).
    fn hist_offset(&self) -> i32 {
        let panel = history_panel(self.size.w, self.size.h, self.history_progress());
        let hl = history_layout(panel, self.history.len());
        self.hist_scroll
            .offset_i32()
            .clamp(0, (hl.content_h - hl.viewport.h).max(0))
    }

    /// Feed the current list bounds to `hist_scroll` (content height grows
    /// with history; viewport follows the sheet tween).
    fn sync_hist_bounds(&mut self) {
        let panel = history_panel(self.size.w, self.size.h, self.history_progress());
        let hl = history_layout(panel, self.history.len());
        self.hist_scroll
            .set_bounds(hl.content_h as f32, hl.viewport.h as f32);
    }

    /// Screen-space rect of history row `i` (content space + scroll).
    fn hist_row_rect(&self, i: usize) -> Rect {
        let panel = history_panel(self.size.w, self.size.h, self.history_progress());
        let hl = history_layout(panel, self.history.len());
        hl.rows
            .get(i)
            .map(|r| r.translate(0, hl.viewport.y - self.hist_offset()))
            .unwrap_or(Rect::ZERO)
    }

    fn history_hit(&self, pos: Point) -> Option<Hit> {
        let p = self.history_progress();
        if p <= 0.5 {
            return None;
        }
        let panel = history_panel(self.size.w, self.size.h, p);
        if panel.contains(pos) {
            let hl = history_layout(panel, self.history.len());
            if hl.clear.contains(pos) {
                return Some(Hit::HistClear);
            }
            // Rows are stored top→bottom = newest→oldest; only hits inside
            // the list viewport land on a row.
            if hl.viewport.contains(pos) {
                for i in 0..hl.rows.len() {
                    if self.hist_row_rect(i).contains(pos) {
                        return Some(Hit::HistRow(i));
                    }
                }
            }
            return Some(Hit::Panel);
        }
        Some(Hit::Scrim)
    }

    fn hit_at(&self, pos: Point) -> Option<Hit> {
        // While the sheet is sliding, swallow touches so nothing lands on a
        // moving target (the 240 ms window is shorter than a deliberate tap).
        if !self.hist_tween.done() {
            return Some(Hit::Panel);
        }
        if let Some(h) = self.history_hit(pos) {
            return Some(h);
        }
        if history_btn_rect(self.size.w).contains(pos) {
            return Some(Hit::HistToggle);
        }
        let layout = keypad_layout(self.size.w, self.size.h);
        layout
            .keys
            .iter()
            .position(|k| k.rect.contains(pos))
            .map(Hit::Key)
    }

    fn hit_rect(&self, hit: Hit) -> Rect {
        match hit {
            Hit::Key(i) => keypad_layout(self.size.w, self.size.h)
                .keys
                .get(i)
                .map_or(Rect::ZERO, |k| k.rect),
            Hit::HistRow(i) => self.hist_row_rect(i),
            Hit::HistClear => {
                let p = self.history_progress();
                let panel = history_panel(self.size.w, self.size.h, p);
                history_layout(panel, self.history.len()).clear
            }
            Hit::HistToggle => history_btn_rect(self.size.w),
            _ => Rect::ZERO,
        }
    }

    fn hit_inside(&self, hit: Hit, pos: Point) -> bool {
        hit.sticky() || self.hit_rect(hit).contains(pos)
    }

    fn activate(&mut self, hit: Hit) {
        match hit {
            Hit::Key(i) => {
                let layout = keypad_layout(self.size.w, self.size.h);
                let Some(key) = layout.keys.get(i) else {
                    return;
                };
                match key.kind {
                    KeyKind::Digit(d) => self.input_char(d),
                    KeyKind::Dot => self.input_char('.'),
                    KeyKind::Percent => self.input_char('%'),
                    KeyKind::Negate => self.negate(),
                    KeyKind::Clear => self.clear(),
                    KeyKind::Backspace => self.backspace(),
                    KeyKind::Op(op) => {
                        let c = op.ch();
                        self.input_char(c);
                    }
                    KeyKind::Equals => self.commit(),
                }
            }
            Hit::HistRow(i) => self.reuse_history(i),
            Hit::HistToggle => {
                // History pill: toggle the sheet.
                if self.history_open {
                    self.close_history();
                } else {
                    self.open_history();
                }
            }
            Hit::HistClear => {
                self.history.clear();
                self.hist_scroll.scroll_to(0.0);
            }
            Hit::Scrim => self.close_history(),
            Hit::Panel => {}
        }
    }

    // -- frame -----------------------------------------------------------------

    fn frame(&self, dirty: bool, animating: bool) -> AppFrame {
        AppFrame {
            dirty,
            animating,
            quit: false,
        }
    }

    fn tick_anims(&mut self, dt: Duration) -> bool {
        self.now_ms += dt.as_millis() as u64;
        let mut running = false;
        if !self.hist_tween.done() {
            running |= self.hist_tween.tick(dt);
        }
        if self.history_open || !self.hist_tween.done() {
            self.sync_hist_bounds();
        }
        running |= self.hist_scroll.tick(dt.as_secs_f32());
        if let Some((_, t)) = &mut self.flash {
            *t += dt.as_secs_f32() / PRESS_FADE_S;
            if *t >= 1.0 {
                self.flash = None;
            } else {
                running = true;
            }
        }
        if let Some((_, hit, inside)) = self.pressed {
            self.press_t = (self.press_t + dt.as_secs_f32() / PRESS_FADE_S).min(1.0);
            if self.press_t < 1.0 {
                running = true;
            }
            // Long-press ⌫ clears the whole expression (AOSP behavior).
            if inside && !self.long_fired {
                self.press_ms += dt.as_millis() as f32;
                if self.press_ms >= BACKSPACE_HOLD_MS {
                    if let Hit::Key(i) = hit {
                        let layout = keypad_layout(self.size.w, self.size.h);
                        if layout.keys.get(i).is_some_and(|k| k.kind == KeyKind::Backspace) {
                            self.long_fired = true;
                            self.clear();
                            self.flash = Some((hit, 0.0));
                        }
                    }
                }
                running = true; // keep the hold timer ticking
            }
        }
        running
    }
}

impl AppState for CalcApp {
    fn app_id(&self) -> &'static str {
        "rabbiz-calculator"
    }
    fn title(&self) -> &'static str {
        "Calculator"
    }

    fn handle_event(&mut self, event: AppEvent, _ti: &TextInputCtl) -> AppFrame {
        match event {
            AppEvent::TouchDown { id, pos } => {
                if self.pressed.is_some() {
                    return self.frame(false, false); // one touch at a time
                }
                let Some(hit) = self.hit_at(pos) else {
                    return self.frame(false, false);
                };
                self.pressed = Some((id, hit, true));
                self.press_t = 0.0;
                self.press_ms = 0.0;
                self.long_fired = false;
                // A press that starts inside the history sheet can become a
                // list scroll (touch slop decides in TouchMove).
                if matches!(hit, Hit::HistRow(_) | Hit::Panel | Hit::HistClear) {
                    self.sync_hist_bounds();
                    self.hist_scroll.on_touch_down(pos.y as f32, self.now_ms);
                }
                self.frame(true, true)
            }
            AppEvent::TouchMove { id, pos } => {
                let Some((tid, hit, inside)) = self.pressed else {
                    return self.frame(false, false);
                };
                if tid != id {
                    return self.frame(false, false);
                }
                if self.hist_scroll.on_touch_move(pos.y as f32, self.now_ms) {
                    return self.frame(true, true);
                }
                let now_inside = self.hit_inside(hit, pos);
                if now_inside != inside {
                    self.pressed = Some((tid, hit, now_inside));
                    self.frame(true, false)
                } else {
                    self.frame(false, false)
                }
            }
            AppEvent::TouchUp { id, pos } => {
                let Some((tid, hit, inside)) = self.pressed.take() else {
                    return self.frame(false, false);
                };
                if tid != id {
                    self.pressed = Some((tid, hit, inside));
                    return self.frame(false, false);
                }
                // A scroll drag is consumed by the list, not treated as a tap.
                let was_drag = self.hist_scroll.on_touch_up(self.now_ms);
                let still_inside = self.hit_inside(hit, pos);
                if inside && still_inside && !was_drag && !self.long_fired {
                    self.activate(hit);
                }
                if matches!(hit, Hit::Key(_)) {
                    self.flash = Some((hit, 0.0));
                }
                self.frame(true, true)
            }
            AppEvent::Key { utf8: Some(s), .. } => {
                for ch in s.chars() {
                    match ch {
                        '0'..='9' | '.' | '%' => self.input_char(ch),
                        '+' => self.input_char('+'),
                        '-' | '−' => self.input_char('−'),
                        '*' | '×' | 'x' | 'X' => self.input_char('×'),
                        '/' | '÷' => self.input_char('÷'),
                        '=' | '\r' | '\n' => self.commit(),
                        _ => {}
                    }
                }
                self.frame(true, false)
            }
            AppEvent::Key { keysym: 0xff08, .. } => {
                self.backspace();
                self.frame(true, false)
            }
            AppEvent::TextCommit(s) => {
                for ch in s.chars() {
                    match ch {
                        '0'..='9' | '.' | '%' | '+' | '−' | '×' | '÷' | '=' => {
                            self.input_char(ch)
                        }
                        '-' => self.input_char('−'),
                        '*' | 'x' | 'X' => self.input_char('×'),
                        '/' => self.input_char('÷'),
                        _ => {}
                    }
                }
                self.frame(true, false)
            }
            AppEvent::TextBackspace(n) => {
                for _ in 0..n {
                    self.backspace();
                }
                self.frame(true, false)
            }
            AppEvent::Tick(dt) => {
                let running = self.tick_anims(dt);
                // Dirty while animating so each tick repaints; the final tick
                // also repaints the settled state, then animation stops.
                self.frame(true, running)
            }
            _ => self.frame(false, false),
        }
    }

    fn render(&mut self, canvas: &mut Canvas, theme: &Theme) {
        self.size = canvas.size();
        ui::render(self, canvas, theme);
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // Headless modes take over before the GUI starts; no args → GUI as usual.
    if args.iter().any(|a| a == "--mcp") {
        mcp::serve();
    }
    if let Some(pos) = args.iter().position(|a| a == "--cli") {
        std::process::exit(cli::run(&args[pos + 1..]));
    }
    rabbiz_app_kit::run_app(CalcApp::new());
}

// ---------------------------------------------------------------------------
// Tests (headless, no Wayland).
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_state_smoke() {
        let mut app = CalcApp::new();
        assert_eq!(app.app_id(), "rabbiz-calculator");
        assert_eq!(app.title(), "Calculator");
        app.input_char('2');
        app.input_char('+');
        app.input_char('3');
        assert_eq!(app.preview, "5");
        app.commit();
        assert_eq!(app.expr, "5");
        assert_eq!(app.history.len(), 1);
        assert_eq!(app.history[0].expr, "2+3");
        assert_eq!(app.history[0].result, "5");
        // Division by zero shows error, keeps expression editable.
        app.input_char('÷');
        app.input_char('0');
        app.input_char('=');
        assert!(app.error.is_some());
        assert_eq!(app.expr, "5÷0");
    }

    #[test]
    fn test_harness_renders_and_handles_keys_and_backspace() {
        let mut h = rabbiz_app_kit::TestHarness::new(CalcApp::new());
        let initial_px = h.render();
        assert_eq!(initial_px.size, Size::new(720, 1600));

        h.key(rabbiz_app_kit::sim_config::char_to_xkb('7'), Some("7"));
        h.key(rabbiz_app_kit::sim_config::char_to_xkb('*'), Some("*"));
        h.key(rabbiz_app_kit::sim_config::char_to_xkb('6'), Some("6"));
        h.key(rabbiz_app_kit::sim_config::char_to_xkb('9'), Some("9"));
        assert_eq!(h.state().expr, "7×69");

        // Backspace (0xFF08 with utf8: None) removes '9' leaving "7×6" = 42.
        h.key(rabbiz_app_kit::sim_config::XKB_BACKSPACE, None);
        assert_eq!(h.state().expr, "7×6");
        assert_eq!(h.state().preview, "42");

        h.key(rabbiz_app_kit::sim_config::XKB_RETURN, Some("\r"));
        assert_eq!(h.state().expr, "42");

        let after_px = h.render();
        assert_ne!(initial_px.data, after_px.data);
    }

    /// Evidence screenshots: renders the main screens to
    /// `/tmp/rabbiz-shots/calculator-<tag>-<state>.png`. `SHOT_TAG` selects
    /// the filename tag (`before`/`after`); defaults to `current`.
    #[test]
    fn shots() {
        use std::path::Path;
        let tag = std::env::var("SHOT_TAG").unwrap_or_else(|_| "current".into());
        let dir = Path::new("/tmp/rabbiz-shots");
        let mut h = rabbiz_app_kit::TestHarness::new(CalcApp::new());

        h.save_png(&dir.join(format!("calculator-{tag}-idle.png")))
            .expect("save idle");

        // Expression with a pending operator (accent highlight on ×).
        h.type_text("7×69");
        assert_eq!(h.state().expr, "7×69");
        h.save_png(&dir.join(format!("calculator-{tag}-expr.png")))
            .expect("save expr");

        // Commit a few results so history has content.
        h.type_text("=");
        h.type_text("9+9=");
        h.type_text("100×3=");
        h.type_text("50-8=");
        assert_eq!(h.state().history.len(), 4);

        // Open the history sheet via its affordance button and let the
        // slide-down tween finish.
        let r = history_btn_rect(720);
        h.tap(Point::new(r.x + r.w / 2, r.y + r.h / 2));
        h.settle(60, Duration::from_millis(16));
        h.save_png(&dir.join(format!("calculator-{tag}-history.png")))
            .expect("save history");
    }
}
