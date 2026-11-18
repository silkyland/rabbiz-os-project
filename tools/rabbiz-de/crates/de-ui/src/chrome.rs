//! Shared chrome pieces used by the screen skeletons (M2-T5): status bar,
//! nav pill, battery glyph, grouped-card rows. All geometry comes from
//! `de-design::tokens` safe-area constants.

use de_api::{Point, Rect, Size};
use de_design::tokens::{self, Theme};
use de_paint::{text, Canvas};

/// Status bar height incl. cutout inset (§7.17: 56 px).
pub const STATUS_H: i32 = tokens::STATUS_BAR_HEIGHT;
/// Nav pill area (gesture bar 32 px).
pub const NAV_H: i32 = tokens::GESTURE_BAR_HEIGHT;
/// Page margin (§7.17: 24 px).
pub const MARGIN: i32 = tokens::PAGE_MARGIN;
/// Content width on 720 px output (672 px).
#[must_use]
pub fn content_w(size: Size) -> i32 {
    size.w - MARGIN * 2
}

/// Label style helpers bound to the type scale.
pub mod style {
    use de_paint::text::LabelStyle;

    #[must_use]
    pub fn body(color: [u8; 4]) -> LabelStyle {
        LabelStyle {
            color,
            ..LabelStyle::default()
        }
    }
    #[must_use]
    pub fn label(color: [u8; 4]) -> LabelStyle {
        LabelStyle {
            size: 22.0,
            line_height: 28.0,
            color,
            ..LabelStyle::default()
        }
    }
    #[must_use]
    pub fn caption(color: [u8; 4]) -> LabelStyle {
        LabelStyle {
            size: 20.0,
            line_height: 26.0,
            color,
            ..LabelStyle::default()
        }
    }
    #[must_use]
    pub fn h2(color: [u8; 4]) -> LabelStyle {
        LabelStyle {
            size: 32.0,
            line_height: 40.0,
            weight: 600,
            color,
            ..LabelStyle::default()
        }
    }
    #[must_use]
    pub fn display(color: [u8; 4]) -> LabelStyle {
        LabelStyle {
            size: 104.0,
            line_height: 112.0,
            weight: 600,
            tnum: true,
            color,
            ..LabelStyle::default()
        }
    }
}

/// Paint `px` centered horizontally at `y`.
pub fn blit_centered(c: &mut Canvas, px: &de_api::Pixels, y: i32) {
    let x = (c.size().w - px.size.w) / 2;
    c.blit(px, Point::new(x, y));
}

/// Draw `s` so its visible glyph (not its text box) is centred in `r`.
/// Used for icon glyphs in buttons and for the letter on a tile.
pub fn draw_ink_centered(c: &mut Canvas, s: &str, st: &text::LabelStyle, r: Rect) {
    let Some(px) = text::label(s, *st, None) else {
        return;
    };
    let (x0, y0, x1, y1) = text::ink_bounds(&px).unwrap_or((0, 0, px.width() as i32, px.height() as i32));
    let (w, h) = (px.width() as i32, px.height() as i32);
    c.blit(
        &de_api::Pixels::new(de_api::Size::new(w, h), px.data().to_vec()),
        Point::new(
            r.x + (r.w - (x1 - x0)) / 2 - x0,
            r.y + (r.h - (y1 - y0)) / 2 - y0,
        ),
    );
}

/// Draw a label; returns the rect the pixels occupy.
pub fn draw(
    c: &mut Canvas,
    s: &str,
    st: &text::LabelStyle,
    x: i32,
    y: i32,
    max_w: Option<i32>,
) -> Rect {
    if let Some(px) = text::label(s, *st, max_w) {
        let r = Rect::new(x, y, px.width() as i32, px.height() as i32);
        c.blit(
            &de_api::Pixels::new(de_api::Size::new(r.w, r.h), px.data().to_vec()),
            Point::new(x, y),
        );
        return r;
    }
    Rect::new(x, y, 0, 0)
}

/// Centered label horizontally.
pub fn draw_centered(c: &mut Canvas, s: &str, st: &text::LabelStyle, y: i32) {
    draw_centered_x(c, s, st, c.size().w / 2, y);
}

/// Label centered on `cx` at `y`.
pub fn draw_centered_x(c: &mut Canvas, s: &str, st: &text::LabelStyle, cx: i32, y: i32) {
    if let Some(px) = text::label(s, *st, None) {
        let w = px.width() as i32;
        let h = px.height() as i32;
        c.blit(
            &de_api::Pixels::new(de_api::Size::new(w, h), px.data().to_vec()),
            Point::new(cx - w / 2, y),
        );
    }
}

