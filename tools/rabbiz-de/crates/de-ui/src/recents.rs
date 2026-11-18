//! P4 recents (M3-T4): horizontal portrait card carousel with live window
//! snapshots, app label + icon above each card, and an explicit
//! "clear all" pill.
//!
//! `RecentEntry.snapshot` is the last frame the compositor captured for the
//! window (`de-ui` stays data-in/pixels-out); `None` draws a placeholder
//! surface with the app icon — a window that just closed still looks right.
//!
//! [`RecentsModel`] is the pure logic side (focus, dismiss, clear-all) so
//! the gesture wiring in `de-core` stays a thin adapter.

use std::sync::Arc;

use de_api::{Pixels, Point, Rect, Size};
use de_design::tokens::{self, Theme};
use de_paint::Canvas;

use crate::chrome::{self, style, IconEntry, StatusInfo};
use crate::i18n::Strings;

/// Centred card bounds, also used as the navigation morph endpoint.
#[must_use]
pub fn card_rect(size: Size) -> Rect {
    let w = (size.w as f32 * 0.62).round().max(1.0) as i32;
    let h = (size.h as f32 * 0.62).round().max(1.0) as i32;
    Rect::new((size.w - w) / 2, (size.h - h) / 2, w, h)
}

#[must_use]
pub fn card_stride(size: Size) -> i32 {
    card_rect(size).w + (size.w as f32 * 0.13).round() as i32
}

#[must_use]
pub fn card_at(size: Size, index: usize, pos: f32) -> Rect {
    card_rect(size).translate(
        ((index as f32 - pos) * card_stride(size) as f32).round() as i32,
        0,
    )
}

/// Apply the card mask once at capture time, not on every carousel frame.
pub fn round_snapshot(mut px: Pixels, card: Rect) -> Pixels {
    let radius = tokens::RADIUS_CARD * px.size.w as f32 / card.w.max(1) as f32;
    mask_rounded_corners(&mut px, radius);
    px
}

/// One recents card.
#[derive(Debug, Clone)]
pub struct RecentEntry {
    pub id: String,
    pub title: String,
    pub icon: Option<Pixels>,
    /// Last captured window frame (scaled into the card at render).
    pub snapshot: Option<Arc<Pixels>>,
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
        -(self.focus() as i32) * card_stride(size)
    }
}

/// Render the recents screen.
#[must_use]
pub fn render(size: Size, s: &Strings, t: &Theme, status: &StatusInfo, m: &RecentsModel) -> Pixels {
    render_with_backdrop(size, s, t, status, m, None)
}

/// Runtime Recents uses the same frozen Home backdrop as the navigation drag.
#[must_use]
pub fn render_with_backdrop(size: Size, s: &Strings, t: &Theme, status: &StatusInfo,
    m: &RecentsModel, backdrop: Option<&Pixels>) -> Pixels {
    render_at_pos(size, s, t, status, m, backdrop, m.focus as f32, None)
}

/// Render Recents at a continuous fractional card index `pos` (for 1:1 drag + spring snap).
#[must_use]
pub fn render_at_pos(
    size: Size,
    s: &Strings,
    t: &Theme,
    status: &StatusInfo,
    m: &RecentsModel,
    backdrop: Option<&Pixels>,
    pos: f32,
    morphing_card: Option<usize>,
) -> Pixels {
    let mut c = Canvas::filled(size, t.bg);
    if let Some(px) = backdrop {
        c.blit(px, de_api::Point::ZERO);
        let mut veil = t.bg;
        veil[3] = BACKDROP_VEIL_ALPHA;
        c.fill_rect(Rect::new(0, 0, size.w, size.h), veil);
    }
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

    for (i, e) in m.entries.iter().enumerate() {
        let card = card_at(size, i, pos);
        let (x, y) = (card.x, card.y);
        if x + card.w < 0 || x > size.w {
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
            Some(card.w - 44),
        );
        // The compositor draws the transitioning card in its own layer.
        if morphing_card == Some(i) {
            continue;
        }
        if let Some(px) = &e.snapshot {
            c.blit_scaled(px, card, 1.0);
        } else {
            c.fill_rounded_rect(card, tokens::RADIUS_CARD, t.surface2);
            let ie2 = IconEntry {
                id: e.id.clone(),
                label: e.title.clone(),
                icon: e.icon.clone(),
            };
            chrome::draw_app_icon(
                &mut c,
                t,
                &ie2,
                Rect::new(x + (card.w - 96) / 2, y + (card.h - 96) / 2, 96, 96),
            );
        }
    }

    // Clear-all pill (only when more than one card).
    if m.entries.len() > 1 {
        let r = clear_all_rect(size);
        c.fill_rounded_rect(r, tokens::RADIUS_PILL, t.glass);
        chrome::draw_centered_x(
            &mut c,
            s.get("recents.clear_all"),
            &style::label(t.text),
            r.x + r.w / 2,
            r.y + 20,
        );
    }
    chrome::nav_pill(&mut c, t);
    c.into_pixels()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecentsHit {
    Card(usize),
    ClearAll,
}

fn clear_all_rect(size: Size) -> Rect {
    let pill_w = 220;
    Rect::new(
        (size.w - pill_w) / 2,
        size.h - chrome::NAV_H - 84,
        pill_w,
        68,
    )
}

#[must_use]
pub fn hit_test(size: Size, count: usize, index: f32, pos: Point) -> Option<RecentsHit> {
    if count > 1 && clear_all_rect(size).contains(pos) {
        return Some(RecentsHit::ClearAll);
    }
    (0..count)
        .find(|&i| card_at(size, i, index).contains(pos))
        .map(RecentsHit::Card)
}

pub const BACKDROP_VEIL_ALPHA: u8 = 118;

fn mask_rounded_corners(px: &mut de_api::Pixels, radius: f32) {
    let w = px.size.w;
    let h = px.size.h;
    let r = (radius.ceil() as i32).min(w / 2).min(h / 2);
    if r <= 0 {
        return;
    }
    for y in 0..r {
        let dy = radius - y as f32 - 0.5;
        for x in 0..r {
            let dx = radius - x as f32 - 0.5;
            let dist = (dx * dx + dy * dy).sqrt();
            if dist > radius + 0.5 {
                for (cx, cy) in [(x, y), (w - 1 - x, y), (x, h - 1 - y), (w - 1 - x, h - 1 - y)] {
                    let i = ((cy * w + cx) * 4) as usize;
                    px.data[i..i + 4].fill(0);
                }
            } else if dist > radius - 0.5 {
                let alpha = ((radius + 0.5 - dist).clamp(0.0, 1.0) * 255.0).round() as u32;
                for (cx, cy) in [(x, y), (w - 1 - x, y), (x, h - 1 - y), (w - 1 - x, h - 1 - y)] {
                    let i = ((cy * w + cx) * 4) as usize;
                    for c in 0..4 {
                        px.data[i + c] = ((u32::from(px.data[i + c]) * alpha + 127) / 255) as u8;
                    }
                }
            }
        }
    }
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

    #[test]
    fn card_tap_uses_the_in_flight_position_at_both_handset_sizes() {
        for size in [Size::new(720, 1600), Size::new(1080, 2400)] {
            let card = card_at(size, 1, 0.65);
            let tap = Point::new(card.x + card.w / 2, card.y + card.h / 2);
            assert_eq!(hit_test(size, 3, 0.65, tap), Some(RecentsHit::Card(1)));
            assert_eq!(card_at(size, 0, 0.0), card_rect(size));
        }
    }
}
