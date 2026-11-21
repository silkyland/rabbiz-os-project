//! Notification panel (pulled from the top-left) and Quick actions (pulled
//! from the top-right): full-screen frosted overlays that slide over the
//! home screen or an app. Matte glass over a pre-blurred backdrop, no
//! shadows (`de_paint::glass`). Geometry is pure and shared by `render_*` and
//! `*_hit`, so a tap can never disagree with what is drawn.

use de_api::{Pixels, Point, Rect, Size};
use de_design::tokens::{self, Theme};
use de_paint::{glass, text, Canvas};

use crate::chrome::{self, style};
use crate::i18n::Strings;
use crate::media_card::{self, MediaCardModel};
use crate::notification_card::{self, CardContent, Ink};

const SIDE: i32 = 28;
const CARD_RADIUS: f32 = 36.0;
const CARD_TINT: [u8; 4] = [0x22, 0x16, 0x28, 0x64];
/// Now-playing surface: darker than the list cards so cream ink stays legible.
const MEDIA_TINT: [u8; 4] = [0x1E, 0x14, 0x24, 0xC8];
const CARD_TINT_ON: [u8; 4] = [0xFF, 0xF8, 0xF0, 0xD8];
const CARD_TINT_DIM: [u8; 4] = [0x22, 0x16, 0x28, 0x38];
/// Text straight on the dimmed backdrop.
const ON_DIM: [u8; 4] = [0xFF, 0xF8, 0xF0, 0xFF];
const ON_DIM_SUB: [u8; 4] = [0xFF, 0xF8, 0xF0, 0xB8];
const ON_DIM_MUTED: [u8; 4] = [0xFF, 0xF8, 0xF0, 0x66];

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
    /// The now-playing card (route the tap with `media_card::hit_card`).
    Media,
    ClearAll,
}

const NOTIF_TOP: i32 = 210;
const NOTIF_CARD_H: i32 = notification_card::CONTENT_H;
const NOTIF_GAP: i32 = 14;
/// Landscape: width of the left column (clock, media) before the list.
const LAND_LEFT_W: i32 = 600;
/// Bottom reserve for the gesture bar.
const NAV_RESERVE: i32 = 56;

/// Landscape = wider than tall: header and media sit in a left column, the
/// notification list takes the rest.
#[must_use]
pub fn is_landscape(size: Size) -> bool {
    size.w > size.h
}

fn clear_all_rect(size: Size) -> Rect {
    if is_landscape(size) {
        Rect::new(SIDE + LAND_LEFT_W - 96, 82, 96, 96)
    } else {
        Rect::new(size.w - SIDE - 96, 82, 96, 96)
    }
}

/// Where everything sits in the notification panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotifLayout {
    /// Now-playing card, when a player is active.
    pub media: Option<Rect>,
    /// Notification cards, newest first (stops where the list runs out of room).
    pub cards: Vec<Rect>,
    /// Column the empty-state message is centred in.
    pub list: Rect,
}

/// Geometry of the notification panel for `count` notifications.
#[must_use]
pub fn notif_layout(size: Size, count: usize, has_media: bool) -> NotifLayout {
    let bottom = size.h - NAV_RESERVE;
    let (col_x, col_w, mut y, media_col) = if is_landscape(size) {
        let x = SIDE + LAND_LEFT_W + SIDE;
        (x, size.w - x - SIDE, tokens::STATUS_BAR_HEIGHT + 16, true)
    } else {
        (SIDE, size.w - SIDE * 2, NOTIF_TOP, false)
    };
    let mut media = None;
    if has_media {
        if media_col {
            media = Some(Rect::new(SIDE, NOTIF_TOP, LAND_LEFT_W, media_card::CARD_HEIGHT));
        } else {
            media = Some(Rect::new(col_x, y, col_w, media_card::CARD_HEIGHT));
            y += media_card::CARD_HEIGHT + NOTIF_GAP;
        }
    }
    let list = Rect::new(col_x, y, col_w, (bottom - y).max(0));
    let mut cards = Vec::new();
    for _ in 0..count {
        if y + NOTIF_CARD_H > bottom {
            break;
        }
        cards.push(Rect::new(col_x, y, col_w, NOTIF_CARD_H));
        y += NOTIF_CARD_H + NOTIF_GAP;
    }
    NotifLayout { media, cards, list }
}

