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

use std::path::PathBuf;
use std::time::Instant;

use de_api::Pixels;
use de_design::motion::{bezier_eval, BEZIER_ENTER, STAGGER_MAX, STAGGER_MS};
use rabbiz_app_kit::text;
use rabbiz_app_kit::ui;
use rabbiz_app_kit::{AppEvent, AppFrame, AppState, Canvas, Point, Rect, TextInputCtl, Theme};
use wayland_protocols::wp::text_input::zv3::client::zwp_text_input_v3::ContentPurpose;

use model::{Contact, TextBuf};

// ---- Layout constants (px; all geometry derives from the live canvas size) ----
const SEARCH_H: i32 = 56;
const ROW_PITCH: i32 = 100;
const ROW_H: i32 = 88;
const FAV_CARD_H: i32 = 104;
const FAB_D: i32 = 56;
const FIELD_H: i32 = 76;
const PHONE_CARD_H: i32 = 128;
const EMAIL_CARD_H: i32 = 128;
const NOTE_H: i32 = 104;
const SECTION_GAP: i32 = 18;
const DRAG_SLOP: i32 = 10;
/// First-paint stagger budget (wall-clock; no Tick needed — see
/// `stagger_offset`). Matches notes' intro feel.
const INTRO_TOTAL_MS: f32 = STAGGER_MAX as f32 * STAGGER_MS as f32 + 200.0;

// ---- Material Icons Round codepoints (verified against the bundled TTF) ----
const IC_PERSON: &str = "\u{e7fd}";
const IC_PERSON_ADD: &str = "\u{e7fe}";
const IC_SEARCH: &str = "\u{e8b6}";
const IC_CLOSE: &str = "\u{e5cd}";
const IC_STAR: &str = "\u{e838}";
const IC_STAR_BORDER: &str = "\u{e83a}";
const IC_DELETE: &str = "\u{e872}";
const IC_ADD: &str = "\u{e145}";
const IC_MAIL: &str = "\u{e158}";
const IC_SMS: &str = "\u{e625}";
const IC_CALL: &str = "\u{e0b0}";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Screen {
    List,
    Detail,
    Form,
}

/// Which text buffer currently owns the IME.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Focus {
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
enum Hit {
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
struct PhoneRow {
    label: TextBuf,
    number: TextBuf,
}

#[derive(Debug, Clone)]
struct EmailRow {
    label: TextBuf,
    address: TextBuf,
}

#[derive(Debug, Clone)]
struct Form {
    name: TextBuf,
    phones: Vec<PhoneRow>,
    emails: Vec<EmailRow>,
    note: TextBuf,
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

// ---- Layout structs (computed identically for render and hit-test) ----

#[derive(Debug, Clone, Copy)]
struct ListGeom {
    search: Rect,
    favs_label_y: i32, // y of the "FAVORITES" caption (0 when hidden)
    favs_top: i32,     // y of favorites cards (0 when hidden)
    section_y: i32,    // y of "ALL CONTACTS"/"RESULTS" caption
    vp: Rect,
    fab: Rect,
}

#[derive(Debug, Clone, Copy)]
struct DetailGeom {
    back: Rect,
    edit: Rect,
    del: Rect,
    fav: Rect,
    call: Rect,
    message: Rect,
    vp: Rect,
}

#[derive(Debug, Clone, Copy)]
struct FormGeom {
    back: Rect,
    vp: Rect,
}

#[derive(Debug, Clone, Copy)]
enum FormItem {
    Label(&'static str, i32),
    Name {
        card: Rect,
    },
    Phone {
        idx: usize,
        card: Rect,
        label: Rect,
        number: Rect,
        remove: Rect,
    },
    AddPhone {
        rect: Rect,
    },
    Email {
        idx: usize,
        card: Rect,
        label: Rect,
        address: Rect,
        remove: Rect,
    },
    AddEmail {
        rect: Rect,
    },
    Note {
        card: Rect,
    },
}

#[derive(Debug, Clone, Copy)]
enum DetailItem {
    /// Contact header block (avatar + name) at content-y.
    Header {
        y: i32,
    },
    Label(&'static str, i32), // section caption at content-y
    Phone {
        idx: usize,
        y: i32,
        h: i32,
        call: Rect,
        sms: Rect,
    },
    Email {
        idx: usize,
        y: i32,
        h: i32,
    },
    NoteCard {
        y: i32,
        h: i32,
    },
}

#[derive(Debug, Clone, Copy)]
struct Drag {
    id: i32,
    start_y: i32,
    start_scroll: i32,
    moved: bool,
}

struct ContactsApp {
    dir: PathBuf,
    contacts: Vec<Contact>, // sorted: favorites first, then name
    screen: Screen,
    search: TextBuf,
    search_focus: bool,
    // Detail
    open_id: String,
    delete_armed: bool,
    // Form
    form: Form,
    editing_id: Option<String>,
    focus: Focus,
    form_error: bool,
    // Interaction
    pressed: Option<Hit>,
    drag: Option<Drag>,
    scroll: i32,
    vp: (i32, i32),
    intro_start: Instant,
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

    fn open_contact(&self) -> Option<&Contact> {
        self.contacts.iter().find(|c| c.id == self.open_id)
    }

    fn favorites(&self) -> Vec<usize> {
        self.contacts
            .iter()
            .enumerate()
            .filter(|(_, c)| c.favorite)
            .map(|(i, _)| i)
            .collect()
    }