/// What the status bar paints (M3-T1): built from `ServicesSnapshot` + the
/// clock string the compositor formats (the shell never reads time itself).
#[derive(Debug, Clone, Default)]
#[allow(clippy::struct_excessive_bools)] // indicator flags, not a state machine
pub struct StatusInfo {
    pub clock: String,
    /// `None` = no battery present.
    pub battery_pct: Option<u8>,
    pub charging: bool,
    pub battery_full: bool,
    /// Wi-Fi shown when `Some`; the glyph itself encodes on/off today.
    pub wifi: Option<bool>,
    pub bluetooth: bool,
    pub dnd: bool,
    pub airplane: bool,
    /// Privacy dots (camera/mic in use right now).
    pub privacy_cam: bool,
    pub privacy_mic: bool,
}

impl StatusInfo {
    /// Clock + plain battery text (skeleton/sim convenience).
    #[must_use]
    pub fn new(clock: &str, battery: &str) -> Self {
        Self {
            clock: clock.into(),
            battery_pct: battery.trim_end_matches('%').parse().ok(),
            ..Self::default()
        }
    }

    /// From a live services snapshot (device path).
    #[must_use]
    pub fn from_snapshot(snap: &de_api::services::ServicesSnapshot, clock: &str) -> Self {
        use de_api::services::{BluetoothState, WifiState};
        Self {
            clock: clock.into(),
            battery_pct: snap.battery.percent,
            charging: snap.battery.charging,
            battery_full: snap.battery.full,
            wifi: match snap.wifi {
                WifiState::Connected { .. } | WifiState::Connecting | WifiState::Disconnected => {
                    Some(true)
                }
                WifiState::Disabled => Some(false),
                WifiState::Unknown => None,
            },
            bluetooth: matches!(
                snap.bluetooth,
                BluetoothState::On | BluetoothState::Connected { .. }
            ),
            dnd: snap.dnd,
            airplane: snap.airplane,
            privacy_cam: snap.privacy.camera,
            privacy_mic: snap.privacy.mic,
        }
    }
}

/// 56 px status bar: clock left (tnum), privacy dots beside it, then
/// airplane/DND/BT/Wi-Fi glyphs and the battery block right-aligned.
pub fn status_bar(c: &mut Canvas, t: &Theme, info: &StatusInfo) {
    status_bar_fg(c, t, info, t.text);
}

/// Status bar over wallpaper/imagery: plum scrim band + cream glyphs so it
/// stays readable on any wallpaper (the light theme's `t.text` is dark plum
/// and disappears on dark wallpapers).
pub fn status_bar_on_wallpaper(c: &mut Canvas, t: &Theme, info: &StatusInfo) {
    c.fill_rect(
        Rect::new(0, 0, c.size().w, tokens::STATUS_BAR_HEIGHT),
        [0x10, 0x0B, 0x14, 0x59], // plum ~35%
    );
    status_bar_fg(c, t, info, [0xFF, 0xF8, 0xF0, 0xFF]); // cream
}

fn status_bar_fg(c: &mut Canvas, t: &Theme, info: &StatusInfo, fg: [u8; 4]) {
    let w = c.size().w;
    let ic = |col: [u8; 4]| text::LabelStyle::icon(22.0, col);
    let fg_dim = [fg[0], fg[1], fg[2], (u16::from(fg[3]) * 3 / 5) as u8];
    // Left: clock after the 72 px cutout inset.
    let clock_r = draw(
        c,
        &info.clock,
        &style::label(fg),
        tokens::CUTOUT_INSET,
        14,
        None,
    );
    // Privacy dots right after the clock (green camera/mic, §7.17).
    let mut dot_x = clock_r.x + clock_r.w + 12;
    for active in [info.privacy_cam, info.privacy_mic] {
        if !active {
            continue;
        }
        c.fill_rounded_rect(Rect::new(dot_x, 22, 10, 10), 999.0, t.success);
        dot_x += 16;
    }
    // Right edge, right→left: battery, wifi, bt, dnd, airplane.
    let mut x = w - MARGIN;
    if let Some(pct) = info.battery_pct {
        let label = format!("{pct}");
        if let Some(px) = text::label(&label, style::label(fg), None) {
            let (bw, bh) = (px.width() as i32, px.height() as i32);
            x -= bw;
            c.blit(
                &de_api::Pixels::new(de_api::Size::new(bw, bh), px.data().to_vec()),
                Point::new(x, 14),
            );
        }
        let g = if info.charging || info.battery_full {
            "\u{e1a3}" // battery_charging_full
        } else {
            "\u{e1a4}" // battery_full
        };
        let col = if info.charging { t.accent } else { fg };
        let r = draw(c, g, &ic(col), x - 26, 14, None);
        x = r.x - 8;
    }
    for (show, g, col) in [
        (info.wifi.unwrap_or(false), "\u{e63e}", fg),   // wifi
        (info.wifi == Some(false), "\u{e648}", fg_dim), // wifi_off
        (info.bluetooth, "\u{e1a7}", fg),               // bluetooth
        (info.dnd, "\u{e644}", fg),                     // do_not_disturb_on
        (info.airplane, "\u{e195}", t.accent),          // airplanemode_active
    ] {
        if !show {
            continue;
        }
        let r = draw(c, g, &ic(col), x - 26, 14, None);
        x = r.x - 6;
    }
}

