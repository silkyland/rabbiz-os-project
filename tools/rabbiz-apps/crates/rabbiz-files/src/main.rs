//! RabbizOS Files — mobile-first file manager (es-file-explorer /
//! Dolphin-on-Plasma-Mobile equivalent, Rust native).
//!
//! Navigation: breadcrumb path bar + history (edge-swipe / two-finger swipe
//! back, two-finger swipe right = forward). Views: list or 3-column grid.
//! Sort: name/date/size/type, folders first. Hidden files toggle. Filename
//! search within the current directory. Long-press or two-finger tap enters
//! selection mode (M3 checkboxes + bottom action bar). Operations: new
//! folder, rename, copy, move (in-app destination picker), permanent delete
//! with confirm — no freedesktop trash is implemented, deletes are final.
//! Files open via `xdg-open`/`gio`/`kde-open` when available, else the
//! details sheet.
//!
//! Known kit limitation: `Tick` only fires after a Wayland event, so there
//! is no frame timer — flick scroll applies its projected offset instantly
//! instead of gliding, and the edge-swipe commits without a slide animation.

mod cli;
mod gestures;
mod interaction;
mod layout;
mod model;
mod ui;

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use de_api::Rect;
use rabbiz_app_kit::{AppEvent, AppFrame, AppState, Canvas, TextInputCtl, Theme};
use wayland_protocols::wp::text_input::zv3::client::zwp_text_input_v3::ContentPurpose;

use gestures::Tracker;
use layout as lay;
use model::{Entry, SortKey};

// ---------------------------------------------------------------------------
// Small UI model types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub(crate) struct TextBuf {
    pub(crate) s: String,
    /// Byte offset, always on a char boundary.
    pub(crate) cursor: usize,
}

