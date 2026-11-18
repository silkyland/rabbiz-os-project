//! Notification panel (pulled from the top-left) and Quick actions (pulled
//! from the top-right): full-screen frosted overlays that slide over the
//! home screen or an app. Matte glass over a pre-blurred backdrop, no
//! shadows (`de_paint::glass`). Geometry is pure and shared by `render_*` and
//! `*_hit`, so a tap can never disagree with what is drawn.

use de_api::{Pixels, Point, Rect, Size};
use de_design::tokens::Theme;
use de_paint::{glass, text, Canvas};

use crate::chrome::{self, style};
use crate::i18n::Strings;

const SIDE: i32 = 28;
const CARD_RADIUS: f32 = 44.0;
/// A plum dim over the blurred backdrop, so milky glass cards stand out from
/// it on any wallpaper (the light sand one included).
const PANEL_VEIL: [u8; 4] = [0x20, 0x14, 0x28, 0x72];
const CARD_TINT: [u8; 4] = [0xFF, 0xF8, 0xF0, 0x8C];
const CARD_TINT_ON: [u8; 4] = [0xFF, 0xF8, 0xF0, 0xDC];
/// Text straight on the dimmed backdrop.
const ON_DIM: [u8; 4] = [0xFF, 0xF8, 0xF0, 0xFF];
const ON_DIM_2: [u8; 4] = [0xFF, 0xF8, 0xF0, 0xC8];

// --- notifications ---------------------------------------------------------

/// One notification as the panel shows it.
#[derive(Debug, Clone)]
pub struct Notification {
    pub id: u32,
    pub app: String,
    pub title: String,
    pub body: String,
    /// "now", "26 min", "13:21": formatted by the compositor.
    pub when: String,
    pub icon: Option<Pixels>,
}

/// What a tap on the notification panel hit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotifHit {
    /// Index into the list.
    Card(usize),
    ClearAll,
}

const NOTIF_TOP: i32 = 210;
const NOTIF_CARD_H: i32 = 148;
const NOTIF_GAP: i32 = 14;

fn clear_all_rect(size: Size) -> Rect {
    Rect::new(size.w - SIDE - 64, 108, 64, 64)
}

/// Card rectangles for `count` notifications (stops at the screen bottom).
#[must_use]
pub fn notif_rects(size: Size, count: usize) -> Vec<Rect> {
    let mut out = Vec::new();
    let mut y = NOTIF_TOP;
    for _ in 0..count {
        if y + NOTIF_CARD_H > size.h - 120 {
            break;
        }
        out.push(Rect::new(SIDE, y, size.w - SIDE * 2, NOTIF_CARD_H));
        y += NOTIF_CARD_H + NOTIF_GAP;
    }
    out
}

#[must_use]
pub fn notif_hit(size: Size, pos: Point, count: usize) -> Option<NotifHit> {
    if count > 0 && clear_all_rect(size).contains(pos) {
        return Some(NotifHit::ClearAll);
    }
    notif_rects(size, count)
        .iter()
        .position(|r| r.contains(pos))
        .map(NotifHit::Card)
}

fn backdrop_with_veil(size: Size, backdrop: &Pixels) -> Canvas {
    let mut c = Canvas::new(size);
    c.blit(backdrop, Point::ZERO);
    c.fill_rect(Rect::new(0, 0, size.w, size.h), PANEL_VEIL);
    c
}

fn header(c: &mut Canvas, t: &Theme, clock: &str, date: &str) {
    let clock_style = text::LabelStyle {
        size: 76.0,
        line_height: 84.0,
        weight: 600,
        tnum: true,
        color: ON_DIM,
        ..text::LabelStyle::default()
    };
    chrome::draw(c, clock, &clock_style, SIDE + 4, 96, None);
    let _ = t;
    chrome::draw(
        c,
        date,
        &style::label(ON_DIM_2),
        SIDE + 8,
        184,
        None,
    );
}

