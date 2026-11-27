//! rabbiz-contacts — native Contacts app for RabbizOS.
//!
//! Screens: contact list (search + favorites strip + FAB), contact detail
//! (call / message / edit / delete / favorite), and an add/edit form
//! (name, labelled phones, labelled emails, note). All state persists to the
//! shared store in `store.rs` (`~/.local/share/rabbiz-contacts/contacts.json`)
//! which the dialer and SMS app read — edits here are edits there.
//!
//! Launch integration: phone rows exec `rabbiz-dialer --dial <number>` and
//! message rows exec `rabbiz-sms --to <number>` (via `setsid`, same detach
//! pattern as de-apps launch). Both peers prefill/open the target — the
//! dialer never auto-calls and SMS never auto-sends.

mod headless;
mod model;
mod store;
mod ui;
mod views;

use std::path::PathBuf;
use std::time::Instant;

use rabbiz_app_kit::{AppEvent, AppFrame, AppState, Canvas, TextInputCtl, Theme};
use wayland_protocols::wp::text_input::zv3::client::zwp_text_input_v3::ContentPurpose;

use model::{Contact, TextBuf};

const DRAG_SLOP: i32 = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Screen {
    List,
    Detail,
    Form,
}

/// Which text buffer currently owns the IME.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Focus {
    None,
    Search,
    Name,
    PhoneLabel(usize),
    PhoneNum(usize),
    EmailLabel(usize),
    EmailAddr(usize),
    Note,
}

/// Everything the finger can land on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Hit {
    Search,
    SearchClear,
    FavCard(usize), // index into favorites list
    Row(usize),     // contact index in `contacts`
    DialRow(usize), // quick-dial button on a contact row
    Fab,
    EmptyAdd,
    Back,
    Edit,
    Delete,
    FavToggle,
    Call,
    Message,
    Dial(usize), // phone index within the open contact
    Sms(usize),
    Field(Focus),
    AddPhone,
    AddEmail,
    RemovePhone(usize),
    RemoveEmail(usize),
    Save,
    Cancel,
    List, // scroll viewport, no action on tap
    Detail,
    Form,
}

// ---- Form model: every field owns its TextBuf so per-field cursors and
// Thai-grapheme editing survive focus switches for free. ----

#[derive(Debug, Clone)]
pub(crate) struct PhoneRow {
    pub(crate) label: TextBuf,
    pub(crate) number: TextBuf,
}

#[derive(Debug, Clone)]
pub(crate) struct EmailRow {
    pub(crate) label: TextBuf,
    pub(crate) address: TextBuf,
}

#[derive(Debug, Clone)]
pub(crate) struct Form {
    pub(crate) name: TextBuf,
    pub(crate) phones: Vec<PhoneRow>,
    pub(crate) emails: Vec<EmailRow>,
    pub(crate) note: TextBuf,
}

impl Form {
    fn blank() -> Self {
        Self {
            name: TextBuf::new(),
            phones: vec![PhoneRow {
                label: TextBuf::new(),
                number: TextBuf::new(),
            }],
            emails: Vec::new(),
            note: TextBuf::new(),
        }
    }

    fn from_contact(c: &Contact) -> Self {
        Self {
            name: TextBuf::from_text(&c.name),
            phones: if c.phones.is_empty() {
                vec![PhoneRow {
                    label: TextBuf::new(),
                    number: TextBuf::new(),
                }]
            } else {
                c.phones
                    .iter()
                    .map(|p| PhoneRow {
                        label: TextBuf::from_text(&p.label),
                        number: TextBuf::from_text(&p.number),
                    })
                    .collect()
            },
            emails: c
                .emails
                .iter()
                .map(|e| EmailRow {
                    label: TextBuf::from_text(&e.label),
                    address: TextBuf::from_text(&e.address),
                })
                .collect(),
            note: TextBuf::from_text(&c.note),
        }
    }

