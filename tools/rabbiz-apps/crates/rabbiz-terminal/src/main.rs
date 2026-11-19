//! RabbizOS Terminal — command input + scrollback per docs/BASE-APPS.md.
//!
//! MVP model: one command at a time through `script -qfec` (PTY), run
//! synchronously — the app-kit event loop has no frame timer (`Tick` only
//! fires after Wayland events), so an async worker's output would stay
//! invisible until the next touch. Commands therefore block the UI for
//! their duration; the "full PTY session" milestone brings real async.
//! Output is ANSI-stripped and assembled into scrollback lines. Line editing
//! (cursor, word ops, history up/down), drag-scrollback, quick-key rows
//! (Termux/a-Shell convention: ESC/TAB/CTRL/arrows as on-screen keys since
//! the OSK has none).
//!
//! Headless modes live in `headless.rs`: `--cli run <cmd>` text CLI and
//! `--mcp` MCP server (`terminal_run` tool) share the same
//! `script -qfec` execution path via `headless::run`.
//!
//! Deliberate theme choice: the terminal surface uses `theme.canvas` +
//! `theme.text` in BOTH light and dark — token-pure, so the cream theme gets
//! a cream terminal rather than a hardcoded dark rectangle.

mod ansi;
mod edit;
mod headless;

use std::path::{Path, PathBuf};

use rabbiz_app_kit::{text, ui, AppEvent, AppFrame, AppState, Canvas, Rect, TextInputCtl, Theme};
use wayland_protocols::wp::text_input::zv3::client::zwp_text_input_v3::ContentPurpose;

use edit::{ctrl_char, input_window, wrap_cols, History, LineEdit};

const MAX_LINES: usize = 2000;
const TERM_SIZE: f32 = 18.0;
const TERM_LH: i32 = 26;
const KEY_H: i32 = 46;
const KEY_GAP: i32 = 8;
const KEY_ROWS: usize = 2;

/// What a quick-key button does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Act {
    Esc,
    Tab,
    Ctrl,
    Clear,
    Run,
    Sym(char),
    Left,
    Right,
    Up,
    Down,
}

#[derive(Clone, Copy)]
struct KeyDef {
    label: &'static str,
    act: Act,
}

const KEYS: &[KeyDef] = &[
    // Row 1 — editing keys (Termux extra-keys convention).
    KeyDef {
        label: "Esc",
        act: Act::Esc,
    },
    KeyDef {
        label: "Tab",
        act: Act::Tab,
    },
    KeyDef {
        label: "←",
        act: Act::Left,
    },
    KeyDef {
        label: "→",
        act: Act::Right,
    },
    KeyDef {
        label: "↑",
        act: Act::Up,
    },
    KeyDef {
        label: "↓",
        act: Act::Down,
    },
    KeyDef {
        label: "Ctrl",
        act: Act::Ctrl,
    },
    KeyDef {
        label: "Clear",
        act: Act::Clear,
    },
    // Row 2 — symbols + run.
    KeyDef {
        label: "-",
        act: Act::Sym('-'),
    },
    KeyDef {
        label: "/",
        act: Act::Sym('/'),
    },
    KeyDef {
        label: "~",
        act: Act::Sym('~'),
    },
    KeyDef {
        label: ".",
        act: Act::Sym('.'),
    },
    KeyDef {
        label: "=",
        act: Act::Sym('='),
    },
    KeyDef {
        label: "|",
        act: Act::Sym('|'),
    },
    KeyDef {
        label: ";",
        act: Act::Sym(';'),
    },
    KeyDef {
        label: "Run",
        act: Act::Run,
    },
];

struct Layout {
    log: Rect,
    prompt: Rect,
    keys: Vec<(Rect, usize)>, // rect + index into KEYS
    cols: usize,
    visible: usize,
}

/// Everything render computes that events also need (hit rects + grid math).
struct Geom {
    log: Rect,
    prompt: Rect,
    keys: Vec<(Rect, usize)>,
    visible: usize,
}

