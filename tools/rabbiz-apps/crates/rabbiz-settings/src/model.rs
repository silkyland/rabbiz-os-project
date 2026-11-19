//! Pure settings-screen model: row layout, hit targets, labels.
//! No I/O — everything here is unit-testable.

use de_api::{Rect, Size};
use rabbiz_app_kit::ui;

/// Standard interactive row height.
pub const ROW_H: i32 = 84;
/// Slider rows are taller (title + value + track).
pub const SLIDER_H: i32 = 112;
/// Section header band.
pub const SECTION_H: i32 = 44;
/// Wallpaper swatch card height.
pub const SWATCH_H: i32 = 148;
/// Keyboard option row height inside the expanded section.
pub const KEY_H: i32 = 76;
/// Timeout pill height.
pub const PILL_H: i32 = 58;
pub const PILL_GAP: i32 = 10;
/// Gap between cards.
pub const GAP: i32 = 12;
/// Extra scroll room below the last row.
pub const BOTTOM_PAD: i32 = 48;
/// Timeout choices shown as pills: (seconds, pill label).
pub const TIMEOUT_OPTIONS: &[(u32, &str)] = &[
    (15, "15s"),
    (30, "30s"),
    (60, "1m"),
    (300, "5m"),
    (600, "10m"),
    (1800, "30m"),
    (0, "Never"),
];
/// Pills per row in the timeout picker.
pub const TIMEOUT_COLS: usize = 3;

/// Which expandable groups are open.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Open {
    pub wallpaper: bool,
    pub timeout: bool,
    pub keyboard: bool,
}

/// Stable hit target for every interactive element.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hit {
    /// Non-interactive row (section header, note).
    None,
    DarkTheme,
    Brightness,
    Volume,
    Mute,
    Dnd,
    Wifi,
    Bluetooth,
    WallpaperToggle,
    WallpaperPick(usize),
    TimeoutToggle,
    TimeoutPick(u32),
    KeyboardToggle,
    ThirdParty,
    KeyboardPick(usize),
}

