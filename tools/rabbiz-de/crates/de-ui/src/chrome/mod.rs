//! Shared chrome pieces used by the screen skeletons (M2-T5): status bar,
//! nav pill, battery glyph, grouped-card rows. All geometry comes from
//! `de-design::tokens` safe-area constants.

pub mod menu;
pub mod nav_bar;

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
    let (x0, y0, x1, y1) =
        text::ink_bounds(&px).unwrap_or((0, 0, px.width() as i32, px.height() as i32));
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
    /// Wi-Fi RSSI 0..=100 when known → picks the level glyph.
    pub wifi_strength: Option<u8>,
    /// Cellular slots in slot order; empty hides the whole mobile cluster
    /// (the AOSP `SignalClusterView` region).
    pub cellular: Vec<de_api::services::CellularSlot>,
    /// Ringer mode — icon painted only when not `Normal`.
    pub ringer: de_api::services::RingerMode,
    /// Alarm scheduled → alarm-clock icon.
    pub alarm: bool,
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
            wifi_strength: match snap.wifi {
                WifiState::Connected { strength, .. } => strength,
                _ => None,
            },
            cellular: snap.cellular.clone(),
            ringer: snap.ringer,
            alarm: snap.alarm_set,
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

/// Blit `s` so its right edge sits at `x`; returns the new left edge.
fn draw_right(c: &mut Canvas, s: &str, st: &text::LabelStyle, x: i32, y: i32) -> i32 {
    if let Some(px) = text::label(s, *st, None) {
        let w = px.width() as i32;
        c.blit(
            &de_api::Pixels::new(de_api::Size::new(w, px.height() as i32), px.data().to_vec()),
            Point::new(x - w, y),
        );
        return x - w;
    }
    x
}

/// Four rising bars (AOSP `SignalStrength` levels 0..=4); `None` = all dim.
/// Drawn, not a font glyph, so the level actually shows.
fn signal_bars(c: &mut Canvas, x: i32, level: Option<u8>, fg: [u8; 4], dim: [u8; 4]) {
    const H: [i32; 4] = [7, 11, 15, 19];
    const BASE: i32 = 40; // baseline aligned with the 14..40 icon band
    let lv = level.unwrap_or(0).min(4);
    for (i, h) in H.iter().enumerate() {
        let col = if (i as u8) < lv { fg } else { dim };
        c.fill_rounded_rect(Rect::new(x + i as i32 * 6, BASE - h, 4, *h), 1.5, col);
    }
}

/// Small bold label for the `4G+`/`VoLTE` badges and the carrier text.
fn badge_style(col: [u8; 4]) -> text::LabelStyle {
    text::LabelStyle {
        size: 18.0,
        line_height: 22.0,
        weight: 600,
        color: col,
        ..Default::default()
    }
}

/// Mobile cluster per SIM (`SignalClusterView`), walking right→left from
/// `x`: bars, slot digit (multi-SIM only), `data_type`, `VoLTE` badge.
/// Returns the next free x. Hidden in airplane mode, like Android.
fn cellular_cluster(c: &mut Canvas, info: &StatusInfo, x: i32, fg: [u8; 4], dim: [u8; 4]) -> i32 {
    if info.airplane {
        return x;
    }
    let mut x = x;
    for (i, slot) in info.cellular.iter().enumerate().rev() {
        let bars_w = 4 * 4 + 3 * 2; // 4 bars × 4 px + 3 × 2 px gaps
        signal_bars(c, x - bars_w, slot.strength, fg, dim);
        x -= bars_w + 6;
        if info.cellular.len() > 1 {
            let mini = text::LabelStyle {
                size: 13.0,
                line_height: 16.0,
                weight: 600,
                color: fg,
                ..Default::default()
            };
            x = draw_right(c, &(i + 1).to_string(), &mini, x, 12) - 4;
        }
        if let Some(ty) = &slot.data_type {
            x = draw_right(c, ty, &badge_style(fg), x, 18) - 5;
        }
        if slot.volte {
            x = draw_right(c, "VoLTE", &badge_style(fg), x, 18) - 10;
        }
    }
    x
}