struct TerminalApp {
    cwd: PathBuf,
    edit: LineEdit,
    lines: Vec<String>,
    /// Bumped on every scrollback mutation so the wrap cache can't go stale
    /// when `clear` + repopulation lands on the same `lines.len()`.
    gen: u64,
    hist: History,
    scroll: i32,
    drag: Option<(i32, i32)>, // (start_y, start_scroll)
    ctrl: bool,
    osk_active: bool,
    pressed: Option<usize>,
    wrapped: Vec<String>,
    wrap_key: (usize, usize, u64), // (cols, lines.len, gen)
    geom: Option<Geom>,
    blink: f32,
    cursor_on: bool,
}

impl TerminalApp {
    fn new() -> Self {
        Self {
            cwd: home_dir(),
            edit: LineEdit::new(),
            lines: vec!["RabbizOS Terminal — type a command, ↵ to run".into()],
            gen: 0,
            hist: History::default(),
            scroll: 0,
            drag: None,
            ctrl: false,
            osk_active: false,
            pressed: None,
            wrapped: Vec::new(),
            wrap_key: (usize::MAX, usize::MAX, u64::MAX),
            geom: None,
            blink: 0.0,
            cursor_on: true,
        }
    }

    fn layout(&self, w: i32, h: i32, top: i32) -> Layout {
        let m = ui::MARGIN;
        // Bottom-up: quick keys, prompt card, scrollback fills the rest.
        let keys_h = KEY_ROWS as i32 * KEY_H + (KEY_ROWS as i32 - 1) * KEY_GAP;
        let keys_top = h - ui::BOTTOM_SAFE - keys_h;
        let prompt = Rect::new(m, keys_top - 10 - 58, w - m * 2, 58);
        let log = Rect::new(m, top + 8, w - m * 2, (prompt.y - 10 - top - 8).max(80));

        let cw = cell_w_for().max(1) as usize;
        let cols = ((log.w - 32).max(1) as usize / cw).max(1);
        let visible = ((log.h - 16).max(1) / TERM_LH) as usize;

        let n_keys = KEYS.len();
        let per_row = n_keys.div_ceil(KEY_ROWS);
        let mut keys = Vec::with_capacity(n_keys);
        for i in 0..n_keys {
            let row = i / per_row;
            let col = i % per_row;
            let in_row = (n_keys - row * per_row).min(per_row);
            let bw = (w - m * 2 - KEY_GAP * (in_row as i32 - 1)) / in_row as i32;
            let x = m + col as i32 * (bw + KEY_GAP);
            let y = keys_top + row as i32 * (KEY_H + KEY_GAP);
            keys.push((Rect::new(x, y, bw, KEY_H), i));
        }
        Layout {
            log,
            prompt,
            keys,
            cols,
            visible,
        }
    }

    fn push_line(&mut self, s: String) {
        self.lines.push(s);
        self.gen += 1;
        if self.lines.len() > MAX_LINES {
            let drain = self.lines.len() - MAX_LINES;
            self.lines.drain(..drain);
        }
    }

    fn push_lines(&mut self, new: Vec<String>) {
        for l in new {
            self.push_line(l);
        }
        self.scroll = 0;
    }

    fn clear_lines(&mut self) {
        self.lines.clear();
        self.gen += 1;
        self.scroll = 0;
    }

    /// Submit the input line: builtin (clear/cd/exit) or run the command.
    /// Returns true when the app should quit.
    fn submit(&mut self) -> bool {
        let cmd = self.edit.as_str().trim().to_string();
        self.edit.set("");
        self.ctrl = false;
        if cmd.is_empty() {
            return false;
        }
        self.hist.push(&cmd);
        self.push_line(format!("$ {cmd}"));

        if cmd == "clear" {
            self.clear_lines();
            return false;
        }
        if cmd == "cd" || cmd.starts_with("cd ") {
            let rest = cmd[2..].trim();
            if rest.is_empty() {
                self.cwd = home_dir();
            } else {
                let target = rest.trim_matches('"').trim_matches('\'');
                let next = expand_tilde(target, &self.cwd);
                if next.is_dir() {
                    self.cwd = next.canonicalize().unwrap_or(next);
                } else {
                    self.push_line(format!("cd: no such directory: {target}"));
                }
            }
            return false;
        }
        if matches!(cmd.as_str(), "exit" | "logout" | "quit") {
            return true;
        }

        // Synchronous PTY run (see module docs for why not a worker thread).
        // Same execution path as `--cli`/`--mcp` via headless::run.
        match headless::run(&cmd, &self.cwd, None) {
            Ok(out) => self.push_lines(out.lines),
            Err(e) => self.push_lines(vec![format!("error: {e}")]),
        }
        false
    }