/// What a placed row should draw.
#[derive(Debug, Clone)]
pub enum View {
    Section(&'static str),
    Switch {
        title: &'static str,
        sub: Option<String>,
        on: bool,
        /// `true` when the backing state could not be read (e.g. no rfkill).
        unknown: bool,
    },
    Slider {
        title: &'static str,
        /// `None` = control unavailable on this device.
        frac: Option<f32>,
        value: String,
    },
    Nav {
        title: &'static str,
        value: String,
        open: bool,
    },
    Swatch {
        index: usize,
        name: String,
        selected: bool,
    },
    Pill {
        label: &'static str,
        selected: bool,
    },
    KeyRow {
        name: String,
        badge: &'static str,
        enabled: bool,
    },
    Note(&'static str),
}

/// Runtime facts the layout needs (values read from prefs / hardware).
#[derive(Debug, Default)]
pub struct Facts {
    pub dark: bool,
    pub brightness: Option<f32>,
    pub volume: Option<f32>,
    pub muted: bool,
    pub dnd: bool,
    pub wifi: Option<bool>,
    pub bt: Option<bool>,
    pub wallpaper: String,
    pub timeout_sec: u32,
    pub keyboard_name: String,
    pub third_party: bool,
    pub keyboards: Vec<KbFact>,
}

#[derive(Debug, Clone)]
pub struct KbFact {
    pub name: String,
    pub installed: bool,
    pub active: bool,
}

/// A row with its viewport-space rect (scroll already applied).
#[derive(Debug, Clone)]
pub struct Placed {
    pub rect: Rect,
    pub view: View,
    pub hit: Hit,
}

struct Item {
    view: View,
    hit: Hit,
    r: Rect,
}

/// Human label for a screen-timeout value.
#[must_use]
pub fn timeout_label(sec: u32) -> String {
    match sec {
        0 => "Never".to_string(),
        s if s < 60 => format!("{s} seconds"),
        60 => "1 minute".to_string(),
        s if s % 60 == 0 => format!("{} minutes", s / 60),
        _ => format!("{sec} seconds"),
    }
}

/// Display name for a wallpaper id (unknown ids pass through).
#[must_use]
pub fn wallpaper_name(id: &str) -> String {
    match id {
        "ember" => "Ember".to_string(),
        "dusk" => "Dusk".to_string(),
        "sand" => "Sand".to_string(),
        "aurora" => "Aurora".to_string(),
        "moon" => "Moon".to_string(),
        "oled" => "OLED".to_string(),
        other => other.to_string(),
    }
}

/// Parse `wpctl get-volume @DEFAULT_AUDIO_SINK@` output:
/// `Volume: 0.45` or `Volume: 0.45 [MUTED]` → `(fraction, muted)`.
#[must_use]
pub fn parse_volume(out: &str) -> Option<(f32, bool)> {
    let line = out.lines().next()?;
    let rest = line.split(':').nth(1)?.trim();
    let muted = rest.contains("MUTED");
    let num = rest.split_whitespace().next()?;
    let v: f32 = num.parse().ok()?;
    Some((v.clamp(0.0, 1.0), muted))
}

/// Slider rail inside a slider card.
#[must_use]
pub fn slider_track(card: Rect) -> Rect {
    Rect::new(card.x + 28, card.y + card.h - 56, card.w - 56, 12)
}

/// Fraction 0..=1 for a touch x inside the slider rail.
#[must_use]
pub fn slider_frac(x: i32, track: Rect) -> f32 {
    if track.w <= 0 {
        return 0.0;
    }
    ((x - track.x) as f32 / track.w as f32).clamp(0.0, 1.0)
}

/// Clamp scroll offset into `[0, content_h - viewport_h]`.
#[must_use]
pub fn clamp_scroll(scroll: i32, content_h: i32, viewport_h: i32) -> i32 {
    let max = (content_h - viewport_h).max(0);
    scroll.clamp(0, max)
}

/// Percentage label for a 0..=1 fraction.
#[must_use]
pub fn pct(f: f32) -> String {
    format!("{}%", (f.clamp(0.0, 1.0) * 100.0).round() as i32)
}

/// Lay out the whole settings list in content space (y starts at `top`),
/// then translate by `-scroll` into viewport space. Returns the placed rows
/// and the total content height (for scroll clamping).
#[must_use]
pub fn layout(size: Size, top: i32, scroll: i32, open: &Open, f: &Facts) -> (Vec<Placed>, i32) {
    let cw = (size.w - ui::MARGIN * 2).max(0);
    let x = ui::MARGIN;
    let mut items: Vec<Item> = Vec::new();
    let mut y = top;

    let section = |items: &mut Vec<Item>, y: &mut i32, name: &'static str| {
        items.push(Item {
            view: View::Section(name),
            hit: Hit::None,
            r: Rect::new(x, *y, cw, SECTION_H),
        });
        *y += SECTION_H;
    };
    let card =
        |items: &mut Vec<Item>, y: &mut i32, view: View, hit: Hit, h: i32, w: i32, rx: i32| {
            items.push(Item {
                view,
                hit,
                r: Rect::new(rx, *y, w, h),
            });
            *y += h + GAP;
        };

    // ---- Display -----------------------------------------------------
    section(&mut items, &mut y, "DISPLAY");
    card(
        &mut items,
        &mut y,
        View::Switch {
            title: "Dark theme",
            sub: Some(if f.dark {
                "Dark".into()
            } else {
                "Light".into()
            }),
            on: f.dark,
            unknown: false,
        },
        Hit::DarkTheme,
        ROW_H,
        cw,
        x,
    );
    card(
        &mut items,
        &mut y,
        View::Slider {
            title: "Brightness",
            frac: f.brightness,
            value: f.brightness.map_or_else(|| "—".into(), pct),
        },
        Hit::Brightness,
        SLIDER_H,
        cw,
        x,
    );

    // ---- Sound -------------------------------------------------------
    section(&mut items, &mut y, "SOUND");
    card(
        &mut items,
        &mut y,
        View::Slider {
            title: "Volume",
            frac: f.volume,
            value: if f.volume.is_none() {
                "—".into()
            } else if f.muted {
                "Muted".into()
            } else {
                pct(f.volume.unwrap_or(0.0))
            },
        },
        Hit::Volume,
        SLIDER_H,
        cw,
        x,
    );
    card(
        &mut items,
        &mut y,
        View::Switch {
            title: "Mute",
            sub: Some("Silence all audio".into()),
            on: f.muted,
            unknown: f.volume.is_none(),
        },
        Hit::Mute,
        ROW_H,
        cw,
        x,
    );

    // ---- Notifications ------------------------------------------------
    section(&mut items, &mut y, "NOTIFICATIONS");
    card(
        &mut items,
        &mut y,
        View::Switch {
            title: "Do Not Disturb",
            sub: Some("Pause notifications".into()),
            on: f.dnd,
            unknown: false,
        },
        Hit::Dnd,
        ROW_H,
        cw,
        x,
    );

    // ---- Connectivity --------------------------------------------------
    section(&mut items, &mut y, "CONNECTIVITY");
    for (hit, title, state) in [
        (Hit::Wifi, "Wi-Fi", f.wifi),
        (Hit::Bluetooth, "Bluetooth", f.bt),
    ] {
        card(
            &mut items,
            &mut y,
            View::Switch {
                title,
                sub: Some(match state {
                    Some(true) => "On".into(),
                    Some(false) => "Off".into(),
                    None => "Unknown".into(),
                }),
                on: state.unwrap_or(false),
                unknown: state.is_none(),
            },
            hit,
            ROW_H,
            cw,
            x,
        );
    }

    // ---- Personalization ----------------------------------------------
    section(&mut items, &mut y, "PERSONALIZATION");
    card(
        &mut items,
        &mut y,
        View::Nav {
            title: "Wallpaper",
            value: wallpaper_name(&f.wallpaper),
            open: open.wallpaper,
        },
        Hit::WallpaperToggle,
        ROW_H,
        cw,
        x,
    );
    if open.wallpaper {
        let ids = de_paint::wallpaper::IDS;
        let n = ids.len() as i32;
        let sw = (cw - (n - 1) * GAP) / n;
        for (i, id) in ids.iter().enumerate() {
            let rx = x + i as i32 * (sw + GAP);
            items.push(Item {
                view: View::Swatch {
                    index: i,
                    name: wallpaper_name(id),
                    selected: *id == f.wallpaper,
                },
                hit: Hit::WallpaperPick(i),
                r: Rect::new(rx, y, sw, SWATCH_H),
            });
        }
        y += SWATCH_H + GAP;
    }

    // ---- System --------------------------------------------------------
    section(&mut items, &mut y, "SYSTEM");
    card(
        &mut items,
        &mut y,
        View::Nav {
            title: "Screen timeout",
            value: timeout_label(f.timeout_sec),
            open: open.timeout,
        },
        Hit::TimeoutToggle,
        ROW_H,
        cw,
        x,
    );
    if open.timeout {
        let pw = (cw - (TIMEOUT_COLS as i32 - 1) * PILL_GAP) / TIMEOUT_COLS as i32;
        let rows = TIMEOUT_OPTIONS.len().div_ceil(TIMEOUT_COLS);
        for (ri, chunk) in TIMEOUT_OPTIONS.chunks(TIMEOUT_COLS).enumerate() {
            for (ci, (sec, label)) in chunk.iter().enumerate() {
                let rx = x + ci as i32 * (pw + PILL_GAP);
                let ry = y + ri as i32 * (PILL_H + PILL_GAP);
                items.push(Item {
                    view: View::Pill {
                        label,
                        selected: f.timeout_sec == *sec,
                    },
                    hit: Hit::TimeoutPick(*sec),
                    r: Rect::new(rx, ry, pw, PILL_H),
                });
            }
        }
        y += rows as i32 * PILL_H + (rows as i32 - 1) * PILL_GAP + GAP;
    }
    card(
        &mut items,
        &mut y,
        View::Nav {
            title: "Keyboard",
            value: f.keyboard_name.clone(),
            open: open.keyboard,
        },
        Hit::KeyboardToggle,
        ROW_H,
        cw,
        x,
    );
    if open.keyboard {
        card(
            &mut items,
            &mut y,
            View::Switch {
                title: "Allow 3rd-party keyboards",
                sub: None,
                on: f.third_party,
                unknown: false,
            },
            Hit::ThirdParty,
            ROW_H,
            cw,
            x,
        );
        for (i, k) in f.keyboards.iter().enumerate() {
            let badge = if k.active {
                "Active"
            } else if k.installed {
                "Installed"
            } else {
                "Missing"
            };
            items.push(Item {
                view: View::KeyRow {
                    name: k.name.clone(),
                    badge,
                    enabled: k.installed,
                },
                hit: Hit::KeyboardPick(i),
                r: Rect::new(x, y, cw, KEY_H),
            });
            y += KEY_H + GAP;
        }
    }

    // ---- Footer notes ---------------------------------------------------
    items.push(Item {
        view: View::Note("Saved prefs apply the next time the desktop starts."),
        hit: Hit::None,
        r: Rect::new(x, y, cw, 24),
    });
    y += 24;
    items.push(Item {
        view: View::Note("Brightness, volume, Wi-Fi and Bluetooth apply immediately."),
        hit: Hit::None,
        r: Rect::new(x, y, cw, 24),
    });
    y += 24;

    let content_h = y - top + BOTTOM_PAD;
    let vis_top = top;
    let vis_bot = size.h - ui::BOTTOM_SAFE;
    let placed = items
        .into_iter()
        .filter_map(|it| {
            let vr = Rect::new(it.r.x, it.r.y - scroll, it.r.w, it.r.h);
            (vr.y + vr.h >= vis_top && vr.y <= vis_bot).then_some(Placed {
                rect: vr,
                view: it.view,
                hit: it.hit,
            })
        })
        .collect();
    (placed, content_h)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts() -> Facts {
        Facts {
            dark: true,
            brightness: Some(0.6),
            volume: Some(0.4),
            muted: false,
            dnd: true,
            wifi: Some(true),
            bt: Some(false),
            wallpaper: "ember".into(),
            timeout_sec: 600,
            keyboard_name: "Rabbiz Keyboard".into(),
            third_party: true,
            keyboards: vec![KbFact {
                name: "Rabbiz Keyboard".into(),
                installed: true,
                active: true,
            }],
        }
    }

    #[test]
    fn collapsed_layout_has_all_sections() {
        let (rows, _) = layout(Size::new(720, 1600), 100, 0, &Open::default(), &facts());
        let sections: Vec<&str> = rows
            .iter()
            .filter_map(|p| match &p.view {
                View::Section(s) => Some(*s),
                _ => None,
            })
            .collect();
        assert_eq!(
            sections,
            [
                "DISPLAY",
                "SOUND",
                "NOTIFICATIONS",
                "CONNECTIVITY",
                "PERSONALIZATION",
                "SYSTEM"
            ]
        );
    }

    #[test]
    fn expanded_sections_add_rows() {
        let open = Open {
            wallpaper: true,
            timeout: true,
            keyboard: true,
        };
        // Tall viewport so nothing is culled — we assert on row presence.
        let (rows, content_h) = layout(Size::new(720, 4000), 100, 0, &open, &facts());
        assert!(rows.iter().any(|p| matches!(p.view, View::Swatch { .. })));
        assert!(rows.iter().any(|p| matches!(p.view, View::Pill { .. })));
        assert!(rows.iter().any(|p| matches!(p.view, View::KeyRow { .. })));
        assert!(
            content_h > 1600,
            "expanded content should need scrolling on the panel"
        );
    }

    #[test]
    fn scroll_translates_and_culls() {
        let f = facts();
        let open = Open::default();
        let (_, h0) = layout(Size::new(720, 1600), 100, 0, &open, &f);
        let (rows, _) = layout(Size::new(720, 1600), 100, h0, &open, &f);
        // Deep scroll: only footer-ish rows remain visible.
        assert!(rows.iter().all(|p| p.rect.y + p.rect.h > 100));
        assert!(rows.iter().all(|p| p.rect.y < 1600 - ui::BOTTOM_SAFE + 200));
    }

    #[test]
    fn parse_volume_handles_mute_and_garbage() {
        assert_eq!(parse_volume("Volume: 0.45\n"), Some((0.45, false)));
        assert_eq!(parse_volume("Volume: 1.00 [MUTED]\n"), Some((1.0, true)));
        assert_eq!(parse_volume("Volume: 0.0 [MUTED]"), Some((0.0, true)));
        assert_eq!(parse_volume("nonsense"), None);
        assert_eq!(parse_volume(""), None);
    }

    #[test]
    fn timeout_labels() {
        assert_eq!(timeout_label(0), "Never");
        assert_eq!(timeout_label(15), "15 seconds");
        assert_eq!(timeout_label(60), "1 minute");
        assert_eq!(timeout_label(600), "10 minutes");
        assert_eq!(timeout_label(90), "90 seconds");
    }

    #[test]
    fn clamp_scroll_bounds() {
        assert_eq!(clamp_scroll(-50, 2000, 1400), 0);
        assert_eq!(clamp_scroll(9999, 2000, 1400), 600);
        assert_eq!(clamp_scroll(100, 1000, 1400), 0);
        assert_eq!(clamp_scroll(50, 2000, 1400), 50);
    }

    #[test]
    fn slider_frac_maps_track() {
        let t = Rect::new(28, 0, 100, 12);
        assert_eq!(slider_frac(28, t), 0.0);
        assert_eq!(slider_frac(78, t), 0.5);
        assert_eq!(slider_frac(200, t), 1.0);
        assert_eq!(slider_frac(-50, t), 0.0);
    }

    #[test]
    fn wallpaper_names() {
        assert_eq!(wallpaper_name("ember"), "Ember");
        assert_eq!(wallpaper_name("aurora"), "Aurora");
        assert_eq!(wallpaper_name("weird"), "weird");
    }
}
