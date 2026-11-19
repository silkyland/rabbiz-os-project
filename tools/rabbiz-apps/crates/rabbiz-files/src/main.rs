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
mod layout;
mod model;

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use de_api::{Point, Rect};
use rabbiz_app_kit::text;
use rabbiz_app_kit::ui;
use rabbiz_app_kit::{AppEvent, AppFrame, AppState, Canvas, TextInputCtl, Theme};
use wayland_protocols::wp::text_input::zv3::client::zwp_text_input_v3::ContentPurpose;

use gestures::{edge_commits, two_finger_dir, GestureEvent, Tracker};
use layout as lay;
use model::{Entry, FileKind, SortKey};

const PATH_BAR_H: i32 = 46;
const SEARCH_H: i32 = 54;
const BANNER_H: i32 = 58;
const FOOTER_H: i32 = 40;
const ACTBAR_H: i32 = 68;
/// Projected scroll distance for a flick release (px) — applied instantly
/// because the kit has no frame timer (see module docs).
const FLICK_PROJ_PX: f32 = 220.0;
/// Cap on recursive work shown in details (mirrors model::WALK_CAP).
const DETAIL_ITEM_CAP: usize = model::WALK_CAP;

// ---------------------------------------------------------------------------
// Small UI model types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct TextBuf {
    s: String,
    /// Byte offset, always on a char boundary.
    cursor: usize,
}

