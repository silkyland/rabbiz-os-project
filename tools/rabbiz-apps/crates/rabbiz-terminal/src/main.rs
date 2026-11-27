//! RabbizOS Terminal — command input + scrollback per docs/BASE-APPS.md.
//!
//! MVP model: one command at a time through `script -qfec` (PTY), run
//! synchronously — the app-kit event loop has no frame timer (`Tick` only
//! fires while a frame reports `animating`), so an async worker's output
//! would stay invisible until the next touch. Commands therefore block the
//! UI for their duration; the "full PTY session" milestone brings real
//! async. Output is ANSI-stripped and assembled into scrollback lines. Line
//! editing (cursor, word ops, history up/down), fling scrollback with a
//! scrollbar + jump-to-bottom pill, and a horizontally scrollable quick-key
//! row (Termux extra-keys convention: Tab/Ctrl/arrows live on-screen since
//! the OSK has none).
//!
//! Headless modes live in `headless.rs`: `--cli run <cmd>` text CLI and
//! `--mcp` MCP server (`terminal_run` tool) share the same
//! `script -qfec` execution path via `headless::run`.
//!
//! Deliberate theme choice: the terminal surface uses the DARKER of
//! `theme.canvas`/`theme.text` for its fill and the lighter one for ink in
//! both themes — Android Terminal/Termux convention (a terminal stays dark
//! even in light mode) — while remaining token-pure.

mod ansi;
mod edit;
mod headless;

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Instant;

use rabbiz_app_kit::{
    icons,
    scroll::{self, ScrollView},
    text, tokens, ui, widgets, AppEvent, AppFrame, AppState, Canvas, Rect, TextInputCtl, Theme,
};
use wayland_protocols::wp::text_input::zv3::client::zwp_text_input_v3::ContentPurpose;

use edit::{ctrl_char, input_window, wrap_cols, History, LineEdit};

const MAX_LINES: usize = 2000;
const TERM_SIZE: f32 = 18.0;
const TERM_LH: i32 = 26;
/// Quick-key strip height — the ≥48px touch-target contract.
const KEY_H: i32 = 48;
const KEY_GAP: i32 = 8;
const KEY_MIN_W: i32 = 56;
const KEY_ICON: f32 = 24.0;
/// Prompt band height at the bottom of the terminal surface.
const PROMPT_H: i32 = 52;
/// Inner side padding inside the terminal surface.
const TERM_PAD: i32 = 14;
/// Lane reserved at the right of the scrollback viewport for the scrollbar.
const SCROLLBAR_LANE: i32 = 10;
/// Jump-to-bottom FAB edge (widgets::FabSize::Small).
const JUMP_SZ: i32 = 60;
/// Width always reserved for the CTRL badge so long input never runs under it.
const BADGE_W: i32 = 84;
/// Cursor blink period (s): half on, half off.
const BLINK_PERIOD: f32 = 1.06;
/// `arrow_forward` — verified against the bundled MaterialIconsRound cmap
/// (same check icons.rs runs); not yet exported by `rabbiz_app_kit::icons`.
const ICON_ARROW_FORWARD: &str = "\u{e5c8}";

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
    /// Text label (ignored when `icon` is set — arrow keys use Material
    /// glyphs because Noto has no ←→↑↓ and rasterized them as tofu).
    label: &'static str,
    icon: Option<&'static str>,
    act: Act,
}