    /// Snapshot into a `Contact` (caller sets id/timestamps/favorite).
    fn to_contact(&self, base: &Contact) -> Contact {
        let mut c = base.clone();
        c.name = self.name.text.trim().to_string();
        c.phones = self
            .phones
            .iter()
            .map(|p| model::Phone {
                label: p.label.text.trim().to_string(),
                number: p.number.text.trim().to_string(),
            })
            .collect();
        c.emails = self
            .emails
            .iter()
            .map(|e| model::Email {
                label: e.label.text.trim().to_string(),
                address: e.address.text.trim().to_string(),
            })
            .collect();
        c.note = self.note.text.trim().to_string();
        c.prune_empty();
        c
    }
}

#[derive(Debug, Clone, Copy)]
struct Drag {
    id: i32,
    start_y: i32,
    start_scroll: i32,
    moved: bool,
}

pub(crate) struct ContactsApp {
    dir: PathBuf,
    pub(crate) contacts: Vec<Contact>, // sorted: favorites first, then name
    pub(crate) screen: Screen,
    pub(crate) search: TextBuf,
    pub(crate) search_focus: bool,
    // Detail
    open_id: String,
    pub(crate) delete_armed: bool,
    // Form
    pub(crate) form: Form,
    pub(crate) editing_id: Option<String>,
    pub(crate) focus: Focus,
    pub(crate) form_error: bool,
    // Interaction
    pub(crate) pressed: Option<Hit>,
    drag: Option<Drag>,
    pub(crate) scroll: i32,
    pub(crate) vp: (i32, i32),
    pub(crate) intro_start: Instant,
}

impl ContactsApp {
    fn new() -> Self {
        let dir = store::data_dir();
        let contacts = store::load(&dir);
        Self {
            dir,
            contacts,
            screen: Screen::List,
            search: TextBuf::new(),
            search_focus: false,
            open_id: String::new(),
            delete_armed: false,
            form: Form::blank(),
            editing_id: None,
            focus: Focus::None,
            form_error: false,
            pressed: None,
            drag: None,
            scroll: 0,
            vp: (720, 1600),
            intro_start: Instant::now(),
        }
    }

    fn frame(&self, dirty: bool) -> AppFrame {
        AppFrame {
            dirty,
            // No continuous animation: press feedback is a static state and
            // the intro stagger is wall-clock (see stagger_offset). Ticks are
            // not required for correctness.
            animating: false,
            quit: false,
        }
    }

    // ---------------- focus / text input ----------------

    /// The TextBuf owned by the current focus (None when nothing focused).
    fn active_buf(&mut self) -> Option<&mut TextBuf> {
        match self.focus {
            Focus::None => None,
            Focus::Search => Some(&mut self.search),
            Focus::Name => Some(&mut self.form.name),
            Focus::PhoneLabel(i) => self.form.phones.get_mut(i).map(|p| &mut p.label),
            Focus::PhoneNum(i) => self.form.phones.get_mut(i).map(|p| &mut p.number),
            Focus::EmailLabel(i) => self.form.emails.get_mut(i).map(|e| &mut e.label),
            Focus::EmailAddr(i) => self.form.emails.get_mut(i).map(|e| &mut e.address),
            Focus::Note => Some(&mut self.form.note),
        }
    }

    fn set_focus(&mut self, f: Focus, ti: &TextInputCtl) {
        if self.focus == f {
            return;
        }
        self.focus = f;
        self.search_focus = f == Focus::Search;
        match f {
            Focus::None => ti.deactivate(),
            _ => {
                ti.activate(ContentPurpose::Normal);
                if let Some(b) = self.active_buf() {
                    ti.set_surrounding(b.as_str(), b.cursor);
                }
            }
        }
    }

    fn defocus(&mut self, ti: &TextInputCtl) {
        self.set_focus(Focus::None, ti);
    }

    /// Newlines are stripped for every field except the note.
    fn multiline(f: Focus) -> bool {
        f == Focus::Note
    }

    fn text_commit(&mut self, s: &str, ti: &TextInputCtl) -> bool {
        let multiline = Self::multiline(self.focus);
        let Some(buf) = self.active_buf() else {
            return false;
        };
        let s = if multiline {
            s.to_string()
        } else {
            s.replace('\n', " ")
        };
        buf.insert_str(&s);
        ti.set_surrounding(buf.as_str(), buf.cursor);
        true
    }