    /// Apply a quick-key action. Returns true when the app should quit.
    fn act(&mut self, act: Act) -> bool {
        match act {
            Act::Esc => {
                self.edit.set("");
                self.ctrl = false;
            }
            Act::Tab => {
                self.edit.insert_str("\t");
                self.ctrl = false;
            }
            Act::Ctrl => self.ctrl = !self.ctrl,
            Act::Clear => self.clear_lines(),
            Act::Run => return self.submit(),
            Act::Sym(c) => {
                if self.ctrl {
                    if let Some(cc) = ctrl_char(c) {
                        self.edit.insert_str(&cc.to_string());
                    }
                } else {
                    self.edit.insert_str(&c.to_string());
                }
                self.ctrl = false;
            }
            Act::Left => {
                self.edit.left();
                self.ctrl = false;
            }
            Act::Right => {
                self.edit.right();
                self.ctrl = false;
            }
            Act::Up => {
                let s = self.hist.up(self.edit.as_str());
                self.edit.set(&s);
                self.ctrl = false;
            }
            Act::Down => {
                let s = self.hist.down(self.edit.as_str());
                self.edit.set(&s);
                self.ctrl = false;
            }
        }
        false
    }

    /// Insert committed/typed text at the cursor, honoring the Ctrl latch
    /// for single chars and readline control characters.
    fn text_input(&mut self, s: &str) {
        let mut chars = s.chars();
        if let (Some(c), None) = (chars.next(), chars.next()) {
            if self.ctrl {
                self.ctrl = false;
                if let Some(cc) = ctrl_char(c) {
                    self.edit.insert_str(&cc.to_string());
                }
                return;
            }
            if let Some(op) = control_op(c) {
                match op {
                    ControlOp::Insert(ch) => self.edit.insert_str(&ch.to_string()),
                    ControlOp::Backspace => {
                        self.edit.backspace();
                    }
                    ControlOp::DeleteFwd => {
                        self.edit.delete_fwd();
                    }
                    ControlOp::Home => self.edit.home(),
                    ControlOp::End => self.edit.end(),
                    ControlOp::KillWord => {
                        self.edit.kill_word_back();
                    }
                    ControlOp::KillBefore => {
                        self.edit.kill_before_cursor();
                    }
                    ControlOp::KillAfter => {
                        self.edit.kill_after_cursor();
                    }
                    ControlOp::ClearInput => self.edit.set(""),
                    ControlOp::ClearScreen => self.clear_lines(),
                }
                return;
            }
            if c.is_control() {
                // Stray C0 (ESC etc.) with no binding: drop it.
                return;
            }
        }
        self.ctrl = false;
        self.edit.insert_str(s);
    }

    fn refresh_wrap(&mut self, cols: usize) {
        let key = (cols, self.lines.len(), self.gen);
        if key == self.wrap_key {
            return;
        }
        self.wrap_key = key;
        self.wrapped = self.lines.iter().flat_map(|l| wrap_cols(l, cols)).collect();
        let max = self.max_scroll();
        if self.scroll > max {
            self.scroll = max;
        }
    }

    /// How far the view may scroll up from the bottom, in wrapped rows.
    fn max_scroll(&self) -> i32 {
        let visible = self.geom.as_ref().map_or(1, |g| g.visible.max(1)) as i64;
        (self.wrapped.len() as i64 - visible).max(0) as i32
    }
}