/// The notification panel as a full-screen frame over `backdrop` (already blurred).
#[must_use]
pub fn render_notifications(
    size: Size,
    s: &Strings,
    t: &Theme,
    backdrop: &Pixels,
    clock: &str,
    date: &str,
    items: &[Notification],
) -> Pixels {
    let mut c = backdrop_with_veil(size, backdrop);
    header(&mut c, t, clock, date);
    if !items.is_empty() {
        let r = clear_all_rect(size);
        glass::frost(&mut c, backdrop, r, 32.0, CARD_TINT);
        chrome::draw_ink_centered(&mut c, "\u{e0b8}", &text::LabelStyle::icon(30.0, t.text), r);
    }
    if items.is_empty() {
        chrome::draw_centered(&mut c, s.get("panel.no_notifications"), &style::body(ON_DIM_2), size.h / 2 - 20);
    }
    for (n, r) in items.iter().zip(notif_rects(size, items.len())) {
        glass::frost(&mut c, backdrop, r, CARD_RADIUS, CARD_TINT);
        let icon = Rect::new(r.x + 24, r.y + 26, 72, 72);
        if let Some(px) = &n.icon {
            c.blit_scaled(px, icon, 1.0);
        } else {
            c.fill_rounded_rect(icon, 999.0, t.glass_hi);
            chrome::draw_ink_centered(
                &mut c,
                &n.app
                    .chars()
                    .next()
                    .map_or_else(String::new, |ch| ch.to_uppercase().collect()),
                &style::h2(t.text),
                icon,
            );
        }
        let tx = r.x + 120;
        let when = text::label(&n.when, style::caption(t.text3), None);
        let when_w = when.as_ref().map_or(0, |p| p.width() as i32);
        chrome::draw(&mut c, &n.title, &style::h2(t.text), tx, r.y + 22, Some(r.w - 120 - when_w - 48));
        chrome::draw(&mut c, &n.when, &style::caption(t.text3), r.x + r.w - 24 - when_w, r.y + 30, None);
        chrome::draw(&mut c, &n.body, &style::body(t.text2), tx, r.y + 72, Some(r.w - 120 - 24));
    }
    chrome::nav_pill(&mut c, t);
    c.into_pixels()
}

// --- quick actions -----------------------------------------------------------

/// A toggle tile.
#[derive(Debug, Clone)]
pub struct QuickTile {
    pub id: &'static str,
    pub label: String,
    /// Material Icons codepoint.
    pub glyph: &'static str,
    pub on: bool,
    /// Whether anything can change it on this device; unavailable tiles are dimmed.
    pub available: bool,
    /// Second line on the wide pills ("On", a network name…).
    pub detail: String,
}

/// Everything the Quick panel shows. The first four tiles are the wide pills,
/// the rest the round toggles.
#[derive(Debug, Clone)]
pub struct QuickModel {
    pub tiles: Vec<QuickTile>,
    /// 0.0 – 1.0.
    pub brightness: f32,
    pub volume: f32,
}

pub const WIDE_TILES: usize = 4;
pub const ROUND_TILES: usize = 8;

/// What a touch on the Quick panel hit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum QuickHit {
    Tile(usize),
    /// A slider and the position along it, 0.0 – 1.0.
    Brightness(f32),
    Volume(f32),
    Settings,
    Power,
}

/// Pure geometry of the Quick panel.
#[derive(Debug, Clone)]
pub struct QuickLayout {
    pub settings: Rect,
    pub power: Rect,
    pub wide: [Rect; WIDE_TILES],
    pub sliders: [Rect; 2],
    pub round: [Rect; ROUND_TILES],
}

#[must_use]
pub fn quick_layout(size: Size) -> QuickLayout {
    let inner = size.w - SIDE * 2;
    let gap = 16;
    let half = (inner - gap) / 2;
    let wide = |i: i32| {
        Rect::new(
            SIDE + (i % 2) * (half + gap),
            216 + (i / 2) * (116 + gap),
            half,
            116,
        )
    };
    let sliders_y = 216 + 2 * (116 + gap) + 24;
    let col = (inner - 3 * gap) / 4;
    let round = |i: i32| {
        Rect::new(
            SIDE + (i % 4) * (col + gap),
            sliders_y + 2 * (96 + gap) + 40 + (i / 4) * (col + 52 + gap),
            col,
            col,
        )
    };
    QuickLayout {
        settings: Rect::new(size.w - SIDE - 64 * 2 - 16, 108, 64, 64),
        power: Rect::new(size.w - SIDE - 64, 108, 64, 64),
        wide: [wide(0), wide(1), wide(2), wide(3)],
        sliders: [
            Rect::new(SIDE, sliders_y, inner, 96),
            Rect::new(SIDE, sliders_y + 96 + gap, inner, 96),
        ],
        round: [
            round(0),
            round(1),
            round(2),
            round(3),
            round(4),
            round(5),
            round(6),
            round(7),
        ],
    }
}

