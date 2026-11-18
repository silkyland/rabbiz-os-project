//! P3 app drawer (M3-T3): search field, A–Z grouped icon list with section
//! headers, right-edge alphabet scrub rail, and a scroll offset the
//! compositor drives (`scroll_px`, finger-tracked).
//!
//! Entries arrive already sorted (catalog order); `IconEntry` carries real
//! icon pixels when `de-apps` resolved them.

use de_api::{Pixels, Rect, Size};
use de_design::tokens::{self, Theme};
use de_paint::{text, Canvas};

use crate::chrome::{self, style, IconEntry, StatusInfo};
use crate::i18n::Strings;

const ROW_H: i32 = 88;
const HEADER_H: i32 = 40;
const RAIL_W: i32 = 36;

/// Section header group: label + which entries follow (catalog order).
#[must_use]
pub fn sections(apps: &[IconEntry]) -> Vec<(String, usize)> {
    let mut out: Vec<(String, usize)> = Vec::new();
    let mut last = String::new();
    for (i, e) in apps.iter().enumerate() {
        let key = e.label.chars().next().map_or_else(
            || "#".into(),
            |c| {
                if c.is_ascii_alphabetic() {
                    c.to_ascii_uppercase().to_string()
                } else {
                    "#".into()
                }
            },
        );
        if key != last {
            last.clone_from(&key);
            out.push((key, i));
        }
    }
    out
}

