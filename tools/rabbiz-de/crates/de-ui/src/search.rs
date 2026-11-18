//! P14 global search (M3-T3): 72 px field with query + caret, results
//! grouped by kind (apps / actions / settings), icon rows.
//!
//! Results arrive pre-ranked from `de_apps::Catalog::search` — the render
//! is a pure view.

use de_api::{Pixels, Rect, Size};
use de_design::tokens::{self, Theme};
use de_paint::{text, Canvas};

use crate::chrome::{self, style, IconEntry};
use crate::i18n::Strings;

/// What a result row is (drives the group header + i18n key).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchKind {
    App,
    Action,
    Setting,
}

/// One result row.
#[derive(Debug, Clone)]
pub struct SearchRow {
    pub kind: SearchKind,
    /// Desktop-id (App/Action) or settings page id.
    pub id: String,
    pub title: String,
    /// Second line: action parent's app name, category, etc.
    pub subtitle: String,
    pub icon: Option<Pixels>,
}

/// Render the search overlay. `rows` are already ranked; the view groups
/// them by kind in the order they appear.
pub fn render(size: Size, s: &Strings, t: &Theme, query: &str, rows: &[SearchRow]) -> Pixels {
    let mut c = Canvas::filled(size, t.bg);
    c.fill_rect(Rect::new(0, 0, size.w, size.h), t.scrim);

    // Query field with trailing caret when a query is present.
    let field = Rect::new(chrome::MARGIN, 96, chrome::content_w(size), 72);
    c.fill_rounded_rect(field, tokens::RADIUS_PILL, t.surface2);
    chrome::draw(&mut c, "\u{e8b6}", &text::LabelStyle::icon(26.0, t.text3), field.x + 24, field.y + 22, None);
    let shown = if query.is_empty() { s.get("search.hint") } else { query };
    let col = if query.is_empty() { t.text3 } else { t.text };
    let r = chrome::draw(&mut c, shown, &style::body(col), field.x + 64, field.y + 20, Some(field.w - 88));
    if !query.is_empty() {
        c.fill_rect(Rect::new(r.x + r.w + 2, r.y + 2, 3, r.h - 4), t.accent);
    }

    // Grouped rows: kind order of first appearance, fixed kind sequence.
    // Card geometry comes from `row_rects` so `hit_test` can't drift.
    let rects = row_rects(size, rows);
    for kind in [SearchKind::App, SearchKind::Action, SearchKind::Setting] {
        let Some(first) = rows.iter().position(|r| r.kind == kind) else {
            continue;
        };
        let first_rect = rects[first];
        if first_rect.is_empty() {
            continue;
        }
        let header = match kind {
            SearchKind::App => s.get("search.apps"),
            SearchKind::Action => s.get("search.actions"),
            SearchKind::Setting => s.get("search.settings"),
        };
        chrome::draw(&mut c, header, &style::caption(t.text3), chrome::MARGIN + 8, first_rect.y - 36, None);
        for (i, row) in rows.iter().enumerate() {
            if row.kind != kind || rects[i].is_empty() {
                continue;
            }
            let card = rects[i];
            c.fill_rounded_rect(card, tokens::RADIUS_ROW, t.surface1);
            let e = IconEntry { id: row.id.clone(), label: row.title.clone(), icon: row.icon.clone() };
            chrome::draw_app_icon(&mut c, t, &e, Rect::new(card.x + 12, card.y + 14, 52, 52));
            chrome::draw(&mut c, &row.title, &style::body(t.text), card.x + 80, card.y + 14, Some(card.w - 100));
            if !row.subtitle.is_empty() {
                chrome::draw(&mut c, &row.subtitle, &style::caption(t.text3), card.x + 80, card.y + 46, Some(card.w - 100));
            }
        }
    }
    if rows.is_empty() && !query.is_empty() {
        chrome::draw_centered(&mut c, s.get("search.none"), &style::body(t.text3), size.h / 2);
    }
    chrome::nav_pill(&mut c, t);
    c.into_pixels()
}

/// Row rects in the order `rows` are painted (grouped, headers skipped) —
/// shared by [`render`] and [`hit_test`] so they can't drift apart.
fn row_rects(size: Size, rows: &[SearchRow]) -> Vec<Rect> {
    let mut rects = vec![Rect::ZERO; rows.len()];
    let mut y = 96 + 72 + 24; // field.y + field.h + gap
    for kind in [SearchKind::App, SearchKind::Action, SearchKind::Setting] {
        if !rows.iter().any(|r| r.kind == kind) {
            continue;
        }
        y += 36; // group header
        for (i, row) in rows.iter().enumerate() {
            if row.kind != kind {
                continue;
            }
            if y + 88 > size.h - 120 {
                break;
            }
            rects[i] = Rect::new(chrome::MARGIN, y, chrome::content_w(size), 80);
            y += 80 + 12;
        }
        y += 8;
    }
    rects
}

/// Which result row `pos` tapped (index into `rows`), or `None`.
#[must_use]
pub fn hit_test(size: Size, rows: &[SearchRow], pos: de_api::Point) -> Option<usize> {
    row_rects(size, rows)
        .iter()
        .position(|r| !r.is_empty() && r.contains(pos))
}