enum ControlOp {
    Insert(char),
    Backspace,
    DeleteFwd,
    Home,
    End,
    KillWord,
    KillBefore,
    KillAfter,
    ClearInput,
    ClearScreen,
}

/// Readline-style control characters arriving as single utf8 chars
/// (Ctrl+A/E/W/U/K/D/C/L, DEL, Tab).
fn control_op(c: char) -> Option<ControlOp> {
    Some(match c {
        '\u{1}' => ControlOp::Home,         // Ctrl+A
        '\u{5}' => ControlOp::End,          // Ctrl+E
        '\u{17}' => ControlOp::KillWord,    // Ctrl+W
        '\u{15}' => ControlOp::KillBefore,  // Ctrl+U
        '\u{B}' => ControlOp::KillAfter,    // Ctrl+K
        '\u{4}' => ControlOp::DeleteFwd,    // Ctrl+D
        '\u{3}' => ControlOp::ClearInput,   // Ctrl+C
        '\u{C}' => ControlOp::ClearScreen,  // Ctrl+L
        '\u{7F}' => ControlOp::Backspace,   // DEL
        '\u{9}' => ControlOp::Insert('\t'), // Tab
        _ => return None,
    })
}

pub(crate) fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

fn expand_tilde(target: &str, cwd: &Path) -> PathBuf {
    if target == "~" || target.starts_with("~/") {
        let mut p = home_dir();
        if let Some(rest) = target.strip_prefix("~/") {
            p.push(rest);
        }
        p
    } else if target.starts_with('/') {
        PathBuf::from(target)
    } else {
        cwd.join(target)
    }
}

/// Width of one terminal cell, measured from the bundled font's digit
/// advance (Noto digits are tabular). Cached per process.
fn cell_w_for() -> i32 {
    use std::sync::OnceLock;
    static W: OnceLock<i32> = OnceLock::new();
    *W.get_or_init(|| {
        let mut st = ui::styled(TERM_SIZE, 400, [255, 255, 255, 255]);
        st.line_height = TERM_LH as f32;
        text::label("0", st, None)
            .map(|px| px.width() as i32)
            .unwrap_or(10)
            .max(6)
    })
}

fn disp_cwd(cwd: &Path) -> String {
    let home = home_dir();
    if let Ok(rest) = cwd.strip_prefix(&home) {
        if rest.as_os_str().is_empty() {
            return "~".into();
        }
        return format!("~/{}", rest.display());
    }
    cwd.display().to_string()
}

