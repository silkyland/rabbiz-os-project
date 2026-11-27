mod input;
mod render;

use std::path::PathBuf;
use std::time::Instant;

use de_design::motion::{bezier_eval, BEZIER_ENTER, STAGGER_MAX, STAGGER_MS};
use de_paint::fonts::font_system;
use rabbiz_app_kit::text::shape::Shaped;
use rabbiz_app_kit::text::{LabelStyle, TextWrap};
use rabbiz_app_kit::widgets::{FabSize, SearchPill, TopAppBar};
use rabbiz_app_kit::{
    ui, AppEvent, AppFrame, AppState, Canvas, Point, Rect, ScrollView, Size, TextInputCtl, Theme,
};
use wayland_protocols::wp::text_input::zv3::client::zwp_text_input_v3::ContentPurpose;

use crate::model::{matching_indices, now_unix, Note, TextBuf};
use crate::store as notes_store;
use render::caret_geometry;

/// Keep-style card height: title + 2-line preview + edited caption.
pub(super) const ROW_H: i32 = 112;
pub(super) const ROW_GAP: i32 = 4;
pub(super) const SEARCH_H: i32 = SearchPill::HEIGHT;
pub(super) const FAB_D: i32 = FabSize::Standard.px();
/// Title field metrics (borderless Keep-style editor, first line of body).
pub(super) const TITLE_SIZE: f32 = 30.0;
pub(super) const TITLE_LH: f32 = 38.0;
/// Vertical gap between the title block and the body block (content px).
pub(super) const TITLE_BODY_GAP: i32 = 14;
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
    SearchClear,
    Row(u64), // note id
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Pending {
    None,
    OpenNote(u64),
    PlaceCursor,
    DefocusSearch,
}