    fn text_backspace(&mut self, n: usize, ti: &TextInputCtl) -> bool {
        let Some(buf) = self.active_buf() else {
            return false;
        };
        if buf.delete_before(n.max(1)) {
            ti.set_surrounding(buf.as_str(), buf.cursor);
            return true;
        }
        false
    }

    fn key(&mut self, keysym: u32, utf8: Option<&str>, ti: &TextInputCtl) -> bool {
        if self.focus == Focus::None {
            return false;
        }
        let multiline = Self::multiline(self.focus);
        let mut defocus_now = false;
        let changed = {
            let Some(buf) = self.active_buf() else {
                return false;
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
                        defocus_now = true;
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
        if defocus_now {
            self.defocus(ti);
            return true;
        }
        if changed {
            if let Some(b) = self.active_buf() {
                ti.set_surrounding(b.as_str(), b.cursor);
            }
        }
        true
    }

    // ---------------- navigation / actions ----------------

    fn open_detail(&mut self, id: &str, ti: &TextInputCtl) {
        self.defocus(ti);
        self.open_id = id.to_string();
        self.screen = Screen::Detail;
        self.scroll = 0;
        self.delete_armed = false;
    }

    fn back_to_list(&mut self, ti: &TextInputCtl) {
        self.defocus(ti);
        self.screen = Screen::List;
        self.scroll = 0;
        self.delete_armed = false;
    }

    fn start_add(&mut self, ti: &TextInputCtl) {
        self.defocus(ti);
        self.form = Form::blank();
        self.editing_id = None;
        self.screen = Screen::Form;
        self.scroll = 0;
        self.form_error = false;
        self.focus = Focus::None;
    }

    fn start_edit(&mut self, ti: &TextInputCtl) {
        self.defocus(ti);
        let Some(c) = self.open_contact().cloned() else {
            return;
        };
        self.form = Form::from_contact(&c);
        self.editing_id = Some(c.id.clone());
        self.screen = Screen::Form;
        self.scroll = 0;
        self.form_error = false;
        self.focus = Focus::None;
    }

    fn cancel_form(&mut self, ti: &TextInputCtl) {
        // Discard: the store is only written on Save. Editing returns to the
        // contact's detail page; a new contact returns to the list.
        self.defocus(ti);
        if let Some(id) = self.editing_id.take() {
            self.open_id = id;
            self.screen = Screen::Detail;
        } else {
            self.screen = Screen::List;
        }
        self.scroll = 0;
        self.delete_armed = false;
    }

    fn save_form(&mut self, ti: &TextInputCtl) {
        // Build from the live buffers; editing keeps id/created/favorite.
        let base = match &self.editing_id {
            Some(id) => self
                .contacts
                .iter()
                .find(|c| &c.id == id)
                .cloned()
                .unwrap_or_else(|| Contact::new(model::fresh_id(), "", model::now_unix())),
            None => Contact::new(model::fresh_id(), "", model::now_unix()),
        };
        let mut c = self.form.to_contact(&base);
        c.updated = model::now_unix();
        if !c.is_meaningful() {
            self.form_error = true;
            return;
        }
        c.prune_empty();
        match self.editing_id.clone() {
            Some(id) => {
                if let Some(slot) = self.contacts.iter_mut().find(|x| x.id == id) {
                    *slot = c.clone();
                }
                self.open_id = id;
            }
            None => {
                self.contacts.push(c.clone());
                self.open_id = c.id.clone();
            }
        }
        model::sort_contacts(&mut self.contacts);
        store::save(&self.dir, &self.contacts);
        self.screen = Screen::Detail;
        self.scroll = 0;
        self.delete_armed = false;
        self.form_error = false;
        self.focus = Focus::None;
        let _ = ti;
    }

    fn toggle_favorite(&mut self) {
        let Some(c) = self.contacts.iter_mut().find(|c| c.id == self.open_id) else {
            return;
        };
        c.favorite = !c.favorite;
        // Favorite is a list-ordering flag, not content — don't bump `updated`.
        model::sort_contacts(&mut self.contacts);
        store::save(&self.dir, &self.contacts);
    }

    fn delete_open(&mut self, ti: &TextInputCtl) {
        let id = self.open_id.clone();
        self.contacts.retain(|c| c.id != id);
        store::save(&self.dir, &self.contacts);
        self.back_to_list(ti);
    }

    pub(crate) fn open_contact(&self) -> Option<&Contact> {
        self.contacts.iter().find(|c| c.id == self.open_id)
    }

    pub(crate) fn favorites(&self) -> Vec<usize> {
        self.contacts
            .iter()
            .enumerate()
            .filter(|(_, c)| c.favorite)
            .map(|(i, _)| i)
            .collect()
    }

    pub(crate) fn filtered(&self) -> Vec<usize> {
        model::matching_indices(&self.contacts, &self.search.text)
    }

    /// Launch a peer RabbizOS app detached (setsid, like de-apps launch)
    /// with the number hand-off args both peers accept:
    /// `rabbiz-dialer --dial <number>` prefills the keypad (no auto-call),
    /// `rabbiz-sms --to <number>` opens the thread or a prefilled compose.
    fn peer_argv(prog: &str, flag: &str, number: &str) -> (String, Vec<String>) {
        (
            prog.to_string(),
            vec![flag.to_string(), number.trim().to_string()],
        )
    }

    fn launch_peer_argv(prog: &str, args: &[String]) {
        use std::process::Stdio;
        let _ = std::process::Command::new("setsid")
            .arg(prog)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
    }

    fn launch_peer(prog: &str, flag: &str, number: &str) {
        let (prog, args) = Self::peer_argv(prog, flag, number);
        Self::launch_peer_argv(&prog, &args);
    }

    fn dial(&self, phone_idx: usize) {
        if let Some(c) = self.open_contact() {
            if let Some(p) = c.phones.get(phone_idx) {
                if !p.number.trim().is_empty() {
                    Self::launch_peer("rabbiz-dialer", "--dial", &p.number);
                }
            }
        }
    }

    fn sms(&self, phone_idx: usize) {
        if let Some(c) = self.open_contact() {
            if let Some(p) = c.phones.get(phone_idx) {
                if !p.number.trim().is_empty() {
                    Self::launch_peer("rabbiz-sms", "--to", &p.number);
                }
            }
        }
    }

    fn dispatch(&mut self, hit: Hit, ti: &TextInputCtl) {
        match hit {
            // ---- list ----
            Hit::Search => self.set_focus(Focus::Search, ti),
            Hit::SearchClear => {
                self.search.set("");
                self.scroll = 0;
                if let Focus::Search = self.focus {
                    ti.set_surrounding("", 0);
                }
            }
            Hit::FavCard(i) => {
                let favs = self.favorites();
                if let Some(&ci) = favs.get(i) {
                    let id = self.contacts[ci].id.clone();
                    self.open_detail(&id, ti);
                }
            }
            Hit::Row(ci) => {
                let id = self.contacts[ci].id.clone();
                self.open_detail(&id, ti);
            }
            Hit::DialRow(ci) => {
                // Quick-dial straight from the list: launch the dialer
                // prefilled with the primary number (no auto-call).
                if let Some(p) = self.contacts[ci].primary_phone() {
                    Self::launch_peer("rabbiz-dialer", "--dial", &p.number);
                }
            }
            Hit::Fab | Hit::EmptyAdd => self.start_add(ti),
            // ---- detail ----
            Hit::Back => self.back_to_list(ti),
            Hit::Edit => self.start_edit(ti),
            Hit::Delete => {
                if self.delete_armed {
                    self.delete_open(ti);
                } else {
                    self.delete_armed = true;
                }
            }
            Hit::FavToggle => self.toggle_favorite(),
            Hit::Call => self.dial(0),
            Hit::Message => self.sms(0),
            Hit::Dial(i) => self.dial(i),
            Hit::Sms(i) => self.sms(i),
            Hit::Detail => {}
            // ---- form ----
            Hit::Field(f) => self.set_focus(f, ti),
            Hit::AddPhone => {
                self.form.phones.push(PhoneRow {
                    label: TextBuf::new(),
                    number: TextBuf::new(),
                });
                self.scroll = self.max_scroll(self.vp.0, self.vp.1);
            }
            Hit::AddEmail => {
                self.form.emails.push(EmailRow {
                    label: TextBuf::new(),
                    address: TextBuf::new(),
                });
                self.scroll = self.max_scroll(self.vp.0, self.vp.1);
            }
            Hit::RemovePhone(i) => {
                if self.form.phones.len() > 1 {
                    self.form.phones.remove(i);
                } else {
                    // Keep at least one row; just clear it.
                    if let Some(p) = self.form.phones.get_mut(0) {
                        p.label.set("");
                        p.number.set("");
                    }
                }
                if let Focus::PhoneLabel(j) | Focus::PhoneNum(j) = self.focus {
                    if j >= self.form.phones.len() {
                        self.defocus(ti);
                    }
                }
            }
            Hit::RemoveEmail(i) => {
                self.form.emails.remove(i);
                if let Focus::EmailLabel(j) | Focus::EmailAddr(j) = self.focus {
                    if j >= self.form.emails.len() {
                        self.defocus(ti);
                    }
                }
            }
            Hit::Save => self.save_form(ti),
            Hit::Cancel => self.cancel_form(ti),
            Hit::List | Hit::Form => {}
        }
    }
}

// ===========================================================================
// AppState
// ===========================================================================

impl AppState for ContactsApp {
    fn app_id(&self) -> &'static str {
        "rabbiz-contacts"
    }
    fn title(&self) -> &'static str {
        "Contacts"
    }

