//! RabbizOS Search — fast native app + web search (`rabbiz-app-kit`).
//!
//! Launches directly from the Home search bar:
//! - Status-bar aware top search pill (auto-focused on open).
//! - Starts clean (no app clutter) until you start typing.
//! - Matching installed apps appear first (tap to launch).
//! - Native web search cards (domain + title + snippet) load asynchronously;
//!   tap any card or "Open in <Engine>" to launch in Chrome.
//! - Default search engine (Google, Perplexity, DuckDuckGo, Yahoo, Live) is
//!   configured in Settings (`Prefs::search_engine`).

mod apps;
mod cli;
mod web;

use std::time::{Duration, Instant};

use de_api::{Point, Size};
use de_config::prefs::{search_engine_label, search_url};
use de_config::Prefs;
use de_design::tokens;
use de_paint::text;
use rabbiz_app_kit::{
    icons, ui, AppEvent, AppFrame, AppState, Canvas, Rect, TextBuffer, TextInputCtl, Theme,
};
use wayland_protocols::wp::text_input::zv3::client::zwp_text_input_v3::ContentPurpose;

use apps::{discover_apps, filter_apps, launch_app, open_url_in_browser, AppItem};
use web::{WebHit, WebState, WebWorker};

const PAD: i32 = ui::MARGIN;
const TOP_Y: i32 = tokens::STATUS_BAR_HEIGHT + 16;
const BAR_H: i32 = 72;
const CONTENT_Y: i32 = TOP_Y + BAR_H + 18;
const APP_ROW_H: i32 = 84;
const ACTION_ROW_H: i32 = 80;
const WEB_CARD_H: i32 = 152;
const ROW_GAP: i32 = 12;
const DRAG_SLOP: i32 = 14;
const AUTO_SEARCH_DEBOUNCE: Duration = Duration::from_millis(450);

#[derive(Debug, Clone, PartialEq, Eq)]
enum Hit {
    None,
    Back,
    SearchField,
    Clear,
    App(usize),
    OpenEngine,
    WebResult(usize),
}

#[derive(Debug, Clone)]
enum CardView {
    Section(String),
    AppRow { name: String },
    OpenEngineRow { engine: String, query: String },
    WebCard { hit: WebHit },
    StatusText(String),
}

#[derive(Debug, Clone)]
struct Placed {
    rect: Rect,
    view: CardView,
    hit: Hit,
}

struct TouchTrack {
    id: i32,
    start: Point,
    last: Point,
    scrolling: bool,
    hit: Hit,
}

struct SearchApp {
    size: Size,
    prefs: Prefs,
    installed: Vec<AppItem>,
    query: TextBuffer,
    focused: bool,
    auto_focused: bool,
    scroll: i32,
    touch: Option<TouchTrack>,
    worker: WebWorker,
    web: WebState,
    pending_web_at: Option<Instant>,
    last_edit: Instant,
    should_quit: bool,
}

impl SearchApp {
    fn new() -> Self {
        let prefs = Prefs::load();
        let installed = discover_apps();
        Self {
            size: Size::new(720, 1600),
            prefs,
            installed,
            query: TextBuffer::new(),
            focused: true,
            auto_focused: false,
            scroll: 0,
            touch: None,
            worker: WebWorker::start(),
            web: WebState::default(),
            pending_web_at: None,
            last_edit: Instant::now(),
            should_quit: false,
        }
    }

    fn back_rect(&self) -> Rect {
        Rect::new(PAD, TOP_Y + 4, 64, BAR_H - 8)
    }

    fn field_rect(&self) -> Rect {
        let x = PAD + 74;
        Rect::new(x, TOP_Y, (self.size.w - x - PAD).max(120), BAR_H)
    }

    fn clear_rect(&self) -> Rect {
        let f = self.field_rect();
        Rect::new(f.x + f.w - 60, f.y + 4, 56, f.h - 8)
    }