fn status_bar_fg(c: &mut Canvas, t: &Theme, info: &StatusInfo, fg: [u8; 4]) {
    use de_api::services::RingerMode;
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
    // Carrier label beside the clock when any SIM reports an operator name
    // (AOSP puts it on the keyguard/expanded shade; the reference phone
    // shows it top-left). Hidden in airplane mode like the mobile cluster.
    let carrier = info
        .cellular
        .iter()
        .map(|s| s.carrier.as_str())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("  ");
    if !info.airplane && !carrier.is_empty() {
        draw(
            c,
            &carrier,
            &badge_style(fg),
            dot_x,
            18,
            Some((w / 2 - dot_x).max(60)),
        );
    }
    // Right edge, right→left: battery, per-SIM clusters (slot N rightmost),
    // wifi, ringer, bt, dnd, airplane, alarm — the AOSP system-icons order.
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
    // Mobile cluster (SignalClusterView), one per SIM — slot 1 leftmost.
    x = cellular_cluster(c, info, x, fg, fg_dim);
    let wifi_g = match info.wifi_strength {
        Some(0..=25) => "\u{e4ca}",  // wifi_1_bar
        Some(26..=55) => "\u{e4d9}", // wifi_2_bar
        _ => "\u{e63e}",             // wifi (full / unknown level)
    };
    for (show, g, col) in [
        (info.wifi.unwrap_or(false), wifi_g, fg),       // wifi level
        (info.wifi == Some(false), "\u{e648}", fg_dim), // wifi_off
        (info.ringer == RingerMode::Vibrate, "\u{e62d}", fg), // vibration
        (info.ringer == RingerMode::Silent, "\u{e7f6}", fg), // notifications_off
        (info.bluetooth, "\u{e1a7}", fg),               // bluetooth
        (info.dnd, "\u{e644}", fg),                     // do_not_disturb_on
        (info.airplane, "\u{e195}", t.accent),          // airplanemode_active
        (info.alarm, "\u{e855}", fg),                   // alarm
    ] {
        if !show {
            continue;
        }
        let r = draw(c, g, &ic(col), x - 26, 14, None);
        x = r.x - 6;
    }
}