#[must_use]
pub fn quick_hit(size: Size, pos: Point) -> Option<QuickHit> {
    let l = quick_layout(size);
    if l.settings.contains(pos) {
        return Some(QuickHit::Settings);
    }
    if l.power.contains(pos) {
        return Some(QuickHit::Power);
    }
    for (i, r) in l.wide.iter().enumerate() {
        if r.contains(pos) {
            return Some(QuickHit::Tile(i));
        }
    }
    for (i, r) in l.round.iter().enumerate() {
        // The label under the circle belongs to the tile.
        if Rect::new(r.x, r.y, r.w, r.h + 52).contains(pos) {
            return Some(QuickHit::Tile(WIDE_TILES + i));
        }
    }
    let frac = |r: &Rect| ((pos.x - r.x) as f32 / r.w as f32).clamp(0.0, 1.0);
    if l.sliders[0].contains(pos) {
        return Some(QuickHit::Brightness(frac(&l.sliders[0])));
    }
    if l.sliders[1].contains(pos) {
        return Some(QuickHit::Volume(frac(&l.sliders[1])));
    }
    None
}

fn slider(c: &mut Canvas, backdrop: &Pixels, t: &Theme, r: Rect, glyph: &str, frac: f32) {
    glass::frost(c, backdrop, r, 48.0, CARD_TINT);
    // Fill: a lighter glass capsule from the left edge to the knob.
    let fill_w = (r.w as f32 * frac.clamp(0.0, 1.0)) as i32;
    if fill_w > 0 {
        let fr = Rect::new(r.x, r.y, fill_w.max(r.h / 2), r.h);
        c.fill_rounded_rect(fr, 48.0, CARD_TINT_ON);
    }
    chrome::draw_ink_centered(
        c,
        glyph,
        &text::LabelStyle::icon(34.0, t.accent),
        Rect::new(r.x + 14, r.y, 68, r.h),
    );
}

#[must_use]
pub fn render_quick(
    size: Size,
    s: &Strings,
    t: &Theme,
    backdrop: &Pixels,
    clock: &str,
    date: &str,
    m: &QuickModel,
) -> Pixels {
    let mut c = backdrop_with_veil(size, backdrop);
    header(&mut c, t, clock, date);
    let l = quick_layout(size);
    for (r, glyph) in [(l.settings, "\u{e8b8}"), (l.power, "\u{e8ac}")] {
        glass::frost(&mut c, backdrop, r, 32.0, CARD_TINT);
        chrome::draw_ink_centered(&mut c, glyph, &text::LabelStyle::icon(30.0, t.text), r);
    }
    for (i, r) in l.wide.iter().enumerate() {
        let Some(tile) = m.tiles.get(i) else { continue };
        glass::frost(&mut c, backdrop, *r, 58.0, if tile.on { CARD_TINT_ON } else { CARD_TINT });
        let disc = Rect::new(r.x + 18, r.y + 18, 80, 80);
        c.fill_rounded_rect(disc, 999.0, if tile.on { t.accent } else { t.glass_hi });
        let glyph_col = if tile.on { t.text_on_accent } else if tile.available { t.text } else { t.text3 };
        chrome::draw_ink_centered(&mut c, tile.glyph, &text::LabelStyle::icon(38.0, glyph_col), disc);
        let col = if tile.available { t.text } else { t.text3 };
        chrome::draw(&mut c, &tile.label, &style::body(col), r.x + 112, r.y + 22, Some(r.w - 112 - 16));
        let detail = if tile.available { &tile.detail } else { s.get("qs.unavailable") };
        chrome::draw(&mut c, detail, &style::caption(t.text3), r.x + 112, r.y + 64, Some(r.w - 112 - 16));
    }
    slider(&mut c, backdrop, t, l.sliders[0], "\u{e518}", m.brightness);
    slider(&mut c, backdrop, t, l.sliders[1], "\u{e050}", m.volume);
    for (i, r) in l.round.iter().enumerate() {
        let Some(tile) = m.tiles.get(WIDE_TILES + i) else { continue };
        glass::frost(&mut c, backdrop, *r, r.w as f32 / 2.0, if tile.on { CARD_TINT_ON } else { CARD_TINT });
        let glyph_col = if tile.on { t.accent } else if tile.available { t.text } else { t.text3 };
        chrome::draw_ink_centered(&mut c, tile.glyph, &text::LabelStyle::icon(40.0, glyph_col), *r);
        chrome::draw_centered_x(&mut c, &tile.label, &style::caption(ON_DIM), r.x + r.w / 2, r.y + r.h + 10);
    }
    chrome::nav_pill(&mut c, t);
    c.into_pixels()
}

