//! RabbizOS Notes — Google Keep-style list + editor per docs/BASE-APPS.md.
//!
//! List: title + preview + edited time, drag-scroll, search field, FAB.
//! Editor: tap-to-position cursor (cosmic-text hit test), grapheme-safe
//! editing, autosave to `~/.local/share/rabbiz-notes/<id>.json` on every
//! mutation (atomic tmp+rename in `store.rs`).
//!
//! Rendering notes: the body is shaped ONCE per (text,width) into a
//! `de_paint::text::shape::Shaped` (cosmic-text, Glyph wrap so Thai breaks
//! anywhere), rasterized per dirty frame, and blitted through a clip rect —
//! `Canvas::blit` has no scissor, so `blit_clipped` emulates one via
//! `blit_region`. The same trick clips list rows to the scroll viewport.

mod cli;
mod model;
mod store;

use std::path::PathBuf;

use de_api::Pixels;
use de_design::motion::{bezier_eval, BEZIER_ENTER, STAGGER_MAX, STAGGER_MS};
use de_paint::fonts::font_system;
use rabbiz_app_kit::text::shape::Shaped;
use rabbiz_app_kit::text::{self, LabelStyle, TextWrap};
use rabbiz_app_kit::{ui, AppEvent, AppFrame, AppState, Canvas, Point, Rect, TextInputCtl, Theme};
use wayland_protocols::wp::text_input::zv3::client::zwp_text_input_v3::ContentPurpose;

use model::{format_edited, matching_indices, now_unix, Note, TextBuf};
use store as notes_store;