impl TextBuf {
    pub(crate) fn new(s: &str) -> Self {
        Self {
            s: s.to_string(),
            cursor: s.len(),
        }
    }
    pub(crate) fn insert(&mut self, ins: &str) {
        let pos = self.cursor.min(self.s.len());
        self.s.insert_str(pos, ins);
        self.cursor = pos + ins.len();
    }
    /// Delete up to `n` characters before the cursor.
    pub(crate) fn backspace(&mut self, n: usize) {
        for _ in 0..n {
            if self.cursor == 0 {
                break;
            }
            let mut c = self.cursor - 1;
            while c > 0 && !self.s.is_char_boundary(c) {
                c -= 1;
            }
            self.s.remove(c);
            self.cursor = c;
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Focus {
    None,
    Search,
    Dialog,
}

#[derive(Debug, Clone)]
pub(crate) enum Dialog {
    NewFolder,
    Rename(PathBuf),
    Delete { paths: Vec<PathBuf>, items: usize },
    Details(PathBuf),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SelAct {
    Open,
    Rename,
    Copy,
    Move,
    Delete,
    Info,
    Done,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OpKind {
    Copy,
    Move,
}

impl OpKind {
    pub(crate) fn verb(self) -> &'static str {
        match self {
            OpKind::Copy => "Copy",
            OpKind::Move => "Move",
        }
    }
    fn past(self) -> &'static str {
        match self {
            OpKind::Copy => "copied",
            OpKind::Move => "moved",
        }
    }
}

/// Row identity in the virtualized item list: the ".." parent row or a real
/// entry index into `vis`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Item {
    Parent,
    Ent(usize),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Hit {
    Back,
    Home,
    Crumb(usize),
    SearchBtn,
    SearchField,
    SearchClear,
    SortBtn,
    ViewBtn,
    Fab,
    Sel(SelAct),
    Paste,
    PickerCancel,
    MenuSort(SortKey),
    MenuHidden,
    DlgOk,
    DlgCancel,
    DlgField,
    Retry,
}

// ---------------------------------------------------------------------------
// App state
// ---------------------------------------------------------------------------

pub(crate) struct FilesApp {
    pub(crate) home: PathBuf,
    pub(crate) cwd: PathBuf,
    /// Raw directory entries (unsorted/unfiltered).
    raw: Vec<Entry>,
    pub(crate) load_error: Option<String>,
    /// Visit history for back/forward gestures.
    hist: Vec<PathBuf>,
    hist_i: usize,

    pub(crate) key: SortKey,
    pub(crate) show_hidden: bool,
    pub(crate) grid: bool,
    pub(crate) search: TextBuf,
    pub(crate) search_open: bool,

    /// Derived display list — rebuilt lazily via `needs_sync`.
    pub(crate) vis: Vec<Entry>,
    pub(crate) items: Vec<Item>,
    pub(crate) needs_sync: bool,

    // geometry filled by render()
    pub(crate) cur_w: i32,
    pub(crate) cur_h: i32,
    pub(crate) view_top: i32,
    pub(crate) view_h: i32,
    pub(crate) scroll: f32,
    pub(crate) scroll_anchor: Option<f32>,
    pub(crate) drag_dy: i32,
    pub(crate) pressed_item: Option<usize>,
    pub(crate) pressed_hit: Option<Hit>,

    tracker: Tracker,
    /// Edge-swipe visual offset (px, ≥0).
    pub(crate) nav_dx: i32,

    pub(crate) selection: HashSet<PathBuf>,
    pub(crate) select_mode: bool,

    pub(crate) dialog: Option<Dialog>,
    pub(crate) text: TextBuf,
    pub(crate) focus: Focus,
    pub(crate) menu_open: bool,
    pub(crate) menu_rect: Rect,

    pub(crate) pending_op: Option<(OpKind, Vec<PathBuf>)>,
    pub(crate) status: Option<(String, bool)>, // (message, is_error)

    pub(crate) hits: Vec<(Rect, Hit)>,
    pub(crate) crumbs: Vec<(String, Rect)>,
    /// Jump target per crumb (parallel to `crumbs`).
    pub(crate) crumb_targets: Vec<PathBuf>,
}

impl FilesApp {
    /// `initial` is the directory a `%f` file-arg resolved to (see
    /// `cli::browse_dir_arg`); `None` starts at `$HOME`.
    fn new(initial: Option<PathBuf>) -> Self {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .unwrap_or_else(|| PathBuf::from("/"));
        let mut app = Self {
            home: home.clone(),
            cwd: home,
            raw: Vec::new(),
            load_error: None,
            hist: Vec::new(),
            hist_i: 0,
            key: SortKey::Name,
            show_hidden: false,
            grid: false,
            search: TextBuf::new(""),
            search_open: false,
            vis: Vec::new(),
            items: Vec::new(),
            needs_sync: true,
            cur_w: 720,
            cur_h: 1600,
            view_top: 0,
            view_h: 0,
            scroll: 0.0,
            scroll_anchor: None,
            drag_dy: 0,
            pressed_item: None,
            pressed_hit: None,
            tracker: Tracker::new(),
            nav_dx: 0,
            selection: HashSet::new(),
            select_mode: false,
            dialog: None,
            text: TextBuf::new(""),
            focus: Focus::None,
            menu_open: false,
            menu_rect: Rect::new(0, 0, 0, 0),
            pending_op: None,
            status: None,
            hits: Vec::new(),
            crumbs: Vec::new(),
            crumb_targets: Vec::new(),
        };
        app.reload();
        // A `%f` file-arg lands via `navigate` so Back returns to $HOME.
        if let Some(dir) = initial {
            app.navigate(dir);
        }
        app
    }

    // ---------------- data ----------------

    pub(crate) fn reload(&mut self) {
        match model::load_dir(&self.cwd) {
            Ok(e) => {
                self.raw = e;
                self.load_error = None;
            }
            Err(e) => {
                self.raw.clear();
                self.load_error = Some(e.to_string());
            }
        }
        self.needs_sync = true;
        self.scroll = 0.0;
        self.scroll_anchor = None;
        self.drag_dy = 0;
    }

    pub(crate) fn sync(&mut self) {
        if !self.needs_sync {
            return;
        }
        self.needs_sync = false;
        self.vis = model::visible_entries(
            &self.raw,
            self.key,
            self.show_hidden,
            &self.search.s.to_lowercase(),
        );
        let has_parent = self.cwd != Path::new("/");
        let mut items = Vec::with_capacity(self.vis.len() + 1);
        if has_parent {
            items.push(Item::Parent);
        }
        items.extend((0..self.vis.len()).map(Item::Ent));
        self.items = items;
        self.clamp_scroll();
    }

    pub(crate) fn content_count(&self) -> usize {
        self.items.len()
    }

    pub(crate) fn max_scroll(&self) -> i32 {
        lay::max_scroll(self.content_count(), self.view_h, self.grid)
    }

    pub(crate) fn clamp_scroll(&mut self) {
        self.scroll = lay::clamp_scroll(self.scroll, self.max_scroll());
    }

    // ---------------- navigation ----------------

    pub(crate) fn navigate(&mut self, path: PathBuf) {
        if path == self.cwd {
            return;
        }
        // Truncate forward history, push current.
        self.hist.truncate(self.hist_i + 1);
        self.hist.push(self.cwd.clone());
        self.hist_i = self.hist.len() - 1;
        self.cwd = path;
        self.clear_transient();
        self.reload();
    }

    pub(crate) fn go_back(&mut self) {
        if self.hist_i > 0 {
            self.hist_i -= 1;
            self.cwd = self.hist[self.hist_i].clone();
            self.clear_transient();
            self.reload();
        } else if let Some(p) = self.cwd.parent().map(Path::to_path_buf) {
            if p != self.cwd {
                self.cwd = p;
                self.clear_transient();
                self.reload();
            }
        }
    }

    pub(crate) fn go_forward(&mut self) {
        if self.hist_i + 1 < self.hist.len() {
            self.hist_i += 1;
            self.cwd = self.hist[self.hist_i].clone();
            self.clear_transient();
            self.reload();
        }
    }

    pub(crate) fn can_back(&self) -> bool {
        self.hist_i > 0 || self.cwd.parent().is_some()
    }

    fn clear_transient(&mut self) {
        self.selection.clear();
        self.select_mode = false;
        self.dialog = None;
        self.menu_open = false;
        self.pressed_item = None;
        self.pressed_hit = None;
        self.nav_dx = 0;
        if self.focus == Focus::Dialog {
            self.focus = Focus::None;
        }
    }

    // ---------------- selection & actions ----------------

    pub(crate) fn item_path(&self, idx: usize) -> Option<PathBuf> {
        match self.items.get(idx) {
            Some(Item::Ent(i)) => self.vis.get(*i).map(|e| e.path.clone()),
            _ => None,
        }
    }

    pub(crate) fn toggle_select(&mut self, path: PathBuf) {
        if !self.selection.remove(&path) {
            self.selection.insert(path);
        }
        if self.selection.is_empty() {
            self.select_mode = false;
        }
    }

    pub(crate) fn enter_selection(&mut self, path: PathBuf) {
        self.select_mode = true;
        self.selection.clear();
        self.selection.insert(path);
    }

    pub(crate) fn open_item(&mut self, idx: usize, ti: &TextInputCtl) {
        match self.items.get(idx).copied() {
            Some(Item::Parent) => {
                if let Some(p) = self.cwd.parent() {
                    self.navigate(p.to_path_buf());
                }
            }
            Some(Item::Ent(i)) => {
                let act = self.vis.get(i).map(|e| (e.is_dir, e.path.clone()));
                if let Some((is_dir, path)) = act {
                    if is_dir {
                        self.navigate(path);
                    } else {
                        self.open_file(&path, ti);
                    }
                }
            }
            None => {}
        }
    }

    /// Try external openers in order; fall back to the details sheet.
    pub(crate) fn open_file(&mut self, path: &Path, _ti: &TextInputCtl) {
        if open_external(path) {
            return;
        }
        self.show_details(path.to_path_buf());
    }

    pub(crate) fn show_details(&mut self, path: PathBuf) {
        self.menu_open = false;
        self.dialog = Some(Dialog::Details(path));
        self.focus = Focus::None;
    }

    pub(crate) fn delete_selected(&mut self) {
        self.menu_open = false;
        let paths: Vec<PathBuf> = self.selection.iter().cloned().collect();
        if paths.is_empty() {
            return;
        }
        let items: usize = paths.iter().map(|p| model::count_tree(p)).sum();
        self.dialog = Some(Dialog::Delete { paths, items });
        self.focus = Focus::None;
    }

    pub(crate) fn confirm_delete(&mut self) {
        let Some(Dialog::Delete { paths, .. }) = self.dialog.take() else {
            return;
        };
        let mut ok = 0usize;
        let mut err: Option<String> = None;
        for p in &paths {
            match model::remove_entry(p) {
                Ok(()) => ok += 1,
                Err(e) => err = Some(format!("{}: {e}", p.display())),
            }
        }
        self.selection.clear();
        self.select_mode = false;
        self.status = Some(match err {
            Some(e) => (format!("Deleted {ok}; failed: {e}"), true),
            None => (
                format!("Deleted {ok} item{}", if ok == 1 { "" } else { "s" }),
                false,
            ),
        });
        self.reload();
    }

    pub(crate) fn rename_selected(&mut self, ti: &TextInputCtl) {
        self.menu_open = false;
        if self.selection.len() != 1 {
            return;
        }
        let path = self.selection.iter().next().unwrap().clone();
        self.text = TextBuf::new(
            path.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
                .as_str(),
        );
        self.dialog = Some(Dialog::Rename(path));
        self.focus_dialog(ti);
    }

    pub(crate) fn confirm_rename(&mut self) {
        let Some(Dialog::Rename(path)) = self.dialog.take() else {
            return;
        };
        let Some(name) = model::clean_name(&self.text.s) else {
            self.status = Some(("Invalid name".into(), true));
            return;
        };
        let dst = path.parent().unwrap_or(Path::new("/")).join(&name);
        if dst == path {
            return;
        }
        if dst.exists() {
            self.status = Some((format!("“{name}” already exists"), true));
            return;
        }
        match std::fs::rename(&path, &dst) {
            Ok(()) => {
                self.status = Some((format!("Renamed to {name}"), false));
                self.selection.clear();
                self.select_mode = false;
            }
            Err(e) => self.status = Some((format!("Rename failed: {e}"), true)),
        }
        self.reload();
    }

    pub(crate) fn new_folder(&mut self, ti: &TextInputCtl) {
        self.menu_open = false;
        self.text = TextBuf::new("New folder");
        self.dialog = Some(Dialog::NewFolder);
        self.focus_dialog(ti);
    }

    pub(crate) fn confirm_new_folder(&mut self) {
        self.dialog = None;
        let Some(name) = model::clean_name(&self.text.s) else {
            self.status = Some(("Invalid name".into(), true));
            return;
        };
        let name = model::unique_name(&self.cwd, &name);
        match model::make_dir(&self.cwd.join(&name)) {
            Ok(()) => self.status = Some((format!("Created {name}"), false)),
            Err(e) => self.status = Some((format!("Create failed: {e}"), true)),
        }
        self.reload();
    }

    pub(crate) fn start_op(&mut self, kind: OpKind) {
        self.menu_open = false;
        let srcs: Vec<PathBuf> = self.selection.iter().cloned().collect();
        if srcs.is_empty() {
            return;
        }
        self.pending_op = Some((kind, srcs));
        self.select_mode = false;
        self.selection.clear();
        self.status = None;
    }

    pub(crate) fn paste_here(&mut self) {
        let Some((kind, srcs)) = self.pending_op.take() else {
            return;
        };
        let mut ok = 0usize;
        let mut err: Option<String> = None;
        for src in &srcs {
            // Moving an item into the directory it already lives in would
            // just duplicate it under a "(1)" name — skip honestly.
            if kind == OpKind::Move && src.parent() == Some(self.cwd.as_path()) {
                err = Some(format!("{} is already here", src.display()));
                continue;
            }
            let Some(name) = src.file_name().map(|n| n.to_string_lossy().into_owned()) else {
                continue;
            };
            let dst = self.cwd.join(model::unique_name(&self.cwd, &name));
            let r = match kind {
                OpKind::Copy => model::copy_entry(src, &dst),
                OpKind::Move => model::move_entry(src, &dst),
            };
            match r {
                Ok(()) => ok += 1,
                Err(e) => err = Some(format!("{}: {e}", src.display())),
            }
        }
        self.status = Some(match err {
            Some(e) => (format!("{} {} with errors: {e}", ok, kind.past()), true),
            None => (
                format!(
                    "{} {} item{} to {}",
                    ok,
                    kind.past(),
                    if ok == 1 { "" } else { "s" },
                    self.cwd.display()
                ),
                false,
            ),
        });
        self.reload();
    }

    // ---------------- text input ----------------

    pub(crate) fn focus_dialog(&mut self, ti: &TextInputCtl) {
        self.focus = Focus::Dialog;
        ti.activate(ContentPurpose::Normal);
        ti.set_surrounding(&self.text.s, self.text.cursor);
    }

    pub(crate) fn focused_buf(&mut self) -> Option<&mut TextBuf> {
        match self.focus {
            Focus::Dialog => Some(&mut self.text),
            Focus::Search => Some(&mut self.search),
            Focus::None => None,
        }
    }

    pub(crate) fn after_text_change(&mut self, ti: &TextInputCtl) {
        if self.focus == Focus::Search {
            self.needs_sync = true;
        }
        let (s, c) = match self.focus {
            Focus::Dialog => (self.text.s.clone(), self.text.cursor),
            Focus::Search => (self.search.s.clone(), self.search.cursor),
            Focus::None => return,
        };
        ti.set_surrounding(&s, c);
    }
}

// ---------------------------------------------------------------------------
// External open + AppState
// ---------------------------------------------------------------------------

/// Try `xdg-open`, `gio open`, `kde-open`, `exo-open` in order. Returns true
/// if a launcher process was spawned (success of the open itself is the
/// launcher's problem — honest limitation, noted in-app via details fallback
/// only when NO launcher exists).
fn open_external(path: &Path) -> bool {
    let p = path.as_os_str();
    for cmd in [
        ("xdg-open", vec![p]),
        ("gio", vec![std::ffi::OsStr::new("open"), p]),
        ("kde-open", vec![p]),
        ("exo-open", vec![p]),
    ] {
        if let Ok(child) = std::process::Command::new(cmd.0)
            .args(&cmd.1)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
        {
            drop(child); // launcher detaches; zombie reaped at exit — fine.
            return true;
        }
    }
    false
}

impl AppState for FilesApp {
    fn app_id(&self) -> &'static str {
        "rabbiz-files"
    }
    fn title(&self) -> &'static str {
        "Files"
    }

    fn handle_event(&mut self, event: AppEvent, ti: &TextInputCtl) -> AppFrame {
        match event {
            AppEvent::TouchDown { id, pos } => {
                self.status = None; // toast dismissed on next touch
                self.tracker.down(id, pos, std::time::Instant::now());
                // Press-state for buttons/items.
                self.pressed_hit = self.hit_at(pos).cloned();
                if self.pressed_hit.is_none() && self.dialog.is_none() {
                    self.pressed_item = self.item_at(pos);
                }
                AppFrame {
                    dirty: true,
                    animating: false,
                    quit: false,
                }
            }
            AppEvent::TouchMove { id, pos } => {
                let now = std::time::Instant::now();
                for ev in self.tracker.move_to(id, pos, now) {
                    self.gesture(ev, ti);
                }
                AppFrame {
                    dirty: true,
                    animating: false,
                    quit: false,
                }
            }
            AppEvent::TouchUp { id, pos } => {
                let now = std::time::Instant::now();
                for ev in self.tracker.up(id, pos, now) {
                    self.gesture(ev, ti);
                }
                self.pressed_hit = None;
                self.pressed_item = None;
                self.scroll_anchor = None;
                AppFrame {
                    dirty: true,
                    animating: false,
                    quit: false,
                }
            }
            AppEvent::Key { keysym, utf8 } => self.key(keysym, utf8.as_deref(), ti),
            AppEvent::TextCommit(s) => {
                if let Some(b) = self.focused_buf() {
                    b.insert(&s);
                    self.after_text_change(ti);
                }
                AppFrame {
                    dirty: true,
                    animating: false,
                    quit: false,
                }
            }
            AppEvent::TextBackspace(n) => {
                if let Some(b) = self.focused_buf() {
                    b.backspace(n);
                    self.after_text_change(ti);
                }
                AppFrame {
                    dirty: true,
                    animating: false,
                    quit: false,
                }
            }
            _ => AppFrame::default(),
        }
    }

    fn render(&mut self, c: &mut Canvas, t: &Theme) {
        self.render_app(c, t);
    }
}

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    // Headless modes run before the Wayland app is touched at all:
    // `--cli <cmd>` for the text CLI, `--mcp` for the stdio MCP server.
    // Both share `model.rs`, so they behave exactly like the GUI.
    if argv.iter().any(|a| a == "--mcp") {
        cli::serve_mcp();
    }
    if argv.first().map(String::as_str) == Some("--cli") {
        std::process::exit(cli::run_cli(&argv[1..]));
    }
    match argv.first().map(String::as_str) {
        Some("-h") | Some("--help") | Some("help") => {
            print!("{}", cli::help_text());
        }
        Some(flag) if flag.starts_with('-') => {
            eprintln!("error: unknown flag '{flag}'\n");
            eprint!("{}", cli::help_text());
            std::process::exit(2);
        }
        // `rabbiz-files <path>`: the desktop entry passes files/dirs via
        // `%f`; a file opens its parent dir (browser, not viewer).
        Some(_) if argv.len() == 1 => match cli::browse_dir_arg(&argv[0]) {
            Ok(dir) => {
                let app = FilesApp::new(Some(dir));
                rabbiz_app_kit::run_app(app);
            }
            Err(e) => {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        },
        Some(_) => {
            eprintln!("error: too many arguments\n");
            eprint!("{}", cli::help_text());
            std::process::exit(2);
        }
        None => {
            let app = FilesApp::new(None);
            rabbiz_app_kit::run_app(app);
        }
    }
}