    fn layout_cards(&self) -> (Vec<Placed>, i32) {
        let q = self.query.as_str().trim();
        if q.is_empty() {
            return (Vec::new(), 0);
        }
        let cw = (self.size.w - PAD * 2).max(160);
        let mut items = Vec::new();
        let mut y = CONTENT_Y - self.scroll;

        // 1. Matching apps first.
        let matched = filter_apps(&self.installed, q);
        if !matched.is_empty() {
            items.push(Placed {
                rect: Rect::new(PAD, y, cw, 34),
                view: CardView::Section("Apps".into()),
                hit: Hit::None,
            });
            y += 38;
            for (idx, app) in matched.iter().enumerate() {
                items.push(Placed {
                    rect: Rect::new(PAD, y, cw, APP_ROW_H),
                    view: CardView::AppRow {
                        name: app.name.clone(),
                    },
                    hit: Hit::App(idx),
                });
                y += APP_ROW_H + ROW_GAP;
            }
            y += 8;
        }

        // 2. Web search section (uses default engine configured in Settings).
        let engine_name = search_engine_label(&self.prefs.search_engine);
        items.push(Placed {
            rect: Rect::new(PAD, y, cw, 34),
            view: CardView::Section(format!("Web · {engine_name}")),
            hit: Hit::None,
        });
        y += 38;

        items.push(Placed {
            rect: Rect::new(PAD, y, cw, ACTION_ROW_H),
            view: CardView::OpenEngineRow {
                engine: engine_name.to_string(),
                query: q.to_string(),
            },
            hit: Hit::OpenEngine,
        });
        y += ACTION_ROW_H + ROW_GAP;

        if self.web.loading && self.web.query == q {
            items.push(Placed {
                rect: Rect::new(PAD, y, cw, 60),
                view: CardView::StatusText("Searching...".into()),
                hit: Hit::None,
            });
            y += 68;
        } else if let Some(err) = self.web.error.as_ref().filter(|_| self.web.query == q) {
            items.push(Placed {
                rect: Rect::new(PAD, y, cw, 60),
                view: CardView::StatusText(err.clone()),
                hit: Hit::None,
            });
            y += 68;
        } else if self.web.query == q {
            for (i, hit) in self.web.hits.iter().enumerate() {
                items.push(Placed {
                    rect: Rect::new(PAD, y, cw, WEB_CARD_H),
                    view: CardView::WebCard { hit: hit.clone() },
                    hit: Hit::WebResult(i),
                });
                y += WEB_CARD_H + ROW_GAP;
            }
        }

        let content_h = (y + self.scroll) - CONTENT_Y + ui::BOTTOM_SAFE + 24;
        (items, content_h.max(0))
    }

    fn max_scroll(&self) -> i32 {
        let (_, content_h) = self.layout_cards();
        let viewport = (self.size.h - CONTENT_Y).max(1);
        (content_h - viewport).max(0)
    }

    fn hit_at(&self, pos: Point) -> Hit {
        if self.back_rect().contains(pos) {
            return Hit::Back;
        }
        if !self.query.is_empty() && self.clear_rect().contains(pos) {
            return Hit::Clear;
        }
        if self.field_rect().contains(pos) {
            return Hit::SearchField;
        }
        if pos.y < CONTENT_Y {
            return Hit::None;
        }
        let (items, _) = self.layout_cards();
        for item in items {
            if item.hit != Hit::None && item.rect.contains(pos) {
                return item.hit;
            }
        }
        Hit::None
    }

    fn after_query_change(&mut self, ti: &TextInputCtl) {
        self.scroll = 0;
        self.last_edit = Instant::now();
        ti.set_surrounding(self.query.as_str(), self.query.cursor());
        if self.query.as_str().trim().is_empty() {
            self.pending_web_at = None;
            self.worker.clear();
            self.web = self.worker.snapshot();
        } else {
            self.pending_web_at = Some(Instant::now() + AUTO_SEARCH_DEBOUNCE);
        }
    }