/// 200×8 nav pill at the bottom (y = h - 24).
pub fn nav_pill(c: &mut Canvas, t: &Theme) {
    let size = c.size();
    let r = Rect::new((size.w - 200) / 2, size.h - 24, 200, 8);
    c.fill_rounded_rect(r, 4.0, t.text3);
}

/// An app tile for home/drawer/search/recents: icon pixels when the
/// compositor resolved them (`de-apps::icons`), else the shell draws the
/// first-party sand tile + initial.
#[derive(Debug, Clone)]
pub struct IconEntry {
    pub id: String,
    pub label: String,
    pub icon: Option<de_api::Pixels>,
}

impl IconEntry {
    /// Plain labelled tile (golden/sim path — no asset lookup).
    #[must_use]
    pub fn new(id: &str, label: &str) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            icon: None,
        }
    }
}

/// Draw an app icon inside `r`: real pixels when present, else the sand
/// tile + plum initial (same look as `de-apps::icons::tile_icon`).
pub fn draw_app_icon(c: &mut Canvas, t: &Theme, e: &IconEntry, r: Rect) {
    if let Some(px) = &e.icon {
        if px.size.w == r.w && px.size.h == r.h {
            c.blit(px, r.origin());
        } else {
            c.blit_scaled(px, r, 1.0);
        }
    } else {
        c.fill_rounded_rect(
            r,
            tokens::ICON_TILE_RADIUS * (r.w as f32 / 96.0),
            t.icon_tile,
        );
        let letter: String = e
            .label
            .chars()
            .find(|ch| ch.is_alphanumeric())
            .map_or_else(|| "?".into(), |ch| ch.to_uppercase().collect());
        let st = text::LabelStyle {
            size: r.w as f32 * 0.44,
            line_height: r.w as f32 * 0.5,
            weight: 600,
            color: t.icon_glyph,
            ..Default::default()
        };
        draw_ink_centered(c, &letter, &st, r);
    }
}

/// Grouped-card row: 88 px row, `line` divider, label left, value+chevron right.
pub fn settings_row(
    c: &mut Canvas,
    t: &Theme,
    y: i32,
    name: &str,
    value: &str,
    first: bool,
    last: bool,
) {
    let w = c.size().w;
    let r = Rect::new(MARGIN, y, w - MARGIN * 2, 88);
    // Only the group's outer corners are round: the first row paints a
    // rounded block that reaches into the next row, the last one up into the
    // previous row (same fill, so the overlap is invisible).
    let reach = tokens::RADIUS_CARD as i32;
    if first && !last {
        c.fill_rounded_rect(Rect::new(r.x, y, r.w, r.h + reach), tokens::RADIUS_CARD, t.surface1);
    } else if last && !first {
        c.fill_rounded_rect(Rect::new(r.x, y - reach, r.w, r.h + reach), tokens::RADIUS_CARD, t.surface1);
    } else if first && last {
        c.fill_rounded_rect(r, tokens::RADIUS_CARD, t.surface1);
    } else {
        c.fill_rect(r, t.surface1);
    }
    if !first {
        c.fill_rect(Rect::new(r.x + 16, y, r.w - 32, 1), t.line);
    }
    draw(c, name, &style::body(t.text), r.x + 24, y + 26, None);
    if let Some(px) = text::label(value, style::body(t.text3), None) {
        let (vw, vh) = (px.width() as i32, px.height() as i32);
        c.blit(
            &de_api::Pixels::new(de_api::Size::new(vw, vh), px.data().to_vec()),
            Point::new(r.x + r.w - 24 - vw - 24, y + 26),
        );
    }
    // chevron_right glyph.
    draw_ink_centered(
        c,
        "\u{e5cc}",
        &text::LabelStyle::icon(24.0, t.text3),
        Rect::new(r.x + r.w - 48, y, 32, r.h),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_width() {
        assert_eq!(content_w(Size::new(720, 1600)), 672);
        assert_eq!(content_w(Size::new(1080, 2400)), 1032);
    }
}
