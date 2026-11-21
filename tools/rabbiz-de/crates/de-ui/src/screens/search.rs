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
use crate::interaction::universal_search::{self, SearchResult};

const FIELD_Y: i32 = 96;
const FIELD_H: i32 = 72;
const ROWS_GAP: i32 = 24;
const CALC_PILL_GAP_TOP: i32 = 16;
const CALC_PILL_H: i32 = 64;
const CALC_PILL_GAP_BOTTOM: i32 = 16;

/// Evaluate `query` as an inline arithmetic expression and return the cleanly
/// formatted result string (e.g. `"1250 * 1.07"` → `Some("1337.5")`).
#[must_use]
pub fn inline_calc_result(query: &str) -> Option<String> {
    universal_search::eval_math_expr(query).map(universal_search::format_calc_result)
}

/// Return matching built-in Settings shortcuts for `query` as `(title, section)`.
#[must_use]
pub fn settings_shortcuts(query: &str) -> Vec<(String, &'static str)> {
    universal_search::match_settings(query)
        .into_iter()
        .filter_map(|r| match r {
            SearchResult::SettingJump { title, section } => Some((title, section)),
            _ => None,
        })
        .collect()
}

/// Bounds of the inline calculation result pill when `query` evaluates to a
/// valid arithmetic expression, or `None` otherwise.
#[must_use]
pub fn calc_pill_rect(size: Size, query: &str) -> Option<Rect> {
    inline_calc_result(query).map(|_| {
        Rect::new(
            chrome::MARGIN,
            FIELD_Y + FIELD_H + CALC_PILL_GAP_TOP,
            chrome::content_w(size),
            CALC_PILL_H,
        )
    })
}

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
#[must_use]
pub fn render(size: Size, s: &Strings, t: &Theme, query: &str, rows: &[SearchRow]) -> Pixels {
    render_with_caret(size, s, t, query, rows, !query.is_empty())
}

/// Render the search overlay with explicit caret visibility (for blinking `|`).
#[must_use]
pub fn render_with_caret(
    size: Size,
    s: &Strings,
    t: &Theme,
    query: &str,
    rows: &[SearchRow],
    show_caret: bool,
) -> Pixels {
    let mut c = Canvas::filled(size, t.bg);
    c.fill_rect(Rect::new(0, 0, size.w, size.h), t.scrim);

    // Query field with trailing caret when a query is present.
    let field = Rect::new(chrome::MARGIN, FIELD_Y, chrome::content_w(size), FIELD_H);
    c.fill_rounded_rect(field, tokens::RADIUS_PILL, t.surface2);
    chrome::draw_ink_centered(
        &mut c,
        "\u{e8b6}",
        &text::LabelStyle::icon(26.0, t.text3),
        Rect::new(field.x + 16, field.y, 44, field.h),
    );
    let shown = if query.is_empty() {
        s.get("search.hint")
    } else {
        query
    };
    let col = if query.is_empty() { t.text3 } else { t.text };
    let r = chrome::draw(
        &mut c,
        shown,
        &style::body(col),
        field.x + 64,
        field.y + 20,
        Some(field.w - 88),
    );
    if show_caret {
        let caret_x = if query.is_empty() {
            field.x + 64
        } else {
            r.x + r.w + 2
        };
        c.fill_rect(
            Rect::new(caret_x, field.y + (field.h - 36) / 2, 3, 36),
            t.accent,
        );
    }

    // Inline calculation result pill above grouped rows when `query` is a valid
    // arithmetic expression.
    let calc_result = inline_calc_result(query);
    if let (Some(formatted), Some(pill)) = (calc_result.as_deref(), calc_pill_rect(size, query)) {
        c.fill_rounded_rect(pill, tokens::RADIUS_PILL, t.surface1);
        let label = format!("= {formatted}");
        chrome::draw(
            &mut c,
            &label,
            &style::h2(t.accent),
            pill.x + 24,
            pill.y + (pill.h - 40) / 2,
            Some(pill.w - 48),
        );
    }

    // Grouped rows: kind order of first appearance, fixed kind sequence.
    // Card geometry comes from `row_rects_for_query` so `hit_test_for_query`
    // can't drift.
    let rects = row_rects_for_query(size, query, rows);
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
        chrome::draw(
            &mut c,
            header,
            &style::caption(t.text3),
            chrome::MARGIN + 8,
            first_rect.y - 36,
            None,
        );
        for (i, row) in rows.iter().enumerate() {
            if row.kind != kind || rects[i].is_empty() {
                continue;
            }
            let card = rects[i];
            c.fill_rounded_rect(card, tokens::RADIUS_ROW, t.surface1);
            let icon_r = row_icon(card);
            if row.kind == SearchKind::Action && row.icon.is_none() {
                c.fill_rounded_rect(icon_r, tokens::RADIUS_PILL, t.accent_dim);
                chrome::draw_ink_centered(
                    &mut c,
                    "\u{e8b6}",
                    &text::LabelStyle::icon(26.0, t.text_on_accent),
                    icon_r,
                );
            } else {
                let e = IconEntry {
                    id: row.id.clone(),
                    label: row.title.clone(),
                    icon: row.icon.clone(),
                };
                chrome::draw_app_icon(&mut c, t, &e, icon_r);
            }
            chrome::draw(
                &mut c,
                &row.title,
                &style::body(t.text),
                card.x + 80,
                card.y + 14,
                Some(card.w - 100),
            );
            if !row.subtitle.is_empty() {
                chrome::draw(
                    &mut c,
                    &row.subtitle,
                    &style::caption(t.text3),
                    card.x + 80,
                    card.y + 46,
                    Some(card.w - 100),
                );
            }
        }
    }
    if rows.is_empty() && calc_result.is_none() && !query.is_empty() {
        chrome::draw_centered(
            &mut c,
            s.get("search.none"),
            &style::body(t.text3),
            size.h / 2,
        );
    }
    chrome::nav_pill(&mut c, t);
    c.into_pixels()
}