    fn handle_event(&mut self, event: AppEvent, ti: &TextInputCtl) -> AppFrame {
        let (w, h) = self.vp;
        self.scroll = self.scroll.clamp(0, self.max_scroll(w, h));
        let mut dirty = false;

        match event {
            AppEvent::TouchDown { id, pos } => {
                self.delete_armed = false;
                self.pressed = self.hit_at(pos);
                let vp = self.viewport(w, h);
                if vp.contains(pos) {
                    self.drag = Some(Drag {
                        id,
                        start_y: pos.y,
                        start_scroll: self.scroll,
                        moved: false,
                    });
                }
                dirty = true;
            }
            AppEvent::TouchMove { id, pos } => {
                if let Some(d) = self.drag {
                    if d.id == id {
                        let delta = d.start_y - pos.y;
                        if !d.moved && delta.abs() > DRAG_SLOP {
                            if let Some(d) = self.drag.as_mut() {
                                d.moved = true;
                            }
                            self.pressed = None;
                        }
                        if self.drag.as_ref().is_some_and(|d| d.moved) {
                            self.scroll = (d.start_scroll + delta).clamp(0, self.max_scroll(w, h));
                            dirty = true;
                        }
                    }
                }
            }
            AppEvent::TouchUp { id, pos } => {
                let moved = self.drag.take().is_some_and(|d| d.id == id && d.moved);
                let hit = self.pressed.take();
                if !moved {
                    if let Some(hit) = hit {
                        if self.hit_at(pos) == Some(hit) {
                            self.dispatch(hit, ti);
                        }
                    } else if self.focus != Focus::None {
                        // Tap on blank area defocuses the field.
                        self.defocus(ti);
                    }
                }
                dirty = true;
            }
            AppEvent::Key { keysym, utf8 } => {
                if keysym == 0xFF1B {
                    // Escape: leave the form or defocus.
                    match self.screen {
                        Screen::Form => self.cancel_form(ti),
                        _ => self.defocus(ti),
                    }
                } else {
                    self.key(keysym, utf8.as_deref(), ti);
                }
                dirty = true;
            }
            AppEvent::TextCommit(s) => {
                dirty = self.text_commit(&s, ti);
            }
            AppEvent::TextBackspace(n) => {
                dirty = self.text_backspace(n, ti);
            }
            AppEvent::PointerMove { .. } | AppEvent::PointerButton { .. } => {
                // Hover/non-primary buttons: no action (touch-first app).
            }
            AppEvent::Scroll { dy, .. } => {
                // Wheel/trackpad: positive dy scrolls content down per the
                // kit contract → decrease the scroll offset.
                self.scroll = (self.scroll - dy.round() as i32).clamp(0, self.max_scroll(w, h));
                dirty = true;
            }
            AppEvent::Tick(_) => {
                // Wall-clock intro needs no tick; keep dirty=false so the
                // loop doesn't spin. (animating is never set.)
            }
        }

        self.frame(dirty)
    }