    fn filtered(&self) -> Vec<usize> {
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

    // ---------------- geometry ----------------

    fn list_geom(&self, w: i32, h: i32) -> ListGeom {
        let m = ui::MARGIN;
        let header_bottom = ui::TOP_SAFE + 16 + 80; // title + optional subtitle
        let search = Rect::new(m, header_bottom + 8, w - m * 2, SEARCH_H);
        let show_favs = self.search.text.trim().is_empty() && !self.favorites().is_empty();
        // Cursor walk: search → [FAVORITES label + cards] → section label → vp.
        let mut y = search.bottom() + 12;
        let mut favs_label_y = 0;
        let mut favs_top = 0;
        if show_favs {
            favs_label_y = y;
            y += 24;
            favs_top = y;
            y += FAV_CARD_H + 14;
        }
        let section_y = y;
        y += 26; // caption + gap before the rows
        let vp = Rect::new(m, y, w - m * 2, (h - ui::BOTTOM_SAFE - 16 - y).max(120));
        let fab = Rect::new(
            w - m - FAB_D,
            h - ui::BOTTOM_SAFE - 16 - FAB_D,
            FAB_D,
            FAB_D,
        );
        ListGeom {
            search,
            favs_label_y,
            favs_top,
            section_y,
            vp,
            fab,
        }
    }

    fn detail_geom(&self, w: i32, h: i32) -> DetailGeom {
        let m = ui::MARGIN;
        let y = ui::TOP_SAFE + 12;
        let back = Rect::new(m, y, 120, 48);
        let del = Rect::new(w - m - 104, y, 104, 48);
        let edit = Rect::new(del.x - 92 - 8, y, 92, 48);
        let fav = Rect::new(edit.x - 52 - 8, y, 52, 48);
        let vp_top = y + 48 + 12;
        let vp = Rect::new(m, vp_top, w - m * 2, h - ui::BOTTOM_SAFE - 16 - vp_top);
        DetailGeom {
            back,
            edit,
            del,
            fav,
            call: call_bar(w, h),
            message: message_bar(w, h),
            vp,
        }
    }

    fn form_geom(&self, w: i32, h: i32) -> FormGeom {
        let m = ui::MARGIN;
        let y = ui::TOP_SAFE + 12;
        let back = Rect::new(m, y, 110, 48);
        let vp_top = y + 48 + 12;
        // Leave room for the fixed Save bar at the bottom.
        let vp = Rect::new(m, vp_top, w - m * 2, h - ui::BOTTOM_SAFE - 16 - 84 - vp_top);
        FormGeom { back, vp }
    }

    /// Contact index under `pos` in the list viewport, honoring scroll and
    /// the first-paint stagger offset.
    fn list_row_at(&self, g: &ListGeom, pos: Point) -> Option<usize> {
        if !g.vp.contains(pos) {
            return None;
        }
        let ids = self.filtered();
        let dy = pos.y - g.vp.y + self.scroll;
        if dy < 0 {
            return None;
        }
        let vis = dy / ROW_PITCH;
        let staggered_top =
            g.vp.y + vis * ROW_PITCH - self.scroll + self.stagger_offset(vis as usize);
        if pos.y < staggered_top || pos.y >= staggered_top + ROW_H {
            return None;
        }
        ids.get(vis as usize).copied()
    }

    // ---------------- detail items (shared by render + hit-test) -----------

    /// Ordered content items for the detail screen in *screen* coords
    /// (scroll applied). `content_h` is the un-scrolled height.
    fn detail_items(&self, w: i32) -> (Vec<DetailItem>, i32) {
        let m = ui::MARGIN;
        let mut items = Vec::new();
        let mut y = 0i32;
        let Some(c) = self.open_contact() else {
            return (items, 0);
        };

        // Header block: avatar + name (+ primary number caption).
        items.push(DetailItem::Header { y: 0 });
        y += HEADER_H + 8;

        // Phones.
        let phones: Vec<(usize, &model::Phone)> = c
            .phones
            .iter()
            .enumerate()
            .filter(|(_, p)| !p.number.trim().is_empty())
            .collect();
        if !phones.is_empty() {
            items.push(DetailItem::Label("PHONES", y));
            y += 26;
            for (idx, _) in &phones {
                let row_y = y;
                let call = Rect::new(w - m - 120, row_y + 12, 56, 56);
                let sms = Rect::new(w - m - 184, row_y + 12, 56, 56);
                items.push(DetailItem::Phone {
                    idx: *idx,
                    y: row_y,
                    h: PHONE_ROW_H,
                    call,
                    sms,
                });
                y += PHONE_ROW_H + 10;
            }
            y += SECTION_GAP;
        }

        // Emails.
        let emails: Vec<(usize, &model::Email)> = c
            .emails
            .iter()
            .enumerate()
            .filter(|(_, e)| !e.address.trim().is_empty())
            .collect();
        if !emails.is_empty() {
            items.push(DetailItem::Label("EMAILS", y));
            y += 26;
            for (idx, _) in &emails {
                items.push(DetailItem::Email {
                    idx: *idx,
                    y,
                    h: EMAIL_ROW_H,
                });
                y += EMAIL_ROW_H + 10;
            }
            y += SECTION_GAP;
        }

        // Note.
        if !c.note.trim().is_empty() {
            items.push(DetailItem::Label("NOTE", y));
            y += 26;
            let lines = c.note.lines().count().max(1) as i32;
            let h = (40 + lines * 30).clamp(72, 220);
            items.push(DetailItem::NoteCard { y, h });
            y += h + SECTION_GAP;
        }

        // Nothing else? Show a hint line.
        if phones.is_empty() && emails.is_empty() && c.note.trim().is_empty() {
            items.push(DetailItem::Label(
                "No phone, email or note yet — tap Edit to add.",
                y,
            ));
            y += 40;
        }

        (items, y + 16)
    }

    // ---------------- form items (shared by render + hit-test) -------------

    fn form_items(&self, w: i32) -> (Vec<FormItem>, i32) {
        let m = ui::MARGIN;
        let inner_w = w - m * 2;
        let mut items = Vec::new();
        let mut y = 0i32;

        // Name card.
        items.push(FormItem::Name {
            card: Rect::new(0, y, inner_w, FIELD_H),
        });
        y += FIELD_H + SECTION_GAP;

        // Phones.
        items.push(FormItem::Label("PHONES", y));
        y += 26;
        for i in 0..self.form.phones.len() {
            let card = Rect::new(0, y, inner_w, PHONE_CARD_H);
            let label = Rect::new(20, y + 12, inner_w - 80, 30);
            let number = Rect::new(20, y + 66, inner_w - 80, 40);
            let remove = Rect::new(inner_w - 56, y + 12, 44, 44);
            items.push(FormItem::Phone {
                idx: i,
                card,
                label,
                number,
                remove,
            });
            y += PHONE_CARD_H + 10;
        }
        items.push(FormItem::AddPhone {
            rect: Rect::new(0, y, inner_w, 48),
        });
        y += 48 + SECTION_GAP;

        // Emails (label always shown so the first address can be added).
        items.push(FormItem::Label("EMAILS", y));
        y += 26;
        for i in 0..self.form.emails.len() {
            let card = Rect::new(0, y, inner_w, EMAIL_CARD_H);
            let label = Rect::new(20, y + 12, inner_w - 80, 30);
            let address = Rect::new(20, y + 66, inner_w - 80, 40);
            let remove = Rect::new(inner_w - 56, y + 12, 44, 44);
            items.push(FormItem::Email {
                idx: i,
                card,
                label,
                address,
                remove,
            });
            y += EMAIL_CARD_H + 10;
        }
        items.push(FormItem::AddEmail {
            rect: Rect::new(0, y, inner_w, 48),
        });
        y += 48 + SECTION_GAP;

        // Note.
        items.push(FormItem::Label("NOTE", y));
        y += 26;
        items.push(FormItem::Note {
            card: Rect::new(0, y, inner_w, NOTE_H),
        });
        y += NOTE_H + 16;

        (items, y)
    }

    // ---------------- hit testing ----------------

    fn hit_at(&self, pos: Point) -> Option<Hit> {
        let (w, h) = self.vp;
        match self.screen {
            Screen::List => self.list_hit(w, h, pos),
            Screen::Detail => self.detail_hit(w, h, pos),
            Screen::Form => self.form_hit(w, h, pos),
        }
    }

    fn list_hit(&self, w: i32, h: i32, pos: Point) -> Option<Hit> {
        let g = self.list_geom(w, h);
        let search = g.search;
        if search.contains(pos) {
            if !self.search.text.is_empty()
                && Rect::new(search.x + search.w - 46, search.y + 14, 30, 30).contains(pos)
            {
                return Some(Hit::SearchClear);
            }
            return Some(Hit::Search);
        }
        if g.favs_top != 0 {
            let favs = self.favorites();
            let n = favs.len() as i32;
            let card_w = (w - ui::MARGIN * 2 - 12 * (n - 1)) / n;
            for (i, _) in favs.iter().enumerate() {
                let cx = ui::MARGIN + i as i32 * (card_w + 12);
                if Rect::new(cx, g.favs_top, card_w, FAV_CARD_H).contains(pos) {
                    return Some(Hit::FavCard(i));
                }
            }
        }
        // Empty-state "Add contact" button — inside the viewport, so it must
        // be tested before the generic list-area hit.
        if self.contacts.is_empty() {
            let btn = Rect::new(w / 2 - 110, g.vp.y + g.vp.h / 2 + 78, 220, 56);
            if btn.contains(pos) {
                return Some(Hit::EmptyAdd);
            }
        }
        if g.vp.contains(pos) {
            if let Some(ci) = self.list_row_at(&g, pos) {
                // Quick-dial button on the row's right edge.
                let row_top = g.vp.y
                    + self
                        .filtered()
                        .iter()
                        .position(|&x| x == ci)
                        .map(|v| v as i32)
                        .unwrap_or(0)
                        * ROW_PITCH
                    - self.scroll;
                let call_btn = Rect::new(g.vp.x + g.vp.w - 72, row_top + (ROW_H - 56) / 2, 56, 56);
                if call_btn.contains(pos) {
                    return Some(Hit::DialRow(ci));
                }
                return Some(Hit::Row(ci));
            }
            return Some(Hit::List);
        }
        if g.fab.contains(pos) && !self.search_focus && !self.contacts.is_empty() {
            return Some(Hit::Fab);
        }
        None
    }

    fn detail_hit(&self, w: i32, h: i32, pos: Point) -> Option<Hit> {
        let g = self.detail_geom(w, h);
        if g.back.contains(pos) {
            return Some(Hit::Back);
        }
        if g.edit.contains(pos) {
            return Some(Hit::Edit);
        }
        if g.del.contains(pos) {
            return Some(Hit::Delete);
        }
        if g.fav.contains(pos) {
            return Some(Hit::FavToggle);
        }
        // Fixed call/message bar (bottom of screen, above gesture area).
        if g.call.contains(pos) {
            return Some(Hit::Call);
        }
        if g.message.contains(pos) {
            return Some(Hit::Message);
        }
        // Scrollable content.
        if !g.vp.contains(pos) {
            return None;
        }
        let (items, _) = self.detail_items(w);
        for it in items {
            match it {
                DetailItem::Phone {
                    idx,
                    y,
                    h: rh,
                    call,
                    sms,
                } => {
                    let card = Rect::new(ui::MARGIN, y, w - ui::MARGIN * 2, rh);
                    let row_s = Rect::new(card.x, card.y + g.vp.y - self.scroll, card.w, rh);
                    let call_s = call.translate(0, g.vp.y - self.scroll);
                    let sms_s = sms.translate(0, g.vp.y - self.scroll);
                    // Sub-buttons first (they sit inside the row's right edge).
                    if call_s.contains(pos) {
                        return Some(Hit::Dial(idx));
                    }
                    if sms_s.contains(pos) {
                        return Some(Hit::Sms(idx));
                    }
                    // Tapping the row itself dials — fast path for the
                    // common "tap a number to call" gesture.
                    if row_s.contains(pos) {
                        return Some(Hit::Dial(idx));
                    }
                }
                DetailItem::Email { y, h: rh, .. } => {
                    let row_s =
                        Rect::new(ui::MARGIN, y + g.vp.y - self.scroll, w - ui::MARGIN * 2, rh);
                    if row_s.contains(pos) {
                        return Some(Hit::Detail); // emails are display-only
                    }
                }
                _ => {}
            }
        }
        Some(Hit::Detail)
    }

    fn form_hit(&self, w: i32, h: i32, pos: Point) -> Option<Hit> {
        let g = self.form_geom(w, h);
        if g.back.contains(pos) {
            return Some(Hit::Cancel);
        }
        if save_bar(w, h).contains(pos) {
            return Some(Hit::Save);
        }
        if !g.vp.contains(pos) {
            return None;
        }
        let (items, _) = self.form_items(w);
        let dy = g.vp.y - self.scroll; // content→screen offset
        for it in items {
            let shift = |r: Rect| r.translate(ui::MARGIN, dy);
            match it {
                FormItem::Name { card } => {
                    if shift(card).contains(pos) {
                        return Some(Hit::Field(Focus::Name));
                    }
                }
                FormItem::Phone {
                    idx,
                    label,
                    number,
                    remove,
                    ..
                } => {
                    if shift(remove).contains(pos) {
                        return Some(Hit::RemovePhone(idx));
                    }
                    if shift(number).contains(pos) {
                        return Some(Hit::Field(Focus::PhoneNum(idx)));
                    }
                    if shift(label).contains(pos) {
                        return Some(Hit::Field(Focus::PhoneLabel(idx)));
                    }
                }
                FormItem::AddPhone { rect } => {
                    if shift(rect).contains(pos) {
                        return Some(Hit::AddPhone);
                    }
                }
                FormItem::Email {
                    idx,
                    label,
                    address,
                    remove,
                    ..
                } => {
                    if shift(remove).contains(pos) {
                        return Some(Hit::RemoveEmail(idx));
                    }
                    if shift(address).contains(pos) {
                        return Some(Hit::Field(Focus::EmailAddr(idx)));
                    }
                    if shift(label).contains(pos) {
                        return Some(Hit::Field(Focus::EmailLabel(idx)));
                    }
                }
                FormItem::AddEmail { rect } => {
                    if shift(rect).contains(pos) {
                        return Some(Hit::AddEmail);
                    }
                }
                FormItem::Note { card } => {
                    if shift(card).contains(pos) {
                        return Some(Hit::Field(Focus::Note));
                    }
                }
                FormItem::Label(..) => {}
            }
        }
        Some(Hit::Form)
    }

    // ---------------- scroll limits ----------------

    fn max_scroll(&self, w: i32, h: i32) -> i32 {
        match self.screen {
            Screen::List => {
                let g = self.list_geom(w, h);
                let n = self.filtered().len() as i32;
                let content = n * ROW_PITCH + 16;
                (content - g.vp.h).max(0)
            }
            Screen::Detail => {
                let g = self.detail_geom(w, h);
                let (_, ch) = self.detail_items(w);
                // Reserve space for the fixed call/message bar when present.
                let bar = if self
                    .open_contact()
                    .is_some_and(|c| c.primary_phone().is_some())
                {
                    84
                } else {
                    0
                };
                (ch - (g.vp.h - bar).max(0)).max(0)
            }
            Screen::Form => {
                let g = self.form_geom(w, h);
                let (_, ch) = self.form_items(w);
                (ch - g.vp.h).max(0)
            }
        }
    }

    fn viewport(&self, w: i32, h: i32) -> Rect {
        match self.screen {
            Screen::List => self.list_geom(w, h).vp,
            Screen::Detail => self.detail_geom(w, h).vp,
            Screen::Form => self.form_geom(w, h).vp,
        }
    }

    // ---------------- intro stagger (wall-clock, no Tick needed) ----------

    fn stagger_offset(&self, i: usize) -> i32 {
        let ms = self.intro_start.elapsed().as_secs_f32() * 1000.0;
        if ms >= INTRO_TOTAL_MS || i >= STAGGER_MAX {
            return 0;
        }
        let start = i as f32 * STAGGER_MS as f32;
        let t = ((ms - start).max(0.0)) / 160.0;
        if t >= 1.0 {
            0
        } else {
            ((1.0 - bezier_eval(BEZIER_ENTER, t)) * 14.0).round() as i32
        }
    }
}

// ---- small draw helpers ----

fn alpha(c: [u8; 4], a: u8) -> [u8; 4] {
    [c[0], c[1], c[2], a]
}

/// Rasterize + blit a Material icon with ink bounds centered on (cx, cy).
fn draw_icon(c: &mut Canvas, codepoint: &str, size: f32, color: [u8; 4], cx: i32, cy: i32) {
    if let Some(px) = text::icon(codepoint, size, color) {
        let (x0, y0, x1, y1) =
            text::ink_bounds(&px).unwrap_or((0, 0, px.width() as i32, px.height() as i32));
        let ox = cx - (x0 + x1) / 2;
        let oy = cy - (y0 + y1) / 2;
        c.blit(
            &Pixels::new(
                de_api::Size::new(px.width() as i32, px.height() as i32),
                px.data().to_vec(),
            ),
            Point::new(ox, oy),
        );
    }
}

/// Avatar circle: tinted bg + initial (or person glyph), same look as the
/// dialer's avatars (same palette via `model::hash_color`).
fn draw_avatar(c: &mut Canvas, r: Rect, name: &str, tint: [u8; 4]) {
    c.fill_rounded_rect(r, (r.h as f32) / 2.0, alpha(tint, 0x2E));
    if let Some(ch) = name.chars().next() {
        let s: String = ch.to_uppercase().collect();
        let st = ui::styled(r.h as f32 * 0.44, 600, tint);
        ui::draw_centered(c, &s, &st, r);
    } else {
        draw_icon(
            c,
            IC_PERSON,
            r.h as f32 * 0.52,
            tint,
            r.x + r.w / 2,
            r.y + r.h / 2,
        );
    }
}

/// Blit `px` clipped to `clip` (Canvas has no scissor — same trick as notes).
fn blit_clipped(canvas: &mut Canvas, px: &Pixels, dx: i32, dy: i32, clip: Rect) {
    let dst = Rect::new(dx, dy, px.size.w, px.size.h);
    let Some(r) = dst.intersect(clip) else { return };
    let region = Rect::new(r.x - dx, r.y - dy, r.w, r.h);
    canvas.blit_region(px, region, Point::new(r.x, r.y));
}

/// Caption/text/focus/placeholder bundle for `draw_field`.
#[derive(Clone, Copy)]
struct FieldSpec<'a> {
    caption: &'a str,
    buf: &'a TextBuf,
    focused: bool,
    placeholder: &'a str,
}

/// Draw one form field: card + small-caps caption + text + caret when
/// focused. `card` may extend past `clip_vp` while scrolling — the card bg
/// is drawn on the intersection and all text is blit-clipped.
fn draw_field(c: &mut Canvas, t: &Theme, card: Rect, spec: FieldSpec, clip_vp: Rect) {
    let FieldSpec {
        caption,
        buf,
        focused,
        placeholder,
    } = spec;
    let Some(vis) = card.intersect(clip_vp) else {
        return;
    };
    c.fill_rounded_rect(vis, 20.0, t.surface1);
    let clip = vis;
    // Caption.
    let cap = Rect::new(card.x + 20, card.y + 10, card.w - 40, 20);
    if let Some(px) = text::label_clamped(caption, ui::styled(13.0, 600, t.text3), cap.w, 1) {
        let p = text::cache::to_pixels(&px);
        blit_clipped(c, &p, cap.x, cap.y, clip);
    }
    let text_r = Rect::new(card.x + 20, card.y + 34, card.w - 40, card.h - 48);
    let tclip = text_r.intersect(clip).unwrap_or(text_r);
    if buf.text.is_empty() {
        if let Some(px) = text::label_clamped(placeholder, ui::body(t.text3), text_r.w, 1) {
            let p = text::cache::to_pixels(&px);
            blit_clipped(c, &p, text_r.x, text_r.y + 2, tclip);
        }
    } else {
        let style = ui::body(t.text);
        // Note field may contain newlines — draw up to 3 lines.
        let n = if buf.text.contains('\n') {
            buf.text.lines().count().min(3)
        } else {
            1
        };
        for (i, line) in buf.text.lines().take(n).enumerate() {
            if let Some(px) = text::label_clamped(line, style, text_r.w, 1) {
                let p = text::cache::to_pixels(&px);
                blit_clipped(c, &p, text_r.x, text_r.y + i as i32 * 30, tclip);
            }
        }
    }
    if focused {
        // Caret after the last visible line's text (approx end position).
        let style = ui::body(t.text);
        let last = buf.text.lines().last().unwrap_or("");
        let wpx = text::label(last, style, None)
            .map(|px| px.width() as i32)
            .unwrap_or(0);
        let line_i = if buf.text.contains('\n') {
            buf.text.lines().count().saturating_sub(1).min(2)
        } else {
            0
        };
        let cx = (text_r.x + wpx + 2).min(text_r.x + text_r.w - 4);
        let cy = text_r.y + line_i as i32 * 30;
        if let Some(r) = Rect::new(cx, cy + 2, 3, 26).intersect(tclip) {
            c.fill_rect(r, t.accent);
        }
    }
}

/// Section caption ("PHONES", "ALL CONTACTS", …).
fn draw_section(c: &mut Canvas, t: &Theme, s: &str, x: i32, y: i32) {
    ui::draw_text(c, s, &ui::styled(13.0, 600, t.text3), x, y, None);
}

fn save_bar(w: i32, h: i32) -> Rect {
    Rect::new(
        ui::MARGIN,
        h - ui::BOTTOM_SAFE - 16 - 72,
        w - ui::MARGIN * 2,
        72,
    )
}

fn call_bar(w: i32, h: i32) -> Rect {
    Rect::new(
        ui::MARGIN,
        h - ui::BOTTOM_SAFE - 16 - 72,
        w - ui::MARGIN * 2 - 140,
        72,
    )
}

fn message_bar(w: i32, h: i32) -> Rect {
    Rect::new(w - ui::MARGIN - 132, h - ui::BOTTOM_SAFE - 16 - 72, 132, 72)
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

impl ContactsApp {
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

    // ---------------- list screen ----------------

    fn render_list(&mut self, c: &mut Canvas, t: &Theme, w: i32, h: i32) {
        let g = self.list_geom(w, h);
        // Header: title + count subtitle. A single space keeps draw_header's
        // two-line layout (and therefore the search field position) stable
        // when the list is empty.
        let subtitle = if self.contacts.is_empty() {
            " ".to_string()
        } else {
            format!(
                "{} contact{}",
                self.contacts.len(),
                if self.contacts.len() == 1 { "" } else { "s" }
            )
        };
        ui::draw_header(c, t, "Contacts", &subtitle);

        // -- search field ----------------------------------------------------
        c.fill_rounded_rect(g.search, SEARCH_H as f32 / 2.0, t.surface1);
        draw_icon(
            c,
            IC_SEARCH,
            22.0,
            t.accent,
            g.search.x + 30,
            g.search.y + g.search.h / 2,
        );
        let text_x = g.search.x + 52;
        let avail =
            g.search.x + g.search.w - text_x - if self.search.text.is_empty() { 16 } else { 48 };
        if self.search.text.is_empty() {
            ui::draw_text(
                c,
                "Search name, number or email",
                &ui::body(t.text3),
                text_x,
                g.search.y + 17,
                Some(avail),
            );
        } else {
            let st = ui::body(t.text);
            if let Some(px) = text::label_clamped(&self.search.text, st, avail, 1) {
                let p = text::cache::to_pixels(&px);
                blit_clipped(
                    c,
                    &p,
                    text_x,
                    g.search.y + g.search.h / 2 - p.size.h / 2,
                    g.search,
                );
                if self.search_focus {
                    let cx = (text_x + p.size.w + 2).min(g.search.x + g.search.w - 12);
                    let r = Rect::new(cx, g.search.y + 16, 3, g.search.h - 32);
                    if let Some(r) = r.intersect(g.search) {
                        c.fill_rect(r, t.accent);
                    }
                }
            }
        }
        if self.search.text.is_empty() && self.search_focus {
            let r = Rect::new(text_x, g.search.y + 16, 3, g.search.h - 32);
            if let Some(r) = r.intersect(g.search) {
                c.fill_rect(r, t.accent);
            }
        }
        if !self.search.text.is_empty() {
            let xr = Rect::new(g.search.x + g.search.w - 46, g.search.y + 14, 30, 30);
            draw_icon(c, IC_CLOSE, 20.0, t.text3, xr.x + 15, xr.y + 15);
        }

        // -- favorites strip (hidden while searching) ------------------------
        let show_favs = self.search.text.trim().is_empty() && !self.favorites().is_empty();
        if show_favs {
            draw_section(c, t, "FAVORITES", ui::MARGIN, g.favs_label_y);
            let favs = self.favorites();
            let n = favs.len() as i32;
            let card_w = (w - ui::MARGIN * 2 - 12 * (n - 1)) / n;
            for (i, &ci) in favs.iter().enumerate() {
                let cx = ui::MARGIN + i as i32 * (card_w + 12);
                let card = Rect::new(cx, g.favs_top, card_w, FAV_CARD_H);
                let pressed = self.pressed == Some(Hit::FavCard(i));
                c.fill_rounded_rect(card, 24.0, if pressed { t.surface3 } else { t.surface1 });
                let ct = &self.contacts[ci];
                draw_avatar(
                    c,
                    Rect::new(card.x + card.w / 2 - 26, card.y + 12, 52, 52),
                    &ct.name,
                    model::avatar_tint(ct),
                );
                ui::draw_centered(
                    c,
                    &ct.name,
                    &ui::styled(15.0, 600, t.text),
                    Rect::new(card.x, card.y + 72, card.w, 20),
                );
                if let Some(p) = ct.primary_phone() {
                    ui::draw_centered(
                        c,
                        &model::format_number(&p.number),
                        &ui::styled(12.0, 400, t.text3),
                        Rect::new(card.x, card.y + 92, card.w, 16),
                    );
                }
            }
        }

        // -- section label + rows ---------------------------------------------
        draw_section(
            c,
            t,
            if self.search.text.trim().is_empty() {
                "ALL CONTACTS"
            } else {
                "RESULTS"
            },
            ui::MARGIN,
            g.section_y,
        );

        let ids = self.filtered();
        if self.contacts.is_empty() {
            self.render_empty(c, t, w, h, &g);
        } else if ids.is_empty() {
            ui::draw_centered(
                c,
                &format!("No contacts match \"{}\"", self.search.text.trim()),
                &ui::styled(20.0, 400, t.text3),
                Rect::new(g.vp.x, g.vp.y + g.vp.h / 2 - 20, g.vp.w, 40),
            );
        }

        for (vis, &ci) in ids.iter().enumerate() {
            let ct = &self.contacts[ci];
            let mut y = g.vp.y + vis as i32 * ROW_PITCH - self.scroll;
            y += self.stagger_offset(vis);
            let row = Rect::new(g.vp.x, y, g.vp.w, ROW_H);
            let Some(vis_row) = row.intersect(g.vp) else {
                continue;
            };
            let pressed = self.pressed == Some(Hit::Row(ci));
            ui::draw_card(c, t, vis_row, pressed);
            // M3 state-layer on press (90 ms-class feedback, static frame).
            if pressed {
                c.fill_rounded_rect(vis_row, 24.0, ui::state_layer(t, 0.10));
            }

            let inner = Rect::new(row.x + 14, row.y, row.w - 14 - 90, row.h);
            let Some(tclip) = inner.intersect(g.vp) else {
                continue;
            };
            draw_avatar(
                c,
                Rect::new(row.x + 14, row.y + (row.h - 60) / 2, 60, 60),
                &ct.name,
                model::avatar_tint(ct),
            );
            // Name (+ favorite star).
            let name_w = inner.w - 30;
            if let Some(px) = text::label_clamped(&ct.name, ui::label(t.text), name_w, 1) {
                let p = text::cache::to_pixels(&px);
                blit_clipped(c, &p, inner.x, inner.y + 12, tclip);
            }
            if ct.favorite {
                draw_icon(
                    c,
                    IC_STAR,
                    18.0,
                    t.warning,
                    inner.x + inner.w - 8,
                    inner.y + 20,
                );
            }
            // Primary number (+ label).
            if let Some(p) = ct.primary_phone() {
                let meta = if p.label.trim().is_empty() {
                    model::format_number(&p.number)
                } else {
                    format!("{} · {}", p.label, model::format_number(&p.number))
                };
                ui::draw_text(
                    c,
                    &meta,
                    &ui::styled(16.0, 400, t.text3),
                    inner.x,
                    inner.y + 48,
                    Some(inner.w),
                );
            } else if !ct.emails.is_empty() {
                ui::draw_text(
                    c,
                    &ct.emails[0].address,
                    &ui::styled(16.0, 400, t.text3),
                    inner.x,
                    inner.y + 48,
                    Some(inner.w),
                );
            }
            // Quick-dial button.
            let cb = Rect::new(row.x + row.w - 72, row.y + (row.h - 56) / 2, 56, 56);
            let cb_vis = cb.intersect(g.vp).unwrap_or(cb);
            let cbp = self.pressed == Some(Hit::DialRow(ci));
            c.fill_rounded_rect(
                cb_vis,
                28.0,
                alpha(t.success, if cbp { 0x66 } else { 0x24 }),
            );
            draw_icon(c, IC_CALL, 26.0, t.success, cb.x + 28, cb.y + 28);
        }

        // -- FAB ---------------------------------------------------------------
        if !self.search_focus && !self.contacts.is_empty() {
            let fab = g.fab;
            let fab_bg = if self.pressed == Some(Hit::Fab) {
                t.accent_dim
            } else {
                t.accent
            };
            c.fill_rounded_rect(fab, (fab.h / 2) as f32, fab_bg);
            draw_icon(
                c,
                IC_ADD,
                30.0,
                t.text_on_accent,
                fab.x + fab.w / 2,
                fab.y + fab.h / 2,
            );
        }
    }

    fn render_empty(&self, c: &mut Canvas, t: &Theme, w: i32, _h: i32, g: &ListGeom) {
        let cy = g.vp.y + g.vp.h / 2 - 90;
        draw_icon(c, IC_PERSON_ADD, 72.0, t.text3, w / 2, cy);
        ui::draw_centered(
            c,
            "No contacts yet",
            &ui::title(t.text2),
            Rect::new(0, cy + 56, w, 44),
        );
        ui::draw_centered(
            c,
            "Add the people you call and text most",
            &ui::body(t.text3),
            Rect::new(0, cy + 108, w, 36),
        );
        // "Add contact" action — same rect hit-tested in list_hit.
        let btn = Rect::new(w / 2 - 110, cy + 168, 220, 56);
        let pressed = self.pressed == Some(Hit::EmptyAdd);
        ui::draw_pill_button(c, t, btn, "Add contact", true, pressed);
    }

    // ---------------- detail screen ----------------

    fn render_detail(&mut self, c: &mut Canvas, t: &Theme, w: i32, h: i32) {
        let g = self.detail_geom(w, h);

        // Top bar pills.
        ui::draw_pill_button(
            c,
            t,
            g.back,
            "‹ Contacts",
            false,
            self.pressed == Some(Hit::Back),
        );
        // Favorite star toggle.
        let fav_on = self.open_contact().is_some_and(|x| x.favorite);
        let fr = if self.pressed == Some(Hit::FavToggle) {
            Rect::new(g.fav.x + 2, g.fav.y + 2, g.fav.w - 4, g.fav.h - 4)
        } else {
            g.fav
        };
        c.fill_rounded_rect(
            fr,
            (fr.h / 2) as f32,
            if fav_on {
                alpha(t.warning, 0x30)
            } else {
                t.surface2
            },
        );
        draw_icon(
            c,
            if fav_on { IC_STAR } else { IC_STAR_BORDER },
            24.0,
            if fav_on { t.warning } else { t.text2 },
            fr.x + fr.w / 2,
            fr.y + fr.h / 2,
        );
        ui::draw_pill_button(c, t, g.edit, "Edit", false, self.pressed == Some(Hit::Edit));
        // Delete pill: danger-filled when armed.
        let del_bg = if self.delete_armed {
            t.danger
        } else {
            t.surface2
        };
        let del_fg = if self.delete_armed {
            t.text_on_accent
        } else {
            t.text2
        };
        let dr = if self.pressed == Some(Hit::Delete) {
            Rect::new(g.del.x + 2, g.del.y + 2, g.del.w - 4, g.del.h - 4)
        } else {
            g.del
        };
        c.fill_rounded_rect(dr, (dr.h / 2) as f32, del_bg);
        ui::draw_centered(
            c,
            if self.delete_armed { "Sure?" } else { "Delete" },
            &ui::label(del_fg),
            dr,
        );

        let Some(ct) = self.open_contact().cloned() else {
            // Contact vanished (deleted elsewhere) — fall back to list.
            self.screen = Screen::List;
            return;
        };

        // Scrollable content.
        let (items, content_h) = self.detail_items(w);
        let bar_h = if ct.primary_phone().is_some() { 84 } else { 0 };
        let max_scroll = (content_h - (g.vp.h - bar_h).max(0)).max(0);
        self.scroll = self.scroll.clamp(0, max_scroll);
        let dy = g.vp.y - self.scroll;

        for it in items {
            match it {
                DetailItem::Header { y } => {
                    // Avatar + name block.
                    let head = Rect::new(ui::MARGIN, y + dy, w - ui::MARGIN * 2, HEADER_H);
                    let Some(hv) = head.intersect(g.vp) else {
                        continue;
                    };
                    let av = Rect::new(head.x, head.y + 8, 84, 84);
                    draw_avatar(c, av, &ct.name, model::avatar_tint(&ct));
                    let tx = head.x + 104;
                    let name_r = Rect::new(tx, head.y + 16, head.w - 104, 44);
                    if let Some(px) = text::label_clamped(&ct.name, ui::title(t.text), name_r.w, 2)
                    {
                        let p = text::cache::to_pixels(&px);
                        blit_clipped(c, &p, tx, name_r.y, hv);
                    }
                    if let Some(p) = ct.primary_phone() {
                        let meta = if p.label.trim().is_empty() {
                            model::format_number(&p.number)
                        } else {
                            format!("{} · {}", p.label, model::format_number(&p.number))
                        };
                        ui::draw_text(
                            c,
                            &meta,
                            &ui::body(t.text3),
                            tx,
                            head.y + 66,
                            Some(name_r.w),
                        );
                    } else if let Some(e) = ct.primary_email() {
                        ui::draw_text(
                            c,
                            &e.address,
                            &ui::body(t.text3),
                            tx,
                            head.y + 66,
                            Some(name_r.w),
                        );
                    }
                }
                DetailItem::Label(s, y) => {
                    if s.is_empty() || y + dy > g.vp.y + g.vp.h || y + dy + 26 < g.vp.y {
                        continue;
                    }
                    draw_section(c, t, s, ui::MARGIN, y + dy);
                }
                DetailItem::Phone {
                    idx,
                    y,
                    h: rh,
                    call,
                    sms,
                } => {
                    let card = Rect::new(ui::MARGIN, y + dy, w - ui::MARGIN * 2, rh);
                    let Some(cv) = card.intersect(g.vp) else {
                        continue;
                    };
                    let pressed_dial = self.pressed == Some(Hit::Dial(idx));
                    let pressed_sms = self.pressed == Some(Hit::Sms(idx));
                    ui::draw_card(c, t, cv, pressed_dial || pressed_sms);
                    let p = &ct.phones[idx];
                    // Label caption + number.
                    if !p.label.trim().is_empty() {
                        draw_section(c, t, &p.label.to_uppercase(), card.x + 20, card.y + 12);
                    }
                    ui::draw_text(
                        c,
                        &model::format_number(&p.number),
                        &ui::styled(24.0, 600, t.text),
                        card.x + 20,
                        card.y + (if p.label.trim().is_empty() { 14 } else { 36 }),
                        Some(card.w - 220),
                    );
                    // Call + SMS buttons.
                    let call_s = call.translate(0, dy);
                    let sms_s = sms.translate(0, dy);
                    let cbg = if pressed_dial {
                        alpha(t.success, 0x55)
                    } else {
                        alpha(t.success, 0x22)
                    };
                    if let Some(r) = call_s.intersect(g.vp) {
                        c.fill_rounded_rect(r, 28.0, cbg);
                    }
                    draw_icon(c, IC_CALL, 26.0, t.success, call_s.x + 28, call_s.y + 28);
                    let sbg = if pressed_sms {
                        alpha(t.accent, 0x55)
                    } else {
                        alpha(t.accent, 0x22)
                    };
                    if let Some(r) = sms_s.intersect(g.vp) {
                        c.fill_rounded_rect(r, 28.0, sbg);
                    }
                    draw_icon(c, IC_SMS, 26.0, t.accent, sms_s.x + 28, sms_s.y + 28);
                }
                DetailItem::Email { idx, y, h: rh } => {
                    let card = Rect::new(ui::MARGIN, y + dy, w - ui::MARGIN * 2, rh);
                    let Some(cv) = card.intersect(g.vp) else {
                        continue;
                    };
                    ui::draw_card(c, t, cv, false);
                    let e = &ct.emails[idx];
                    draw_icon(c, IC_MAIL, 26.0, t.accent, card.x + 34, card.y + rh / 2);
                    let tx = card.x + 64;
                    let label = if e.label.trim().is_empty() {
                        "email".to_string()
                    } else {
                        e.label.clone()
                    };
                    draw_section(c, t, &label.to_uppercase(), tx, card.y + 12);
                    ui::draw_text(
                        c,
                        &e.address,
                        &ui::styled(20.0, 500, t.text),
                        tx,
                        card.y + 32,
                        Some(card.w - 84),
                    );
                }
                DetailItem::NoteCard { y, h: nh } => {
                    let card = Rect::new(ui::MARGIN, y + dy, w - ui::MARGIN * 2, nh);
                    let Some(cv) = card.intersect(g.vp) else {
                        continue;
                    };
                    ui::draw_card(c, t, cv, false);
                    let inner = Rect::new(card.x + 20, card.y + 10, card.w - 40, card.h - 20);
                    let clip = inner.intersect(g.vp).unwrap_or(inner);
                    for (i, line) in ct.note.lines().take(6).enumerate() {
                        ui::draw_text(
                            c,
                            line,
                            &ui::body(t.text2),
                            inner.x,
                            inner.y + i as i32 * 30,
                            Some(inner.w),
                        );
                    }
                    let _ = clip;
                }
            }
        }

        // Fixed call/message bar.
        if ct.primary_phone().is_some() {
            let call = g.call;
            let msg = g.message;
            let cp = self.pressed == Some(Hit::Call);
            let mp = self.pressed == Some(Hit::Message);
            c.fill_rounded_rect(
                call,
                36.0,
                if cp {
                    mix(t.success, [0, 0, 0, 255], 0.15)
                } else {
                    t.success
                },
            );
            draw_icon(
                c,
                IC_CALL,
                30.0,
                t.text_on_accent,
                call.x + 40,
                call.y + call.h / 2,
            );
            ui::draw_text(
                c,
                "Call",
                &ui::styled(22.0, 600, t.text_on_accent),
                call.x + 76,
                call.y + call.h / 2 - 14,
                Some(call.w - 90),
            );
            c.fill_rounded_rect(
                msg,
                36.0,
                if mp {
                    mix(t.accent, [0, 0, 0, 255], 0.15)
                } else {
                    t.accent
                },
            );
            draw_icon(
                c,
                IC_SMS,
                30.0,
                t.text_on_accent,
                msg.x + 40,
                msg.y + msg.h / 2,
            );
            ui::draw_text(
                c,
                "Message",
                &ui::styled(22.0, 600, t.text_on_accent),
                msg.x + 76,
                msg.y + msg.h / 2 - 14,
                Some(msg.w - 90),
            );
        }
    }

    // ---------------- form screen ----------------

    fn render_form(&mut self, c: &mut Canvas, t: &Theme, w: i32, h: i32) {
        let g = self.form_geom(w, h);
        let editing = self.editing_id.is_some();

        // Top bar (Cancel only — Save lives in the fixed bottom bar).
        ui::draw_pill_button(
            c,
            t,
            g.back,
            "‹ Cancel",
            false,
            self.pressed == Some(Hit::Cancel),
        );
        let save_pressed = self.pressed == Some(Hit::Save);

        // Fixed Save bar at bottom (always reachable regardless of scroll).
        let bar = save_bar(w, h);
        let bar_bg = if save_pressed { t.accent_dim } else { t.accent };
        c.fill_rounded_rect(bar, 36.0, bar_bg);
        ui::draw_centered(
            c,
            "Save contact",
            &ui::styled(24.0, 600, t.text_on_accent),
            bar,
        );
        if self.form_error {
            ui::draw_centered(
                c,
                "Add a name or a phone number",
                &ui::styled(15.0, 500, t.danger),
                Rect::new(ui::MARGIN, bar.y - 26, w - ui::MARGIN * 2, 22),
            );
        }

        let (items, content_h) = self.form_items(w);
        let max_scroll = (content_h - g.vp.h).max(0);
        self.scroll = self.scroll.clamp(0, max_scroll);
        let dy = g.vp.y - self.scroll;

        for it in items {
            match it {
                FormItem::Label(s, y) => {
                    if y + dy > g.vp.y + g.vp.h || y + dy + 26 < g.vp.y {
                        continue;
                    }
                    draw_section(c, t, s, ui::MARGIN + 4, y + dy);
                }
                FormItem::Name { card } => {
                    let r = card.translate(ui::MARGIN, dy);
                    draw_field(
                        c,
                        t,
                        r,
                        FieldSpec {
                            caption: "NAME",
                            buf: &self.form.name,
                            focused: self.focus == Focus::Name,
                            placeholder: "Full name",
                        },
                        g.vp,
                    );
                }
                FormItem::Phone {
                    idx,
                    card,
                    label,
                    number,
                    remove,
                } => {
                    let r = card.translate(ui::MARGIN, dy);
                    let Some(v) = r.intersect(g.vp) else { continue };
                    c.fill_rounded_rect(v, 20.0, t.surface1);
                    let Some(row) = self.form.phones.get(idx) else {
                        continue;
                    };
                    // Two stacked sub-fields inside the card.
                    let lab = label.translate(ui::MARGIN, dy);
                    let num = number.translate(ui::MARGIN, dy);
                    let rem = remove.translate(ui::MARGIN, dy);
                    // Label field (top strip).
                    if let Some(lv) = Rect::new(lab.x, lab.y, lab.w, lab.h).intersect(g.vp) {
                        draw_mini_field(
                            c,
                            t,
                            lv,
                            "LABEL",
                            &row.label,
                            self.focus == Focus::PhoneLabel(idx),
                            "mobile, work…",
                        );
                    }
                    // Divider.
                    let div_y = lab.y + lab.h + 4;
                    if div_y > g.vp.y && div_y < g.vp.y + g.vp.h {
                        c.fill_rect(Rect::new(v.x + 20, div_y, v.w - 40, 1), t.line);
                    }
                    if let Some(nv) = Rect::new(num.x, num.y, num.w, num.h).intersect(g.vp) {
                        draw_mini_field(
                            c,
                            t,
                            nv,
                            "NUMBER",
                            &row.number,
                            self.focus == Focus::PhoneNum(idx),
                            "081 234 5678",
                        );
                    }
                    // Remove button.
                    let pressed_rm = self.pressed == Some(Hit::RemovePhone(idx));
                    if let Some(rv) = rem.intersect(g.vp) {
                        c.fill_rounded_rect(
                            rv,
                            22.0,
                            if pressed_rm {
                                alpha(t.danger, 0x30)
                            } else {
                                t.surface2
                            },
                        );
                        draw_icon(
                            c,
                            IC_DELETE,
                            20.0,
                            t.danger,
                            rv.x + rv.w / 2,
                            rv.y + rv.h / 2,
                        );
                    }
                }
                FormItem::AddPhone { rect } => {
                    let r = rect.translate(ui::MARGIN, dy);
                    let Some(v) = r.intersect(g.vp) else { continue };
                    let pressed = self.pressed == Some(Hit::AddPhone);
                    c.fill_rounded_rect(v, 24.0, if pressed { t.surface3 } else { t.surface2 });
                    draw_icon(c, IC_ADD, 20.0, t.accent, v.x + 26, v.y + v.h / 2);
                    ui::draw_text(
                        c,
                        "Add phone",
                        &ui::styled(18.0, 600, t.accent),
                        v.x + 48,
                        v.y + v.h / 2 - 12,
                        Some(v.w - 60),
                    );
                }
                FormItem::Email {
                    idx,
                    card,
                    label,
                    address,
                    remove,
                } => {
                    let r = card.translate(ui::MARGIN, dy);
                    let Some(v) = r.intersect(g.vp) else { continue };
                    c.fill_rounded_rect(v, 20.0, t.surface1);
                    let Some(row) = self.form.emails.get(idx) else {
                        continue;
                    };
                    let lab = label.translate(ui::MARGIN, dy);
                    let addr = address.translate(ui::MARGIN, dy);
                    let rem = remove.translate(ui::MARGIN, dy);
                    if let Some(lv) = Rect::new(lab.x, lab.y, lab.w, lab.h).intersect(g.vp) {
                        draw_mini_field(
                            c,
                            t,
                            lv,
                            "LABEL",
                            &row.label,
                            self.focus == Focus::EmailLabel(idx),
                            "home, work…",
                        );
                    }
                    let div_y = lab.y + lab.h + 4;
                    if div_y > g.vp.y && div_y < g.vp.y + g.vp.h {
                        c.fill_rect(Rect::new(v.x + 20, div_y, v.w - 40, 1), t.line);
                    }
                    if let Some(av) = Rect::new(addr.x, addr.y, addr.w, addr.h).intersect(g.vp) {
                        draw_mini_field(
                            c,
                            t,
                            av,
                            "EMAIL",
                            &row.address,
                            self.focus == Focus::EmailAddr(idx),
                            "name@example.com",
                        );
                    }
                    let pressed_rm = self.pressed == Some(Hit::RemoveEmail(idx));
                    if let Some(rv) = rem.intersect(g.vp) {
                        c.fill_rounded_rect(
                            rv,
                            22.0,
                            if pressed_rm {
                                alpha(t.danger, 0x30)
                            } else {
                                t.surface2
                            },
                        );
                        draw_icon(
                            c,
                            IC_DELETE,
                            20.0,
                            t.danger,
                            rv.x + rv.w / 2,
                            rv.y + rv.h / 2,
                        );
                    }
                }
                FormItem::AddEmail { rect } => {
                    let r = rect.translate(ui::MARGIN, dy);
                    let Some(v) = r.intersect(g.vp) else { continue };
                    let pressed = self.pressed == Some(Hit::AddEmail);
                    c.fill_rounded_rect(v, 24.0, if pressed { t.surface3 } else { t.surface2 });
                    draw_icon(c, IC_ADD, 20.0, t.accent, v.x + 26, v.y + v.h / 2);
                    ui::draw_text(
                        c,
                        "Add email",
                        &ui::styled(18.0, 600, t.accent),
                        v.x + 48,
                        v.y + v.h / 2 - 12,
                        Some(v.w - 60),
                    );
                }
                FormItem::Note { card } => {
                    let r = card.translate(ui::MARGIN, dy);
                    draw_field(
                        c,
                        t,
                        r,
                        FieldSpec {
                            caption: "NOTE",
                            buf: &self.form.note,
                            focused: self.focus == Focus::Note,
                            placeholder: "Notes about this contact",
                        },
                        g.vp,
                    );
                }
            }
        }

        // Title hint under the top bar.
        ui::draw_text(
            c,
            if editing {
                "Edit contact"
            } else {
                "New contact"
            },
            &ui::title(t.text),
            ui::MARGIN,
            ui::TOP_SAFE + 68,
            Some(w - ui::MARGIN * 2),
        );
    }
}

/// Compact single-line field used inside phone/email cards (no card chrome —
/// the parent card provides it).
fn draw_mini_field(
    c: &mut Canvas,
    t: &Theme,
    r: Rect,
    caption: &str,
    buf: &TextBuf,
    focused: bool,
    placeholder: &str,
) {
    let _ = caption;
    let text_r = Rect::new(r.x, r.y + 4, r.w, r.h - 4);
    if buf.text.is_empty() {
        ui::draw_text(
            c,
            placeholder,
            &ui::styled(17.0, 400, t.text3),
            text_r.x,
            text_r.y + 2,
            Some(text_r.w),
        );
    } else {
        if let Some(px) = text::label_clamped(&buf.text, ui::body(t.text), text_r.w, 1) {
            let p = text::cache::to_pixels(&px);
            blit_clipped(c, &p, text_r.x, text_r.y + 2, text_r);
        }
    }
    if focused {
        let style = ui::body(t.text);
        let wpx = text::label(&buf.text, style, None)
            .map(|px| px.width() as i32)
            .unwrap_or(0);
        let cx = (text_r.x + wpx + 2).min(text_r.x + text_r.w - 4);
        let rc = Rect::new(cx, text_r.y, 3, text_r.h - 6);
        if let Some(rc) = rc.intersect(text_r) {
            c.fill_rect(rc, t.accent);
        }
    }
}

fn mix(a: [u8; 4], b: [u8; 4], t: f32) -> [u8; 4] {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t) as u8;
    [l(a[0], b[0]), l(a[1], b[1]), l(a[2], b[2]), l(a[3], b[3])]
}

const HEADER_H: i32 = 104;
const PHONE_ROW_H: i32 = 84;
const EMAIL_ROW_H: i32 = 76;

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
}