const ROW_H: i32 = 96;
const ROW_GAP: i32 = 12;
const SEARCH_H: i32 = 56;
const FAB_D: i32 = 56;
const EDIT_PAD: i32 = 20;
const BODY_SIZE: f32 = 24.0;
const BODY_LH: f32 = 32.0;
/// Stagger intro budget: STAGGER_MAX items x STAGGER_MS + settle tail.
const INTRO_TOTAL_MS: f32 = 8.0 * STAGGER_MS as f32 + 200.0;
const DRAG_SLOP: i32 = 10;
/// Estimated on-screen-keyboard height while a text field is focused.
/// `rabbiz-osk` is a bottom layer-shell surface: 5 rows x 62px + 40px strip
/// + gaps + 32px nav bar ≈ 407px; apps receive no layer-shell inset, so the
///   editor reserves this much bottom space whenever it owns text focus.
///   (A shared-kit `keyboard_inset` event would replace this guess.)
const OSK_RESERVE: i32 = 430;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Focus {
    List,
    Search,
    Editor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Hit {
    Back,
    Delete,
    New,
    Search,
    Row(u64), // note id
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pending {
    None,
    OpenNote(u64),
    PlaceCursor,
    DefocusSearch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DragKind {
    List,
    Editor,
}

#[derive(Debug, Clone, Copy)]
struct Drag {
    id: i32,
    kind: DragKind,
    start_y: i32,
    start_scroll: i32,
    moved: bool,
}

/// Layout shared by render (draw) and events (hit-test). Recomputed every
/// render from the live canvas size — never hardcode 720x1600.
#[derive(Debug, Clone, Copy)]
struct Geom {
    search: Rect,
    list_vp: Rect,
    fab: Rect,
    back: Rect,
    del: Rect,
    editor_card: Rect,
    text_area: Rect,
}

struct NotesApp {
    dir: PathBuf,
    notes: Vec<Note>, // sorted updated-desc
    focus: Focus,
    search: TextBuf,
    edit: TextBuf,
    editing: Option<u64>, // note id — survives resorting
    edit_gen: u64,
    shaped: Option<Shaped>,
    shaped_key: (u64, i32),
    body_color: [u8; 4],
    geom: Option<Geom>,
    scroll: i32,      // list scroll, px
    edit_scroll: i32, // editor scroll, px
    pressed: Option<Hit>,
    drag: Option<Drag>,
    pending: Pending,
    delete_armed: bool,
    intro_ms: f32,
}

impl NotesApp {
    fn new() -> Self {
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

    fn frame(&self, dirty: bool) -> AppFrame {
        AppFrame {
            dirty,
            animating: self.intro_ms < INTRO_TOTAL_MS,
            quit: false,
        }
    }

    // -- persistence ---------------------------------------------------------

    /// Write the live edit buffer back into its note (title + updated +
    /// body), re-sort, and autosave. Called after every edit mutation.
    fn sync_edit(&mut self) {
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

    fn open_note(&mut self, id: u64, ti: &TextInputCtl) {
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

    fn close_editor(&mut self, ti: &TextInputCtl) {
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

    fn new_note(&mut self, ti: &TextInputCtl) {
        let id = notes_store::fresh_id(&self.dir, &self.notes);
        let n = Note::new(id, "", now_unix());
        notes_store::save_note(&self.dir, &n);
        self.notes.insert(0, n);
        self.open_note(id, ti);
    }

    fn delete_editing(&mut self, ti: &TextInputCtl) {
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

    fn focus_search(&mut self, ti: &TextInputCtl) {
        self.focus = Focus::Search;
        self.delete_armed = false;
        ti.activate(ContentPurpose::Normal);
        ti.set_surrounding(self.search.as_str(), self.search.cursor);
    }

    fn defocus_search(&mut self, ti: &TextInputCtl) {
        if self.focus == Focus::Search {
            self.focus = Focus::List;
            ti.deactivate();
        }
    }

    // -- text input ----------------------------------------------------------

    /// Insert committed/typed text into whichever field has focus.
    fn text_commit(&mut self, s: &str, ti: &TextInputCtl) -> bool {
        match self.focus {
            Focus::Editor => {
                self.edit.insert_str(s);
                self.edit_gen += 1;
                self.sync_edit();
                ti.set_surrounding(self.edit.as_str(), self.edit.cursor);
                true
            }
            Focus::Search => {
                // Search is single-line: newlines collapse to spaces.
                let s = s.replace('\n', " ");
                self.search.insert_str(&s);
                ti.set_surrounding(self.search.as_str(), self.search.cursor);
                true
            }
            Focus::List => false,
        }
    }

    fn text_backspace(&mut self, n: usize, ti: &TextInputCtl) -> bool {
        match self.focus {
            Focus::Editor => {
                if self.edit.delete_before(n.max(1)) {
                    self.edit_gen += 1;
                    self.sync_edit();
                    ti.set_surrounding(self.edit.as_str(), self.edit.cursor);
                    return true;
                }
                false
            }
            Focus::Search => {
                if self.search.delete_before(n.max(1)) {
                    ti.set_surrounding(self.search.as_str(), self.search.cursor);
                    return true;
                }
                false
            }
            Focus::List => false,
        }
    }

    fn key(&mut self, keysym: u32, utf8: Option<&str>, ti: &TextInputCtl) -> bool {
        match self.focus {
            Focus::Editor => self.edit_key(keysym, utf8, true, ti),
            Focus::Search => self.edit_key(keysym, utf8, false, ti),
            Focus::List => false,
        }
    }

    /// Shared editing keys; `multiline` distinguishes editor from search.
    /// Returns true when the key was consumed (caller repaints).
    fn edit_key(
        &mut self,
        keysym: u32,
        utf8: Option<&str>,
        multiline: bool,
        ti: &TextInputCtl,
    ) -> bool {
        let changed = {
            let buf = if multiline {
                &mut self.edit
            } else {
                &mut self.search
            };
            let mut changed = false;
            match keysym {
                0xFF08 => changed = buf.delete_grapheme_back(), // Backspace
                0xFF0D => {
                    // Return/Enter
                    if multiline {
                        buf.insert_str("\n");
                        changed = true;
                    } else {
                        self.focus = Focus::List;
                        ti.deactivate();
                        return true;
                    }
                }
                0xFFFF => changed = buf.delete_grapheme_fwd(), // Delete
                0xFF51 => buf.left(),                          // Left
                0xFF53 => buf.right(),                         // Right
                0xFF50 => buf.line_home(),                     // Home
                0xFF57 => buf.line_end(),                      // End
                _ => match utf8 {
                    Some(s) if s.chars().all(|c| !c.is_control() || c == '\t' || c == '\n') => {
                        if multiline {
                            buf.insert_str(s);
                        } else {
                            buf.insert_str(&s.replace('\n', " "));
                        }
                        changed = true;
                    }
                    _ => return false,
                },
            }
            changed
        };
        if multiline && changed {
            self.edit_gen += 1;
            self.sync_edit();
        }
        let (t, c) = if multiline {
            (self.edit.as_str(), self.edit.cursor)
        } else {
            (self.search.as_str(), self.search.cursor)
        };
        ti.set_surrounding(t, c);
        true
    }

    // -- shaped text (editor body) -------------------------------------------

    fn body_style(&self) -> LabelStyle {
        let mut st = ui::styled(BODY_SIZE, 400, self.body_color);
        st.line_height = BODY_LH;
        st
    }

    /// (Re)shape the editor body when (text,width) changed. Called from
    /// render (has theme) and lazily from events via stored color.
    fn ensure_shaped(&mut self) {
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
    fn caret_pos(&self) -> Option<(f32, f32, f32)> {
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

    fn list_max_scroll(&self) -> i32 {
        let Some(g) = self.geom else { return 0 };
        let n = matching_indices(&self.notes, &self.search.text).len() as i32;
        let content = n * (ROW_H + ROW_GAP) + 16;
        (content - g.list_vp.h).max(0)
    }

    fn edit_max_scroll(&self) -> i32 {
        let Some(g) = self.geom else { return 0 };
        let total = self
            .shaped
            .as_ref()
            .map(|s| s.extent().1)
            .unwrap_or(g.text_area.h);
        (total - g.text_area.h).max(0)
    }

    /// Note id under `pos` for the current scroll, or None.
    fn row_at(&self, pos: Point) -> Option<u64> {
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

    // -- events ---------------------------------------------------------------

    fn on_touch_down(&mut self, id: i32, pos: Point, ti: &TextInputCtl) -> AppFrame {
        self.delete_armed = false;
        self.pressed = None;
        let Some(g) = self.geom else {
            return self.frame(true);
        };
        match self.focus {
            Focus::Editor => {
                if g.back.contains(pos) {
                    self.pressed = Some(Hit::Back);
                } else if g.del.contains(pos) {
                    self.pressed = Some(Hit::Delete);
                } else if g.editor_card.contains(pos) {
                    self.drag = Some(Drag {
                        id,
                        kind: DragKind::Editor,
                        start_y: pos.y,
                        start_scroll: self.edit_scroll,
                        moved: false,
                    });
                    self.pending = Pending::PlaceCursor;
                }
            }
            Focus::List | Focus::Search => {
                // FAB is hidden while the search OSK is up — don't hit-test it.
                if self.focus != Focus::Search && g.fab.contains(pos) {
                    self.pressed = Some(Hit::New);
                } else if g.search.contains(pos) {
                    self.pressed = Some(Hit::Search);
                    self.focus_search(ti);
                } else if g.list_vp.contains(pos) {
                    if let Some(note_id) = self.row_at(pos) {
                        self.pressed = Some(Hit::Row(note_id));
                        self.pending = Pending::OpenNote(note_id);
                    } else {
                        // Blank list area: tap dismisses the search keyboard.
                        self.pending = Pending::DefocusSearch;
                    }
                    self.drag = Some(Drag {
                        id,
                        kind: DragKind::List,
                        start_y: pos.y,
                        start_scroll: self.scroll,
                        moved: false,
                    });
                }
            }
        }
        self.frame(true)
    }

    fn on_touch_move(&mut self, id: i32, pos: Point) -> AppFrame {
        let Some(d) = self.drag else {
            return self.frame(false);
        };
        if d.id != id {
            return self.frame(false);
        }
        let delta = d.start_y - pos.y;
        if !d.moved && delta.abs() > DRAG_SLOP {
            if let Some(d) = self.drag.as_mut() {
                d.moved = true;
            }
            self.pressed = None;
            self.pending = Pending::None;
        }
        if let Some(d) = self.drag {
            let max = match d.kind {
                DragKind::List => self.list_max_scroll(),
                DragKind::Editor => self.edit_max_scroll(),
            };
            let next = (d.start_scroll + delta).clamp(0, max);
            match d.kind {
                DragKind::List => self.scroll = next,
                DragKind::Editor => self.edit_scroll = next,
            }
        }
        self.frame(true)
    }

    fn on_touch_up(&mut self, id: i32, pos: Point, ti: &TextInputCtl) -> AppFrame {
        if let Some(d) = self.drag {
            if d.id == id && !d.moved {
                match self.pending {
                    Pending::OpenNote(note_id) => self.open_note(note_id, ti),
                    Pending::PlaceCursor => self.place_cursor(pos, ti),
                    Pending::DefocusSearch => self.defocus_search(ti),
                    Pending::None => {}
                }
            }
        }
        self.drag = None;
        self.pending = Pending::None;
        match self.pressed.take() {
            Some(Hit::Back) => self.close_editor(ti),
            Some(Hit::Delete) => {
                if self.delete_armed {
                    self.delete_editing(ti);
                } else {
                    self.delete_armed = true;
                }
            }
            Some(Hit::New) => self.new_note(ti),
            _ => {}
        }
        self.frame(true)
    }

    /// Tap-to-position the editor cursor through cosmic-text hit testing.
    fn place_cursor(&mut self, pos: Point, ti: &TextInputCtl) {
        let Some(g) = self.geom else { return };
        if !g.editor_card.contains(pos) {
            return;
        }
        self.ensure_shaped();
        let Some(sh) = &self.shaped else { return };
        let cx = pos.x as f32 - g.text_area.x as f32;
        let cy = pos.y as f32 - g.text_area.y as f32 + self.edit_scroll as f32;
        if let Some(cur) = sh.buf.hit(cx, cy.max(0.0)) {
            self.edit.set_cursor_line_col(cur.line, cur.index);
            ti.set_surrounding(self.edit.as_str(), self.edit.cursor);
        }
    }

    fn on_tick(&mut self, dt: std::time::Duration) -> AppFrame {
        self.intro_ms += dt.as_secs_f32() * 1000.0;
        self.frame(self.intro_ms < INTRO_TOTAL_MS)
    }

    /// Mouse wheel/trackpad scroll: wheel whichever surface has focus.
    /// `dy` follows the wl_pointer axis sign (positive = scroll down →
    /// larger offset); flip if on-device feel disagrees.
    fn on_scroll(&mut self, dy: f32) -> AppFrame {
        let delta = dy.round() as i32;
        if delta == 0 {
            return self.frame(false);
        }
        match self.focus {
            Focus::Editor => {
                let max = self.edit_max_scroll();
                let next = (self.edit_scroll + delta).clamp(0, max);
                if next != self.edit_scroll {
                    self.edit_scroll = next;
                    return self.frame(true);
                }
            }
            Focus::List | Focus::Search => {
                let max = self.list_max_scroll();
                let next = (self.scroll + delta).clamp(0, max);
                if next != self.scroll {
                    self.scroll = next;
                    return self.frame(true);
                }
            }
        }
        self.frame(false)
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

// ---------------------------------------------------------------------------
// Drawing helpers
// ---------------------------------------------------------------------------

/// Blit `px` at (dx,dy) clipped to `clip` — Canvas has no scissor, so we
/// intersect and use `blit_region` for the visible slice.
fn blit_clipped(canvas: &mut Canvas, px: &Pixels, dx: i32, dy: i32, clip: Rect) {
    let dst = Rect::new(dx, dy, px.size.w, px.size.h);
    let Some(r) = dst.intersect(clip) else { return };
    let region = Rect::new(r.x - dx, r.y - dy, r.w, r.h);
    canvas.blit_region(px, region, Point::new(r.x, r.y));
}

/// Single-line clamped label, right-aligned at `right_x`, vertically centred
/// on `cy`, clipped to `clip`.
fn draw_text_right(
    canvas: &mut Canvas,
    s: &str,
    style: &text::LabelStyle,
    right_x: i32,
    cy: i32,
    max_w: i32,
    clip: Rect,
) {
    let Some(px) = text::label_clamped(s, *style, max_w.max(16), 1) else {
        return;
    };
    let pixels = text::cache::to_pixels(&px);
    let x = right_x - pixels.size.w;
    let y = cy - pixels.size.h / 2;
    blit_clipped(canvas, &pixels, x, y, clip);
}

/// Caret rectangle clipped to `clip`.
fn draw_caret(canvas: &mut Canvas, x: i32, y: i32, h: i32, color: [u8; 4], clip: Rect) {
    if let Some(r) = Rect::new(x, y, 3, h).intersect(clip) {
        canvas.fill_rect(r, color);
    }
}

/// Caret geometry for `line`/`index` (cosmic-text byte index inside the
/// line) → (x, line_top, line_height) in content coords.
fn caret_geometry(sh: &Shaped, line: usize, index: usize) -> Option<(f32, f32, f32)> {
    let mut empty_fallback: Option<(f32, f32, f32)> = None;
    let mut tail: Option<(f32, f32, f32)> = None;
    for run in sh.buf.layout_runs() {
        if run.line_i != line {
            continue;
        }
        let (top, lh) = (run.line_top, run.line_height);
        let end_x = run.glyphs.last().map(|g| g.x + g.w).unwrap_or(0.0);
        // Fallback for `index` beyond every glyph (trailing whitespace or an
        // empty line): caret after the last glyph of the last visual line.
        tail = Some((end_x, top, lh));
        let Some(first) = run.glyphs.first() else {
            if empty_fallback.is_none() {
                empty_fallback = Some((0.0, top, lh));
            }
            continue;
        };
        let last = run.glyphs.last().unwrap();
        // The first visual line whose last glyph ends at/after `index` owns
        // the caret; glyph byte ranges are contiguous across wraps.
        if index > last.end {
            continue;
        }
        let x = if index <= first.start {
            first.x
        } else {
            let mut x = end_x;
            for g in run.glyphs {
                if index <= g.start {
                    x = g.x;
                    break;
                }
                if index < g.end {
                    let span = (g.end - g.start).max(1) as f32;
                    x = g.x + g.w * ((index - g.start) as f32 / span);
                    break;
                }
                if index == g.end {
                    x = g.x + g.w;
                    break;
                }
            }
            x
        };
        return Some((x, top, lh));
    }
    empty_fallback.or(tail)
}

/// First-line preview for a list row: body minus its title line, collapsed
/// to one clamped line's worth of text.
fn preview_text(body: &str) -> String {
    let mut lines = body.lines();
    let _title = lines.next();
    let rest = lines.collect::<Vec<_>>().join(" ");
    let collapsed = rest.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() > 140 {
        let mut s: String = collapsed.chars().take(140).collect();
        s.push('…');
        s
    } else {
        collapsed
    }
}

// ---------------------------------------------------------------------------
// List screen
// ---------------------------------------------------------------------------

impl NotesApp {
    fn render_list(&mut self, canvas: &mut Canvas, theme: &Theme, g: Geom) {
        let w = canvas.size().w;
        ui::draw_header(canvas, theme, "Notes", "");

        // -- search field ----------------------------------------------------
        canvas.fill_rounded_rect(g.search, SEARCH_H as f32 / 2.0, theme.surface1);
        // Material "search" icon glyph.
        if let Some(px) = text::icon("\u{e8b6}", 22.0, theme.text3) {
            let p = text::cache::to_pixels(&px);
            let iy = g.search.y + (g.search.h - p.size.h) / 2;
            blit_clipped(canvas, &p, g.search.x + 16, iy, g.search);
        }
        let qstyle = ui::body(theme.text);
        let text_x = g.search.x + 52;
        let text_w = (g.search.w - 52 - 16).max(16);
        let clip = g.search;
        let cy = g.search.y + g.search.h / 2;
        if self.search.text.is_empty() {
            ui::draw_text(
                canvas,
                "Search notes",
                &ui::body(theme.text3),
                text_x,
                cy - 14,
                Some(text_w),
            );
        } else {
            if let Some(px) = text::label_clamped(&self.search.text, qstyle, text_w, 1) {
                let p = text::cache::to_pixels(&px);
                blit_clipped(canvas, &p, text_x, cy - p.size.h / 2, clip);
                if self.focus == Focus::Search {
                    draw_caret(
                        canvas,
                        text_x + p.size.w + 2,
                        cy - 12,
                        24,
                        theme.accent,
                        clip,
                    );
                }
            }
        }
        if self.focus == Focus::Search && self.search.text.is_empty() {
            draw_caret(canvas, text_x, cy - 12, 24, theme.accent, clip);
        }

        // -- note rows ---------------------------------------------------------
        let ids = matching_indices(&self.notes, &self.search.text);
        let vp = g.list_vp;
        let content_h = ids.len() as i32 * (ROW_H + ROW_GAP) + 16;
        let max_scroll = (content_h - vp.h).max(0);
        self.scroll = self.scroll.clamp(0, max_scroll);

        if self.notes.is_empty() {
            let r = Rect::new(0, vp.y + vp.h / 2 - 60, w, 140);
            ui::draw_centered(canvas, "No notes yet", &ui::title(theme.text2), r);
            ui::draw_centered(
                canvas,
                "Tap + to write your first note",
                &ui::body(theme.text3),
                Rect::new(0, r.y + 52, w, 40),
            );
        } else if ids.is_empty() {
            ui::draw_centered(
                canvas,
                &format!("No notes match \"{}\"", self.search.text.trim()),
                &ui::body(theme.text3),
                Rect::new(0, vp.y + vp.h / 2 - 20, w, 40),
            );
        }

        let title_style = ui::label(theme.text);
        let prev_style = ui::caption(theme.text2);
        let time_style = ui::caption(theme.text3);
        let now = now_unix();
        let row_h = ROW_H + ROW_GAP;

        for (vis_i, &note_i) in ids.iter().enumerate() {
            let note = &self.notes[note_i];
            let mut y = vp.y + vis_i as i32 * row_h - self.scroll;
            y += self.stagger_offset(vis_i);
            let row = Rect::new(vp.x, y, vp.w, ROW_H);
            let Some(vis) = row.intersect(vp) else {
                continue;
            };
            let pressed = self.pressed == Some(Hit::Row(note.id));
            ui::draw_card(canvas, theme, vis, pressed);

            let inner = Rect::new(row.x + 20, row.y, row.w - 40, row.h);
            let Some(tclip) = inner.intersect(vp) else {
                continue;
            };

            // Title (one line, ellipsized).
            if let Some(px) = text::label_clamped(&note.title, title_style, inner.w, 1) {
                let p = text::cache::to_pixels(&px);
                blit_clipped(canvas, &p, inner.x, inner.y + 14, tclip);
            }
            // Preview (one line).
            let prev = preview_text(&note.body);
            if !prev.is_empty() {
                if let Some(px) = text::label_clamped(&prev, prev_style, inner.w, 1) {
                    let p = text::cache::to_pixels(&px);
                    blit_clipped(canvas, &p, inner.x, inner.y + 48, tclip);
                }
            }
            // Edited time, right-aligned near the top.
            let when = format_edited(note.updated, now);
            draw_text_right(
                canvas,
                &when,
                &time_style,
                inner.x + inner.w,
                inner.y + 26,
                150,
                tclip,
            );
        }

        // -- FAB ---------------------------------------------------------------
        // Hidden while the search OSK is up: it would sit under the keyboard.
        if self.focus != Focus::Search {
            let fab = g.fab;
            let fab_bg = if self.pressed == Some(Hit::New) {
                theme.accent_dim
            } else {
                theme.accent
            };
            canvas.fill_rounded_rect(fab, (fab.h / 2) as f32, fab_bg);
            let plus = ui::styled(30.0, 600, theme.text_on_accent);
            ui::draw_centered(canvas, "+", &plus, fab);
        }
    }

    /// Vertical offset for the first-paint stagger (translate-only; rows
    /// never animate large surfaces, per the motion budget).
    fn stagger_offset(&self, i: usize) -> i32 {
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

// ---------------------------------------------------------------------------
// Editor screen
// ---------------------------------------------------------------------------

impl NotesApp {
    fn render_editor(&mut self, canvas: &mut Canvas, theme: &Theme, g: Geom) {
        // Top bar.
        ui::draw_pill_button(
            canvas,
            theme,
            g.back,
            "‹ Notes",
            false,
            self.pressed == Some(Hit::Back),
        );
        // Delete pill: danger-filled when armed for the second tap.
        let del_bg = if self.delete_armed {
            theme.danger
        } else {
            theme.surface2
        };
        let del_fg = if self.delete_armed {
            theme.text_on_accent
        } else {
            theme.text2
        };
        let dr = if self.pressed == Some(Hit::Delete) {
            Rect::new(g.del.x + 2, g.del.y + 2, g.del.w - 4, g.del.h - 4)
        } else {
            g.del
        };
        canvas.fill_rounded_rect(dr, (dr.h / 2) as f32, del_bg);
        ui::draw_centered(canvas, "Delete", &ui::label(del_fg), dr);

        // Card + clipped text area.
        ui::draw_card(canvas, theme, g.editor_card, false);
        let area = g.text_area;

        self.ensure_shaped();
        // Clamp scroll and keep the caret visible (auto-follow on every
        // render — cheap and correct after taps, inserts and resizes).
        let max_scroll = self.edit_max_scroll();
        if let Some((_cx, ctop, ch)) = self.caret_pos() {
            let caret_bottom = ctop + ch;
            if ctop - 12.0 < self.edit_scroll as f32 {
                self.edit_scroll = (ctop as i32 - 12).max(0);
            }
            if caret_bottom + 12.0 > self.edit_scroll as f32 + area.h as f32 {
                self.edit_scroll = (caret_bottom as i32 + 12 - area.h).clamp(0, max_scroll);
            }
        }
        self.edit_scroll = self.edit_scroll.clamp(0, max_scroll);

        if self.edit.text.is_empty() {
            ui::draw_text(
                canvas,
                "Start typing…",
                &ui::body(theme.text3),
                area.x,
                area.y + 4,
                Some(area.w),
            );
        } else if let Some(sh) = &self.shaped {
            let style = self.body_style();
            if let Ok(mut fs) = font_system().lock() {
                if let Some(px) = sh.rasterize(&mut fs, style.color) {
                    let p = text::cache::to_pixels(&px);
                    blit_clipped(canvas, &p, area.x, area.y - self.edit_scroll, area);
                }
            }
            // Caret.
            if let Some((cx, ctop, ch)) = self.caret_pos() {
                let sx = area.x + cx.round() as i32 - 1;
                let sy = area.y + ctop.round() as i32 - self.edit_scroll;
                let sh_h = (ch * 0.72).round() as i32;
                draw_caret(canvas, sx, sy + 4, sh_h.max(8), theme.accent, area);
            }
        } else {
            // Shaping unavailable — unwrapped fallback, still clipped.
            for (ln, line) in self.edit.text.lines().take(40).enumerate() {
                ui::draw_text(
                    canvas,
                    line,
                    &ui::body(theme.text),
                    area.x,
                    area.y + ln as i32 * 32 - self.edit_scroll,
                    Some(area.w),
                );
            }
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // Headless modes run before the Wayland app: `--cli <cmd>` for the text
    // CLI, `--mcp` for the stdio MCP server. Both share `store.rs`.
    if let Some(i) = args.iter().position(|a| a == "--cli") {
        std::process::exit(cli::run_cli(&args[i + 1..]));
    }
    if args.iter().any(|a| a == "--mcp") {
        rabbiz_app_kit::mcp::serve(cli::TOOLS, "rabbiz-notes", "0.1.0");
    }
    rabbiz_app_kit::run_app(NotesApp::new());
}