/// Editor scroll drag (the list scrolls through `ScrollView` instead).
#[derive(Debug, Clone, Copy)]
pub(super) struct Drag {
    pub(super) id: i32,
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
    /// Editor chrome: back icon at left, delete at right (icon buttons).
    pub(super) bar: Rect,
    pub(super) back: Rect,
    pub(super) del: Rect,
    /// Editor scroll viewport (title + body content scrolls inside).
    pub(super) edit_vp: Rect,
    /// Text inset inside `edit_vp` — origin of the title/body content.
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
    /// Editor shaping split at the first newline: line 0 is the TITLE-size
    /// block, the rest is the body block — the Keep look over a single
    /// plain-text buffer (the title is still derived, never stored).
    pub(super) title_shaped: Option<Shaped>,
    pub(super) body_shaped: Option<Shaped>,
    pub(super) shaped_key: (u64, i32),
    pub(super) body_color: [u8; 4],
    pub(super) geom: Option<Geom>,
    /// List scroll physics (fling + rubber-band overscroll).
    pub(super) list_sv: ScrollView,
    /// Touch id that went down inside the list viewport.
    pub(super) list_touch: Option<i32>,
    /// Monotonic clock for ScrollView velocity timestamps.
    pub(super) t0: Instant,
    pub(super) edit_scroll: i32, // editor scroll, px
    pub(super) pressed: Option<Hit>,
    /// Editor scroll drag (list drags live in `list_sv`).
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
            title_shaped: None,
            body_shaped: None,
            shaped_key: (u64::MAX, -1),
            body_color: [0, 0, 0, 255],
            geom: None,
            list_sv: ScrollView::new(),
            list_touch: None,
            t0: Instant::now(),
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
            animating: self.intro_ms < INTRO_TOTAL_MS || self.list_sv.is_animating(),
            quit: false,
        }
    }

    pub(super) fn now_ms(&self) -> u64 {
        self.t0.elapsed().as_millis() as u64
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
        self.title_shaped = None;
        self.body_shaped = None;
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
        self.title_shaped = None;
        self.body_shaped = None;
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
        self.title_shaped = None;
        self.body_shaped = None;
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

    // -- shaped text (editor, split at the first newline) ---------------------

    pub(super) fn body_style(&self) -> LabelStyle {
        let mut st = ui::styled(BODY_SIZE, 400, self.body_color);
        st.line_height = BODY_LH;
        st
    }

    pub(super) fn title_style(&self) -> LabelStyle {
        let mut st = ui::styled(TITLE_SIZE, 600, self.body_color);
        st.line_height = TITLE_LH;
        st
    }

    /// Split `text` at the first newline: (title line, body rest).
    pub(super) fn title_body_split(text: &str) -> (&str, &str) {
        match text.find('\n') {
            Some(i) => (&text[..i], &text[i + 1..]),
            None => (text, ""),
        }
    }

    /// (Re)shape title + body when (text,width) changed. Called from
    /// render (has theme) and lazily from events via stored color.
    pub(super) fn ensure_shaped(&mut self) {
        let Some(g) = self.geom else { return };
        if g.text_area.w <= 0 {
            return;
        }
        if self.editing.is_none() {
            self.title_shaped = None;
            self.body_shaped = None;
            return;
        }
        let key = (self.edit_gen, g.text_area.w);
        if self.shaped_key == key {
            return;
        }
        let (title_txt, body_txt) = Self::title_body_split(self.edit.as_str());
        let (title_txt, body_txt) = (title_txt.to_string(), body_txt.to_string());
        let title_style = self.title_style();
        let body_style = self.body_style();
        let w = g.text_area.w as f32;
        if let Ok(mut fs) = font_system().lock() {
            self.title_shaped = Some(Shaped::shape(
                &mut fs,
                &title_txt,
                &title_style,
                Some(w),
                TextWrap::Glyph,
            ));
            self.body_shaped = Some(Shaped::shape(
                &mut fs,
                &body_txt,
                &body_style,
                Some(w),
                TextWrap::Glyph,
            ));
            self.shaped_key = key;
        }
    }

    /// Title block height in content px — at least one title line tall so
    /// the placeholder/empty-title caret sits inside the same band.
    pub(super) fn title_block_h(&self) -> i32 {
        self.title_shaped
            .as_ref()
            .map(|s| s.extent().1)
            .unwrap_or(TITLE_LH as i32)
            .max(TITLE_LH as i32)
    }

    /// Composed editor content height (title + gap + body) in content px.
    pub(super) fn content_height(&self) -> i32 {
        let body_h = self
            .body_shaped
            .as_ref()
            .map(|s| s.extent().1)
            .unwrap_or(0)
            .max(BODY_LH as i32);
        self.title_block_h() + TITLE_BODY_GAP + body_h
    }

    /// Caret position in *content* coords (before scroll): (x, top, height).
    /// Cursor on buffer line 0 → title block; line ≥1 → body block offset
    /// by the title block height + gap.
    pub(super) fn caret_pos(&self) -> Option<(f32, f32, f32)> {
        let (line, index) = self.edit.cursor_line_col();
        if line == 0 {
            let sh = self.title_shaped.as_ref()?;
            Some(
                caret_geometry(sh, 0, index).unwrap_or((0.0, 0.0, TITLE_LH)),
            )
        } else {
            let sh = self.body_shaped.as_ref()?;
            let dy = (self.title_block_h() + TITLE_BODY_GAP) as f32;
            caret_geometry(sh, line - 1, index).map(|(x, t, h)| (x, t + dy, h))
        }
    }

    // -- geometry ------------------------------------------------------------

    fn compute_geom(&self, w: i32, h: i32) -> Geom {
        let m = ui::MARGIN;
        let size = Size::new(w, h);
        // Editor top bar shares the TopAppBar slot (back + delete icons).
        let bar = TopAppBar::layout(size, ui::TOP_SAFE);
        let back = TopAppBar::back_rect(bar);
        let del = TopAppBar::action_rect(bar);

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

        // While editing, the OSK covers the bottom ~430px — end the
        // viewport above it so the caret never hides behind the keyboard.
        // (Reserve applies only while the editor owns text focus, i.e.
        // focus == Editor; the guess is documented at OSK_RESERVE.)
        let edit_bottom = if self.focus == Focus::Editor {
            h - OSK_RESERVE - 8
        } else {
            h - ui::BOTTOM_SAFE - 8
        };
        let edit_vp = Rect::new(
            0,
            bar.bottom() + 4,
            w,
            (edit_bottom - bar.bottom() - 4).max(120),
        );
        // Borderless Keep-style fields: text inset only by the margin —
        // no boxed card around title or body.
        let text_area = Rect::new(
            m + 8,
            edit_vp.y + 12,
            (w - (m + 8) * 2).max(40),
            (edit_vp.h - 24).max(80),
        );
        Geom {
            search,
            list_vp,
            fab,
            bar,
            back,
            del,
            edit_vp,
            text_area,
        }
    }

    pub(super) fn edit_max_scroll(&self) -> i32 {
        let Some(g) = self.geom else { return 0 };
        (self.content_height() - g.text_area.h).max(0)
    }

    /// Note id under `pos` for the current scroll, or None.
    pub(super) fn row_at(&self, pos: Point) -> Option<u64> {
        let g = self.geom?;
        if !g.list_vp.contains(pos) {
            return None;
        }
        let scroll = self.list_sv.offset_i32();
        let dy = pos.y - g.list_vp.y + scroll;
        if dy < 0 {
            return None;
        }
        let vis_i = dy / (ROW_H + ROW_GAP);
        let ids = matching_indices(&self.notes, &self.search.text);
        let &note_i = ids.get(vis_i as usize)?;
        // Undo the first-paint stagger so hit-testing matches what is drawn.
        let row_top = g.list_vp.y + vis_i * (ROW_H + ROW_GAP) - scroll
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Note;
    use std::path::Path;

    fn app_with_notes(notes: Vec<Note>) -> NotesApp {
        let dir = std::env::temp_dir().join(format!(
            "rabbiz-notes-shots-{}",
            std::process::id()
        ));
        let mut app = NotesApp::new();
        app.dir = dir;
        app.notes = notes;
        app
    }

    #[test]
    fn harness_renders_note_states() {
        let tag = std::env::var("SHOT_TAG").unwrap_or_else(|_| "current".into());
        let out = Path::new("/tmp/rabbiz-shots");
        let now = now_unix();
        let notes: Vec<Note> = (0..12)
            .map(|i| {
                Note::new(
                    100 + i as u64,
                    &format!(
                        "Note {i} title\nFirst preview line {i}\nSecond preview line {i} with more words"
                    ),
                    now - i as i64 * 3600,
                )
            })
            .collect();

        let mut h = rabbiz_app_kit::TestHarness::new(app_with_notes(notes));
        h.save_png(&out.join(format!("notes-{tag}-list.png"))).unwrap();

        // Editor: tap the first row.
        let _ = h.render();
        let g = h.state().geom.unwrap();
        h.tap(Point::new(g.list_vp.x + 40, g.list_vp.y + 40));
        h.save_png(&out.join(format!("notes-{tag}-editor.png"))).unwrap();
        h.tap(g.back.center());
        let _ = h.render();

        // Search with no match.
        h.tap(Point::new(g.search.x + 60, g.search.y + g.search.h / 2));
        h.type_text("zzzznope");
        h.save_png(&out.join(format!("notes-{tag}-nomatch.png"))).unwrap();

        let mut h_empty = rabbiz_app_kit::TestHarness::new(app_with_notes(Vec::new()));
        h_empty.save_png(&out.join(format!("notes-{tag}-empty.png"))).unwrap();
    }
}