/// Render the drawer. `scroll_px` shifts the list up (0 = top).
#[must_use]
pub fn render(
    size: Size,
    s: &Strings,
    t: &Theme,
    status: &StatusInfo,
    apps: &[IconEntry],
    scroll_px: i32,
) -> Pixels {
    let mut c = Canvas::filled(size, t.canvas);
    chrome::status_bar(&mut c, t, status);

    // Search field.
    let sf = Rect::new(
        chrome::MARGIN,
        chrome::STATUS_H + 16,
        chrome::content_w(size),
        56,
    );
    c.fill_rounded_rect(sf, tokens::RADIUS_PILL, t.surface2);
    chrome::draw_ink_centered(
        &mut c,
        "\u{e8b6}",
        &text::LabelStyle::icon(24.0, t.text3),
        Rect::new(sf.x + 16, sf.y, 40, sf.h),
    );
    chrome::draw(
        &mut c,
        s.get("common.search"),
        &style::label(t.text3),
        sf.x + 60,
        sf.y + 15,
        None,
    );

    // Scrollable region: below the search field, above the nav pill.
    let view = Rect::new(
        0,
        sf.y + sf.h + 12,
        size.w,
        size.h - (sf.y + sf.h + 12) - chrome::NAV_H - 8,
    );
    let list = Rect::new(
        chrome::MARGIN,
        view.y,
        size.w - chrome::MARGIN - RAIL_W,
        view.h,
    );

    // The list is laid out at full height into its own canvas, then the
    // visible window is blitted (correct partial-row clipping).
    let total = layout_height(apps);
    if total > 0 {
        let mut lc = Canvas::new(Size::new(list.w, total));
        let mut y = 0;
        for (key, start) in sections(apps) {
            chrome::draw(&mut lc, &key, &style::caption(t.text3), 4, y + 8, None);
            y += HEADER_H;
            for e in &apps[start..next_start(&sections(apps), &key, apps.len())] {
                let r = Rect::new(4, y + 12, 56, 56);
                chrome::draw_app_icon(&mut lc, t, e, r);
                chrome::draw(
                    &mut lc,
                    &e.label,
                    &style::body(t.text),
                    76,
                    y + 23,
                    Some(list.w - 76 - 16),
                );
                y += ROW_H;
                // Row divider (not after the section's last row).
                lc.fill_rect(Rect::new(76, y - 1, list.w - 76 - 4, 1), t.line);
            }
        }
        let px = lc.into_pixels();
        let region = Rect::new(
            0,
            scroll_px.max(0).min((total - view.h).max(0)),
            list.w,
            view.h.min(total),
        );
        c.blit_region(&px, region, list.origin());
    }

    // A–Z rail on the right edge (inset clear of the 24 px EdgeRight strip).
    let mut ry = view.y + 8;
    let letters = rail_letters(apps);
    for ch in letters {
        if ry > size.h - 80 {
            break;
        }
        chrome::draw(&mut c, &ch, &style::caption(t.text3), size.w - 32, ry, None);
        ry += 30;
    }
    chrome::nav_pill(&mut c, t);
    c.into_pixels()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DrawerHit {
    Search,
    App(String),
    Letter(String),
}

/// Hit-test the drawer search bar, A–Z rail, and scrolled app rows.
#[must_use]
pub fn hit_test(size: Size, apps: &[IconEntry], scroll_px: i32, pos: de_api::Point) -> Option<DrawerHit> {
    let sf = Rect::new(
        chrome::MARGIN,
        chrome::STATUS_H + 12,
        size.w - chrome::MARGIN * 2,
        56,
    );
    if sf.contains(pos) {
        return Some(DrawerHit::Search);
    }
    let view = Rect::new(
        chrome::MARGIN,
        sf.y + sf.h + 16,
        size.w - chrome::MARGIN - 40,
        size.h - (sf.y + sf.h + 16) - chrome::NAV_H,
    );
    if pos.x >= size.w - 44 && pos.y >= view.y {
        let mut ry = view.y + 8;
        for ch in rail_letters(apps) {
            if ry > size.h - 80 {
                break;
            }
            if pos.y >= ry - 4 && pos.y < ry + 26 {
                return Some(DrawerHit::Letter(ch));
            }
            ry += 30;
        }
    }
    if !view.contains(pos) {
        return None;
    }
    let total = layout_height(apps);
    let clamped_scroll = scroll_px.max(0).min((total - view.h).max(0));
    let local_y = pos.y - view.y + clamped_scroll;
    let secs = sections(apps);
    let mut y = 0;
    for (k, start) in &secs {
        y += HEADER_H;
        let end = next_start(&secs, k, apps.len());
        for app in &apps[*start..end] {
            if local_y >= y && local_y < y + ROW_H {
                return Some(DrawerHit::App(app.id.clone()));
            }
            y += ROW_H;
        }
    }
    None
}

/// Scroll offset (px) that brings section `letter` to the top of the drawer.
#[must_use]
pub fn scroll_for_letter(apps: &[IconEntry], letter: &str) -> Option<i32> {
    let secs = sections(apps);
    let mut y = 0;
    for (k, start) in &secs {
        if k == letter {
            return Some(y);
        }
        let end = next_start(&secs, k, apps.len());
        y += HEADER_H + (end - start) as i32 * ROW_H;
    }
    None
}

/// Entries of the section that starts at `key` (index lookup).
fn next_start(secs: &[(String, usize)], key: &str, len: usize) -> usize {
    let mut seen = false;
    for (k, i) in secs {
        if seen {
            return *i;
        }
        if k == key {
            seen = true;
        }
    }
    len
}

/// Total scrollable height of the list.
#[must_use]
pub fn layout_height(apps: &[IconEntry]) -> i32 {
    sections(apps).len() as i32 * HEADER_H + apps.len() as i32 * ROW_H
}

/// Letters present on the rail (A–Z always + `#` when non-latin exists).
fn rail_letters(apps: &[IconEntry]) -> Vec<String> {
    let mut v: Vec<String> = "ABCDEFGHIJKLMNOPQRSTUVWXYZ"
        .chars()
        .map(|c| c.to_string())
        .collect();
    if apps.iter().any(|e| {
        e.label
            .chars()
            .next()
            .is_some_and(|c| !c.is_ascii_alphabetic())
    }) {
        v.push("#".into());
    }
    v
}

/// Scroll offset (px) that brings section `letter`'s first row to the top
/// of the view — used by the rail scrub gesture.
#[must_use]
pub fn scroll_to_letter(apps: &[IconEntry], letter: &str) -> i32 {
    let mut y = 0;
    for (key, start) in sections(apps) {
        if key == letter {
            return y;
        }
        y += HEADER_H + (next_start(&sections(apps), &key, apps.len()) - start) as i32 * ROW_H;
    }
    y
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apps(labels: &[&str]) -> Vec<IconEntry> {
        labels.iter().map(|l| IconEntry::new(l, l)).collect()
    }

    #[test]
    fn groups_by_first_letter() {
        let a = apps(&["Apple", "avocado", "Banana", "กา", "ข้าว"]);
        let s = sections(&a);
        assert_eq!(s[0], ("A".to_string(), 0));
        assert_eq!(s[1], ("B".to_string(), 2));
        assert_eq!(s[2], ("#".to_string(), 3));
    }

    #[test]
    fn scroll_targets() {
        let a = apps(&["A1", "A2", "B1", "B2", "B3"]);
        assert_eq!(scroll_to_letter(&a, "A"), 0);
        assert_eq!(scroll_to_letter(&a, "B"), HEADER_H + 2 * ROW_H);
        assert!(scroll_to_letter(&a, "Z") >= HEADER_H + 3 * ROW_H);
    }

    #[test]
    fn height_is_sections_plus_rows() {
        let a = apps(&["A", "B", "C"]);
        assert_eq!(layout_height(&a), 3 * HEADER_H + 3 * ROW_H);
    }
}
