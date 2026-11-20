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
    history_btn_rect, history_panel, history_rows, keypad_layout, Hit, HIST_CLOSE_S, HIST_OPEN_S,
    PRESS_FADE_S,
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

    fn history_hit(&self, pos: Point) -> Option<Hit> {
        let p = self.history_progress();
        if p <= 0.5 {
            return None;
        }
        let panel = history_panel(self.size.w, self.size.h, p);
        if panel.contains(pos) {
            let (_, clear, rows) = history_rows(panel, self.history.len());
            if clear.contains(pos) {
                return Some(Hit::HistClear);
            }
            // Rows are stored top→bottom = newest→oldest.
            for (i, r) in rows.iter().enumerate() {
                if r.contains(pos) {
                    return Some(Hit::HistRow(i));
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
            Hit::HistRow(i) => {
                let p = self.history_progress();
                let panel = history_panel(self.size.w, self.size.h, p);
                let (_, _, rows) = history_rows(panel, self.history.len());
                rows.get(i).copied().unwrap_or(Rect::ZERO)
            }
            Hit::HistClear => {
                let p = self.history_progress();
                let panel = history_panel(self.size.w, self.size.h, p);
                let (_, clear, _) = history_rows(panel, self.history.len());
                clear
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
        let mut running = false;
        if !self.hist_tween.done() {
            running |= self.hist_tween.tick(dt);
        }
        if let Some((_, t)) = &mut self.flash {
            *t += dt.as_secs_f32() / PRESS_FADE_S;
            if *t >= 1.0 {
                self.flash = None;
            } else {
                running = true;
            }
        }
        if self.pressed.is_some() {
            self.press_t = (self.press_t + dt.as_secs_f32() / PRESS_FADE_S).min(1.0);
            if self.press_t < 1.0 {
                running = true;
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
                self.frame(true, true)
            }
            AppEvent::TouchMove { id, pos } => {
                let Some((tid, hit, inside)) = self.pressed else {
                    return self.frame(false, false);
                };
                if tid != id {
                    return self.frame(false, false);
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
                let still_inside = self.hit_inside(hit, pos);
                if inside && still_inside {
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
}