#[cfg(test)]
mod tests {
    use super::*;
    use de_design::tokens;

    const SIZE: Size = Size { w: 720, h: 1600 };

    fn model() -> QuickModel {
        let tile = |id, glyph, on| QuickTile {
            id,
            label: id.to_string(),
            glyph,
            on,
            available: true,
            detail: "On".into(),
        };
        QuickModel {
            tiles: (0..12).map(|i| tile("x", "\u{e63e}", i % 2 == 0)).collect(),
            brightness: 0.6,
            volume: 0.3,
        }
    }

    fn backdrop() -> Pixels {
        Canvas::filled(SIZE, [200, 150, 120, 255]).into_pixels()
    }

    #[test]
    fn quick_layout_fits_the_screen_without_overlap() {
        let l = quick_layout(SIZE);
        let all: Vec<Rect> = [l.settings, l.power]
            .into_iter()
            .chain(l.wide)
            .chain(l.sliders)
            .chain(l.round.iter().map(|r| Rect::new(r.x, r.y, r.w, r.h + 52)))
            .collect();
        let bounds = Rect::new(0, 0, SIZE.w, SIZE.h - 40);
        for (i, a) in all.iter().enumerate() {
            assert!(bounds.contains(Point::new(a.x, a.y)) && bounds.contains(Point::new(a.x + a.w - 1, a.y + a.h - 1)), "{a:?} off screen");
            for b in &all[i + 1..] {
                assert!(a.intersect(*b).is_none(), "{a:?} overlaps {b:?}");
            }
        }
    }

    #[test]
    fn quick_hits_map_to_what_is_drawn() {
        let l = quick_layout(SIZE);
        let c = |r: Rect| Point::new(r.x + r.w / 2, r.y + r.h / 2);
        assert_eq!(quick_hit(SIZE, c(l.wide[2])), Some(QuickHit::Tile(2)));
        assert_eq!(quick_hit(SIZE, c(l.round[5])), Some(QuickHit::Tile(WIDE_TILES + 5)));
        assert_eq!(quick_hit(SIZE, c(l.power)), Some(QuickHit::Power));
        match quick_hit(SIZE, Point::new(l.sliders[0].x + l.sliders[0].w / 4, l.sliders[0].y + 40)) {
            Some(QuickHit::Brightness(f)) => assert!((f - 0.25).abs() < 0.01),
            other => panic!("{other:?}"),
        }
        assert!(matches!(quick_hit(SIZE, c(l.sliders[1])), Some(QuickHit::Volume(_))));
        assert_eq!(quick_hit(SIZE, Point::new(2, 800)), None);
    }

    #[test]
    fn notification_cards_stack_and_hit() {
        let r = notif_rects(SIZE, 3);
        assert_eq!(r.len(), 3);
        assert!(r[1].y > r[0].y + r[0].h);
        assert_eq!(notif_hit(SIZE, Point::new(100, r[1].y + 10), 3), Some(NotifHit::Card(1)));
        assert_eq!(notif_hit(SIZE, clear_all_rect(SIZE).origin(), 3), Some(NotifHit::ClearAll));
        assert_eq!(notif_hit(SIZE, clear_all_rect(SIZE).origin(), 0), None);
        assert!(notif_rects(SIZE, 50).len() < 50, "list stops above the bottom");
    }

    #[test]
    fn panels_render_full_screen() {
        let s = Strings::default();
        let t = tokens::DEFAULT;
        let q = render_quick(SIZE, &s, &t, &backdrop(), "09:41", "Mon, 9 Jun", &model());
        assert_eq!(q.size, SIZE);
        let items = vec![Notification {
            id: 1,
            app: "Chats".into(),
            title: "Alex".into(),
            body: "See you at 6".into(),
            when: "now".into(),
            icon: None,
        }];
        let n = render_notifications(SIZE, &s, &t, &backdrop(), "09:41", "Mon, 9 Jun", &items);
        assert_eq!(n.size, SIZE);
        let empty = render_notifications(SIZE, &s, &t, &backdrop(), "09:41", "Mon, 9 Jun", &[]);
        assert_ne!(n.data, empty.data);
    }
}
