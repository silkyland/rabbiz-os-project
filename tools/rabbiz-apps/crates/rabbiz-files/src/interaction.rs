//! Gesture, hit-testing, tap dispatch, and keyboard handling for `rabbiz-files`.

use de_api::Point;
use rabbiz_app_kit::{AppFrame, TextInputCtl};
use wayland_protocols::wp::text_input::zv3::client::zwp_text_input_v3::ContentPurpose;

use crate::gestures::{edge_commits, two_finger_dir, GestureEvent};
use crate::layout as lay;
use crate::{Dialog, FilesApp, Focus, Hit, Item, OpKind, SelAct, TextBuf};

/// Projected scroll distance for a flick release (px) — applied instantly
/// because the kit has no frame timer (see module docs).
const FLICK_PROJ_PX: f32 = 220.0;

impl FilesApp {
    // ---------------- gestures ----------------

    pub(crate) fn gesture(&mut self, ev: GestureEvent, ti: &TextInputCtl) {
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
    pub(crate) fn item_at(&self, pos: Point) -> Option<usize> {
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

    pub(crate) fn hit_at(&self, pos: Point) -> Option<&Hit> {
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

    pub(crate) fn key(&mut self, keysym: u32, utf8: Option<&str>, ti: &TextInputCtl) -> AppFrame {
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
