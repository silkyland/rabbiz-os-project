use rabbiz_app_kit::{AppFrame, Point, TextInputCtl};

use super::{Drag, DragKind, Focus, Hit, NotesApp, Pending, DRAG_SLOP, INTRO_TOTAL_MS};

impl NotesApp {
    // -- text input ----------------------------------------------------------

    /// Insert committed/typed text into whichever field has focus.
    pub(super) fn text_commit(&mut self, s: &str, ti: &TextInputCtl) -> bool {
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

    pub(super) fn text_backspace(&mut self, n: usize, ti: &TextInputCtl) -> bool {
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

    pub(super) fn key(&mut self, keysym: u32, utf8: Option<&str>, ti: &TextInputCtl) -> bool {
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

    // -- events ---------------------------------------------------------------

    pub(super) fn on_touch_down(&mut self, id: i32, pos: Point, ti: &TextInputCtl) -> AppFrame {
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

    pub(super) fn on_touch_move(&mut self, id: i32, pos: Point) -> AppFrame {
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

    pub(super) fn on_touch_up(&mut self, id: i32, pos: Point, ti: &TextInputCtl) -> AppFrame {
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

    pub(super) fn on_tick(&mut self, dt: std::time::Duration) -> AppFrame {
        self.intro_ms += dt.as_secs_f32() * 1000.0;
        self.frame(self.intro_ms < INTRO_TOTAL_MS)
    }

    /// Mouse wheel/trackpad scroll: wheel whichever surface has focus.
    /// `dy` follows the wl_pointer axis sign (positive = scroll down →
    /// larger offset); flip if on-device feel disagrees.
    pub(super) fn on_scroll(&mut self, dy: f32) -> AppFrame {
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