impl TextBuf {
    fn new(s: &str) -> Self {
        Self {
            s: s.to_string(),
            cursor: s.len(),
        }
    }
    fn insert(&mut self, ins: &str) {
        let pos = self.cursor.min(self.s.len());
        self.s.insert_str(pos, ins);
        self.cursor = pos + ins.len();
    }
    /// Delete up to `n` characters before the cursor.
    fn backspace(&mut self, n: usize) {
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
enum Focus {
    None,
    Search,
    Dialog,
}

#[derive(Debug, Clone)]
enum Dialog {
    NewFolder,
    Rename(PathBuf),
    Delete { paths: Vec<PathBuf>, items: usize },
    Details(PathBuf),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SelAct {
    Open,
    Rename,
    Copy,
    Move,
    Delete,
    Info,
    Done,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OpKind {
    Copy,
    Move,
}

impl OpKind {
    fn verb(self) -> &'static str {
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
enum Item {
    Parent,
    Ent(usize),
}

#[derive(Debug, Clone, PartialEq)]
enum Hit {
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

struct FilesApp {
    home: PathBuf,
    cwd: PathBuf,
    /// Raw directory entries (unsorted/unfiltered).
    raw: Vec<Entry>,
    load_error: Option<String>,
    /// Visit history for back/forward gestures.
    hist: Vec<PathBuf>,
    hist_i: usize,

    key: SortKey,
    show_hidden: bool,
    grid: bool,
    search: TextBuf,
    search_open: bool,

    /// Derived display list — rebuilt lazily via `needs_sync`.
    vis: Vec<Entry>,
    items: Vec<Item>,
    needs_sync: bool,

    // geometry filled by render()
    cur_w: i32,
    cur_h: i32,
    view_top: i32,
    view_h: i32,
    scroll: f32,
    scroll_anchor: Option<f32>,
    drag_dy: i32,
    pressed_item: Option<usize>,
    pressed_hit: Option<Hit>,

    tracker: Tracker,
    /// Edge-swipe visual offset (px, ≥0).
    nav_dx: i32,

    selection: HashSet<PathBuf>,
    select_mode: bool,

    dialog: Option<Dialog>,
    text: TextBuf,
    focus: Focus,
    menu_open: bool,
    menu_rect: Rect,

    pending_op: Option<(OpKind, Vec<PathBuf>)>,
    status: Option<(String, bool)>, // (message, is_error)

    hits: Vec<(Rect, Hit)>,
    crumbs: Vec<(String, Rect)>,
    /// Jump target per crumb (parallel to `crumbs`).
    crumb_targets: Vec<PathBuf>,
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

    fn reload(&mut self) {
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

    fn sync(&mut self) {
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

    fn content_count(&self) -> usize {
        self.items.len()
    }

    fn max_scroll(&self) -> i32 {
        lay::max_scroll(self.content_count(), self.view_h, self.grid)
    }

    fn clamp_scroll(&mut self) {
        self.scroll = lay::clamp_scroll(self.scroll, self.max_scroll());
    }

    // ---------------- navigation ----------------

    fn navigate(&mut self, path: PathBuf) {
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

    fn go_back(&mut self) {
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

    fn go_forward(&mut self) {
        if self.hist_i + 1 < self.hist.len() {
            self.hist_i += 1;
            self.cwd = self.hist[self.hist_i].clone();
            self.clear_transient();
            self.reload();
        }
    }

    fn can_back(&self) -> bool {
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

    fn item_path(&self, idx: usize) -> Option<PathBuf> {
        match self.items.get(idx) {
            Some(Item::Ent(i)) => self.vis.get(*i).map(|e| e.path.clone()),
            _ => None,
        }
    }

    fn toggle_select(&mut self, path: PathBuf) {
        if !self.selection.remove(&path) {
            self.selection.insert(path);
        }
        if self.selection.is_empty() {
            self.select_mode = false;
        }
    }

    fn enter_selection(&mut self, path: PathBuf) {
        self.select_mode = true;
        self.selection.clear();
        self.selection.insert(path);
    }

    fn open_item(&mut self, idx: usize, ti: &TextInputCtl) {
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
    fn open_file(&mut self, path: &Path, _ti: &TextInputCtl) {
        if open_external(path) {
            return;
        }
        self.show_details(path.to_path_buf());
    }

    fn show_details(&mut self, path: PathBuf) {
        self.menu_open = false;
        self.dialog = Some(Dialog::Details(path));
        self.focus = Focus::None;
    }

    fn delete_selected(&mut self) {
        self.menu_open = false;
        let paths: Vec<PathBuf> = self.selection.iter().cloned().collect();
        if paths.is_empty() {
            return;
        }
        let items: usize = paths.iter().map(|p| model::count_tree(p)).sum();
        self.dialog = Some(Dialog::Delete { paths, items });
        self.focus = Focus::None;
    }

    fn confirm_delete(&mut self) {
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

    fn rename_selected(&mut self, ti: &TextInputCtl) {
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

    fn confirm_rename(&mut self) {
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

    fn new_folder(&mut self, ti: &TextInputCtl) {
        self.menu_open = false;
        self.text = TextBuf::new("New folder");
        self.dialog = Some(Dialog::NewFolder);
        self.focus_dialog(ti);
    }

    fn confirm_new_folder(&mut self) {
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

    fn start_op(&mut self, kind: OpKind) {
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

    fn paste_here(&mut self) {
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

    fn focus_dialog(&mut self, ti: &TextInputCtl) {
        self.focus = Focus::Dialog;
        ti.activate(ContentPurpose::Normal);
        ti.set_surrounding(&self.text.s, self.text.cursor);
    }

    fn focused_buf(&mut self) -> Option<&mut TextBuf> {
        match self.focus {
            Focus::Dialog => Some(&mut self.text),
            Focus::Search => Some(&mut self.search),
            Focus::None => None,
        }
    }

    fn after_text_change(&mut self, ti: &TextInputCtl) {
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

    // ---------------- gestures ----------------

    fn gesture(&mut self, ev: GestureEvent, ti: &TextInputCtl) {
        match ev {
            GestureEvent::ScrollDelta { dy, .. } => {
                // Scrolling cancels any press highlight.
                self.pressed_item = None;
                self.pressed_hit = None;
                if self.scroll_anchor.is_none() {
                    self.scroll_anchor = Some(self.scroll);
                    self.drag_dy = 0;
                }
                self.drag_dy += dy;
                self.scroll = lay::clamp_scroll(
                    self.scroll_anchor.unwrap() - self.drag_dy as f32,
                    self.max_scroll(),
                );
            }
            GestureEvent::ScrollFlick { vy } => {
                self.scroll_anchor = None;
                // Instant projected scroll — no frame timer for a glide.
                let target = self.scroll - vy * FLICK_PROJ_PX;
                self.scroll = lay::clamp_scroll(target, self.max_scroll());
            }
            GestureEvent::EdgeDrag { dx } => {
                self.nav_dx = dx.min(self.cur_w);
            }
            GestureEvent::EdgeRelease { dx, vx } => {
                self.nav_dx = 0;
                if edge_commits(dx, vx) {
                    self.go_back();
                }
            }
            GestureEvent::TwoFingerRelease { dx } => match two_finger_dir(dx) {
                -1 => self.go_back(),
                1 => self.go_forward(),
                _ => {}
            },
            GestureEvent::TwoFingerTap { pos } | GestureEvent::LongPress { pos } => {
                if self.dialog.is_none() && self.pending_op.is_none() {
                    self.long_press_at(pos);
                }
            }
            GestureEvent::Tap { pos } => self.tap(pos, ti),
        }
    }

    /// Long-press / two-finger tap: enter selection on the item under `pos`,
    /// or toggle it if selection mode is already active.
    fn long_press_at(&mut self, pos: Point) {
        if let Some(idx) = self.item_at(pos) {
            if let Some(p) = self.item_path(idx) {
                if self.select_mode {
                    self.toggle_select(p);
                } else {
                    self.enter_selection(p);
                }
            }
        }
    }

    /// Item index under a viewport point, honoring scroll and the "+parent"
    /// row. Returns None outside the content area or on gaps.
    fn item_at(&self, pos: Point) -> Option<usize> {
        if pos.y < self.view_top || pos.y >= self.view_top + self.view_h {
            return None;
        }
        let y = (pos.y - self.view_top) as f32 + self.scroll;
        let idx = if self.grid {
            lay::cell_at(pos.x, y, self.cur_w)
        } else {
            lay::row_at(y)
        };
        idx.filter(|i| *i < self.items.len())
    }

    // ---------------- hit testing / tap ----------------

    fn hit_at(&self, pos: Point) -> Option<&Hit> {
        // Last registered wins (overlays drawn later).
        self.hits
            .iter()
            .rev()
            .find(|(r, _)| r.contains(pos))
            .map(|(_, h)| h)
    }

    fn tap(&mut self, pos: Point, ti: &TextInputCtl) {
        // Modal dialog: only its own buttons/field are live — everything
        // drawn under the scrim (crumbs, toolbar, rows, menu) must not fire.
        if self.dialog.is_some() {
            if let Some(hit) = self
                .hits
                .iter()
                .rev()
                .find(|(r, h)| {
                    r.contains(pos) && matches!(h, Hit::DlgOk | Hit::DlgCancel | Hit::DlgField)
                })
                .map(|(_, h)| h.clone())
            {
                self.activate_hit(hit, ti);
            }
            return;
        }
        // Overlay dismiss: menu closes on outside tap.
        if self.menu_open {
            let inside = self.menu_rect.contains(pos)
                || self
                    .hits
                    .iter()
                    .any(|(r, h)| matches!(h, Hit::SortBtn) && r.contains(pos));
            if !inside {
                self.menu_open = false;
                return;
            }
        }
        if let Some(hit) = self.hit_at(pos).cloned() {
            self.activate_hit(hit, ti);
            return;
        }
        // Items (only when no modal dialog — already returned above).
        {
            if let Some(idx) = self.item_at(pos) {
                if self.select_mode {
                    if let Some(p) = self.item_path(idx) {
                        self.toggle_select(p);
                    }
                } else if self.pending_op.is_some() {
                    // Picker: only directories navigate.
                    if let Some(Item::Ent(i)) = self.items.get(idx) {
                        if let Some(e) = self.vis.get(*i) {
                            if e.is_dir {
                                self.navigate(e.path.clone());
                            }
                        }
                    } else if matches!(self.items.get(idx), Some(Item::Parent)) {
                        if let Some(p) = self.cwd.parent() {
                            self.navigate(p.to_path_buf());
                        }
                    }
                } else {
                    self.open_item(idx, ti);
                }
            }
        }
    }

    fn activate_hit(&mut self, hit: Hit, ti: &TextInputCtl) {
        match hit {
            Hit::Back => self.go_back(),
            Hit::Home => self.navigate(self.home.clone()),
            Hit::Crumb(i) => {
                if let Some(t) = self.crumb_targets.get(i) {
                    self.navigate(t.clone());
                }
            }
            Hit::SearchBtn => {
                self.search_open = !self.search_open;
                if self.search_open {
                    self.focus = Focus::Search;
                    ti.activate(ContentPurpose::Normal);
                    ti.set_surrounding(&self.search.s, self.search.cursor);
                } else {
                    self.search.s.clear();
                    self.search.cursor = 0;
                    self.needs_sync = true;
                    if self.focus == Focus::Search {
                        self.focus = Focus::None;
                        ti.deactivate();
                    }
                }
            }
            Hit::SearchField => {
                if self.focus != Focus::Search {
                    self.focus = Focus::Search;
                    ti.activate(ContentPurpose::Normal);
                    ti.set_surrounding(&self.search.s, self.search.cursor);
                }
            }
            Hit::SearchClear => {
                self.search = TextBuf::new("");
                self.needs_sync = true;
            }
            Hit::SortBtn => self.menu_open = !self.menu_open,
            Hit::ViewBtn => self.grid = !self.grid,
            Hit::Fab => self.new_folder(ti),
            Hit::Sel(act) => self.sel_action(act, ti),
            Hit::Paste => self.paste_here(),
            Hit::PickerCancel => {
                self.pending_op = None;
            }
            Hit::MenuSort(k) => {
                self.key = k;
                self.menu_open = false;
                self.needs_sync = true;
            }
            Hit::MenuHidden => {
                self.show_hidden = !self.show_hidden;
                self.needs_sync = true;
            }
            Hit::DlgOk => self.dialog_ok(ti),
            Hit::DlgCancel => self.close_dialog(ti),
            Hit::DlgField => {
                if matches!(self.dialog, Some(Dialog::NewFolder | Dialog::Rename(_))) {
                    self.focus_dialog(ti);
                }
            }
            Hit::Retry => self.reload(),
        }
    }

    fn dialog_ok(&mut self, _ti: &TextInputCtl) {
        match self.dialog.clone() {
            Some(Dialog::NewFolder) => self.confirm_new_folder(),
            Some(Dialog::Rename(_)) => self.confirm_rename(),
            Some(Dialog::Delete { .. }) => self.confirm_delete(),
            Some(Dialog::Details(_)) => self.close_dialog(_ti),
            None => {}
        }
    }

    fn close_dialog(&mut self, ti: &TextInputCtl) {
        self.dialog = None;
        if self.focus == Focus::Dialog {
            self.focus = Focus::None;
            ti.deactivate();
        }
    }

    fn sel_action(&mut self, act: SelAct, ti: &TextInputCtl) {
        match act {
            SelAct::Done => {
                self.select_mode = false;
                self.selection.clear();
            }
            SelAct::Open => {
                if self.selection.len() == 1 {
                    let p = self.selection.iter().next().unwrap().clone();
                    self.selection.clear();
                    self.select_mode = false;
                    if p.is_dir() {
                        self.navigate(p);
                    } else {
                        self.open_file(&p, ti);
                    }
                }
            }
            SelAct::Rename => self.rename_selected(ti),
            SelAct::Copy => self.start_op(OpKind::Copy),
            SelAct::Move => self.start_op(OpKind::Move),
            SelAct::Delete => self.delete_selected(),
            SelAct::Info => {
                if self.selection.len() == 1 {
                    let p = self.selection.iter().next().unwrap().clone();
                    self.show_details(p);
                }
            }
        }
    }

    // ---------------- keys / text ----------------

    fn key(&mut self, keysym: u32, utf8: Option<&str>, ti: &TextInputCtl) -> AppFrame {
        match keysym {
            0xff1b => {
                // Esc: close topmost layer.
                if self.dialog.is_some() {
                    self.close_dialog(ti);
                } else if self.menu_open {
                    self.menu_open = false;
                } else if self.search_open {
                    self.search_open = false;
                    self.search = TextBuf::new("");
                    self.needs_sync = true;
                    if self.focus == Focus::Search {
                        self.focus = Focus::None;
                        ti.deactivate();
                    }
                } else if self.select_mode {
                    self.select_mode = false;
                    self.selection.clear();
                } else if self.pending_op.is_some() {
                    self.pending_op = None;
                }
            }
            0xff0d => {
                if self.dialog.is_some() {
                    self.dialog_ok(ti);
                }
            }
            0xff08 => {
                if let Some(b) = self.focused_buf() {
                    b.backspace(1);
                    self.after_text_change(ti);
                } else if self.dialog.is_none() && self.cwd.parent().is_some() {
                    self.go_back();
                }
            }
            _ => {
                if let Some(s) = utf8 {
                    if let Some(b) = self.focused_buf() {
                        b.insert(s);
                        self.after_text_change(ti);
                    }
                }
            }
        }
        AppFrame {
            dirty: true,
            animating: false,
            quit: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

fn alpha(c: [u8; 4], a: u8) -> [u8; 4] {
    [c[0], c[1], c[2], a]
}

/// Rasterize + blit a Material icon with ink bounds centered on (cx, cy).
fn draw_icon(c: &mut Canvas, codepoint: &str, size: f32, color: [u8; 4], cx: i32, cy: i32) {
    if let Some(px) = text::icon(codepoint, size, color) {
        let (x0, y0, x1, y1) =
            text::ink_bounds(&px).unwrap_or((0, 0, px.width() as i32, px.height() as i32));
        c.blit(
            &de_api::Pixels::new(
                de_api::Size::new(px.width() as i32, px.height() as i32),
                px.data().to_vec(),
            ),
            Point::new(cx - (x0 + x1) / 2, cy - (y0 + y1) / 2),
        );
    }
}

fn kind_icon(kind: FileKind) -> &'static str {
    match kind {
        FileKind::Folder => "\u{e2c7}",
        FileKind::Image => "\u{e3f4}",
        FileKind::Audio => "\u{e3a1}",
        FileKind::Video => "\u{e04b}",
        FileKind::Archive => "\u{e149}",
        FileKind::Code => "\u{e86f}",
        FileKind::Document => "\u{e873}",
        FileKind::Generic => "\u{e24d}",
    }
}

fn hit_rect(hits: &mut Vec<(Rect, Hit)>, r: Rect, h: Hit) {
    if r.w > 0 && r.h > 0 {
        hits.push((r, h));
    }
}

fn icon_button(
    c: &mut Canvas,
    t: &Theme,
    hits: &mut Vec<(Rect, Hit)>,
    r: Rect,
    codepoint: &str,
    hit: Hit,
    active: bool,
) {
    c.fill_rounded_rect(r, (r.h as f32) / 2.0, t.surface1);
    let col = if active { t.accent } else { t.text };
    draw_icon(c, codepoint, 26.0, col, r.x + r.w / 2, r.y + r.h / 2);
    hit_rect(hits, r, hit);
}

impl FilesApp {
    // ---------------- render ----------------

    fn render(&mut self, c: &mut Canvas, t: &Theme) {
        let size = c.size();
        self.cur_w = size.w;
        self.cur_h = size.h;
        self.sync();
        self.hits.clear();
        self.crumbs.clear();
        self.crumb_targets.clear();

        let mut hits = std::mem::take(&mut self.hits);
        self.render_browser(c, t, &mut hits);
        if self.menu_open {
            self.render_menu(c, t, &mut hits);
        }
        if let Some(d) = self.dialog.clone() {
            self.render_dialog(c, t, &mut hits, &d);
        }
        self.hits = hits;
    }

    fn render_browser(&mut self, c: &mut Canvas, t: &Theme, hits: &mut Vec<(Rect, Hit)>) {
        let w = self.cur_w;
        let h = self.cur_h;

        // ---- path bar ----
        let bar_y = ui::TOP_SAFE + 8;
        let mut x = ui::MARGIN;
        if self.can_back() {
            let r = Rect::new(x, bar_y, 44, PATH_BAR_H);
            icon_button(c, t, hits, r, "\u{e5c4}", Hit::Back, false);
            x += 48;
        }
        {
            let r = Rect::new(x, bar_y, 44, PATH_BAR_H);
            icon_button(c, t, hits, r, "\u{e88a}", Hit::Home, false);
            x += 48;
        }
        // Breadcrumb segments.
        let segs = self.crumb_segments();
        let sep_w = 14;
        let avail = w - ui::MARGIN - x - 3 * 52 - 16;
        let mut cx = x;
        let n = segs.len();
        for (i, (label, target)) in segs.iter().enumerate() {
            let st = ui::styled(19.0, if i + 1 == n { 600 } else { 400 }, t.text);
            let max_w = if i + 1 == n {
                (avail - cx + x).max(40)
            } else {
                180
            };
            let Some(px) = text::label(label, st, Some(max_w)) else {
                continue;
            };
            let tw = px.width() as i32;
            if cx + tw > w - ui::MARGIN - 3 * 52 {
                break;
            }
            let r = Rect::new(cx - 4, bar_y, tw + 8, PATH_BAR_H);
            self.crumbs.push((label.clone(), r));
            self.crumb_targets.push(target.clone());
            c.blit(
                &de_api::Pixels::new(
                    de_api::Size::new(tw, px.height() as i32),
                    px.data().to_vec(),
                ),
                Point::new(cx, bar_y + (PATH_BAR_H - px.height() as i32) / 2),
            );
            hit_rect(hits, r, Hit::Crumb(self.crumbs.len() - 1));
            cx += tw;
            if i + 1 < n {
                let st_sep = ui::caption(t.text3);
                let sep = " / ";
                if let Some(sp) = text::label(sep, st_sep, None) {
                    c.blit(
                        &de_api::Pixels::new(
                            de_api::Size::new(sp.width() as i32, sp.height() as i32),
                            sp.data().to_vec(),
                        ),
                        Point::new(cx, bar_y + (PATH_BAR_H - sp.height() as i32) / 2),
                    );
                }
                cx += sep_w;
            }
        }
        // Right-side icon buttons.
        let mut bx = w - ui::MARGIN - 48;
        icon_button(
            c,
            t,
            hits,
            Rect::new(bx, bar_y, 44, PATH_BAR_H),
            if self.grid { "\u{e8ef}" } else { "\u{e9b0}" },
            Hit::ViewBtn,
            false,
        );
        bx -= 52;
        icon_button(
            c,
            t,
            hits,
            Rect::new(bx, bar_y, 44, PATH_BAR_H),
            "\u{e164}",
            Hit::SortBtn,
            self.menu_open,
        );
        bx -= 52;
        icon_button(
            c,
            t,
            hits,
            Rect::new(bx, bar_y, 44, PATH_BAR_H),
            "\u{e8b6}",
            Hit::SearchBtn,
            self.search_open || !self.search.s.is_empty(),
        );

        // ---- search row ----
        let mut top = bar_y + PATH_BAR_H + 8;
        if self.search_open {
            let r = Rect::new(ui::MARGIN, top, w - ui::MARGIN * 2 - 44, SEARCH_H - 8);
            c.fill_rounded_rect(r, (r.h as f32) / 2.0, t.surface1);
            draw_icon(c, "\u{e8b6}", 22.0, t.text3, r.x + 26, r.y + r.h / 2);
            let text = if self.search.s.is_empty() {
                "Search files".to_string()
            } else {
                self.search.s.clone()
            };
            let col = if self.search.s.is_empty() {
                t.text3
            } else {
                t.text
            };
            ui::draw_text(
                c,
                &text,
                &ui::styled(20.0, 400, col),
                r.x + 50,
                r.y + (r.h - 26).max(0) / 2,
                Some(r.w - 90),
            );
            hit_rect(hits, r, Hit::SearchField);
            if !self.search.s.is_empty() {
                let cr = Rect::new(r.right() - 40, r.y + 4, 36, r.h - 8);
                c.fill_rounded_rect(cr, (cr.h as f32) / 2.0, t.surface2);
                draw_icon(
                    c,
                    "\u{e5cd}",
                    18.0,
                    t.text,
                    cr.x + cr.w / 2,
                    cr.y + cr.h / 2,
                );
                hit_rect(hits, cr, Hit::SearchClear);
            }
            top += SEARCH_H;
        }

        // ---- picker banner ----
        if let Some((kind, srcs)) = &self.pending_op {
            let r = Rect::new(ui::MARGIN, top, w - ui::MARGIN * 2, BANNER_H - 8);
            c.fill_rounded_rect(r, 18.0, t.surface1);
            ui::draw_text(
                c,
                &format!(
                    "{} {} item{} — pick a folder",
                    kind.verb(),
                    srcs.len(),
                    if srcs.len() == 1 { "" } else { "s" }
                ),
                &ui::label(t.text),
                r.x + 20,
                r.y + 12,
                Some(r.w - 320),
            );
            ui::draw_text(
                c,
                &self.cwd.display().to_string(),
                &ui::caption(t.text3),
                r.x + 20,
                r.y + 34,
                Some(r.w - 320),
            );
            let bw = 120;
            let paste = Rect::new(r.right() - bw - 16 - 96, r.y + 10, bw, r.h - 20);
            ui::draw_pill_button(
                c,
                t,
                paste,
                "Paste here",
                true,
                self.pressed_hit.as_ref() == Some(&Hit::Paste),
            );
            hit_rect(hits, paste, Hit::Paste);
            let cancel = Rect::new(paste.x - 104, r.y + 10, 96, r.h - 20);
            ui::draw_pill_button(c, t, cancel, "Cancel", false, false);
            hit_rect(hits, cancel, Hit::PickerCancel);
            top += BANNER_H;
        }

        // ---- content viewport ----
        self.view_top = top + 8;
        let bottom_reserved =
            ui::BOTTOM_SAFE + FOOTER_H + if self.select_mode { ACTBAR_H + 8 } else { 0 };
        self.view_h = (h - bottom_reserved - self.view_top).max(0);
        self.clamp_scroll();

        // Edge-swipe parallax: content slides right with the finger.
        let ox = self.nav_dx;

        if self.load_error.is_some() {
            self.render_error(c, t, hits);
        } else if self.items.is_empty() {
            self.render_empty(c, t);
        } else {
            self.render_items(c, t, ox);
            self.render_scrollbar(c, t);
        }

        // ---- storage footer ----
        self.render_footer(c, t);

        // ---- FAB ----
        if !self.select_mode && self.pending_op.is_none() && self.dialog.is_none() {
            let d = 56;
            let r = Rect::new(
                w - ui::MARGIN - d,
                h - ui::BOTTOM_SAFE - FOOTER_H - d - 16,
                d,
                d,
            );
            c.fill_rounded_rect(r, (d as f32) / 2.0, t.accent);
            draw_icon(
                c,
                "\u{e2cc}",
                28.0,
                t.text_on_accent,
                r.x + d / 2,
                r.y + d / 2,
            );
            hit_rect(hits, r, Hit::Fab);
        }

        // ---- selection action bar ----
        if self.select_mode {
            self.render_action_bar(c, t, hits);
        }

        // ---- status toast ----
        if let Some((msg, is_err)) = &self.status {
            let col = if *is_err { t.danger } else { t.success };
            let st = ui::styled(18.0, 500, col);
            let max_w = w - ui::MARGIN * 2 - 40;
            let Some(px) = text::label(msg, st, Some(max_w)) else {
                return;
            };
            let tw = px.width() as i32 + 40;
            let th = 44;
            let r = Rect::new(
                (w - tw) / 2,
                h - ui::BOTTOM_SAFE - FOOTER_H - th - 76,
                tw,
                th,
            );
            c.fill_rounded_rect(r, 22.0, t.surface1);
            c.blit(
                &de_api::Pixels::new(
                    de_api::Size::new(px.width() as i32, px.height() as i32),
                    px.data().to_vec(),
                ),
                Point::new(r.x + 20, r.y + (th - px.height() as i32) / 2),
            );
        }
    }

    /// Breadcrumb labels + jump targets, root→leaf.
    fn crumb_segments(&self) -> Vec<(String, PathBuf)> {
        let mut segs: Vec<(String, PathBuf)> = Vec::new();
        let mut cur = PathBuf::from("/");
        segs.push(("/".to_string(), cur.clone()));
        for comp in self.cwd.components() {
            use std::path::Component;
            match comp {
                Component::RootDir => {}
                Component::Normal(os) => {
                    cur.push(os);
                    let label = if cur == self.home {
                        "Home".to_string()
                    } else {
                        os.to_string_lossy().into_owned()
                    };
                    segs.push((label, cur.clone()));
                }
                _ => {}
            }
        }
        // De-duplicate the leading "/" when cwd == "/".
        if self.cwd == Path::new("/") {
            segs.truncate(1);
        } else if segs.len() > 1 && segs[0].1 == Path::new("/") {
            // keep "/" crumb + each component
        }
        segs
    }

    fn render_error(&self, c: &mut Canvas, t: &Theme, hits: &mut Vec<(Rect, Hit)>) {
        let cx = self.cur_w / 2;
        let cy = self.view_top + self.view_h / 2;
        draw_icon(c, "\u{e002}", 56.0, t.danger, cx, cy - 60);
        ui::draw_centered(
            c,
            "Can't open this folder",
            &ui::label(t.text),
            Rect::new(0, cy, self.cur_w, 40),
        );
        let msg = self.load_error.clone().unwrap_or_default();
        ui::draw_centered(
            c,
            &msg,
            &ui::caption(t.text3),
            Rect::new(0, cy + 44, self.cur_w, 40),
        );
        let bw = 160;
        let r = Rect::new(cx - bw / 2, cy + 96, bw, 52);
        ui::draw_pill_button(c, t, r, "Retry", true, false);
        hit_rect(hits, r, Hit::Retry);
    }

    fn render_empty(&self, c: &mut Canvas, t: &Theme) {
        let cx = self.cur_w / 2;
        let cy = self.view_top + self.view_h / 2;
        draw_icon(c, "\u{e2c7}", 56.0, t.text3, cx, cy - 60);
        let (title, sub) = if !self.search.s.is_empty() {
            (
                "No matches".to_string(),
                format!("Nothing matches “{}”", self.search.s),
            )
        } else if !self.show_hidden {
            (
                "Folder is empty".to_string(),
                "Hidden files are not shown".to_string(),
            )
        } else {
            ("Folder is empty".to_string(), String::new())
        };
        ui::draw_centered(
            c,
            &title,
            &ui::label(t.text),
            Rect::new(0, cy, self.cur_w, 40),
        );
        if !sub.is_empty() {
            ui::draw_centered(
                c,
                &sub,
                &ui::caption(t.text3),
                Rect::new(0, cy + 44, self.cur_w, 40),
            );
        }
    }

    fn render_items(&self, c: &mut Canvas, t: &Theme, ox: i32) {
        let view_top = self.view_top as f32;
        let view_h = self.view_h as f32;
        let scroll = self.scroll;
        // Visible index range.
        let first = if self.grid {
            let row = (scroll / (lay::CELL_H + lay::GAP) as f32) as usize;
            row.saturating_sub(1) * lay::GRID_COLS
        } else {
            ((scroll / (lay::ROW_H + lay::GAP) as f32) as usize).saturating_sub(1)
        };
        let last = if self.grid {
            (((scroll + view_h) / (lay::CELL_H + lay::GAP) as f32) as usize + 1) * lay::GRID_COLS
        } else {
            ((scroll + view_h) / (lay::ROW_H + lay::GAP) as f32) as usize + 1
        };
        for idx in first..last.min(self.items.len()) {
            let (y, h_cell) = if self.grid {
                (lay::cell_y(idx, scroll) + view_top, lay::CELL_H as f32)
            } else {
                (lay::row_y(idx, scroll) + view_top, lay::ROW_H as f32)
            };
            if y + h_cell < view_top || y > view_top + view_h {
                continue;
            }
            if self.grid {
                self.render_cell(c, t, idx, y as i32, ox);
            } else {
                self.render_row(c, t, idx, y as i32, ox);
            }
        }
    }

    fn render_row(&self, c: &mut Canvas, t: &Theme, idx: usize, y: i32, ox: i32) {
        let x = lay::PAD + ox;
        let w = self.cur_w - lay::PAD * 2;
        let r = Rect::new(x, y, w, lay::ROW_H);
        let item = self.items[idx];
        let (icon_kind, name, sub, is_sel, is_link) = self.item_info(item);
        let pressed = self.pressed_item == Some(idx) && self.pressed_hit.is_none();

        // Row surface.
        c.fill_rounded_rect(r, 16.0, t.surface1);
        if is_sel {
            c.fill_rounded_rect(r, 16.0, alpha(t.accent_dim, 90));
        } else if pressed {
            c.fill_rounded_rect(r, 16.0, ui::state_layer(t, 0.08));
        }

        let mut cx = x + 16;
        // Checkbox in selection mode.
        if self.select_mode {
            let cb = Rect::new(cx, y + (lay::ROW_H - 26) / 2, 26, 26);
            c.fill_rounded_rect(cb, 7.0, if is_sel { t.accent } else { t.surface3 });
            if is_sel {
                draw_icon(c, "\u{e5ca}", 18.0, t.text_on_accent, cb.x + 13, cb.y + 13);
            }
            cx += 40;
        }
        // Kind icon tile.
        let tile = Rect::new(cx, y + 12, 40, lay::ROW_H - 24);
        c.fill_rounded_rect(tile, 12.0, alpha(t.accent_dim, 60));
        let glyph_col = if item == Item::Parent {
            t.text3
        } else {
            t.accent
        };
        draw_icon(
            c,
            if item == Item::Parent {
                "\u{e5d8}"
            } else {
                kind_icon(icon_kind)
            },
            24.0,
            glyph_col,
            tile.x + tile.w / 2,
            tile.y + tile.h / 2,
        );
        cx += tile.w + 16;
        let text_w = r.right() - 16 - cx;
        ui::draw_text(
            c,
            &name,
            &ui::styled(21.0, if item == Item::Parent { 500 } else { 450 }, t.text),
            cx,
            y + 12,
            Some(text_w),
        );
        let sub_line = if is_link {
            format!("{sub} · link")
        } else {
            sub
        };
        ui::draw_text(
            c,
            &sub_line,
            &ui::caption(t.text3),
            cx,
            y + 38,
            Some(text_w),
        );
        // Trailing modified date for files.
        if let Item::Ent(i) = item {
            if let Some(e) = self.vis.get(i) {
                if !e.is_dir {
                    let date = model::format_time(e.mtime);
                    let st = ui::styled(15.0, 400, t.text3);
                    let max_w = 150;
                    if let Some(px) = text::label(&date, st, Some(max_w)) {
                        c.blit(
                            &de_api::Pixels::new(
                                de_api::Size::new(px.width() as i32, px.height() as i32),
                                px.data().to_vec(),
                            ),
                            Point::new(
                                r.right() - 16 - px.width() as i32,
                                y + (lay::ROW_H - px.height() as i32) / 2,
                            ),
                        );
                    }
                }
            }
        }
    }

    fn render_cell(&self, c: &mut Canvas, t: &Theme, idx: usize, y: i32, ox: i32) {
        let x = lay::cell_x(idx, self.cur_w) + ox;
        let cw = lay::cell_w(self.cur_w);
        let r = Rect::new(x, y, cw, lay::CELL_H);
        let item = self.items[idx];
        let (icon_kind, name, _sub, is_sel, _link) = self.item_info(item);
        let pressed = self.pressed_item == Some(idx) && self.pressed_hit.is_none();

        if is_sel {
            c.fill_rounded_rect(r, 18.0, alpha(t.accent_dim, 90));
        } else if pressed {
            c.fill_rounded_rect(r, 18.0, ui::state_layer(t, 0.08));
        }

        // Checkbox overlay top-left in selection mode.
        if self.select_mode {
            let cb = Rect::new(x + 8, y + 8, 26, 26);
            c.fill_rounded_rect(
                cb,
                8.0,
                if is_sel {
                    t.accent
                } else {
                    alpha(t.surface3, 230)
                },
            );
            if is_sel {
                draw_icon(c, "\u{e5ca}", 18.0, t.text_on_accent, cb.x + 13, cb.y + 13);
            }
        }

        let icon_sz = 52.0;
        let cy = y + 18 + (icon_sz as i32) / 2;
        let glyph = if item == Item::Parent {
            "\u{e5d8}"
        } else {
            kind_icon(icon_kind)
        };
        let col = if item == Item::Parent {
            t.text3
        } else {
            t.text
        };
        draw_icon(c, glyph, icon_sz, col, x + cw / 2, cy);
        ui::draw_centered(
            c,
            &name,
            &ui::styled(17.0, 500, t.text),
            Rect::new(x + 6, y + 84, cw - 12, 40),
        );
        ui::draw_centered(
            c,
            &_sub,
            &ui::caption(t.text3),
            Rect::new(x + 6, y + 122, cw - 12, 30),
        );
    }

    /// Name/subtitle/kind for an item (parent row synthesizes "..").
    fn item_info(&self, item: Item) -> (FileKind, String, String, bool, bool) {
        match item {
            Item::Parent => (
                FileKind::Folder,
                "..".to_string(),
                "Parent folder".to_string(),
                false,
                false,
            ),
            Item::Ent(i) => {
                let e = &self.vis[i];
                let sub = if e.is_dir {
                    "Folder".to_string()
                } else {
                    format!(
                        "{} · {}",
                        model::format_size(e.size),
                        model::format_time(e.mtime)
                    )
                };
                let sel = self.selection.contains(&e.path);
                (e.kind, e.name.clone(), sub, sel, e.is_link)
            }
        }
    }

    fn render_scrollbar(&self, c: &mut Canvas, t: &Theme) {
        let max = self.max_scroll();
        if max <= 0 || self.view_h <= 0 {
            return;
        }
        let content = lay::content_h(self.content_count(), self.grid);
        let track_h = self.view_h;
        let thumb_h = ((track_h as i64 * track_h as i64) / content.max(1) as i64) as i32;
        let thumb_h = thumb_h.clamp(48, track_h);
        let travel = track_h - thumb_h;
        let ty = self.view_top + (travel as f32 * (self.scroll / max as f32)) as i32;
        let r = Rect::new(self.cur_w - 8, ty, 5, thumb_h);
        c.fill_rounded_rect(r, 2.5, alpha(t.text3, 110));
    }

    fn render_footer(&self, c: &mut Canvas, t: &Theme) {
        let y = self.cur_h - ui::BOTTOM_SAFE - FOOTER_H;
        let Some((free, total)) = model::free_space(&self.cwd) else {
            return;
        };
        let used_pct = if total > 0 {
            ((total - free) as f32 / total as f32 * 100.0) as i32
        } else {
            0
        };
        let label = format!(
            "{} free of {} · {}% used",
            model::format_size(free),
            model::format_size(total),
            used_pct
        );
        ui::draw_text(
            c,
            &label,
            &ui::caption(t.text3),
            ui::MARGIN,
            y + (FOOTER_H - 20).max(0) / 2,
            Some(self.cur_w - ui::MARGIN * 2 - 140),
        );
        // Usage bar, right side.
        let bw = 120;
        let bx = self.cur_w - ui::MARGIN - bw;
        let track = Rect::new(bx, y + FOOTER_H / 2 - 4, bw, 8);
        c.fill_rounded_rect(track, 4.0, t.surface3);
        let fillw = (bw as f32 * (used_pct.min(100) as f32 / 100.0)) as i32;
        if fillw > 0 {
            let fr = Rect::new(bx, track.y, fillw, track.h);
            c.fill_rounded_rect(fr, 4.0, if used_pct > 90 { t.danger } else { t.accent });
        }
    }

    fn render_action_bar(&self, c: &mut Canvas, t: &Theme, hits: &mut Vec<(Rect, Hit)>) {
        let w = self.cur_w;
        let y = self.cur_h - ui::BOTTOM_SAFE - FOOTER_H - ACTBAR_H - 8;
        let r = Rect::new(ui::MARGIN, y, w - ui::MARGIN * 2, ACTBAR_H);
        c.fill_rounded_rect(r, 20.0, t.surface1);
        // Count + Done on the left.
        let n = self.selection.len();
        ui::draw_text(
            c,
            &format!("{n} selected"),
            &ui::styled(18.0, 600, t.text),
            r.x + 20,
            r.y + 14,
            Some(160),
        );
        let single = n == 1;
        // Fixed order left→right after the count.
        let mut ax = r.x + 150;
        for (act, label, enabled) in [
            (SelAct::Open, "Open", single),
            (SelAct::Rename, "Rename", single),
            (SelAct::Copy, "Copy", true),
            (SelAct::Move, "Move", true),
            (SelAct::Delete, "Delete", true),
            (SelAct::Info, "Info", single),
            (SelAct::Done, "Done", true),
        ] {
            let bw = 68;
            let br = Rect::new(ax, r.y + 14, bw, ACTBAR_H - 28);
            c.fill_rounded_rect(br, 12.0, if enabled { t.surface2 } else { t.surface1 });
            let col = if enabled { t.text } else { t.text3 };
            ui::draw_centered(c, label, &ui::styled(16.0, 600, col), br);
            if enabled {
                hit_rect(hits, br, Hit::Sel(act));
            }
            ax += bw + 8;
        }
    }

    fn render_menu(&mut self, c: &mut Canvas, t: &Theme, hits: &mut Vec<(Rect, Hit)>) {
        // Panel under the sort button (top-right area).
        let w = 280;
        let x = self.cur_w - ui::MARGIN - 48 - 3 * 52 - w;
        let y = ui::TOP_SAFE + 8 + PATH_BAR_H + 8;
        let rows = SortKey::ALL.len() + 2; // + hidden toggle + spacer
        let h = rows as i32 * 48 + 16;
        let r = Rect::new(x, y, w, h);
        self.menu_rect = r;
        c.fill_rounded_rect(r, 20.0, t.surface1);
        let mut ry = r.y + 8;
        for k in SortKey::ALL {
            let row = Rect::new(r.x + 8, ry, r.w - 16, 44);
            ui::draw_text(
                c,
                k.label(),
                &ui::styled(19.0, 450, t.text),
                row.x + 16,
                row.y + 11,
                Some(row.w - 60),
            );
            if self.key == k {
                draw_icon(c, "\u{e5ca}", 20.0, t.accent, row.right() - 26, row.y + 22);
            }
            hit_rect(hits, row, Hit::MenuSort(k));
            ry += 48;
        }
        // separator + hidden toggle
        c.fill_rect(Rect::new(r.x + 16, ry + 4, r.w - 32, 1), t.line);
        ry += 8;
        let row = Rect::new(r.x + 8, ry, r.w - 16, 44);
        ui::draw_text(
            c,
            "Show hidden files",
            &ui::styled(19.0, 450, t.text),
            row.x + 16,
            row.y + 11,
            Some(row.w - 80),
        );
        ui::draw_switch(c, t, row.right() - 64, row.y + 6, self.show_hidden);
        hit_rect(hits, row, Hit::MenuHidden);
    }

    fn render_dialog(&self, c: &mut Canvas, t: &Theme, hits: &mut Vec<(Rect, Hit)>, d: &Dialog) {
        let w = self.cur_w;
        let h = self.cur_h;
        c.fill_rect(Rect::new(0, 0, w, h), t.scrim);
        let cw = (w - 64).min(560);
        let (ch, title) = dialog_metrics(d);
        let r = Rect::new((w - cw) / 2, (h - ch) / 2, cw, ch);
        c.fill_rounded_rect(r, 28.0, t.surface1);

        let mut y = r.y + 24;
        ui::draw_text(c, title, &ui::title(t.text), r.x + 28, y, Some(cw - 56));
        y += 52;
        match d {
            Dialog::NewFolder | Dialog::Rename(_) => {
                let fr = Rect::new(r.x + 24, y, cw - 48, 56);
                c.fill_rounded_rect(fr, 14.0, t.surface2);
                let shown = if self.text.s.is_empty() {
                    "Name".to_string()
                } else {
                    self.text.s.clone()
                };
                let col = if self.text.s.is_empty() {
                    t.text3
                } else {
                    t.text
                };
                ui::draw_text(
                    c,
                    &shown,
                    &ui::styled(22.0, 400, col),
                    fr.x + 18,
                    fr.y + 15,
                    Some(fr.w - 36),
                );
                hit_rect(hits, fr, Hit::DlgField);
            }
            Dialog::Delete { paths, items } => {
                let names: Vec<String> = paths
                    .iter()
                    .take(3)
                    .map(|p| {
                        p.file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_else(|| p.display().to_string())
                    })
                    .collect();
                let mut msg = if paths.len() == 1 {
                    format!("Delete “{}”?", names[0])
                } else {
                    format!("Delete {} items?", paths.len())
                };
                if *items > paths.len() {
                    msg.push_str(&format!(" ({items} files inside)"));
                }
                msg.push_str("\nThis can't be undone.");
                for line in msg.lines() {
                    ui::draw_text(c, line, &ui::body(t.text2), r.x + 28, y, Some(cw - 56));
                    y += 34;
                }
            }
            Dialog::Details(path) => {
                self.render_details_body(c, t, r, path, &mut y);
            }
        }
        // Buttons.
        let by = r.bottom() - 68;
        let bw = 140;
        let cancel = Rect::new(r.right() - 24 - bw * 2 - 12, by, bw, 48);
        let ok_label = match d {
            Dialog::Details(_) => "Close",
            Dialog::Delete { .. } => "Delete",
            _ => "OK",
        };
        let ok = Rect::new(r.right() - 24 - bw, by, bw, 48);
        ui::draw_pill_button(c, t, cancel, "Cancel", false, false);
        hit_rect(hits, cancel, Hit::DlgCancel);
        ui::draw_pill_button(c, t, ok, ok_label, !matches!(d, Dialog::Details(_)), false);
        hit_rect(hits, ok, Hit::DlgOk);
    }

    fn render_details_body(&self, c: &mut Canvas, t: &Theme, card: Rect, path: &Path, y: &mut i32) {
        let meta = std::fs::metadata(path).ok();
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        let is_dir = meta.as_ref().map(|m| m.is_dir()).unwrap_or(false);
        let link = std::fs::symlink_metadata(path)
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false);
        let rows: Vec<(String, String)> = {
            let mut v = vec![
                ("Name".into(), name.clone()),
                ("Path".into(), path.display().to_string()),
            ];
            if let Some(m) = &meta {
                use std::os::unix::fs::MetadataExt;
                v.push((
                    "Type".into(),
                    if is_dir {
                        "Folder".to_string()
                    } else {
                        model::kind_of(&name, false).label().to_string()
                    },
                ));
                if link {
                    if let Ok(target) = std::fs::read_link(path) {
                        v.push(("Link to".into(), target.display().to_string()));
                    }
                }
                if is_dir {
                    match model::dir_stats(path) {
                        Ok((n, bytes)) => {
                            let cnt = if n >= DETAIL_ITEM_CAP {
                                format!("{DETAIL_ITEM_CAP}+")
                            } else {
                                n.to_string()
                            };
                            v.push((
                                "Contents".into(),
                                format!("{cnt} items · {}", model::format_size(bytes)),
                            ));
                        }
                        Err(e) => v.push(("Contents".into(), format!("error: {e}"))),
                    }
                } else {
                    v.push(("Size".into(), model::format_size(m.len())));
                }
                v.push((
                    "Modified".into(),
                    model::format_time(
                        m.modified()
                            .ok()
                            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                            .map(|d| d.as_secs() as i64)
                            .unwrap_or(0),
                    ),
                ));
                v.push((
                    "Permissions".into(),
                    format!(
                        "{} ({:o})",
                        model::mode_string(m.mode() & 0o777),
                        m.mode() & 0o7777
                    ),
                ));
            } else {
                v.push(("Error".into(), "metadata unavailable".into()));
            }
            v
        };
        for (k, val) in rows {
            if *y > card.bottom() - 90 {
                break;
            }
            ui::draw_text(c, &k, &ui::caption(t.text3), card.x + 28, *y, Some(120));
            ui::draw_text(
                c,
                &val,
                &ui::body(t.text),
                card.x + 160,
                *y,
                Some(card.w - 200),
            );
            *y += 36;
        }
    }
}

/// (height, title) — dialog card metrics.
fn dialog_metrics(d: &Dialog) -> (i32, &'static str) {
    match d {
        Dialog::NewFolder => (300, "New folder"),
        Dialog::Rename(_) => (300, "Rename"),
        Dialog::Delete { .. } => (300, "Delete"),
        Dialog::Details(_) => (560, "Details"),
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

impl model::FileKind {
    fn label(self) -> &'static str {
        match self {
            FileKind::Folder => "Folder",
            FileKind::Image => "Image",
            FileKind::Audio => "Audio",
            FileKind::Video => "Video",
            FileKind::Archive => "Archive",
            FileKind::Code => "Code",
            FileKind::Document => "Document",
            FileKind::Generic => "File",
        }
    }
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
        FilesApp::render(self, c, t);
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
