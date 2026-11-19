use rabbiz_app_kit::{
    ui, AppEvent, AppFrame, AppState, Canvas, Point, Rect, TextInputCtl, Theme,
};

struct CalcApp {
    expr: String,
    result: String,
    pressed_idx: Option<usize>,
}

impl CalcApp {
    fn new() -> Self {
        Self {
            expr: String::new(),
            result: "0".into(),
            pressed_idx: None,
        }
    }

    fn key_rects(w: i32, h: i32) -> Vec<(Rect, &'static str, u8)> {
        // kind: 0 = digit, 1 = fn, 2 = op, 3 = eq
        let rows: [[(&str, u8); 4]; 5] = [
            [("AC", 1), ("⌫", 1), ("%", 1), ("÷", 2)],
            [("7", 0), ("8", 0), ("9", 0), ("×", 2)],
            [("4", 0), ("5", 0), ("6", 0), ("−", 2)],
            [("1", 0), ("2", 0), ("3", 0), ("+", 2)],
            [("±", 0), ("0", 0), (".", 0), ("=", 3)],
        ];
        let gap = 14;
        let pad_x = ui::MARGIN;
        let btn_w = (w - pad_x * 2 - gap * 3) / 4;
        let btn_h = btn_w.min(118);
        let grid_h = btn_h * 5 + gap * 4;
        let start_y = (h - ui::BOTTOM_SAFE - grid_h - 12).max(ui::TOP_SAFE + 240);
        let mut out = Vec::with_capacity(20);
        for (r, row) in rows.iter().enumerate() {
            for (c, (lbl, kind)) in row.iter().enumerate() {
                let x = pad_x + c as i32 * (btn_w + gap);
                let y = start_y + r as i32 * (btn_h + gap);
                out.push((Rect::new(x, y, btn_w, btn_h), *lbl, *kind));
            }
        }
        out
    }

    fn press_key(&mut self, lbl: &str) {
        match lbl {
            "AC" => {
                self.expr.clear();
                self.result = "0".into();
            }
            "⌫" => {
                self.expr.pop();
                self.recalc();
            }
            "=" => {
                self.recalc();
                if self.result != "Error" {
                    self.expr = self.result.clone();
                }
            }
            "±" => {
                if self.expr.starts_with('-') {
                    self.expr.remove(0);
                } else if !self.expr.is_empty() {
                    self.expr.insert(0, '-');
                }
                self.recalc();
            }
            s => {
                self.expr.push_str(s);
                self.recalc();
            }
        }
    }