impl AppState for TerminalApp {
    fn app_id(&self) -> &'static str {
        "rabbiz-terminal"
    }
    fn title(&self) -> &'static str {
        "Terminal"
    }

    fn handle_event(&mut self, event: AppEvent, ti: &TextInputCtl) -> AppFrame {
        let mut dirty = false;
        let mut quit = false;
        match event {
            AppEvent::TouchDown { pos, .. } => {
                if let Some(g) = &self.geom {
                    if let Some((_, ki)) = g.keys.iter().find(|(r, _)| r.contains(pos)) {
                        self.pressed = Some(*ki);
                    } else if g.prompt.contains(pos) {
                        self.osk_active = true;
                        ti.activate(ContentPurpose::Terminal);
                        ti.set_surrounding(self.edit.as_str(), self.edit.cursor());
                    } else if g.log.contains(pos) {
                        self.drag = Some((pos.y, self.scroll));
                    }
                }
                dirty = true;
            }
            AppEvent::TouchMove { pos, .. } => {
                if let Some((sy, ss)) = self.drag {
                    let rows = (sy - pos.y) / TERM_LH;
                    let next = (ss + rows).clamp(0, self.max_scroll());
                    if next != self.scroll {
                        self.scroll = next;
                        dirty = true;
                    }
                }
            }
            AppEvent::TouchUp { pos, .. } => {
                if let Some(ki) = self.pressed.take() {
                    if let Some(g) = &self.geom {
                        if let Some((r, _)) = g.keys.iter().find(|(_, i)| *i == ki) {
                            if r.contains(pos) {
                                quit = self.act(KEYS[ki].act);
                            }
                        }
                    }
                }
                self.drag = None;
                dirty = true;
            }
            AppEvent::TextCommit(s) => {
                self.text_input(&s);
                dirty = true;
            }
            AppEvent::TextBackspace(n) => {
                for _ in 0..n.max(1) {
                    self.edit.backspace();
                }
                dirty = true;
            }
            AppEvent::Key { keysym, utf8 } => {
                match keysym {
                    0xFF0D => quit = self.submit(), // Return
                    0xFF08 => {
                        self.edit.backspace();
                    }
                    0xFFFF => {
                        self.edit.delete_fwd();
                    } // Delete
                    0xFF51 => self.edit.left(),
                    0xFF53 => self.edit.right(),
                    0xFF50 => self.edit.home(),
                    0xFF57 => self.edit.end(),
                    0xFF52 => {
                        let s = self.hist.up(self.edit.as_str());
                        self.edit.set(&s);
                    }
                    0xFF54 => {
                        let s = self.hist.down(self.edit.as_str());
                        self.edit.set(&s);
                    }
                    0xFF09 => self.edit.insert_str("\t"),
                    0xFF1B => {
                        self.edit.set("");
                        self.ctrl = false;
                    }
                    _ => {
                        if let Some(s) = utf8 {
                            self.text_input(&s);
                        }
                    }
                }
                dirty = true;
            }
            AppEvent::Tick(dt) => {
                // Cursor blink phase. Note: the kit only dispatches Tick
                // after Wayland events, so the blink repaints on activity,
                // not on a wall-clock timer.
                self.blink += dt.as_secs_f32();
                const PERIOD: f32 = 1.06;
                let on = (self.blink % PERIOD) < PERIOD / 2.0;
                if on != self.cursor_on {
                    self.cursor_on = on;
                    dirty = true;
                }
            }
            // Pointer hover/scroll/non-primary-button events (app-kit
            // mouse support) are not wired into the terminal yet.
            _ => {}
        }
        AppFrame {
            dirty,
            animating: self.osk_active,
            quit,
        }
    }

    fn render(&mut self, canvas: &mut Canvas, theme: &Theme) {
        let w = canvas.size().w;
        let h = canvas.size().h;
        let top = ui::draw_header(canvas, theme, "Terminal", &disp_cwd(&self.cwd));
        let lay = self.layout(w, h, top);

        let mut st = ui::styled(TERM_SIZE, 400, theme.text);
        st.line_height = TERM_LH as f32;
        let st_ui = ui::styled(17.0, 500, theme.text2);

        // Scrollback surface: theme.canvas (deliberate in both themes).
        canvas.fill_rounded_rect(lay.log, 20.0, theme.canvas);
        // 1px inner border in theme.line.
        canvas.fill_rect(Rect::new(lay.log.x, lay.log.y, lay.log.w, 1), theme.line);
        canvas.fill_rect(
            Rect::new(lay.log.x, lay.log.y + lay.log.h - 1, lay.log.w, 1),
            theme.line,
        );
        canvas.fill_rect(Rect::new(lay.log.x, lay.log.y, 1, lay.log.h), theme.line);
        canvas.fill_rect(
            Rect::new(lay.log.x + lay.log.w - 1, lay.log.y, 1, lay.log.h),
            theme.line,
        );

        self.refresh_wrap(lay.cols);
        let max_scroll = self.max_scroll();
        if self.scroll > max_scroll {
            self.scroll = max_scroll;
        }

        if self.wrapped.is_empty() {
            ui::draw_centered(
                canvas,
                "type a command — ↵ runs, ↑↓ history",
                &ui::caption(theme.text3),
                lay.log,
            );
        } else {
            let start = (self.wrapped.len() as i64 - lay.visible as i64 - self.scroll as i64).max(0)
                as usize;
            let end = (start + lay.visible).min(self.wrapped.len());
            for (i, row) in self.wrapped[start..end].iter().enumerate() {
                let y = lay.log.y + 12 + i as i32 * TERM_LH;
                if y + TERM_LH <= lay.log.y + lay.log.h {
                    ui::draw_text(canvas, row, &st, lay.log.x + 14, y, Some(lay.log.w - 28));
                }
            }
        }

        // Prompt card.
        ui::draw_card(canvas, theme, lay.prompt, false);
        let px = lay.prompt.x + 14;
        let py = lay.prompt.y + 12;
        let dollar = ui::draw_text(
            canvas,
            "$ ",
            &ui::styled(TERM_SIZE, 700, theme.accent),
            px,
            py,
            None,
        );
        let text_x = px + if dollar.w > 0 { dollar.w } else { 20 };

        let (bs, be) = input_window(self.edit.as_str(), self.edit.cursor(), lay.cols);
        let vis = &self.edit.as_str()[bs..be];
        ui::draw_text(
            canvas,
            vis,
            &st,
            text_x,
            py,
            Some(lay.prompt.w - (text_x - lay.prompt.x) - 14),
        );

        // Cursor block after the visible prefix, shown while the OSK is up
        // or on blink phase otherwise.
        if (bs..=be).contains(&self.edit.cursor()) && (self.osk_active || self.cursor_on) {
            let cur_byte = self.edit.cursor() - bs;
            let prefix = &vis[..cur_byte];
            let prefix_w = text::label(prefix, st, None)
                .map(|p| p.width() as i32)
                .unwrap_or(0);
            let cx = text_x + prefix_w;
            canvas.fill_rect(
                Rect::new(cx, py + 2, 3, TERM_LH - 6),
                if self.osk_active {
                    theme.accent
                } else {
                    theme.text3
                },
            );
        }

        // Status bits inside the prompt card, right side.
        if self.ctrl {
            ui::draw_text(
                canvas,
                "CTRL",
                &st_ui,
                lay.prompt.x + lay.prompt.w - 14 - 160,
                py + 4,
                Some(160),
            );
        }

        // Quick keys.
        for (i, (r, ki)) in lay.keys.iter().enumerate() {
            let kd = &KEYS[*ki];
            let primary = matches!(kd.act, Act::Run) || (kd.act == Act::Ctrl && self.ctrl);
            ui::draw_pill_button(
                canvas,
                theme,
                *r,
                kd.label,
                primary,
                self.pressed == Some(i),
            );
        }

        self.geom = Some(Geom {
            log: lay.log,
            prompt: lay.prompt,
            keys: lay.keys,
            visible: lay.visible,
        });
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(code) = headless::dispatch(&args) {
        std::process::exit(code);
    }
    rabbiz_app_kit::run_app(TerminalApp::new());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_tilde_and_cd_target() {
        let home = home_dir();
        assert_eq!(expand_tilde("~", &home), home);
        assert_eq!(expand_tilde("~/x", &home), home.join("x"));
        assert_eq!(expand_tilde("/tmp", &home), PathBuf::from("/tmp"));
        assert_eq!(
            expand_tilde("rel", Path::new("/a/b")),
            PathBuf::from("/a/b/rel")
        );
    }

    #[test]
    fn control_op_mapping() {
        assert!(matches!(control_op('\u{1}'), Some(ControlOp::Home)));
        assert!(matches!(control_op('\u{15}'), Some(ControlOp::KillBefore)));
        assert!(matches!(control_op('\u{3}'), Some(ControlOp::ClearInput)));
        assert!(matches!(control_op('\u{4}'), Some(ControlOp::DeleteFwd)));
        assert!(control_op('x').is_none());
    }

    #[test]
    fn keys_cover_two_rows() {
        assert_eq!(KEYS.len(), 16);
        assert!(KEYS.iter().any(|k| k.act == Act::Run));
        assert!(KEYS.iter().any(|k| k.act == Act::Ctrl));
    }

    #[test]
    fn cell_width_positive() {
        assert!(cell_w_for() >= 6);
    }
}
