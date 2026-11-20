mod input;
mod render;

use std::path::PathBuf;

use de_design::motion::{bezier_eval, BEZIER_ENTER, STAGGER_MAX, STAGGER_MS};
use de_paint::fonts::font_system;
use rabbiz_app_kit::text::shape::Shaped;
use rabbiz_app_kit::text::{LabelStyle, TextWrap};
use rabbiz_app_kit::{ui, AppEvent, AppFrame, AppState, Canvas, Point, Rect, TextInputCtl, Theme};
use wayland_protocols::wp::text_input::zv3::client::zwp_text_input_v3::ContentPurpose;

use crate::model::{matching_indices, now_unix, Note, TextBuf};
use crate::store as notes_store;
use render::caret_geometry;

pub(super) const ROW_H: i32 = 96;
pub(super) const ROW_GAP: i32 = 12;
pub(super) const SEARCH_H: i32 = 56;
pub(super) const FAB_D: i32 = 56;
pub(super) const EDIT_PAD: i32 = 20;
pub(super) const BODY_SIZE: f32 = 24.0;
pub(super) const BODY_LH: f32 = 32.0;
/// Stagger intro budget: STAGGER_MAX items x STAGGER_MS + settle tail.
pub(super) const INTRO_TOTAL_MS: f32 = 8.0 * STAGGER_MS as f32 + 200.0;
pub(super) const DRAG_SLOP: i32 = 10;
/// Estimated on-screen-keyboard height while a text field is focused.
/// `rabbiz-osk` is a bottom layer-shell surface: 5 rows x 62px + 40px strip
/// + gaps + 32px nav bar ≈ 407px; apps receive no layer-shell inset, so the
///   editor reserves this much bottom space whenever it owns text focus.
///   (A shared-kit `keyboard_inset` event would replace this guess.)
pub(super) const OSK_RESERVE: i32 = 430;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Focus {
    List,
    Search,
    Editor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Hit {
    Back,
    Delete,
    New,
    Search,
    Row(u64), // note id
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Pending {
    None,
    OpenNote(u64),
    PlaceCursor,
    DefocusSearch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DragKind {
    List,
    Editor,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Drag {
    pub(super) id: i32,
    pub(super) kind: DragKind,
    pub(super) start_y: i32,
    pub(super) start_scroll: i32,
    pub(super) moved: bool,
}

/// Layout shared by render (draw) and events (hit-test). Recomputed every
/// render from the live canvas size — never hardcode 720x1600.
#[derive(Debug, Clone, Copy)]
pub(super) struct Geom {
    pub(super) search: Rect,
    pub(super) list_vp: Rect,
    pub(super) fab: Rect,
    pub(super) back: Rect,
    pub(super) del: Rect,
    pub(super) editor_card: Rect,
    pub(super) text_area: Rect,
}

pub struct NotesApp {
    pub(super) dir: PathBuf,
    pub(super) notes: Vec<Note>, // sorted updated-desc
    pub(super) focus: Focus,
    pub(super) search: TextBuf,
    pub(super) edit: TextBuf,
    pub(super) editing: Option<u64>, // note id — survives resorting
    pub(super) edit_gen: u64,
    pub(super) shaped: Option<Shaped>,
    pub(super) shaped_key: (u64, i32),
    pub(super) body_color: [u8; 4],
    pub(super) geom: Option<Geom>,
    pub(super) scroll: i32,      // list scroll, px
    pub(super) edit_scroll: i32, // editor scroll, px
    pub(super) pressed: Option<Hit>,
    pub(super) drag: Option<Drag>,
    pub(super) pending: Pending,
    pub(super) delete_armed: bool,
    pub(super) intro_ms: f32,
}

impl NotesApp {
    pub fn new() -> Self {
        let dir = notes_store::data_dir();
        notes_store::migrate_legacy(&dir);
        let notes = notes_store::load_all(&dir);
        Self {
            dir,
            notes,
            focus: Focus::List,
            search: TextBuf::new(),
            edit: TextBuf::new(),
            editing: None,
            edit_gen: 0,
            shaped: None,
            shaped_key: (u64::MAX, -1),
            body_color: [0, 0, 0, 255],
            geom: None,
            scroll: 0,
            edit_scroll: 0,
            pressed: None,
            drag: None,
            pending: Pending::None,
            delete_armed: false,
            intro_ms: 0.0,
        }
    }

    pub(super) fn frame(&self, dirty: bool) -> AppFrame {
        AppFrame {
            dirty,
            animating: self.intro_ms < INTRO_TOTAL_MS,
            quit: false,
        }
    }

    // -- persistence ---------------------------------------------------------

    /// Write the live edit buffer back into its note (title + updated +
    /// body), re-sort, and autosave. Called after every edit mutation.
    pub(super) fn sync_edit(&mut self) {
        let Some(id) = self.editing else { return };
        let now = now_unix();
        if let Some(n) = self.notes.iter_mut().find(|n| n.id == id) {
            n.body = self.edit.text.clone();
            n.touch(now);
            let snapshot = n.clone();
            notes_store::save_note(&self.dir, &snapshot);
        }
        self.notes
            .sort_by(|a, b| b.updated.cmp(&a.updated).then(a.id.cmp(&b.id)));
    }

    // -- navigation ----------------------------------------------------------

    pub(super) fn open_note(&mut self, id: u64, ti: &TextInputCtl) {
        let Some(n) = self.notes.iter().find(|n| n.id == id) else {
            return;
        };
        self.edit.set(&n.body);
        self.editing = Some(id);
        self.edit_gen += 1;
        self.edit_scroll = 0;
        self.shaped = None;
        self.focus = Focus::Editor;
        self.delete_armed = false;
        ti.activate(ContentPurpose::Normal);
        ti.set_surrounding(self.edit.as_str(), self.edit.cursor);
    }

    pub(super) fn close_editor(&mut self, ti: &TextInputCtl) {
        // Autosave already happened per keystroke; drop notes the user
        // emptied completely so blank files don't litter the list.
        if let Some(id) = self.editing.take() {
            let empty = self
                .notes
                .iter()
                .find(|n| n.id == id)
                .is_some_and(|n| n.body.trim().is_empty());
            if empty {
                if let Some(pos) = self.notes.iter().position(|n| n.id == id) {
                    self.notes.remove(pos);
                }
                notes_store::delete_note(&self.dir, id);
            }
        }
        self.edit.set("");
        self.shaped = None;
        self.focus = Focus::List;
        ti.deactivate();
    }

    pub(super) fn new_note(&mut self, ti: &TextInputCtl) {
        let id = notes_store::fresh_id(&self.dir, &self.notes);
        let n = Note::new(id, "", now_unix());
        notes_store::save_note(&self.dir, &n);
        self.notes.insert(0, n);
        self.open_note(id, ti);
    }

    pub(super) fn delete_editing(&mut self, ti: &TextInputCtl) {
        if let Some(id) = self.editing.take() {
            if let Some(pos) = self.notes.iter().position(|n| n.id == id) {
                self.notes.remove(pos);
            }
            notes_store::delete_note(&self.dir, id);
        }
        self.edit.set("");
        self.shaped = None;
        self.focus = Focus::List;
        self.delete_armed = false;
        ti.deactivate();
    }

    pub(super) fn focus_search(&mut self, ti: &TextInputCtl) {
        self.focus = Focus::Search;
        self.delete_armed = false;
        ti.activate(ContentPurpose::Normal);
        ti.set_surrounding(self.search.as_str(), self.search.cursor);
    }

    pub(super) fn defocus_search(&mut self, ti: &TextInputCtl) {
        if self.focus == Focus::Search {
            self.focus = Focus::List;
            ti.deactivate();
        }
    }

    // -- shaped text (editor body) -------------------------------------------

    pub(super) fn body_style(&self) -> LabelStyle {
        let mut st = ui::styled(BODY_SIZE, 400, self.body_color);
        st.line_height = BODY_LH;
        st
    }

    /// (Re)shape the editor body when (text,width) changed. Called from
    /// render (has theme) and lazily from events via stored color.
    pub(super) fn ensure_shaped(&mut self) {
        let Some(g) = self.geom else { return };
        if g.text_area.w <= 0 {
            return;
        }
        if self.editing.is_none() {
            self.shaped = None;
            return;
        }
        let key = (self.edit_gen, g.text_area.w);
        if self.shaped_key == key {
            return;
        }
        let style = self.body_style();
        if let Ok(mut fs) = font_system().lock() {
            self.shaped = Some(Shaped::shape(
                &mut fs,
                self.edit.as_str(),
                &style,
                Some(g.text_area.w as f32),
                TextWrap::Glyph,
            ));
            self.shaped_key = key;
        }
    }

    /// Caret position in *content* coords (before scroll): (x, top, height).
    pub(super) fn caret_pos(&self) -> Option<(f32, f32, f32)> {
        let sh = self.shaped.as_ref()?;
        let (line, index) = self.edit.cursor_line_col();
        caret_geometry(sh, line, index)
    }

    // -- geometry ------------------------------------------------------------

    fn compute_geom(&self, w: i32, h: i32) -> Geom {
        let m = ui::MARGIN;
        let header_bottom = ui::TOP_SAFE + 56;
        let search = Rect::new(m, header_bottom + 8, w - m * 2, SEARCH_H);
        let list_top = search.y + SEARCH_H + 12;
        let list_vp = Rect::new(
            m,
            list_top,
            w - m * 2,
            (h - ui::BOTTOM_SAFE - 12 - list_top).max(80),
        );
        let fab = Rect::new(
            w - m - FAB_D,
            h - ui::BOTTOM_SAFE - 16 - FAB_D,
            FAB_D,
            FAB_D,
        );
        let back = Rect::new(m, ui::TOP_SAFE + 12, 104, 48);
        let del = Rect::new(w - m - 116, ui::TOP_SAFE + 12, 116, 48);
        let card_top = ui::TOP_SAFE + 68;
        // While editing, the OSK covers the bottom ~430px — end the card
        // above it so the caret never hides behind the keyboard.
        let card_bottom = if self.focus == Focus::Editor {
            h - OSK_RESERVE - 12
        } else {
            h - ui::BOTTOM_SAFE - 16
        };
        let editor_card = Rect::new(m, card_top, w - m * 2, (card_bottom - card_top).max(200));
        let text_area = Rect::new(
            editor_card.x + EDIT_PAD,
            editor_card.y + EDIT_PAD,
            (editor_card.w - EDIT_PAD * 2).max(40),
            (editor_card.h - EDIT_PAD * 2).max(80),
        );
        Geom {
            search,
            list_vp,
            fab,
            back,
            del,
            editor_card,
            text_area,
        }
    }

    pub(super) fn list_max_scroll(&self) -> i32 {
        let Some(g) = self.geom else { return 0 };
        let n = matching_indices(&self.notes, &self.search.text).len() as i32;
        let content = n * (ROW_H + ROW_GAP) + 16;
        (content - g.list_vp.h).max(0)
    }

    pub(super) fn edit_max_scroll(&self) -> i32 {
        let Some(g) = self.geom else { return 0 };
        let total = self
            .shaped
            .as_ref()
            .map(|s| s.extent().1)
            .unwrap_or(g.text_area.h);
        (total - g.text_area.h).max(0)
    }

    /// Note id under `pos` for the current scroll, or None.
    pub(super) fn row_at(&self, pos: Point) -> Option<u64> {
        let g = self.geom?;
        if !g.list_vp.contains(pos) {
            return None;
        }
        let dy = pos.y - g.list_vp.y + self.scroll;
        if dy < 0 {
            return None;
        }
        let vis_i = dy / (ROW_H + ROW_GAP);
        let ids = matching_indices(&self.notes, &self.search.text);
        let &note_i = ids.get(vis_i as usize)?;
        // Undo the first-paint stagger so hit-testing matches what is drawn.
        let row_top = g.list_vp.y + vis_i * (ROW_H + ROW_GAP) - self.scroll
            + self.stagger_offset(vis_i as usize);
        if pos.y < row_top || pos.y >= row_top + ROW_H {
            return None;
        }
        Some(self.notes[note_i].id)
    }

    /// Vertical offset for the first-paint stagger (translate-only; rows
    /// never animate large surfaces, per the motion budget).
    pub(super) fn stagger_offset(&self, i: usize) -> i32 {
        if self.intro_ms >= INTRO_TOTAL_MS || i >= STAGGER_MAX {
            return 0;
        }
        let start = i as f32 * STAGGER_MS as f32;
        let t = ((self.intro_ms - start).max(0.0)) / 160.0;
        if t >= 1.0 {
            0
        } else {
            ((1.0 - bezier_eval(BEZIER_ENTER, t)) * 14.0).round() as i32
        }
    }
}

impl AppState for NotesApp {
    fn app_id(&self) -> &'static str {
        "rabbiz-notes"
    }
    fn title(&self) -> &'static str {
        "Notes"
    }

    fn handle_event(&mut self, event: AppEvent, ti: &TextInputCtl) -> AppFrame {
        match event {
            AppEvent::TouchDown { id, pos } => self.on_touch_down(id, pos, ti),
            AppEvent::TouchMove { id, pos } => self.on_touch_move(id, pos),
            AppEvent::TouchUp { id, pos } => self.on_touch_up(id, pos, ti),
            AppEvent::TextCommit(s) => {
                let dirty = self.text_commit(&s, ti);
                self.frame(dirty)
            }
            AppEvent::TextBackspace(n) => {
                let dirty = self.text_backspace(n, ti);
                self.frame(dirty)
            }
            AppEvent::Key { keysym, utf8 } => {
                let dirty = self.key(keysym, utf8.as_deref(), ti);
                self.frame(dirty)
            }
            AppEvent::Scroll { dy, .. } => self.on_scroll(dy),
            // Left-button presses already arrive as Touch*; hover and
            // non-primary buttons carry no actionable state here.
            AppEvent::PointerMove { .. } | AppEvent::PointerButton { .. } => self.frame(false),
            AppEvent::Tick(dt) => self.on_tick(dt),
        }
    }

    fn render(&mut self, canvas: &mut Canvas, theme: &Theme) {
        let size = canvas.size();
        let w = size.w;
        let h = size.h;
        self.body_color = theme.text;
        let g = self.compute_geom(w, h);
        self.geom = Some(g);
        self.ensure_shaped();

        match self.focus {
            Focus::Editor => self.render_editor(canvas, theme, g),
            _ => self.render_list(canvas, theme, g),
        }
    }
}