/// Card rectangles for `count` notifications (no media card).
#[must_use]
pub fn notif_rects(size: Size, count: usize) -> Vec<Rect> {
    notif_layout(size, count, false).cards
}

#[must_use]
pub fn notif_hit(size: Size, pos: Point, count: usize, has_media: bool) -> Option<NotifHit> {
    if count > 0 && clear_all_rect(size).contains(pos) {
        return Some(NotifHit::ClearAll);
    }
    let l = notif_layout(size, count, has_media);
    if l.media.is_some_and(|r| r.contains(pos)) {
        return Some(NotifHit::Media);
    }
    l.cards
        .iter()
        .position(|r| r.contains(pos))
        .map(NotifHit::Card)
}

/// Now-playing card data for the panel.
#[derive(Clone, Copy)]
pub struct MediaView<'a> {
    pub model: MediaCardModel<'a>,
    pub art: Option<&'a Pixels>,
}

/// The sheet surface itself: the blurred scene under the scrim, baked into
/// the panel texture so the whole panel is one frosted surface. It stays
/// aligned with the scene beneath whenever the panel is settled; mid-pull it
/// shifts with the controls exactly like the frosted cards do.
fn backdrop_with_veil(size: Size, backdrop: &Pixels, tint: [u8; 4]) -> Canvas {
    let mut c = Canvas::new(size);
    c.blit(backdrop, Point::new(0, 0));
    c.fill_rect(Rect::new(0, 0, size.w, size.h), tint);
    c
}

fn frost_card(c: &mut Canvas, backdrop: Option<&Pixels>, rect: Rect, radius: f32, tint: [u8; 4]) {
    if let Some(b) = backdrop {
        glass::frost(c, b, rect, radius, tint);
    } else {
        c.fill_rounded_rect(rect, radius, tint);
    }
}

