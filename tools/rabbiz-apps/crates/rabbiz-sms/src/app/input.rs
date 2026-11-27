use rabbiz_app_kit::{AppEvent, AppFrame, TextInputCtl};
use wayland_protocols::wp::text_input::zv3::client::zwp_text_input_v3::ContentPurpose;

use crate::model::delete_before;

use super::{Focus, Hit, SmsApp, View};

impl SmsApp {
    pub(super) fn on_event(&mut self, event: AppEvent, ti: &TextInputCtl) -> AppFrame {
        let mut frame = AppFrame::default();

        if let AppEvent::Tick(_) = event {
            if let Some(t) = &self.transition {
                if t.done() {
                    if t.exit {
                        self.view = View::List;
                        self.peer_key.clear();
                        self.peer_label.clear();
                        self.is_new = false;
                        self.body_draft.clear();
                        self.to_draft.clear();
                    }
                    self.transition = None;
                }
            }
            frame.dirty = true;
            frame.animating = self.transition.is_some();
            return frame;
        }

        let animating = self.transition.is_some();

        match event {
            AppEvent::TouchDown { pos, .. } => {
                frame.dirty = true;
                if animating {
                    return frame;
                }
                self.pressed = None;
                self.drag_last_y = None;
                self.drag_origin_y = None;
                match self.view {
                    View::List => {
                        if self.new_msg_rect.contains(pos) {
                            self.pressed = Some(Hit::NewMsg);
                        } else {
                            if self.list_area.contains(pos) {
                                self.drag_origin_y = Some(pos.y);
                            }
                            self.pressed = self
                                .row_rects
                                .iter()
                                .find_map(|(i, r)| r.contains(pos).then_some(Hit::Row(*i)));
                        }
                        if self.pressed.is_none() {
                            ti.deactivate();
                            self.focus = Focus::None;
                        }
                    }
                    View::Thread => {
                        if self.back_rect.contains(pos) {
                            self.pressed = Some(Hit::Back);
                        } else if self.send_rect.contains(pos) {
                            self.pressed = Some(Hit::Send);
                        } else if self.is_new && self.to_rect.contains(pos) {
                            self.pressed = Some(Hit::ToField);
                            self.focus = Focus::To;
                            ti.activate(ContentPurpose::Phone);
                            ti.set_surrounding(&self.to_draft, self.to_draft.len());
                        } else if self.body_rect.contains(pos) {
                            self.pressed = Some(Hit::BodyField);
                            self.focus = Focus::Body;
                            ti.activate(ContentPurpose::Normal);
                            ti.set_surrounding(&self.body_draft, self.body_draft.len());
                        } else if self.msg_area.contains(pos) {
                            // Message area: begin scroll drag, drop keyboard.
                            self.drag_last_y = Some(pos.y);
                            ti.deactivate();
                            self.focus = Focus::None;
                        } else {
                            ti.deactivate();
                            self.focus = Focus::None;
                        }
                    }
                }
            }
            AppEvent::TouchMove { pos, .. } => {
                if self.view == View::List && !animating {
                    if let Some(origin) = self.drag_origin_y {
                        if self.drag_last_y.is_some() || (origin - pos.y).abs() >= 8 {
                            let last = self.drag_last_y.replace(pos.y).unwrap_or(origin);
                            self.list_scroll = (self.list_scroll + last - pos.y).max(0);
                            self.pressed = None;
                            frame.dirty = true;
                        }
                    }
                } else if self.drag_last_y.is_some() {
                    frame.dirty = true;
                    let last = self.drag_last_y.replace(pos.y).unwrap_or(pos.y);
                    // Pull down from the newest messages to reveal older ones.
                    self.scroll += pos.y - last;
                }
            }
            AppEvent::TouchUp { pos, .. } => {
                frame.dirty = true;
                match self.pressed.take() {
                    Some(Hit::NewMsg) if self.new_msg_rect.contains(pos) => {
                        self.open_thread(String::new(), String::new(), true);
                        ti.activate(ContentPurpose::Phone);
                        ti.set_surrounding("", 0);
                    }
                    Some(Hit::Row(i))
                        if self
                            .row_rects
                            .iter()
                            .any(|(index, rect)| *index == i && rect.contains(pos)) =>
                    {
                        let threads = self.threads();
                        if let Some((key, peer, _)) = threads.get(i) {
                            let (k, p) = (key.clone(), peer.clone());
                            self.open_thread(k, p, false);
                        }
                    }
                    Some(Hit::Back) if self.back_rect.contains(pos) => self.close_thread(ti),
                    Some(Hit::Send) if self.send_rect.contains(pos) => self.send(ti),
                    _ => {}
                }
                self.drag_last_y = None;
                self.drag_origin_y = None;
            }
            AppEvent::Scroll { dy, .. } if !animating => {
                if self.view == View::List {
                    self.list_scroll = (self.list_scroll + dy.round() as i32).max(0);
                } else {
                    self.scroll -= dy.round() as i32;
                }
                self.pressed = None;
                frame.dirty = true;
            }
            AppEvent::TextCommit(s) => {
                self.append_to_focus(&s);
                self.sync_surrounding(ti);
                frame.dirty = true;
            }
            AppEvent::TextBackspace(bytes) => {
                match self.focus {
                    Focus::To => delete_before(&mut self.to_draft, bytes),
                    Focus::Body => delete_before(&mut self.body_draft, bytes),
                    Focus::None => {}
                }
                self.sync_surrounding(ti);
                frame.dirty = true;
            }
            AppEvent::Key { keysym, utf8 } => {
                frame.dirty = true;
                match (keysym, utf8) {
                    (0xff1b, _) => {
                        if self.view == View::Thread {
                            self.close_thread(ti);
                        }
                    }
                    (0xff0d, _) => {
                        if self.view == View::Thread && !self.body_draft.trim().is_empty() {
                            self.send(ti);
                        }
                    }
                    (0xff08, _) => {
                        match self.focus {
                            Focus::To => {
                                self.to_draft.pop();
                            }
                            Focus::Body => {
                                self.body_draft.pop();
                            }
                            Focus::None => {}
                        }
                        self.sync_surrounding(ti);
                    }
                    (_, Some(s)) => {
                        self.append_to_focus(&s);
                        self.sync_surrounding(ti);
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        frame.animating = self.transition.is_some();
        frame
    }
}