/// One horizontally scrollable Termux-style extra-keys row. Editing keys
/// come first so the useful ones are visible without scrolling.
const KEYS: &[KeyDef] = &[
    // Clears the input line — honest label, it does NOT emit an ESC byte.
    KeyDef {
        label: "CLR",
        icon: None,
        act: Act::Esc,
    },
    KeyDef {
        label: "Tab",
        icon: None,
        act: Act::Tab,
    },
    KeyDef {
        label: "",
        icon: Some(icons::BACK), // arrow_back glyph
        act: Act::Left,
    },
    KeyDef {
        label: "",
        icon: Some(ICON_ARROW_FORWARD),
        act: Act::Right,
    },
    KeyDef {
        label: "",
        icon: Some(icons::ARROW_UPWARD),
        act: Act::Up,
    },
    KeyDef {
        label: "",
        icon: Some(icons::ARROW_DOWNWARD),
        act: Act::Down,
    },
    KeyDef {
        label: "Ctrl",
        icon: None,
        act: Act::Ctrl,
    },
    KeyDef {
        label: "-",
        icon: None,
        act: Act::Sym('-'),
    },
    KeyDef {
        label: "/",
        icon: None,
        act: Act::Sym('/'),
    },
    KeyDef {
        label: "~",
        icon: None,
        act: Act::Sym('~'),
    },
    KeyDef {
        label: ".",
        icon: None,
        act: Act::Sym('.'),
    },
    KeyDef {
        label: "=",
        icon: None,
        act: Act::Sym('='),
    },
    KeyDef {
        label: "|",
        icon: None,
        act: Act::Sym('|'),
    },
    KeyDef {
        label: ";",
        icon: None,
        act: Act::Sym(';'),
    },
    // Clears the scrollback (readline C-l convention).
    KeyDef {
        label: "CLS",
        icon: None,
        act: Act::Clear,
    },
    KeyDef {
        label: "Run",
        icon: None,
        act: Act::Run,
    },
];

/// Scrollback line class — `Err` renders in `theme.danger` so command
/// failures read at a glance (cd errors, exec errors, non-zero exits).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LineKind {
    Out,
    Err,
}

/// Everything render computes that events also need (hit rects + grid math).
struct Layout {
    /// Dark terminal surface (scrollback + prompt band), edge-to-edge.
    term: Rect,
    /// Touch zone for scrollback drag/tap (term minus the prompt band).
    log: Rect,
    /// Scrollback text viewport inside `term`.
    view: Rect,
    /// Prompt band at the bottom of `term`.
    prompt: Rect,
    /// Quick-key strip (content scrolls horizontally inside it).
    keys_row: Rect,
    keys: Vec<(Rect, usize)>, // rect + index into KEYS
    /// Max horizontal scroll offset for the key strip.
    keys_max: i32,
    /// Jump-to-bottom pill when scrolled up.
    jump: Option<Rect>,
    cols: usize,
}

/// In-progress key-strip touch: a drag past touch slop scrolls, otherwise
/// the touch ends as a key tap.
struct KeysDrag {
    start_x: i32,
    start_off: i32,
    moved: bool,
    key: Option<usize>,
}

struct TerminalApp {
    cwd: PathBuf,
    edit: LineEdit,
    lines: Vec<(String, LineKind)>,
    /// Bumped on every scrollback mutation so the wrap cache can't go stale
    /// when `clear` + repopulation lands on the same `lines.len()`.
    gen: u64,
    hist: History,
    /// Scrollback scroller — offset px from content top; `max_offset` = bottom.
    sv: ScrollView,
    /// Snap to bottom on next render (set when a command submits).
    snap_bottom: bool,
    /// Active scrollback touch; `Some(interrupted)` records whether the
    /// touch-down stopped a fling (a stop-the-fling tap must not toggle IME).
    log_tap: Option<bool>,
    /// Monotonic clock for ScrollView velocity sampling.
    clock: Instant,
    ctrl: bool,
    /// IME is up because we activated it; reset on deactivate/defocus so
    /// `animating` cannot latch into a permanent repaint loop.
    osk_active: bool,
    pressed: Option<usize>,
    keys_off: i32,
    keys_drag: Option<KeysDrag>,
    jump_pressed: bool,
    /// Exit code of the last run; the prompt leader goes danger on != 0.
    last_status: Option<i32>,
    wrapped: Vec<(String, LineKind)>,
    wrap_key: (usize, usize, u64), // (cols, lines.len, gen)
    geom: Option<Layout>,
    blink: f32,
    cursor_on: bool,
}