fn header(c: &mut Canvas, _t: &Theme, clock: &str, date: &str, _backdrop: Option<&Pixels>) {
    let clock_style = text::LabelStyle {
        size: 80.0,
        line_height: 88.0,
        weight: 600,
        tnum: true,
        color: ON_DIM,
        ..text::LabelStyle::default()
    };
    chrome::draw(c, clock, &clock_style, SIDE + 8, 64, Some(320));
    chrome::draw(
        c,
        date,
        &style::label(ON_DIM_SUB),
        SIDE + 10,
        154,
        Some(320),
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
    media: Option<MediaView<'_>>,
) -> Pixels {
    render_notifications_inner(size, s, t, Some(backdrop), clock, date, items, media)
}

/// Notification panel controls only (transparent gaps for GPU-blurred sheet).
#[must_use]
pub fn render_notifications_controls(
    size: Size,
    s: &Strings,
    t: &Theme,
    clock: &str,
    date: &str,
    items: &[Notification],
    media: Option<MediaView<'_>>,
) -> Pixels {
    render_notifications_inner(size, s, t, None, clock, date, items, media)
}

#[allow(clippy::too_many_arguments)]
fn render_notifications_inner(
    size: Size,
    s: &Strings,
    t: &Theme,
    backdrop: Option<&Pixels>,
    clock: &str,
    date: &str,
    items: &[Notification],
    media: Option<MediaView<'_>>,
) -> Pixels {
    let mut c = match backdrop {
        Some(b) => backdrop_with_veil(size, b, t.scrim),
        None => Canvas::new(size),
    };
    header(&mut c, t, clock, date, backdrop);
    if !items.is_empty() {
        let r = clear_all_rect(size);
        frost_card(&mut c, backdrop, r, 48.0, CARD_TINT);
        chrome::draw_ink_centered(&mut c, "\u{e0b8}", &text::LabelStyle::icon(38.0, ON_DIM), r);
    }
    let layout = notif_layout(size, items.len(), media.is_some());
    if let (Some(r), Some(mv)) = (layout.media, media.as_ref()) {
        frost_card(&mut c, backdrop, r, CARD_RADIUS, MEDIA_TINT);
        media_card::render_card_contents(&mut c, r, t, &mv.model, mv.art);
    }
    if items.is_empty() {
        let h = 96;
        let r = Rect::new(
            layout.list.x,
            layout.list.y + (layout.list.h - h) / 2,
            layout.list.w,
            h,
        );
        frost_card(&mut c, backdrop, r, 36.0, CARD_TINT);
        if let Some(px) = text::label(
            s.get("panel.no_notifications"),
            style::body(ON_DIM_SUB),
            None,
        ) {
            let (w, ph) = (px.width() as i32, px.height() as i32);
            c.blit(
                &Pixels::new(Size::new(w, ph), px.data().to_vec()),
                Point::new(r.x + (r.w - w) / 2, r.y + (r.h - ph) / 2),
            );
        }
    }
    // Cream frost cards: ink must stay dark plum even when the shell theme is DARK.
    let light = tokens::LIGHT;
    let ink = Ink {
        title: light.text,
        body: light.text2,
        meta: light.text3,
        badge_fill: [light.accent[0], light.accent[1], light.accent[2], 0x30],
        badge_ink: light.accent_dim,
        accent: light.accent_dim,
    };
    for (n, r) in items.iter().zip(layout.cards.iter().copied()) {
        frost_card(&mut c, backdrop, r, CARD_RADIUS, CARD_TINT_ON);
        notification_card::paint(
            &mut c,
            r,
            &ink,
            &CardContent {
                app: &n.app,
                title: &n.title,
                body: &n.body,
                when: &n.when,
                icon: n.icon.as_ref(),
            },
        );
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
    pub brightness_available: bool,
    pub volume_available: bool,
    pub feedback: Option<String>,
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
        settings: Rect::new(size.w - SIDE - 96 * 2 - 14, 82, 96, 96),
        power: Rect::new(size.w - SIDE - 96, 82, 96, 96),
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

fn slider(
    c: &mut Canvas,
    backdrop: Option<&Pixels>,
    t: &Theme,
    r: Rect,
    glyph: &str,
    frac: f32,
    available: bool,
) {
    frost_card(c, backdrop, r, 48.0, CARD_TINT);
    // Fill: a lighter glass capsule from the left edge to the knob.
    let fill_w = if available {
        (r.w as f32 * frac.clamp(0.0, 1.0)) as i32
    } else {
        0
    };
    if fill_w > 0 {
        let fr = Rect::new(r.x, r.y, fill_w.max(r.h / 2), r.h);
        c.fill_rounded_rect(fr, 48.0, CARD_TINT_ON);
    }
    chrome::draw_ink_centered(
        c,
        glyph,
        &text::LabelStyle::icon(40.0, if available { t.accent } else { ON_DIM_MUTED }),
        Rect::new(r.x + 16, r.y, 72, r.h),
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
    render_quick_inner(size, s, t, Some(backdrop), clock, date, m)
}

/// Quick actions controls only (transparent gaps for GPU-blurred sheet).
#[must_use]
pub fn render_quick_controls(
    size: Size,
    s: &Strings,
    t: &Theme,
    clock: &str,
    date: &str,
    m: &QuickModel,
) -> Pixels {
    render_quick_inner(size, s, t, None, clock, date, m)
}

fn render_quick_inner(
    size: Size,
    s: &Strings,
    t: &Theme,
    backdrop: Option<&Pixels>,
    clock: &str,
    date: &str,
    m: &QuickModel,
) -> Pixels {
    let mut c = match backdrop {
        Some(b) => backdrop_with_veil(size, b, t.scrim),
        None => Canvas::new(size),
    };
    header(&mut c, t, clock, date, backdrop);
    let l = quick_layout(size);
    for (r, glyph) in [(l.settings, "\u{e8b8}"), (l.power, "\u{e8ac}")] {
        frost_card(&mut c, backdrop, r, 48.0, CARD_TINT);
        chrome::draw_ink_centered(&mut c, glyph, &text::LabelStyle::icon(38.0, ON_DIM), r);
    }
    for (i, r) in l.wide.iter().enumerate() {
        let Some(tile) = m.tiles.get(i) else { continue };
        frost_card(
            &mut c,
            backdrop,
            *r,
            58.0,
            if tile.on { CARD_TINT_ON } else { CARD_TINT },
        );
        let disc = Rect::new(r.x + 16, r.y + 16, 84, 84);
        c.fill_rounded_rect(
            disc,
            999.0,
            if tile.on {
                t.accent
            } else {
                [0xFF, 0xF8, 0xF0, 0x28]
            },
        );
        let glyph_col = if tile.on {
            t.text_on_accent
        } else if tile.available {
            ON_DIM
        } else {
            ON_DIM_MUTED
        };
        chrome::draw_ink_centered(
            &mut c,
            tile.glyph,
            &text::LabelStyle::icon(42.0, glyph_col),
            disc,
        );
        let ink = tokens::LIGHT;
        let col = if tile.on {
            ink.text
        } else if tile.available {
            ON_DIM
        } else {
            ON_DIM_MUTED
        };
        let sub_col = if tile.on {
            ink.text2
        } else if tile.available {
            ON_DIM_SUB
        } else {
            ON_DIM_MUTED
        };
        chrome::draw(
            &mut c,
            &tile.label,
            &style::body(col),
            r.x + 112,
            r.y + 22,
            Some(r.w - 112 - 16),
        );
        let detail = if tile.available {
            &tile.detail
        } else {
            s.get("qs.unavailable")
        };
        chrome::draw(
            &mut c,
            detail,
            &style::caption(sub_col),
            r.x + 112,
            r.y + 64,
            Some(r.w - 112 - 16),
        );
    }
    slider(
        &mut c,
        backdrop,
        t,
        l.sliders[0],
        "\u{e518}",
        m.brightness,
        m.brightness_available,
    );
    slider(
        &mut c,
        backdrop,
        t,
        l.sliders[1],
        "\u{e050}",
        m.volume,
        m.volume_available,
    );
    for (i, r) in l.round.iter().enumerate() {
        let Some(tile) = m.tiles.get(WIDE_TILES + i) else {
            continue;
        };
        let circle_bg = if tile.on {
            t.accent
        } else if tile.available {
            CARD_TINT
        } else {
            CARD_TINT_DIM
        };
        if tile.on {
            c.fill_rounded_rect(*r, r.w as f32 / 2.0, circle_bg);
        } else {
            frost_card(&mut c, backdrop, *r, r.w as f32 / 2.0, circle_bg);
        }
        let glyph_col = if tile.on {
            t.text_on_accent
        } else if tile.available {
            ON_DIM
        } else {
            ON_DIM_MUTED
        };
        chrome::draw_ink_centered(
            &mut c,
            tile.glyph,
            &text::LabelStyle::icon(56.0, glyph_col),
            *r,
        );
        let label = Rect::new(r.x + r.w / 2 - 76, r.y + r.h + 6, 152, 36);
        let label_col = if tile.available { ON_DIM } else { ON_DIM_MUTED };
        if let Some(px) = text::label(&tile.label, style::caption(label_col), Some(label.w - 8)) {
            let w = px.width() as i32;
            let h = px.height() as i32;
            c.blit(
                &Pixels::new(Size::new(w, h), px.data().to_vec()),
                Point::new(label.x + (label.w - w) / 2, r.y + r.h + 10),
            );
        }
    }
    if let Some(message) = &m.feedback {
        chrome::draw_centered(&mut c, message, &style::label(ON_DIM), size.h - 104);
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
            brightness_available: true,
            volume_available: true,
            feedback: None,
        }
    }

    fn backdrop() -> Pixels {
        Canvas::filled(SIZE, [200, 150, 120, 255]).into_pixels()
    }

    #[test]
    fn panel_is_frosted_between_controls() {
        let s = Strings::default();
        let bg = backdrop();
        let px = render_quick(SIZE, &s, &tokens::DEFAULT, &bg, "16:11", "Mon", &model());
        let alpha = |x: i32, y: i32| px.data[((y * SIZE.w + x) * 4 + 3) as usize];
        // The whole sheet is frosted: gaps between controls show the blurred
        // scene under the scrim, never the sharp application behind it.
        assert_eq!(alpha(0, 0), 255);
        assert_eq!(alpha(360, 1250), 255);
        assert!(alpha(80, 250) > 0);
    }

    #[test]
    fn veil_is_the_backdrop_under_a_tint() {
        let t = tokens::DEFAULT;
        let px = backdrop_with_veil(SIZE, &backdrop(), t.scrim).into_pixels();
        let at = |x: i32, y: i32| ((y * SIZE.w + x) * 4) as usize;
        let p = &px.data[at(360, 1250)..at(360, 1250) + 4];
        assert_eq!(p[3], 255, "veil covers the whole sheet");
        // scrim is plum 40%: the backdrop darkens but stays visible.
        assert!(p[0] < 200 && p[0] > 48, "tinted, not replaced: {p:?}");
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
            assert!(
                bounds.contains(Point::new(a.x, a.y))
                    && bounds.contains(Point::new(a.x + a.w - 1, a.y + a.h - 1)),
                "{a:?} off screen"
            );
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
        assert_eq!(
            quick_hit(SIZE, c(l.round[5])),
            Some(QuickHit::Tile(WIDE_TILES + 5))
        );
        assert_eq!(quick_hit(SIZE, c(l.power)), Some(QuickHit::Power));
        match quick_hit(
            SIZE,
            Point::new(l.sliders[0].x + l.sliders[0].w / 4, l.sliders[0].y + 40),
        ) {
            Some(QuickHit::Brightness(f)) => assert!((f - 0.25).abs() < 0.01),
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            quick_hit(SIZE, c(l.sliders[1])),
            Some(QuickHit::Volume(_))
        ));
        assert_eq!(quick_hit(SIZE, Point::new(2, 800)), None);
    }

    #[test]
    fn notification_cards_stack_and_hit() {
        let r = notif_rects(SIZE, 3);
        assert_eq!(r.len(), 3);
        assert!(r[1].y > r[0].y + r[0].h);
        assert_eq!(
            notif_hit(SIZE, Point::new(100, r[1].y + 10), 3, false),
            Some(NotifHit::Card(1))
        );
        assert_eq!(
            notif_hit(SIZE, clear_all_rect(SIZE).origin(), 3, false),
            Some(NotifHit::ClearAll)
        );
        assert_eq!(notif_hit(SIZE, clear_all_rect(SIZE).origin(), 0, false), None);
        assert!(
            notif_rects(SIZE, 50).len() < 50,
            "list stops above the bottom"
        );
    }

    #[test]
    fn landscape_layout_splits_columns_and_stays_on_screen() {
        let land = Size::new(1600, 720);
        let l = notif_layout(land, 6, true);
        let media = l.media.unwrap();
        assert!(media.x + media.w < l.cards[0].x, "media left of the list");
        assert!(l.cards.len() >= 2);
        for r in l.cards.iter().chain(std::iter::once(&media)) {
            assert!(r.x >= 0 && r.y >= 0 && r.x + r.w <= land.w && r.y + r.h <= land.h);
        }
        assert_eq!(
            notif_hit(land, Point::new(media.x + 5, media.y + 5), 6, true),
            Some(NotifHit::Media)
        );
        assert_eq!(
            notif_hit(land, clear_all_rect(land).origin(), 6, true),
            Some(NotifHit::ClearAll)
        );
        // Portrait: media sits above the first card, full width.
        let p = notif_layout(SIZE, 3, true);
        assert!(p.media.unwrap().y + p.media.unwrap().h < p.cards[0].y);
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
        let n = render_notifications(SIZE, &s, &t, &backdrop(), "09:41", "Mon, 9 Jun", &items, None);
        assert_eq!(n.size, SIZE);
        let empty = render_notifications(SIZE, &s, &t, &backdrop(), "09:41", "Mon, 9 Jun", &[], None);
        assert_ne!(n.data, empty.data);
    }
}