    fn recalc(&mut self) {
        if self.expr.trim().is_empty() {
            self.result = "0".into();
            return;
        }
        self.result = eval_expr(&self.expr)
            .map(format_num)
            .unwrap_or_else(|| "Error".into());
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
            AppEvent::TouchDown { pos, .. } => {
                let rects = Self::key_rects(720, 1600);
                self.pressed_idx = rects.iter().position(|(r, _, _)| r.contains(pos));
                AppFrame { dirty: true, ..Default::default() }
            }
            AppEvent::TouchUp { pos, .. } => {
                let rects = Self::key_rects(720, 1600);
                if let Some(i) = self.pressed_idx.take() {
                    if let Some((r, lbl, _)) = rects.get(i) {
                        if r.contains(pos) {
                            self.press_key(lbl);
                        }
                    }
                }
                AppFrame { dirty: true, ..Default::default() }
            }
            AppEvent::Key { utf8: Some(s), .. } => {
                for ch in s.chars() {
                    match ch {
                        '0'..='9' | '.' | '+' | '%' => self.press_key(&ch.to_string()),
                        '-' => self.press_key("−"),
                        '*' => self.press_key("×"),
                        '/' => self.press_key("÷"),
                        '\r' | '\n' | '=' => self.press_key("="),
                        _ => {}
                    }
                }
                AppFrame { dirty: true, ..Default::default() }
            }
            AppEvent::Key { keysym: 0xff08, .. } => {
                self.press_key("⌫");
                AppFrame { dirty: true, ..Default::default() }
            }
            _ => AppFrame::default(),
        }
    }

    fn render(&mut self, canvas: &mut Canvas, theme: &Theme) {
        let w = canvas.size().w;
        let h = canvas.size().h;
        let top = ui::draw_header(canvas, theme, "Calculator", "");

        let rects = Self::key_rects(w, h);
        let disp_h = (rects[0].0.y - top - 24).max(160);
        let disp_rect = Rect::new(ui::MARGIN, top + 8, w - ui::MARGIN * 2, disp_h);
        ui::draw_card(canvas, theme, disp_rect, false);

        let expr_str = if self.expr.is_empty() { " " } else { &self.expr };
        ui::draw_text(
            canvas,
            expr_str,
            &ui::styled(34.0, 400, theme.text2),
            disp_rect.x + 24,
            disp_rect.y + 28,
            Some(disp_rect.w - 48),
        );
        ui::draw_text(
            canvas,
            &self.result,
            &ui::display(theme.text),
            disp_rect.x + 24,
            disp_rect.y + disp_rect.h - 92,
            Some(disp_rect.w - 48),
        );

        for (i, (r, lbl, kind)) in rects.iter().enumerate() {
            let pressed = self.pressed_idx == Some(i);
            let bg = match kind {
                3 => theme.accent,
                2 => theme.surface3,
                1 => theme.surface2,
                _ => theme.surface1,
            };
            let fg = if *kind == 3 { theme.text_on_accent } else { theme.text };
            let rr = if pressed {
                Rect::new(r.x + 3, r.y + 3, (r.w - 6).max(1), (r.h - 6).max(1))
            } else {
                *r
            };
            let rad = if *kind == 3 { 28.0 } else { (rr.h as f32) / 2.0 };
            canvas.fill_rounded_rect(rr, rad, bg);
            if pressed {
                canvas.fill_rounded_rect(rr, rad, [255, 255, 255, 28]);
            }
            ui::draw_centered(canvas, lbl, &ui::styled(32.0, 600, fg), rr);
        }
        let _ = Point::ZERO;
    }
}

fn format_num(v: f64) -> String {
    if !v.is_finite() {
        return "Error".into();
    }
    if (v - v.round()).abs() < 1e-10 && v.abs() < 1e14 {
        return format!("{}", v.round() as i64);
    }
    let s = format!("{v:.8}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn eval_expr(s: &str) -> Option<f64> {
    let norm = s.replace('×', "*").replace('÷', "/").replace('−', "-");
    let bytes = norm.as_bytes();
    let mut i = 0;
    let val = parse_add(bytes, &mut i)?;
    if i < bytes.len() {
        return None;
    }
    Some(val)
}

fn parse_add(b: &[u8], i: &mut usize) -> Option<f64> {
    let mut v = parse_mul(b, i)?;
    while *i < b.len() {
        match b[*i] {
            b'+' => { *i += 1; v += parse_mul(b, i)?; }
            b'-' => { *i += 1; v -= parse_mul(b, i)?; }
            _ => break,
        }
    }
    Some(v)
}

fn parse_mul(b: &[u8], i: &mut usize) -> Option<f64> {
    let mut v = parse_unary(b, i)?;
    while *i < b.len() {
        match b[*i] {
            b'*' => { *i += 1; v *= parse_unary(b, i)?; }
            b'/' => {
                *i += 1;
                let r = parse_unary(b, i)?;
                if r == 0.0 { return None; }
                v /= r;
            }
            b'%' => { *i += 1; v *= 0.01; }
            _ => break,
        }
    }
    Some(v)
}

fn parse_unary(b: &[u8], i: &mut usize) -> Option<f64> {
    if *i < b.len() && b[*i] == b'-' {
        *i += 1;
        return Some(-parse_unary(b, i)?);
    }
    let start = *i;
    while *i < b.len() && (b[*i].is_ascii_digit() || b[*i] == b'.') {
        *i += 1;
    }
    if *i == start {
        return None;
    }
    std::str::from_utf8(&b[start..*i]).ok()?.parse().ok()
}

fn main() {
    rabbiz_app_kit::run_app(CalcApp::new());
}