/// Bottom nav pill hook kept for screen renderers; the single interactive
/// gesture bar is composited live in `DisplayArea::SystemBars` by
/// `nav_bar::gesture_bar_at` so shell screens never show a duplicate bar.
pub fn nav_pill(_c: &mut Canvas, _t: &Theme) {}

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
        c.fill_rounded_rect(
            Rect::new(r.x, y, r.w, r.h + reach),
            tokens::RADIUS_CARD,
            t.surface1,
        );
    } else if last && !first {
        c.fill_rounded_rect(
            Rect::new(r.x, y - reach, r.w, r.h + reach),
            tokens::RADIUS_CARD,
            t.surface1,
        );
    } else if first && last {
        c.fill_rounded_rect(r, tokens::RADIUS_CARD, t.surface1);
    } else {
        c.fill_rect(r, t.surface1);
    }
    if !first {
        c.fill_rect(Rect::new(r.x + 16, y, r.w - 32, 1), t.line);
    }
    let val_max = (r.w / 2 - 48).max(80);
    let mut val_w = 0;
    if let Some(px) = text::label(value, style::body(t.text3), Some(val_max)) {
        let (vw, vh) = (px.width() as i32, px.height() as i32);
        val_w = vw;
        c.blit(
            &de_api::Pixels::new(de_api::Size::new(vw, vh), px.data().to_vec()),
            Point::new(r.x + r.w - 24 - vw - 24, y + 26),
        );
    }
    let name_max = (r.w - 48 - val_w - 40).max(80);
    draw(
        c,
        name,
        &style::body(t.text),
        r.x + 24,
        y + 26,
        Some(name_max),
    );
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
    use de_api::services::{CellularSlot, RingerMode, ServicesSnapshot, WifiState};

    #[test]
    fn content_width() {
        assert_eq!(content_w(Size::new(720, 1600)), 672);
        assert_eq!(content_w(Size::new(1080, 2400)), 1032);
    }

    /// Render just the 56 px bar over `t.bg`.
    fn bar_pixels(info: &StatusInfo) -> de_api::Pixels {
        let t = &de_design::tokens::DEFAULT;
        let mut c = Canvas::filled(Size::new(720, STATUS_H), t.bg);
        status_bar(&mut c, t, info);
        c.into_pixels()
    }

    /// Pixels that differ from the flat background.
    fn ink(px: &de_api::Pixels, bg: [u8; 4]) -> usize {
        px.data
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| **p != bg)
            .count()
    }

    fn sim(carrier: &str) -> CellularSlot {
        CellularSlot {
            carrier: carrier.into(),
            data_type: Some("4G+".into()),
            volte: true,
            strength: Some(3),
        }
    }

    #[test]
    fn status_bar_empty_paints_nothing() {
        let t = &de_design::tokens::DEFAULT;
        assert_eq!(ink(&bar_pixels(&StatusInfo::default()), t.bg), 0);
    }

    #[test]
    fn status_bar_new_indicators_paint_ink() {
        let t = &de_design::tokens::DEFAULT;
        let quiet = StatusInfo::new("09:41", "48");
        let quiet_ink = ink(&bar_pixels(&quiet), t.bg);
        let full = StatusInfo {
            cellular: vec![sim("AIS"), sim("TRUE-H")],
            ringer: RingerMode::Vibrate,
            alarm: true,
            wifi_strength: Some(20),
            ..quiet
        };
        let full_ink = ink(&bar_pixels(&full), t.bg);
        // Two SIM clusters + vibrate + alarm + carrier label + level glyph.
        assert!(
            full_ink > quiet_ink + 200,
            "indicators painted little ink: {quiet_ink} -> {full_ink}"
        );
    }

    #[test]
    fn status_bar_each_indicator_toggles() {
        let t = &de_design::tokens::DEFAULT;
        let base = StatusInfo::new("09:41", "48");
        let base_ink = ink(&bar_pixels(&base), t.bg);
        for info in [
            StatusInfo {
                alarm: true,
                ..base.clone()
            },
            StatusInfo {
                ringer: RingerMode::Silent,
                ..base.clone()
            },
            StatusInfo {
                cellular: vec![sim("AIS")],
                ..base.clone()
            },
        ] {
            assert!(
                ink(&bar_pixels(&info), t.bg) > base_ink,
                "hidden indicator painted nothing"
            );
        }
    }

    #[test]
    fn status_bar_airplane_hides_cellular() {
        let info = StatusInfo {
            airplane: true,
            cellular: vec![sim("AIS")],
            ..StatusInfo::new("09:41", "48")
        };
        // Same bar without the SIM must be pixel-identical: airplane mode
        // suppresses the mobile cluster entirely.
        let no_sim = StatusInfo {
            airplane: true,
            ..StatusInfo::new("09:41", "48")
        };
        assert_eq!(bar_pixels(&info).data, bar_pixels(&no_sim).data);
    }

    #[test]
    fn from_snapshot_maps_status_indicators() {
        let snap = ServicesSnapshot {
            wifi: WifiState::Connected {
                ssid: "Rabbiz-5G".into(),
                strength: Some(72),
            },
            cellular: vec![sim("AIS")],
            ringer: RingerMode::Vibrate,
            alarm_set: true,
            ..ServicesSnapshot::default()
        };
        let info = StatusInfo::from_snapshot(&snap, "09:41");
        assert_eq!(info.wifi, Some(true));
        assert_eq!(info.wifi_strength, Some(72));
        assert_eq!(info.cellular.len(), 1);
        assert_eq!(info.cellular[0].carrier, "AIS");
        assert_eq!(info.ringer, RingerMode::Vibrate);
        assert!(info.alarm);
    }
}
