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
pub fn render_with_backdrop(
    size: Size,
    s: &Strings,
    t: &Theme,
    status: &StatusInfo,
    m: &RecentsModel,
    backdrop: Option<&Pixels>,
) -> Pixels {
    render_at_pos(size, s, t, status, m, backdrop, m.focus as f32, None, None)
}

/// Vertical space reserved above each card for its icon + app title.
pub const CARD_HEADER_H: i32 = 52;

fn draw_card_header(c: &mut Canvas, t: &Theme, e: &RecentEntry, x: i32, y: i32, card_w: i32) {
    let tile = Rect::new(x, y, 36, 36);
    let ie = IconEntry {
        id: e.id.clone(),
        label: e.title.clone(),
        icon: e.icon.clone(),
    };
    chrome::draw_app_icon(c, t, &ie, tile);
    chrome::draw(
        c,
        &e.title,
        &style::label(t.text2),
        x + 44,
        y + 6,
        Some(card_w - 44),
    );
}

fn draw_card_body(c: &mut Canvas, t: &Theme, e: &RecentEntry, card: Rect, alpha: f32) {
    if let Some(px) = &e.snapshot {
        c.blit_scaled(px, card, alpha);
    } else {
        let mut fill = t.surface2;
        fill[3] = (f32::from(fill[3]) * alpha).round() as u8;
        c.fill_rounded_rect(card, tokens::RADIUS_CARD, fill);
        let ie2 = IconEntry {
            id: e.id.clone(),
            label: e.title.clone(),
            icon: e.icon.clone(),
        };
        chrome::draw_app_icon(
            c,
            t,
            &ie2,
            Rect::new(
                card.x + (card.w - 96) / 2,
                card.y + (card.h - 96) / 2,
                96,
                96,
            ),
        );
    }
}

/// Static Recents backdrop (blurred wallpaper/home + veil + status bar + clear-all + nav pill).
/// Cached as a single GPU texture so horizontal card swipes do zero full-screen CPU blits.
#[must_use]
pub fn render_backdrop(
    size: Size,
    s: &Strings,
    t: &Theme,
    status: &StatusInfo,
    backdrop: Option<&Pixels>,
    empty: bool,
    show_clear_all: bool,
) -> Pixels {
    let mut c = Canvas::filled(size, t.bg);
    if let Some(px) = backdrop {
        c.blit(px, de_api::Point::ZERO);
        let mut veil = t.bg;
        veil[3] = BACKDROP_VEIL_ALPHA;
        c.fill_rect(Rect::new(0, 0, size.w, size.h), veil);
    }
    chrome::status_bar(&mut c, t, status);
    if empty {
        chrome::draw_centered(
            &mut c,
            s.get("recents.empty"),
            &style::body(t.text3),
            size.h / 2,
        );
    } else if show_clear_all {
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

/// One pre-rasterized Recents card (`card_size.w × (card_size.h + CARD_HEADER_H)`)
/// containing its icon + title header and scaled snapshot/placeholder.
#[must_use]
pub fn render_card_tile(t: &Theme, e: &RecentEntry, card_size: Size) -> Pixels {
    let mut c = Canvas::new(Size::new(card_size.w, card_size.h + CARD_HEADER_H));
    draw_card_header(&mut c, t, e, 0, 0, card_size.w);
    draw_card_body(
        &mut c,
        t,
        e,
        Rect::new(0, CARD_HEADER_H, card_size.w, card_size.h),
        1.0,
    );
    c.into_pixels()
}

/// Carousel positioned at continuous `pos` (in card-stride units); `morphing_card`
/// omits that card's body while the compositor animates it separately.
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
    lift: Option<CardLift>,
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
        let mut card = card_at(size, i, pos);
        let mut alpha = 1.0;
        if let Some(l) = lift.filter(|l| l.index == i) {
            card.y += l.dy.round() as i32;
            alpha = l.alpha(size);
        }
        let (x, y) = (card.x, card.y);
        if x + card.w < 0 || x > size.w || y + card.h < 0 || alpha <= 0.0 {
            continue;
        }
        draw_card_header(&mut c, t, e, x, y - CARD_HEADER_H, card.w);
        // The compositor draws the transitioning card in its own layer.
        if morphing_card == Some(i) {
            continue;
        }
        draw_card_body(&mut c, t, e, card, alpha);
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

/// A card being swiped up to close its app (Android): it follows the finger
/// (`dy` < 0 is up) and fades as it leaves.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CardLift {
    pub index: usize,
    pub dy: f32,
}

impl CardLift {
    /// Fully opaque at rest, transparent once lifted 60 % of the screen.
    #[must_use]
    pub fn alpha(&self, size: Size) -> f32 {
        1.0 - (-self.dy / (size.h as f32 * 0.6).max(1.0)).clamp(0.0, 1.0)
    }
}

/// Whether a released card swipe closes the app: far enough up, or flung up
/// (px, px/s; up is negative). A fling back down never closes.
#[must_use]
pub fn dismiss_on_release(size: Size, dy: f32, vy: f32) -> bool {
    vy < 600.0 && (-dy >= size.h as f32 * 0.18 || (-dy >= 48.0 && vy <= -1000.0))
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
                for (cx, cy) in [
                    (x, y),
                    (w - 1 - x, y),
                    (x, h - 1 - y),
                    (w - 1 - x, h - 1 - y),
                ] {
                    let i = ((cy * w + cx) * 4) as usize;
                    px.data[i..i + 4].fill(0);
                }
            } else if dist > radius - 0.5 {
                let alpha = ((radius + 0.5 - dist).clamp(0.0, 1.0) * 255.0).round() as u32;
                for (cx, cy) in [
                    (x, y),
                    (w - 1 - x, y),
                    (x, h - 1 - y),
                    (w - 1 - x, h - 1 - y),
                ] {
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
    fn card_swipe_closes_when_far_or_flung_up() {
        let size = Size::new(720, 1600);
        assert!(dismiss_on_release(size, -300.0, 0.0));
        assert!(dismiss_on_release(size, -80.0, -1500.0));
        assert!(!dismiss_on_release(size, -120.0, 0.0));
        assert!(!dismiss_on_release(size, -400.0, 900.0), "flung back down");
        let lift = CardLift {
            index: 0,
            dy: -1000.0,
        };
        assert!(lift.alpha(size) <= 0.0);
        assert!((CardLift { index: 0, dy: 0.0 }.alpha(size) - 1.0).abs() < f32::EPSILON);
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