fn row_icon(card: Rect) -> Rect {
    Rect::new(card.x + 12, card.y + 14, 52, 52)
}

/// Painted icon bounds for a visible search result (without inline calc offset).
#[must_use]
pub fn icon_rect(size: Size, rows: &[SearchRow], index: usize) -> Option<Rect> {
    let row = *row_rects(size, rows).get(index)?;
    (!row.is_empty()).then(|| row_icon(row))
}

/// Painted icon bounds for a visible search result, accounting for any inline
/// calculation result pill rendered for `query`.
#[must_use]
pub fn icon_rect_for_query(
    size: Size,
    query: &str,
    rows: &[SearchRow],
    index: usize,
) -> Option<Rect> {
    let row = *row_rects_for_query(size, query, rows).get(index)?;
    (!row.is_empty()).then(|| row_icon(row))
}

fn row_rects_from_y(size: Size, rows: &[SearchRow], mut y: i32) -> Vec<Rect> {
    let mut rects = vec![Rect::ZERO; rows.len()];
    for kind in [SearchKind::App, SearchKind::Action, SearchKind::Setting] {
        if !rows.iter().any(|r| r.kind == kind) {
            continue;
        }
        y += 36; // group header
        for (i, row) in rows.iter().enumerate() {
            if row.kind != kind {
                continue;
            }
            if y + 80 > size.h - 120 {
                break;
            }
            rects[i] = Rect::new(chrome::MARGIN, y, chrome::content_w(size), 80);
            y += 80 + 12;
        }
        y += 8;
    }
    rects
}

/// Row rects in the order `rows` are painted (grouped, headers skipped) —
/// shared by [`render`] and [`hit_test`] so they can't drift apart.
fn row_rects(size: Size, rows: &[SearchRow]) -> Vec<Rect> {
    row_rects_from_y(size, rows, FIELD_Y + FIELD_H + ROWS_GAP)
}