    fn render(&mut self, canvas: &mut Canvas, theme: &Theme) {
        let size = canvas.size();
        let (w, h) = (size.w, size.h);
        self.vp = (w, h);
        self.scroll = self.scroll.clamp(0, self.max_scroll(w, h));

        match self.screen {
            Screen::List => self.render_list(canvas, theme, w, h),
            Screen::Detail => self.render_detail(canvas, theme, w, h),
            Screen::Form => self.render_form(canvas, theme, w, h),
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // Headless modes run before the Wayland app: `--cli <cmd>` for the text
    // CLI, `--mcp` for the stdio MCP server. Both share store.rs.
    if args.iter().any(|a| a == "--mcp") {
        headless::mcp_serve();
    }
    if let Some(i) = args.iter().position(|a| a == "--cli") {
        std::process::exit(headless::run_cli(&args[i + 1..]));
    }
    rabbiz_app_kit::run_app(ContactsApp::new());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peer_argv_builds_hand_off_args() {
        let (prog, args) = ContactsApp::peer_argv("rabbiz-dialer", "--dial", " 081-234-5678 ");
        assert_eq!(prog, "rabbiz-dialer");
        assert_eq!(args, vec!["--dial".to_string(), "081-234-5678".to_string()]);

        let (prog, args) = ContactsApp::peer_argv("rabbiz-sms", "--to", "+66 81 234 5678");
        assert_eq!(prog, "rabbiz-sms");
        assert_eq!(
            args,
            vec!["--to".to_string(), "+66 81 234 5678".to_string()]
        );
    }

    /// Drives the app through list/detail/form and saves PNGs (the
    /// before/after evidence contract). `XDG_DATA_HOME` is pointed at a
    /// fresh temp dir so the deterministic demo seed loads and nothing in
    /// the real store is touched; no other test in this binary reads the
    /// env var, so mutating it here is safe.
    #[test]
    fn renders_main_screens_to_png() {
        let dir =
            std::env::temp_dir().join(format!("rabbiz-contacts-shots-{}", std::process::id()));
        std::env::set_var("XDG_DATA_HOME", &dir);
        let shots = std::path::PathBuf::from("/tmp/rabbiz-shots");
        let _ = std::fs::create_dir_all(&shots);

        let mut h = rabbiz_app_kit::TestHarness::new(ContactsApp::new());
        assert_eq!(h.state().contacts.len(), 8, "expected demo seed");
        // Skip the wall-clock intro stagger for deterministic shots.
        h.state_mut().intro_start = Instant::now() - std::time::Duration::from_secs(10);

        h.save_png(&shots.join("contacts-after-list.png"))
            .expect("save list shot");

        let id = h.state().contacts[0].id.clone();
        h.state_mut().open_id = id;
        h.state_mut().screen = Screen::Detail;
        h.save_png(&shots.join("contacts-after-detail.png"))
            .expect("save detail shot");

        let ct = h.state().contacts[0].clone();
        h.state_mut().form = Form::from_contact(&ct);
        h.state_mut().editing_id = Some(ct.id.clone());
        h.state_mut().screen = Screen::Form;
        h.save_png(&shots.join("contacts-after-form.png"))
            .expect("save form shot");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
