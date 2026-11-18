//! P4 recents (M3-T4): horizontal portrait card carousel with live window
//! snapshots, app label + icon above each card, swipe-up dismissal, and a
//! "clear all" pill.
//!
//! `RecentEntry.snapshot` is the last frame the compositor captured for the
//! window (`de-ui` stays data-in/pixels-out); `None` draws a placeholder
//! surface with the app icon — a window that just closed still looks right.
//!
//! [`RecentsModel`] is the pure logic side (focus, dismiss, clear-all) so
//! the gesture wiring in `de-core` stays a thin adapter.

use de_api::{Pixels, Rect, Size};
use de_design::tokens::{self, Theme};
use de_paint::{wallpaper::cover_scale, Canvas};

use crate::chrome::{self, style, IconEntry, StatusInfo};
use crate::i18n::Strings;

const CARD_W: i32 = 480;
const CARD_H: i32 = 1040;
const CARD_GAP: i32 = 32;

/// One recents card.
#[derive(Debug, Clone)]
pub struct RecentEntry {
    pub id: String,
    pub title: String,
    pub icon: Option<Pixels>,
    /// Last captured window frame (scaled into the card at render).
    pub snapshot: Option<Pixels>,
}

impl RecentEntry {
    /// Placeholder card (no captured frame).
    #[must_use]
    pub fn new(id: &str, title: &str) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            icon: None,
            snapshot: None,
        }
    }
}

/// The list + focus/dismiss state the compositor owns.
#[derive(Debug, Default)]
pub struct RecentsModel {
    entries: Vec<RecentEntry>,
    /// Centered card index (clamped on access).
    focus: usize,
}

impl RecentsModel {
    #[must_use]
    pub fn new(entries: Vec<RecentEntry>) -> Self {
        Self { entries, focus: 0 }
    }

    #[must_use]
    pub fn entries(&self) -> &[RecentEntry] {
        &self.entries
    }

    #[must_use]
    pub fn focus(&self) -> usize {
        self.focus.min(self.entries.len().saturating_sub(1))
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Window mapped → push/mote to front (newest first, dup id replaces).
    pub fn push(&mut self, e: RecentEntry) {
        self.entries.retain(|x| x.id != e.id);
        self.entries.insert(0, e);
        self.focus = 0;
    }

    /// Card dismissed by index (swipe-up). Focus settles left.
    pub fn dismiss(&mut self, index: usize) -> Option<RecentEntry> {
        if index >= self.entries.len() {
            return None;
        }
        let out = self.entries.remove(index);
        self.focus = self.focus.min(self.entries.len().saturating_sub(1));
        Some(out)
    }

    /// Clear-all pill: everything goes.
    pub fn dismiss_all(&mut self) {
        self.entries.clear();
        self.focus = 0;
    }

    /// Step focus (−1 left / +1 right), clamped at the ends.
    pub fn step(&mut self, delta: i32) {
        let n = self.entries.len().saturating_sub(1) as i32;
        self.focus = (self.focus as i32 + delta).clamp(0, n.max(0)) as usize;
    }

    /// Horizontal pixel offset that centers the focused card.
    #[must_use]
    pub fn focus_offset(&self, size: Size) -> i32 {
        let card_x = |i: i32| (size.w - CARD_W) / 2 + i * (CARD_W + CARD_GAP);
        card_x(0) - card_x(self.focus() as i32)
    }
}

/// Render the recents screen.
#[must_use]
pub fn render(size: Size, s: &Strings, t: &Theme, status: &StatusInfo, m: &RecentsModel) -> Pixels {
    let mut c = Canvas::filled(size, t.bg);
    chrome::status_bar(&mut c, t, status);

    if m.entries.is_empty() {
        chrome::draw_centered(
            &mut c,
            s.get("recents.empty"),
            &style::body(t.text3),
            size.h / 2,
        );
        chrome::nav_pill(&mut c, t);
        return c.into_pixels();
    }

    let y = (size.h - CARD_H) / 2;
    let off = m.focus_offset(size);
    for (i, e) in m.entries.iter().enumerate() {
        let x = (size.w - CARD_W) / 2 + i as i32 * (CARD_W + CARD_GAP) + off;
        if x + CARD_W < 0 || x > size.w {
            continue;
        }
        // Label + icon above the card.
        let tile = Rect::new(x, y - 52, 36, 36);
        let ie = IconEntry {
            id: e.id.clone(),
            label: e.title.clone(),
            icon: e.icon.clone(),
        };
        chrome::draw_app_icon(&mut c, t, &ie, tile);
        chrome::draw(
            &mut c,
            &e.title,
            &style::label(t.text2),
            x + 44,
            y - 46,
            Some(CARD_W - 44),
        );
        // Card body: snapshot cover-scaled, or a placeholder tile.
        c.fill_rounded_rect(
            Rect::new(x, y, CARD_W, CARD_H),
            tokens::RADIUS_CARD,
            t.surface2,
        );
        if let Some(px) = &e.snapshot {
            let inner = Rect::new(x + 4, y + 4, CARD_W - 8, CARD_H - 8);
            let scaled = cover_scale(px, inner.size());
            // Round the snapshot's corners by masking through a tile
            // canvas: draw the image, then punch rounded corners via a
            // clipped blit into a temp card.
            let mut card = Canvas::new(inner.size());
            card.blit(&scaled, de_api::Point::ZERO);
            let card_px = card.into_pixels();
            c.blit_region(
                &card_px,
                Rect::from_origin_size(de_api::Point::ZERO, inner.size()),
                inner.origin(),
            );
        } else {
            let ie2 = IconEntry {
                id: e.id.clone(),
                label: e.title.clone(),
                icon: e.icon.clone(),
            };
            chrome::draw_app_icon(
                &mut c,
                t,
                &ie2,
                Rect::new(x + (CARD_W - 96) / 2, y + (CARD_H - 96) / 2, 96, 96),
            );
        }
    }

    // Clear-all pill (only when more than one card).
    if m.entries.len() > 1 {
        let pill_w = 200;
        let r = Rect::new(
            (size.w - pill_w) / 2,
            size.h - chrome::NAV_H - 72,
            pill_w,
            52,
        );
        c.fill_rounded_rect(r, tokens::RADIUS_PILL, t.glass);
        chrome::draw_centered_x(
            &mut c,
            s.get("recents.clear_all"),
            &style::label(t.text),
            r.x + pill_w / 2,
            r.y + 12,
        );
    }
    chrome::nav_pill(&mut c, t);
    c.into_pixels()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(n: usize) -> RecentsModel {
        RecentsModel::new(
            (0..n)
                .map(|i| RecentEntry::new(&format!("a{i}"), "x"))
                .collect(),
        )
    }

    #[test]
    fn push_dedupes_and_fronts() {
        let mut m = model(2);
        m.push(RecentEntry::new("a1", "x"));
        assert_eq!(m.entries.len(), 2);
        assert_eq!(m.entries[0].id, "a1");
    }

    #[test]
    fn dismiss_keeps_focus_in_range() {
        let mut m = model(3);
        m.step(2);
        assert_eq!(m.focus(), 2);
        m.dismiss(2);
        assert_eq!(m.focus(), 1);
        m.dismiss(0);
        m.dismiss(0);
        assert!(m.is_empty());
        assert_eq!(m.focus(), 0);
        assert!(m.dismiss(0).is_none());
    }

    #[test]
    fn step_clamps_at_ends() {
        let mut m = model(2);
        m.step(-1);
        assert_eq!(m.focus(), 0);
        m.step(5);
        assert_eq!(m.focus(), 1);
    }
}