/// Row rects accounting for any inline calculation result pill rendered for `query`.
fn row_rects_for_query(size: Size, query: &str, rows: &[SearchRow]) -> Vec<Rect> {
    let start_y = match calc_pill_rect(size, query) {
        Some(pill) => pill.y + pill.h + CALC_PILL_GAP_BOTTOM,
        None => FIELD_Y + FIELD_H + ROWS_GAP,
    };
    row_rects_from_y(size, rows, start_y)
}

/// Which result row `pos` tapped (index into `rows`), or `None`.
#[must_use]
pub fn hit_test(size: Size, rows: &[SearchRow], pos: de_api::Point) -> Option<usize> {
    row_rects(size, rows)
        .iter()
        .position(|r| !r.is_empty() && r.contains(pos))
}

/// Which result row `pos` tapped (index into `rows`), accounting for any inline
/// calculation result pill rendered for `query`.
#[must_use]
pub fn hit_test_for_query(
    size: Size,
    query: &str,
    rows: &[SearchRow],
    pos: de_api::Point,
) -> Option<usize> {
    row_rects_for_query(size, query, rows)
        .iter()
        .position(|r| !r.is_empty() && r.contains(pos))
}

#[cfg(test)]
mod tests {
    use super::*;
    use de_api::Point;

    fn sample_rows() -> Vec<SearchRow> {
        vec![
            SearchRow {
                kind: SearchKind::App,
                id: "rabbiz-calc".into(),
                title: "Calculator".into(),
                subtitle: "Utilities".into(),
                icon: None,
            },
            SearchRow {
                kind: SearchKind::Setting,
                id: "wifi".into(),
                title: "Wi-Fi".into(),
                subtitle: "Settings".into(),
                icon: None,
            },
        ]
    }

    #[test]
    fn inline_calc_result_evaluates_math_and_ignores_phone_numbers() {
        assert_eq!(inline_calc_result("1250 * 1.07").as_deref(), Some("1337.5"));
        assert_eq!(inline_calc_result("0812345678"), None);
    }

    #[test]
    fn settings_shortcuts_finds_wifi_jump() {
        let hits = settings_shortcuts("wifi");
        assert!(
            hits.iter()
                .any(|(title, section)| title == "Wi-Fi" && *section == "wifi"),
            "expected Wi-Fi shortcut in {hits:?}"
        );
    }

    #[test]
    fn render_with_math_query_produces_full_buffer_and_shifts_hit_geometry() {
        let size = Size::new(720, 1600);
        let strings = Strings::load(crate::i18n::Locale::En);
        let theme = &tokens::DEFAULT;
        let rows = sample_rows();
        let query = "1250 * 1.07";

        let px = render(size, &strings, theme, query, &rows);
        assert_eq!(px.size, size);
        assert_eq!(px.data.len(), (size.w * size.h * 4) as usize);

        let pill = calc_pill_rect(size, query).expect("math query should produce calc pill");
        assert_eq!(
            hit_test_for_query(size, query, &rows, Point::new(pill.x + 24, pill.y + 24)),
            None,
            "tapping the calc pill must not hit row 0"
        );

        let unshifted = row_rects(size, &rows);
        let shifted = row_rects_for_query(size, query, &rows);
        assert!(
            shifted[0].y > pill.y + pill.h,
            "first app row ({}) must sit below calc pill bottom ({})",
            shifted[0].y,
            pill.y + pill.h
        );
        assert!(
            shifted[0].y > unshifted[0].y,
            "calc pill must shift app rows downward"
        );

        let row0_center = Point::new(
            shifted[0].x + shifted[0].w / 2,
            shifted[0].y + shifted[0].h / 2,
        );
        assert_eq!(hit_test_for_query(size, query, &rows, row0_center), Some(0));

        let row1_center = Point::new(
            shifted[1].x + shifted[1].w / 2,
            shifted[1].y + shifted[1].h / 2,
        );
        assert_eq!(hit_test_for_query(size, query, &rows, row1_center), Some(1));

        let icon0 = icon_rect_for_query(size, query, &rows, 0).expect("row 0 icon rect");
        assert!(icon0.y > pill.y + pill.h);
    }
}