impl TerminalApp {
    fn new() -> Self {
        Self {
            cwd: home_dir(),
            edit: LineEdit::new(),
            lines: Vec::new(),
            gen: 0,
            hist: History::default(),
            sv: ScrollView::new(),
            snap_bottom: false,
            log_tap: None,
            clock: Instant::now(),
            ctrl: false,
            osk_active: false,
            pressed: None,
            keys_off: 0,
            keys_drag: None,
            jump_pressed: false,
            last_status: None,
            wrapped: Vec::new(),
            wrap_key: (usize::MAX, usize::MAX, u64::MAX),
            geom: None,
            blink: 0.0,
            cursor_on: true,
        }
    }

    fn layout(&self, w: i32, h: i32) -> Layout {
        let m = ui::MARGIN;
        // Bottom-up: scrollable quick-key strip, then the terminal surface —
        // edge-to-edge like Android Terminal; a terminal is content, not a card.
        let keys_row = Rect::new(m, h - ui::BOTTOM_SAFE - KEY_H, w - m * 2, KEY_H);
        let term = Rect::new(0, ui::TOP_SAFE + 4, w, keys_row.y - 8 - (ui::TOP_SAFE + 4));
        let prompt = Rect::new(term.x, term.bottom() - PROMPT_H, term.w, PROMPT_H);
        // The viewport top pad is one line high: partially-scrolled rows
        // overdraw into it and get cleanly overpainted each frame.
        let view = Rect::new(
            term.x + TERM_PAD,
            term.y + TERM_LH,
            term.w - TERM_PAD * 2 - SCROLLBAR_LANE,
            (prompt.y - 4) - (term.y + TERM_LH),
        );
        let log = Rect::new(term.x, term.y, term.w, prompt.y - term.y);

        let cw = cell_w_for().max(1) as usize;
        let cols = (view.w.max(1) as usize / cw).max(1);

        let widths = key_widths();
        let mut keys = Vec::with_capacity(KEYS.len());
        let mut x = keys_row.x - self.keys_off;
        for (i, &kw) in widths.iter().enumerate() {
            keys.push((Rect::new(x, keys_row.y, kw, KEY_H), i));
            x += kw + KEY_GAP;
        }
        let keys_max = self.keys_max_for(keys_row.w);

        Layout {
            term,
            log,
            view,
            prompt,
            keys_row,
            keys,
            keys_max,
            jump: None,
            cols,
        }
    }

    fn keys_max_for(&self, row_w: i32) -> i32 {
        let widths = key_widths();
        let content_w: i32 =
            widths.iter().sum::<i32>() + KEY_GAP * (KEYS.len() as i32 - 1).max(0);
        (content_w - row_w).max(0)
    }

    fn now_ms(&self) -> u64 {
        self.clock.elapsed().as_millis().min(u64::MAX as u128) as u64
    }

    fn at_bottom(&self) -> bool {
        self.sv.offset() >= self.sv.max_offset() - 1.0
    }

    /// Activate or dismiss the IME. `osk_active` mirrors the real IME state
    /// both ways so the animation latch can never stick on.
    fn set_osk(&mut self, up: bool, ti: &TextInputCtl) {
        if up {
            self.osk_active = true;
            ti.activate(ContentPurpose::Terminal);
            ti.set_surrounding(self.edit.as_str(), self.edit.cursor());
        } else {
            self.osk_active = false;
            ti.deactivate();
        }
    }

    fn push_line(&mut self, kind: LineKind, s: String) {
        self.lines.push((s, kind));
        self.gen += 1;
        if self.lines.len() > MAX_LINES {
            let drain = self.lines.len() - MAX_LINES;
            self.lines.drain(..drain);
        }
    }

    fn push_lines(&mut self, new: Vec<String>) {
        for l in new {
            self.push_line(LineKind::Out, l);
        }
    }