    fn trigger_web_search_now(&mut self) {
        self.pending_web_at = None;
        let q = self.query.as_str().trim().to_string();
        if !q.is_empty() {
            self.worker.search(&q);
            self.web = self.worker.snapshot();
        }
    }

    fn activate_hit(&mut self, hit: Hit, ti: &TextInputCtl) {
        match hit {
            Hit::Back => {
                ti.deactivate();
                self.should_quit = true;
            }
            Hit::SearchField => {
                self.focused = true;
                ti.activate(ContentPurpose::Normal);
                ti.set_surrounding(self.query.as_str(), self.query.cursor());
            }
            Hit::Clear => {
                self.query.clear();
                self.after_query_change(ti);
                self.focused = true;
                ti.activate(ContentPurpose::Normal);
            }
            Hit::App(idx) => {
                let q = self.query.as_str().to_string();
                let matched = filter_apps(&self.installed, &q);
                if let Some(app) = matched.get(idx) {
                    launch_app(app);
                    ti.deactivate();
                    self.should_quit = true;
                }
            }
            Hit::OpenEngine => {
                let q = self.query.as_str().trim();
                if !q.is_empty() {
                    let url = search_url(&self.prefs.search_engine, q);
                    open_url_in_browser(&url);
                    ti.deactivate();
                    self.should_quit = true;
                }
            }
            Hit::WebResult(idx) => {
                if let Some(item) = self.web.hits.get(idx) {
                    open_url_in_browser(&item.url);
                    ti.deactivate();
                    self.should_quit = true;
                }
            }
            Hit::None => {}
        }
    }

    fn frame(&self, dirty: bool) -> AppFrame {
        let animating = self.web.loading || self.pending_web_at.is_some();
        AppFrame {
            dirty,
            animating,
            quit: self.should_quit,
        }
    }
}