    fn clear_lines(&mut self) {
        self.lines.clear();
        self.gen += 1;
        self.sv.scroll_to(0.0);
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
        // Echo the full prompt like a real shell: `~/dir $ cmd`.
        self.push_line(
            LineKind::Out,
            format!("{} $ {}", disp_cwd(&self.cwd), cmd),
        );
        self.snap_bottom = true;

        if cmd == "clear" {
            self.clear_lines();
            self.last_status = None;
            return false;
        }
        if cmd == "cd" || cmd.starts_with("cd ") {
            let rest = cmd[2..].trim();
            if rest.is_empty() {
                self.cwd = home_dir();
                self.last_status = Some(0);
            } else {
                let target = rest.trim_matches('"').trim_matches('\'');
                let next = expand_tilde(target, &self.cwd);
                if next.is_dir() {
                    self.cwd = next.canonicalize().unwrap_or(next);
                    self.last_status = Some(0);
                } else {
                    self.push_line(LineKind::Err, format!("cd: no such directory: {target}"));
                    self.last_status = Some(1);
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
            Ok(out) => {
                self.last_status = out.code;
                self.push_lines(out.lines);
                match out.code {
                    Some(0) => {}
                    // Visible failure cue: danger-colored status line.
                    Some(c) => self.push_line(LineKind::Err, format!("[exit {c}]")),
                    None => self.push_line(LineKind::Err, "[terminated]".into()),
                }
            }
            Err(e) => {
                self.last_status = Some(1);
                self.push_line(LineKind::Err, format!("error: {e}"));
            }
        }
        false
    }

    /// Apply a quick-key action. Returns true when the app should quit.
    fn act(&mut self, act: Act) -> bool {
        match act {
            Act::Esc => {
                // CLR: clear the input line (this MVP has no persistent PTY
                // to send an ESC byte to; the label is honest about it).
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
        self.wrapped = self
            .lines
            .iter()
            .flat_map(|(l, k)| wrap_cols(l, cols).into_iter().map(move |r| (r, *k)))
            .collect();
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

/// Width of one terminal cell, measured from the bundled font rather than
/// assumed: the mean of the tabular digit advance and the average advance
/// over a representative ASCII sample. Still an approximation — no
/// monospace font is bundled in de-paint yet (see `edit::char_width`) — so
/// `wrap_cols` keeps erring toward wrapping early. Cached per process.
fn cell_w_for() -> i32 {
    static W: OnceLock<i32> = OnceLock::new();
    *W.get_or_init(|| {
        let mut st = ui::styled(TERM_SIZE, 400, [255, 255, 255, 255]);
        st.line_height = TERM_LH as f32;
        const SAMPLE: &str = "abcdefghijklmnopqrstuvwxyz0123456789 -._/~";
        let avg = text::label(SAMPLE, st, None)
            .map(|px| (px.width() as f32 / SAMPLE.chars().count() as f32).ceil() as i32)
            .unwrap_or(9);
        let digit = text::label("0", st, None)
            .map(|px| px.width() as i32)
            .unwrap_or(10);
        ((avg + digit) / 2).max(6)
    })
}

/// Measured pixel width per quick key (icon keys are square-ish).
fn key_widths() -> &'static [i32] {
    static W: OnceLock<Vec<i32>> = OnceLock::new();
    W.get_or_init(|| {
        KEYS.iter()
            .map(|k| {
                if k.icon.is_some() {
                    KEY_MIN_W.max(KEY_ICON as i32 + 32)
                } else {
                    text::label(k.label, ui::label([255, 255, 255, 255]), None)
                        .map(|px| px.width() as i32 + 32)
                        .unwrap_or(KEY_MIN_W)
                        .max(KEY_MIN_W)
                }
            })
            .collect()
    })
}

/// Terminal palette derived from the theme: the darker of `canvas`/`text`
/// is the surface, the lighter is the ink. Always dark, always tokens.
struct TermPal {
    bg: [u8; 4],
    ink: [u8; 4],
    dim: [u8; 4],
    accent: [u8; 4],
}

impl TermPal {
    fn new(t: &Theme) -> Self {
        const BLACK: [u8; 4] = [0, 0, 0, 255]; // comparison only, never painted
        let canvas_is_darker =
            tokens::contrast_ratio(t.canvas, BLACK) <= tokens::contrast_ratio(t.text, BLACK);
        let (bg, ink) = if canvas_is_darker {
            (t.canvas, t.text)
        } else {
            (t.text, t.canvas)
        };
        let dim = ui::mix(ink, bg, 0.37);
        // Copper on near-black fails contrast in the light theme; accent_hi
        // is the token meant for that case.
        let accent = if tokens::contrast_ratio(t.accent, bg) >= 4.5 {
            t.accent
        } else if tokens::contrast_ratio(t.accent_hi, bg) >= 4.5 {
            t.accent_hi
        } else {
            ink
        };
        Self {
            bg,
            ink,
            dim,
            accent,
        }
    }
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

/// Prompt leader `~/dir $ ` fitted under `max_w` by dropping leading path
/// components (`…/dir $ `), falling back to a bare `$ `.
fn prompt_leader(cwd: &Path, st: &text::LabelStyle, max_w: i32) -> (String, i32) {
    let full = format!("{} $ ", disp_cwd(cwd));
    let mut cands = vec![full];
    if let Some(base) = cwd.file_name().and_then(|s| s.to_str()) {
        cands.push(format!("…/{base} $ "));
    }
    cands.push("$ ".into());
    let mut last = ("$ ".to_string(), 0);
    for c in cands {
        let w = text::label(&c, *st, None)
            .map(|px| px.width() as i32)
            .unwrap_or(0);
        if w <= max_w {
            return (c, w);
        }
        last = (c, w);
    }
    last
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
                    if g.keys_row.contains(pos) {
                        let key = g
                            .keys
                            .iter()
                            .find(|(r, _)| r.contains(pos))
                            .map(|(_, i)| *i);
                        self.keys_drag = Some(KeysDrag {
                            start_x: pos.x,
                            start_off: self.keys_off,
                            moved: false,
                            key,
                        });
                        self.pressed = key;
                    } else if g.jump.is_some_and(|r| r.contains(pos)) {
                        self.jump_pressed = true;
                    } else if g.prompt.contains(pos) {
                        self.set_osk(true, ti);
                    } else if g.log.contains(pos) {
                        self.log_tap = Some(self.sv.is_animating());
                        self.sv.on_touch_down(pos.y as f32, self.now_ms());
                    }
                }
                dirty = true;
            }
            AppEvent::TouchMove { pos, .. } => {
                if let Some(kd) = &mut self.keys_drag {
                    let dx = kd.start_x - pos.x;
                    if dx.abs() > scroll::TOUCH_SLOP as i32 {
                        kd.moved = true;
                        self.pressed = None;
                    }
                    if kd.moved {
                        let max = self.geom.as_ref().map_or(0, |g| g.keys_max);
                        let next = (kd.start_off + dx).clamp(0, max);
                        if next != self.keys_off {
                            self.keys_off = next;
                            dirty = true;
                        }
                    }
                }
                if self.log_tap.is_some() {
                    self.sv.on_touch_move(pos.y as f32, self.now_ms());
                    dirty = true;
                }
            }
            AppEvent::TouchUp { pos, .. } => {
                if let Some(kd) = self.keys_drag.take() {
                    self.pressed = None;
                    if !kd.moved {
                        if let Some(ki) = kd.key {
                            if let Some(g) = &self.geom {
                                if g.keys.iter().any(|(r, i)| *i == ki && r.contains(pos)) {
                                    quit = self.act(KEYS[ki].act);
                                }
                            }
                        }
                    }
                    dirty = true;
                }
                if self.jump_pressed {
                    self.jump_pressed = false;
                    let hit = self
                        .geom
                        .as_ref()
                        .and_then(|g| g.jump)
                        .is_some_and(|r| r.contains(pos));
                    if hit {
                        self.sv.scroll_to(self.sv.max_offset());
                    }
                    dirty = true;
                }
                if let Some(interrupted) = self.log_tap.take() {
                    let dragged = self.sv.on_touch_up(self.now_ms());
                    let in_log = self.geom.as_ref().is_some_and(|g| g.log.contains(pos));
                    if !dragged && !interrupted && in_log {
                        // Tap on the scrollback toggles the IME — tap-to-focus
                        // / tap-outside-to-dismiss. This is the defocus path
                        // that resets the osk latch.
                        let up = !self.osk_active;
                        self.set_osk(up, ti);
                    }
                    dirty = true;
                }
            }
            AppEvent::Scroll { dy, .. } => {
                // Mouse wheel / touchpad: positive dy scrolls content down.
                self.sv.scroll_by(dy);
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
                let secs = dt.as_secs_f32();
                // Fling/overscroll physics; dirty while it is still moving.
                if self.sv.tick(secs) || self.sv.is_animating() {
                    dirty = true;
                }
                // Cursor blink phase — driven by whatever ticks arrive
                // (IME-focused animating, fling), not gated on the OSK.
                self.blink += secs;
                let on = (self.blink % BLINK_PERIOD) < BLINK_PERIOD / 2.0;
                if on != self.cursor_on {
                    self.cursor_on = on;
                    dirty = true;
                }
            }
            // PointerMove hover and non-primary PointerButton edges have no
            // meaning for the terminal.
            _ => {}
        }
        AppFrame {
            dirty,
            // Animation cadence only while the IME is genuinely up or the
            // fling is still moving — no permanent repaint loop.
            animating: self.osk_active || self.sv.is_animating(),
            quit,
        }
    }

    fn render(&mut self, canvas: &mut Canvas, theme: &Theme) {
        let w = canvas.size().w;
        let h = canvas.size().h;
        let m = ui::MARGIN;
        self.keys_off = self.keys_off.clamp(0, self.keys_max_for(w - m * 2));
        let mut lay = self.layout(w, h);
        let pal = TermPal::new(theme);

        let mut st = ui::styled(TERM_SIZE, 400, pal.ink);
        st.line_height = TERM_LH as f32;
        let mut st_err = st;
        st_err.color = theme.danger;
        let failed = self.last_status.is_some_and(|c| c != 0);
        let mut st_leader = ui::styled(TERM_SIZE, 700, if failed { theme.danger } else { pal.accent });
        st_leader.line_height = TERM_LH as f32;

        // Terminal surface — always the darker token.
        canvas.fill_rect(lay.term, pal.bg);

        // Wrap + scroll bounds; follow new output only while pinned to bottom.
        self.refresh_wrap(lay.cols);
        let content_h = self.wrapped.len() as i32 * TERM_LH;
        let follow = self.snap_bottom || self.at_bottom();
        self.sv.set_bounds(content_h as f32, lay.view.h as f32);
        if follow {
            self.sv.scroll_to(self.sv.max_offset());
        }
        self.snap_bottom = false;
        lay.jump = (!self.at_bottom()).then(|| {
            Rect::new(
                lay.term.right() - TERM_PAD - JUMP_SZ,
                lay.prompt.y - 12 - JUMP_SZ,
                JUMP_SZ,
                JUMP_SZ,
            )
        });

        // Scrollback rows, pixel-smooth via the ScrollView offset.
        if self.wrapped.is_empty() && self.edit.as_str().is_empty() {
            ui::draw_text(
                canvas,
                "Type a command and press Enter",
                &ui::caption(pal.dim),
                lay.view.x,
                (lay.view.bottom() - 48).max(lay.view.y),
                Some(lay.view.w),
            );
        } else {
            let off = self.sv.offset_i32();
            let first = (off / TERM_LH).max(0) as usize;
            let dy = lay.view.y - off % TERM_LH;
            for (j, (row, kind)) in self.wrapped.iter().skip(first).enumerate() {
                let y = dy + j as i32 * TERM_LH;
                if y >= lay.view.bottom() {
                    break;
                }
                let s = if *kind == LineKind::Err { &st_err } else { &st };
                // max_w = None: rows are pre-wrapped by `wrap_cols`; a stray
                // over-wide row clips at the surface edge instead of wrapping
                // into the next row and corrupting it.
                ui::draw_text(canvas, row, s, lay.view.x, y, None);
            }
        }

        // Clip the strays by repainting the pad strips (Canvas has no scissor).
        canvas.fill_rect(
            Rect::new(lay.term.x, lay.term.y, lay.term.w, lay.view.y - lay.term.y),
            pal.bg,
        );
        canvas.fill_rect(lay.prompt, pal.bg);
        canvas.fill_rect(
            Rect::new(lay.prompt.x, lay.prompt.y, lay.prompt.w, 1),
            theme.line,
        );

        // Prompt line inside the terminal surface: `~/dir $ input█`.
        let text_y = lay.prompt.y + (lay.prompt.h - TERM_LH) / 2;
        let inner_x = lay.prompt.x + TERM_PAD;
        let badge = Rect::new(
            lay.prompt.right() - TERM_PAD - BADGE_W + 10,
            lay.prompt.y + (lay.prompt.h - 32) / 2,
            BADGE_W - 10,
            32,
        );
        let input_right = if self.ctrl {
            badge.x - 10
        } else {
            lay.prompt.right() - TERM_PAD
        };
        let avail = (input_right - inner_x).max(1);
        let (leader, leader_w) = prompt_leader(&self.cwd, &st_leader, avail * 2 / 5);
        ui::draw_text(canvas, &leader, &st_leader, inner_x, text_y, None);
        let text_x = inner_x + leader_w;
        let field_w = (avail - leader_w).max(8);
        // Reserve the final visible cell for the caret.
        let pcols = (field_w / cell_w_for() - 1).max(1) as usize;

        let (bs, be) = input_window(self.edit.as_str(), self.edit.cursor(), pcols);
        let vis = &self.edit.as_str()[bs..be];
        ui::draw_text(canvas, vis, &st, text_x, text_y, Some(field_w));

        // Block cursor: measured cell advance wide. Filled + blinking while
        // the IME is up; hollow + steady when unfocused (terminal convention).
        if (bs..=be).contains(&self.edit.cursor()) {
            let cur_byte = self.edit.cursor() - bs;
            let prefix = &vis[..cur_byte];
            let prefix_w = text::label(prefix, st, None)
                .map(|p| p.width() as i32)
                .unwrap_or(0);
            let cur = Rect::new(
                text_x + prefix_w,
                text_y + 3,
                cell_w_for(),
                TERM_LH - 6,
            );
            if self.osk_active {
                if self.cursor_on {
                    canvas.fill_rounded_rect(cur, 4.0, pal.accent);
                }
            } else {
                canvas.fill_rounded_rect(cur, 4.0, pal.dim);
                canvas.fill_rounded_rect(cur.inset(2), 2.0, pal.bg);
            }
        }

        if self.ctrl {
            canvas.fill_rounded_rect(badge, 16.0, theme.accent);
            ui::draw_centered(canvas, "CTRL", &ui::label(theme.text_on_accent), badge);
        }

        // Scrollbar on the right lane, then the jump-to-bottom pill.
        ui::scrollbar(
            canvas,
            theme,
            Rect::new(lay.view.right() + 4, lay.view.y, 6, lay.view.h),
            self.sv.offset_i32(),
            content_h,
        );
        if let Some(jr) = lay.jump {
            widgets::fab(canvas, theme, jr, icons::ARROW_DOWNWARD, self.jump_pressed);
        }

        // Quick keys — one scrollable row, hard-clipped at the margins by
        // repainting the gutters after drawing.
        for (r, ki) in &lay.keys {
            if r.right() <= lay.keys_row.x || r.x >= lay.keys_row.right() {
                continue;
            }
            let kd = &KEYS[*ki];
            let pressed = self.pressed == Some(*ki);
            if let Some(icon) = kd.icon {
                widgets::icon_btn(
                    canvas,
                    theme,
                    *r,
                    icon,
                    widgets::IconBtnStyle::Tonal,
                    false,
                    pressed,
                );
            } else {
                let primary =
                    matches!(kd.act, Act::Run) || (kd.act == Act::Ctrl && self.ctrl);
                ui::draw_pill_button(canvas, theme, *r, kd.label, primary, pressed);
            }
        }
        canvas.fill_rect(Rect::new(0, lay.keys_row.y, m, KEY_H), theme.bg);
        canvas.fill_rect(Rect::new(w - m, lay.keys_row.y, m, KEY_H), theme.bg);

        self.geom = Some(lay);
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
    use rabbiz_app_kit::{Point, Size};

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
    fn keys_form_scrollable_strip() {
        assert!(KEYS.len() >= 12);
        assert!(KEYS.iter().any(|k| k.act == Act::Run));
        assert!(KEYS.iter().any(|k| k.act == Act::Ctrl));
        assert!(KEY_H >= 48, "quick keys must meet the 48px touch contract");
        assert_eq!(key_widths().len(), KEYS.len());
    }

    #[test]
    fn cell_width_positive() {
        assert!(cell_w_for() >= 6);
    }

    #[test]
    fn osk_latch_resets_on_scrollback_tap() {
        // Regression for the permanent-repaint bug: once the prompt tap set
        // osk_active, `animating` stayed true forever. A scrollback tap is
        // the defocus path — the latch must reset both ways.
        let mut h = rabbiz_app_kit::TestHarness::new(TerminalApp::new());
        // Populate geom so hit rects exist.
        let mut c = Canvas::new(Size::new(720, 1600));
        let theme = *h.theme();
        h.state_mut().render(&mut c, &theme);

        let band_y = h.state().geom.as_ref().unwrap().prompt.y + 10;
        h.tap(Point::new(360, band_y));
        assert!(h.ime_active());
        assert!(h.last_frame().animating, "IME up should tick the cursor");

        let log_y = h.state().geom.as_ref().unwrap().log.y + 40;
        h.tap(Point::new(360, log_y));
        assert!(!h.ime_active(), "scrollback tap should dismiss the IME");
        assert!(
            !h.last_frame().animating,
            "animating must clear when the IME latch resets"
        );

        // And toggles back on — tap terminal to type.
        h.tap(Point::new(360, log_y));
        assert!(h.ime_active());
    }

    /// Drives the app through its main states and saves evidence PNGs to
    /// `/tmp/rabbiz-shots/terminal-<tag>-<state>.png` (`tag` from `SHOT_TAG`,
    /// default "shot"). Run with SHOT_TAG=before / SHOT_TAG=after around UI
    /// changes for the visual record.
    #[test]
    fn harness_renders_states_and_saves_shots() {
        use std::time::Duration;

        let tag = std::env::var("SHOT_TAG").unwrap_or_else(|_| "shot".into());
        let dir = std::path::Path::new("/tmp/rabbiz-shots");
        let mut h = rabbiz_app_kit::TestHarness::new(TerminalApp::new());
        macro_rules! shot {
            ($name:literal) => {
                h.save_png(&dir.join(format!("terminal-{tag}-{}.png", $name)))
                    .expect("save png")
            };
        }

        shot!("fresh");

        // Focus the prompt -> IME activates with Terminal purpose.
        let mut c = Canvas::new(Size::new(720, 1600));
        let theme = *h.theme();
        h.state_mut().render(&mut c, &theme);
        let band_y = h.state().geom.as_ref().unwrap().prompt.y + 10;
        h.tap(Point::new(360, band_y));
        assert!(h.ime_active());
        assert_eq!(h.ime_purpose(), Some(ContentPurpose::Terminal));
        h.type_text("echo hello-rabbiz");
        shot!("typed");

        // Run it (PTY path), then run a failing command for the danger cue.
        h.key(0xFF0D, Some("\r"));
        h.type_text("definitely-not-a-command-xyz");
        h.key(0xFF0D, Some("\r"));
        shot!("ran");

        // Long scrollback -> scroll up: scrollbar + jump pill show.
        h.state_mut()
            .push_lines((0..80).map(|i| format!("line {i:03} lorem ipsum dolor")).collect());
        let _ = h.render(); // refresh geom/bounds with the new content
        let half = h.state().sv.max_offset() * 0.5;
        h.state_mut().sv.scroll_to(half);
        h.swipe(
            Point::new(360, 900),
            Point::new(360, 1100),
            4,
            Duration::from_millis(16),
        );
        assert!(!h.state().at_bottom());
        shot!("scrolled");

        // Jump-to-bottom pill snaps back.
        let jr = h
            .state()
            .geom
            .as_ref()
            .and_then(|g| g.jump)
            .expect("jump pill should show while scrolled up");
        h.tap(jr.center());
        assert!(h.state().at_bottom());
        shot!("bottom");
    }
}