impl AppState for SearchApp {
    fn app_id(&self) -> &'static str {
        "rabbiz-search"
    }

    fn title(&self) -> &'static str {
        "Search"
    }

    fn needs_tick(&self) -> bool {
        !self.auto_focused || self.web.loading || self.pending_web_at.is_some()
    }

    fn handle_event(&mut self, event: AppEvent, ti: &TextInputCtl) -> AppFrame {
        match event {
            AppEvent::TouchDown { id, pos } => {
                let hit = self.hit_at(pos);
                self.touch = Some(TouchTrack {
                    id,
                    start: pos,
                    last: pos,
                    scrolling: false,
                    hit,
                });
                self.frame(false)
            }
            AppEvent::TouchMove { id, pos } => {
                if let Some(t) = self.touch.as_mut().filter(|t| t.id == id) {
                    let dy = pos.y - t.last.y;
                    t.last = pos;
                    if !t.scrolling && (pos.y - t.start.y).abs() > DRAG_SLOP {
                        t.scrolling = true;
                    }
                    if t.scrolling {
                        let max_s = self.max_scroll();
                        self.scroll = (self.scroll - dy).clamp(0, max_s);
                        return self.frame(true);
                    }
                }
                self.frame(false)
            }
            AppEvent::TouchUp { id, pos } => {
                if let Some(t) = self.touch.take().filter(|t| t.id == id) {
                    if !t.scrolling {
                        let hit = self.hit_at(pos);
                        if hit == t.hit {
                            self.activate_hit(hit, ti);
                            return self.frame(true);
                        }
                    }
                }
                self.frame(false)
            }
            AppEvent::Scroll { dy, .. } => {
                let max_s = self.max_scroll();
                self.scroll = (self.scroll + (dy * 32.0).round() as i32).clamp(0, max_s);
                self.frame(true)
            }
            AppEvent::TextCommit(s) => {
                if !s.is_empty() {
                    self.query.insert(&s);
                    self.after_query_change(ti);
                    return self.frame(true);
                }
                self.frame(false)
            }
            AppEvent::TextBackspace(n) => {
                for _ in 0..n.max(1) {
                    self.query.backspace();
                }
                self.after_query_change(ti);
                self.frame(true)
            }
            AppEvent::Key { keysym, utf8 } => {
                match keysym {
                    0xff1b => {
                        if !self.query.is_empty() {
                            self.query.clear();
                            self.after_query_change(ti);
                            return self.frame(true);
                        }
                        ti.deactivate();
                        self.should_quit = true;
                        return self.frame(true);
                    }
                    0xff0d => {
                        ti.deactivate();
                        self.focused = false;
                        self.trigger_web_search_now();
                        return self.frame(true);
                    }
                    0xff08 => {
                        self.query.backspace();
                        self.after_query_change(ti);
                        return self.frame(true);
                    }
                    0xff51 => {
                        self.query.move_left();
                        ti.set_surrounding(self.query.as_str(), self.query.cursor());
                        return self.frame(true);
                    }
                    0xff53 => {
                        self.query.move_right();
                        ti.set_surrounding(self.query.as_str(), self.query.cursor());
                        return self.frame(true);
                    }
                    _ => {
                        if let Some(s) = utf8.filter(|s| !s.is_empty()) {
                            self.query.insert(&s);
                            self.after_query_change(ti);
                            return self.frame(true);
                        }
                    }
                }
                self.frame(false)
            }
            AppEvent::Tick(_) => {
                let mut dirty = false;
                if !self.auto_focused {
                    self.auto_focused = true;
                    self.focused = true;
                    ti.activate(ContentPurpose::Normal);
                    ti.set_surrounding(self.query.as_str(), self.query.cursor());
                    dirty = true;
                }
                if let Some(at) = self.pending_web_at {
                    if Instant::now() >= at {
                        self.trigger_web_search_now();
                        dirty = true;
                    }
                }
                let snap = self.worker.snapshot();
                if snap.seq != self.web.seq
                    || snap.loading != self.web.loading
                    || snap.hits.len() != self.web.hits.len()
                    || snap.error != self.web.error
                {
                    self.web = snap;
                    dirty = true;
                }
                self.frame(dirty)
            }
            AppEvent::PointerMove { .. } | AppEvent::PointerButton { .. } => self.frame(false),
        }
    }

    fn render(&mut self, c: &mut Canvas, t: &Theme) {
        self.size = c.size();
        c.fill_rect(Rect::new(0, 0, self.size.w, self.size.h), t.bg);

        // 1. Scrollable results area first (clipped below CONTENT_Y by sticky header).
        let (items, _) = self.layout_cards();
        if self.query.as_str().trim().is_empty() {
            let msg = format!(
                "Search apps or {}",
                search_engine_label(&self.prefs.search_engine)
            );
            ui::draw_centered(
                c,
                &msg,
                &ui::body(t.text3),
                Rect::new(PAD, CONTENT_Y + 96, self.size.w - PAD * 2, 56),
            );
        } else {
            for item in &items {
                if item.rect.y + item.rect.h < CONTENT_Y || item.rect.y > self.size.h {
                    continue;
                }
                match &item.view {
                    CardView::Section(label) => {
                        ui::draw_text(
                            c,
                            label,
                            &ui::label(t.text3),
                            item.rect.x + 6,
                            item.rect.y + 4,
                            Some(item.rect.w - 12),
                        );
                    }
                    CardView::AppRow { name } => {
                        c.fill_rounded_rect(item.rect, 22.0, t.surface1);
                        let av = Rect::new(item.rect.x + 16, item.rect.y + 16, 52, 52);
                        c.fill_rounded_rect(av, 16.0, t.accent_dim);
                        let initial: String = name.chars().take(1).collect();
                        ui::draw_centered(
                            c,
                            &initial,
                            &ui::styled(24.0, 600, t.text_on_accent),
                            av,
                        );
                        ui::draw_text(
                            c,
                            name,
                            &ui::body(t.text),
                            av.x + av.w + 18,
                            item.rect.y + 28,
                            Some(item.rect.w - 108),
                        );
                    }
                    CardView::OpenEngineRow { engine, query } => {
                        c.fill_rounded_rect(item.rect, 22.0, t.surface2);
                        let ic = Rect::new(item.rect.x + 16, item.rect.y + 16, 48, 48);
                        c.fill_rounded_rect(ic, 24.0, t.accent);
                        ui::draw_centered(
                            c,
                            icons::SEARCH,
                            &text::LabelStyle::icon(26.0, t.text_on_accent),
                            ic,
                        );
                        let title = format!("Open \"{query}\" in {engine}");
                        ui::draw_text(
                            c,
                            &title,
                            &ui::body(t.text),
                            ic.x + ic.w + 16,
                            item.rect.y + 26,
                            Some(item.rect.w - 120),
                        );
                        ui::draw_centered(
                            c,
                            icons::CHEVRON_RIGHT,
                            &text::LabelStyle::icon(26.0, t.text3),
                            Rect::new(item.rect.x + item.rect.w - 46, item.rect.y + 20, 36, 40),
                        );
                    }
                    CardView::WebCard { hit } => {
                        c.fill_rounded_rect(item.rect, 22.0, t.surface1);
                        let tx = item.rect.x + 20;
                        let tw = item.rect.w - 40;
                        ui::draw_text(
                            c,
                            &hit.domain,
                            &ui::caption(t.accent),
                            tx,
                            item.rect.y + 16,
                            Some(tw),
                        );
                        ui::draw_text(
                            c,
                            &hit.title,
                            &ui::styled(24.0, 600, t.text),
                            tx,
                            item.rect.y + 44,
                            Some(tw),
                        );
                        if !hit.snippet.is_empty() {
                            ui::draw_text(
                                c,
                                &hit.snippet,
                                &ui::label(t.text2),
                                tx,
                                item.rect.y + 84,
                                Some(tw),
                            );
                        }
                    }
                    CardView::StatusText(msg) => {
                        ui::draw_text(
                            c,
                            msg,
                            &ui::body(t.text3),
                            item.rect.x + 12,
                            item.rect.y + 16,
                            Some(item.rect.w - 24),
                        );
                    }
                }
            }
        }

        // 2. Sticky top header (below status bar).
        c.fill_rect(Rect::new(0, 0, self.size.w, CONTENT_Y - 4), t.bg);

        // Back button.
        let back = self.back_rect();
        c.fill_rounded_rect(back, 32.0, t.surface2);
        ui::draw_centered(c, icons::BACK, &text::LabelStyle::icon(28.0, t.text), back);

        // Single clean search input pill.
        let field = self.field_rect();
        c.fill_rounded_rect(field, 36.0, t.surface2);
        if self.focused {
            ui::stroke_rounded(c, t, field, 36.0, t.accent, 2);
        }
        let q = self.query.as_str();
        let text_x = field.x + 24;
        let text_w = field.w - if q.is_empty() { 44 } else { 84 };
        if q.is_empty() {
            ui::draw_text(
                c,
                "Search apps & web...",
                &ui::body(t.text3),
                text_x,
                field.y + 22,
                Some(text_w),
            );
        } else {
            ui::draw_text(
                c,
                q,
                &ui::body(t.text),
                text_x,
                field.y + 22,
                Some(text_w),
            );
            let clr = self.clear_rect();
            ui::draw_centered(c, icons::CLOSE, &text::LabelStyle::icon(26.0, t.text3), clr);
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(pos) = args.iter().position(|a| a == "--cli") {
        std::process::exit(cli::run(&args[pos + 1..]));
    }
    if args.iter().any(|a| a == "--mcp") {
        rabbiz_app_kit::mcp::serve(cli::TOOLS, "rabbiz-search", "0.1.0");
    }
    rabbiz_app_kit::run_app(SearchApp::new());
}
