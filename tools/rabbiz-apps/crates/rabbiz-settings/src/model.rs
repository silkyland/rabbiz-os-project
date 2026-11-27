//! Pure settings-screen model: row layout, hit targets, labels.
//! No I/O — everything here is unit-testable.

use de_api::{Rect, Size};
use rabbiz_app_kit::{icons, ui};

/// Standard interactive row height (leading icon + title + summary).
pub const ROW_H: i32 = 84;
/// Slider rows are taller (title + value + track).
pub const SLIDER_H: i32 = 112;
/// Section header band.
pub const SECTION_H: i32 = 40;
/// Wallpaper swatch row height inside the card.
pub const SWATCH_H: i32 = 148;
/// Keyboard option row height inside the expanded section.
pub const KEY_H: i32 = 76;
/// Timeout chip height (kit `CHIP_H`).
pub const PILL_H: i32 = 48;
pub const PILL_GAP: i32 = 10;
/// Inner gap between sibling items (swatches, chips) inside a card.
pub const GAP: i32 = 12;
/// Vertical gap between section cards.
pub const SECTION_GAP: i32 = 20;
/// Extra scroll room below the last row.
pub const BOTTOM_PAD: i32 = 48;

// ---- in-card geometry (shared by layout + draw) -------------------------
/// Leading-icon slot inset inside a card row.
pub const ICON_X: i32 = 20;
/// Leading-icon box edge (24 px glyph centered inside).
pub const ICON_W: i32 = 40;
/// Text column offset inside a card row (icon slot + gap).
pub const TEXT_X: i32 = 76;
/// Trailing padding inside a card row.
pub const TRAIL_PAD: i32 = 20;
/// Inner padding for expansion content (swatch/chip bands) inside a card.
pub const CARD_PAD: i32 = 16;
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
    pub wifi: bool,
    pub bluetooth: bool,
    pub wallpaper: bool,
    pub timeout: bool,
    pub search_engine: bool,
    pub keyboard: bool,
}

/// `keyboard` glyph — present in the bundled MaterialIconsRound cmap but not
/// yet a kit constant.
pub const ICON_KEYBOARD: &str = "\u{e312}";

/// Stable hit target for every interactive element.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hit {
    /// Non-interactive row (section header, note, card background).
    None,
    DarkTheme,
    Brightness,
    Volume,
    Mute,
    Dnd,
    Wifi,
    WifiNetworksToggle,
    WifiRescan,
    WifiPick(usize),
    WifiForget,
    WifiPassField,
    WifiPassConnect,
    WifiPassCancel,
    Bluetooth,
    BtDevicesToggle,
    BtRescan,
    BtPick(usize),
    WallpaperToggle,
    WallpaperPick(usize),
    TimeoutToggle,
    TimeoutPick(u32),
    SearchEngineToggle,
    SearchEnginePick(usize),
    KeyboardToggle,
    OskHaptic,
    OskSound,
    ThirdParty,
    KeyboardPick(usize),
    /// Tap into the search pill (focus + OSK).
    SearchField,
    /// Clear the current search query.
    SearchClear,
}

/// What a placed row should draw.
#[derive(Debug, Clone)]
pub enum View {
    Section(&'static str),
    /// Grouped-card background spanning a section's rows (drawn first).
    CardBg,
    /// 1 px in-card hairline (inset to the text column at draw time).
    Hairline,
    Switch {
        icon: &'static str,
        title: &'static str,
        sub: Option<String>,
        on: bool,
        /// `true` when the backing state could not be read (e.g. no rfkill).
        unknown: bool,
    },
    Slider {
        icon: &'static str,
        title: &'static str,
        /// `None` = control unavailable on this device.
        frac: Option<f32>,
        value: String,
    },
    Nav {
        icon: &'static str,
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
        installed: bool,
        active: bool,
    },
    TextField {
        placeholder: String,
        value: String,
        focused: bool,
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
    pub wifi_ssid: Option<String>,
    pub wifi_nets: Vec<crate::system::WifiNet>,
    pub wifi_scanning: bool,
    pub wifi_pass_ssid: Option<String>,
    pub wifi_pass_text: String,
    pub wifi_pass_focused: bool,
    pub bt: Option<bool>,
    pub bt_device: Option<String>,
    pub bt_devs: Vec<crate::system::BtDev>,
    pub bt_scanning: bool,
    pub wallpaper: String,
    pub timeout_sec: u32,
    pub search_engine: String,
    pub keyboard_name: String,
    pub osk_haptic: bool,
    pub osk_sound: bool,
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
    /// First row of its card — press overlay rounds the top corners.
    pub round_top: bool,
    /// Last row of its card — press overlay rounds the bottom corners.
    pub round_bot: bool,
}

struct Item {
    view: View,
    hit: Hit,
    r: Rect,
    top: bool,
    bot: bool,
}

/// Accumulates the rows of one section card: inserts a `Hairline` between
/// consecutive rows, records the card's vertical extent, and marks the
/// first/last content rows so press feedback can round the card corners.
struct CardBuilder {
    x: i32,
    w: i32,
    y: i32,
    items: Vec<Item>,
    divided: bool,
}

impl CardBuilder {
    fn new(x: i32, w: i32, y: i32) -> Self {
        Self {
            x,
            w,
            y,
            items: Vec::new(),
            divided: false,
        }
    }

    /// Append a full-width row of height `h` (hairline first when needed).
    fn row(&mut self, view: View, hit: Hit, h: i32) {
        if self.divided {
            self.divider();
        }
        self.items.push(Item {
            view,
            hit,
            r: Rect::new(self.x, self.y, self.w, h),
            top: false,
            bot: false,
        });
        self.y += h;
        self.divided = true;
    }

    /// Append an in-card hairline at the current cursor.
    fn divider(&mut self) {
        self.items.push(Item {
            view: View::Hairline,
            hit: Hit::None,
            r: Rect::new(self.x, self.y, self.w, 1),
            top: false,
            bot: false,
        });
        self.y += 1;
    }

    /// Place a non-row item (swatch, chip) at an explicit rect inside the card.
    fn place(&mut self, view: View, hit: Hit, r: Rect) {
        self.items.push(Item {
            view,
            hit,
            r,
            top: false,
            bot: false,
        });
    }

    fn has_rows(&self) -> bool {
        self.items
            .iter()
            .any(|it| !matches!(it.view, View::Hairline))
    }

    /// Emit the card into `items`: `CardBg` first, then the rows with the
    /// first/last content rows flagged for corner rounding. Returns the
    /// card's bottom edge (content-space y).
    fn finish(mut self, items: &mut Vec<Item>) -> i32 {
        let first = self
            .items
            .iter()
            .position(|it| !matches!(it.view, View::Hairline));
        let last = self
            .items
            .iter()
            .rposition(|it| !matches!(it.view, View::Hairline));
        if let Some(i) = first {
            self.items[i].top = true;
        }
        if let Some(i) = last {
            self.items[i].bot = true;
        }
        let card_top = first.map_or(self.y, |i| self.items[i].r.y);
        items.push(Item {
            view: View::CardBg,
            hit: Hit::None,
            r: Rect::new(self.x, card_top, self.w, self.y - card_top),
            top: false,
            bot: false,
        });
        items.extend(self.items);
        self.y
    }
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

/// Slider rail (lane) inside a slider row — spans the text column; the kit
/// slider centers the groove inside it.
#[must_use]
pub fn slider_track(card: Rect) -> Rect {
    Rect::new(
        card.x + TEXT_X,
        card.y + card.h - 56,
        card.w - TEXT_X - TRAIL_PAD,
        36,
    )
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
///
/// `query` filters preference rows by title (case-insensitive substring);
/// when non-empty, expansions and footer notes are skipped and sections
/// with no matching rows are omitted entirely.
#[must_use]
pub fn layout(
    size: Size,
    top: i32,
    scroll: i32,
    open: &Open,
    f: &Facts,
    query: &str,
) -> (Vec<Placed>, i32) {
    let cw = (size.w - ui::MARGIN * 2).max(0);
    let x = ui::MARGIN;
    let q = query.trim().to_lowercase();
    let filtering = !q.is_empty();
    let want = |title: &str| q.is_empty() || title.to_lowercase().contains(&q);
    let mut items: Vec<Item> = Vec::new();
    let mut y = top;

    // Emit the section header + card only when `b` collected at least one
    // real row (search filtering can empty a whole section).
    let finish = |items: &mut Vec<Item>, y: &mut i32, name: &'static str, b: CardBuilder| {
        if b.has_rows() {
            items.push(Item {
                view: View::Section(name),
                hit: Hit::None,
                r: Rect::new(x, *y, cw, SECTION_H),
                top: false,
                bot: false,
            });
            *y = b.finish(items) + SECTION_GAP;
        }
    };

    // ---- Connectivity --------------------------------------------------
    let mut b = CardBuilder::new(x, cw, y + SECTION_H);
    if want("Wi-Fi") {
        let wifi_sub = match f.wifi {
            Some(true) => f
                .wifi_ssid
                .as_ref()
                .map(|s| format!("Connected: {s}"))
                .unwrap_or_else(|| "On · Not connected".into()),
            Some(false) => "Off".into(),
            None => "Unknown".into(),
        };
        b.row(
            View::Switch {
                icon: icons::WIFI,
                title: "Wi-Fi",
                sub: Some(wifi_sub),
                on: f.wifi.unwrap_or(false),
                unknown: f.wifi.is_none(),
            },
            Hit::Wifi,
            ROW_H,
        );
        if f.wifi == Some(true) && !filtering {
            b.divider();
            b.row(
                View::Nav {
                    icon: icons::WIFI,
                    title: "Wi-Fi networks",
                    value: f
                        .wifi_ssid
                        .clone()
                        .unwrap_or_else(|| "Choose".into()),
                    open: open.wifi,
                },
                Hit::WifiNetworksToggle,
                ROW_H,
            );
            if open.wifi {
                b.divider();
                // Rescan + Forget (if connected) pills.
                let has_connected = f.wifi_ssid.is_some();
                let cols = if has_connected { 2 } else { 1 };
                let pw = (cw - CARD_PAD * 2 - (cols - 1) * PILL_GAP) / cols;
                b.y += CARD_PAD;
                b.place(
                    View::Pill {
                        label: if f.wifi_scanning {
                            "Scanning..."
                        } else {
                            "Rescan"
                        },
                        selected: f.wifi_scanning,
                    },
                    Hit::WifiRescan,
                    Rect::new(x + CARD_PAD, b.y, pw, PILL_H),
                );
                if has_connected {
                    b.place(
                        View::Pill {
                            label: "Forget",
                            selected: false,
                        },
                        Hit::WifiForget,
                        Rect::new(x + CARD_PAD + pw + PILL_GAP, b.y, pw, PILL_H),
                    );
                }
                b.y += PILL_H + CARD_PAD;

                // Inline password input for a secured network.
                if let Some(ssid) = &f.wifi_pass_ssid {
                    b.divider();
                    b.row(
                        View::TextField {
                            placeholder: format!("Password for {ssid}"),
                            value: f.wifi_pass_text.clone(),
                            focused: f.wifi_pass_focused,
                        },
                        Hit::WifiPassField,
                        ROW_H,
                    );
                    let pw2 = (cw - CARD_PAD * 2 - PILL_GAP) / 2;
                    b.y += 4;
                    b.place(
                        View::Pill {
                            label: "Connect",
                            selected: true,
                        },
                        Hit::WifiPassConnect,
                        Rect::new(x + CARD_PAD, b.y, pw2, PILL_H),
                    );
                    b.place(
                        View::Pill {
                            label: "Cancel",
                            selected: false,
                        },
                        Hit::WifiPassCancel,
                        Rect::new(x + CARD_PAD + pw2 + PILL_GAP, b.y, pw2, PILL_H),
                    );
                    b.y += PILL_H + CARD_PAD;
                }

                for (i, net) in f.wifi_nets.iter().enumerate() {
                    let tag = if net.active {
                        format!("{} · Connected ({}%)", net.ssid, net.signal)
                    } else if net.saved {
                        format!("{} · Saved ({}%)", net.ssid, net.signal)
                    } else if net.secured {
                        format!("{} · 🔒 {}%", net.ssid, net.signal)
                    } else {
                        format!("{} · {}%", net.ssid, net.signal)
                    };
                    b.row(
                        View::KeyRow {
                            name: tag,
                            installed: true,
                            active: net.active,
                        },
                        Hit::WifiPick(i),
                        KEY_H,
                    );
                }
            }
        }
    }
    if want("Bluetooth") {
        let bt_sub = match f.bt {
            Some(true) => f
                .bt_device
                .as_ref()
                .map(|d| format!("Connected: {d}"))
                .unwrap_or_else(|| "On".into()),
            Some(false) => "Off".into(),
            None => "Unknown".into(),
        };
        b.row(
            View::Switch {
                icon: icons::BLUETOOTH,
                title: "Bluetooth",
                sub: Some(bt_sub),
                on: f.bt.unwrap_or(false),
                unknown: f.bt.is_none(),
            },
            Hit::Bluetooth,
            ROW_H,
        );
        if f.bt == Some(true) && !filtering {
            b.divider();
            b.row(
                View::Nav {
                    icon: icons::BLUETOOTH,
                    title: "Bluetooth devices",
                    value: f.bt_device.clone().unwrap_or_else(|| "Manage".into()),
                    open: open.bluetooth,
                },
                Hit::BtDevicesToggle,
                ROW_H,
            );
            if open.bluetooth {
                b.divider();
                let pw = cw - CARD_PAD * 2;
                b.y += CARD_PAD;
                b.place(
                    View::Pill {
                        label: if f.bt_scanning {
                            "Scanning..."
                        } else {
                            "Scan devices"
                        },
                        selected: f.bt_scanning,
                    },
                    Hit::BtRescan,
                    Rect::new(x + CARD_PAD, b.y, pw, PILL_H),
                );
                b.y += PILL_H + CARD_PAD;
                for (i, dev) in f.bt_devs.iter().enumerate() {
                    let label = if dev.connected {
                        format!("{} · Connected", dev.name)
                    } else if dev.paired {
                        format!("{} · Paired", dev.name)
                    } else {
                        dev.name.clone()
                    };
                    b.row(
                        View::KeyRow {
                            name: label,
                            installed: true,
                            active: dev.connected,
                        },
                        Hit::BtPick(i),
                        KEY_H,
                    );
                }
            }
        }
    }
    finish(&mut items, &mut y, "Connectivity", b);

    // ---- Notifications ------------------------------------------------
    let mut b = CardBuilder::new(x, cw, y + SECTION_H);
    if want("Do Not Disturb") {
        b.row(
            View::Switch {
                icon: icons::DO_NOT_DISTURB,
                title: "Do Not Disturb",
                sub: Some("Pause notifications".into()),
                on: f.dnd,
                unknown: false,
            },
            Hit::Dnd,
            ROW_H,
        );
    }
    finish(&mut items, &mut y, "Notifications", b);

    // ---- Sound -------------------------------------------------------
    let mut b = CardBuilder::new(x, cw, y + SECTION_H);
    if want("Volume") {
        b.row(
            View::Slider {
                icon: icons::VOLUME_UP,
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
        );
    }
    if want("Mute") {
        b.row(
            View::Switch {
                icon: icons::VOLUME_OFF,
                title: "Mute",
                sub: Some("Silence all audio".into()),
                on: f.muted,
                unknown: f.volume.is_none(),
            },
            Hit::Mute,
            ROW_H,
        );
    }
    finish(&mut items, &mut y, "Sound", b);

    // ---- Display -----------------------------------------------------
    let mut b = CardBuilder::new(x, cw, y + SECTION_H);
    if want("Dark theme") {
        b.row(
            View::Switch {
                icon: icons::DARK_MODE,
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
        );
    }
    if want("Brightness") {
        b.row(
            View::Slider {
                icon: icons::BRIGHTNESS,
                title: "Brightness",
                frac: f.brightness,
                value: f.brightness.map_or_else(|| "—".into(), pct),
            },
            Hit::Brightness,
            SLIDER_H,
        );
    }
    finish(&mut items, &mut y, "Display", b);

    // ---- Personalization ----------------------------------------------
    let mut b = CardBuilder::new(x, cw, y + SECTION_H);
    if want("Wallpaper") {
        b.row(
            View::Nav {
                icon: icons::WALLPAPER,
                title: "Wallpaper",
                value: wallpaper_name(&f.wallpaper),
                open: open.wallpaper,
            },
            Hit::WallpaperToggle,
            ROW_H,
        );
        if open.wallpaper && !filtering {
            b.divider();
            let ids = de_paint::wallpaper::IDS;
            let n = ids.len() as i32;
            let sw = (cw - CARD_PAD * 2 - (n - 1) * GAP) / n;
            b.y += CARD_PAD;
            for (i, id) in ids.iter().enumerate() {
                let rx = x + CARD_PAD + i as i32 * (sw + GAP);
                b.place(
                    View::Swatch {
                        index: i,
                        name: wallpaper_name(id),
                        selected: *id == f.wallpaper,
                    },
                    Hit::WallpaperPick(i),
                    Rect::new(rx, b.y, sw, SWATCH_H),
                );
            }
            b.y += SWATCH_H + CARD_PAD;
        }
    }
    finish(&mut items, &mut y, "Personalization", b);

    // ---- System --------------------------------------------------------
    let mut b = CardBuilder::new(x, cw, y + SECTION_H);
    if want("Screen timeout") {
        b.row(
            View::Nav {
                icon: icons::SCHEDULE,
                title: "Screen timeout",
                value: timeout_label(f.timeout_sec),
                open: open.timeout,
            },
            Hit::TimeoutToggle,
            ROW_H,
        );
        if open.timeout && !filtering {
            b.divider();
            let pw =
                (cw - CARD_PAD * 2 - (TIMEOUT_COLS as i32 - 1) * PILL_GAP) / TIMEOUT_COLS as i32;
            let rows = TIMEOUT_OPTIONS.len().div_ceil(TIMEOUT_COLS);
            b.y += CARD_PAD;
            for (ri, chunk) in TIMEOUT_OPTIONS.chunks(TIMEOUT_COLS).enumerate() {
                for (ci, (sec, label)) in chunk.iter().enumerate() {
                    let rx = x + CARD_PAD + ci as i32 * (pw + PILL_GAP);
                    let ry = b.y + ri as i32 * (PILL_H + PILL_GAP);
                    b.place(
                        View::Pill {
                            label,
                            selected: f.timeout_sec == *sec,
                        },
                        Hit::TimeoutPick(*sec),
                        Rect::new(rx, ry, pw, PILL_H),
                    );
                }
            }
            b.y += rows as i32 * PILL_H + (rows as i32 - 1) * PILL_GAP + CARD_PAD;
        }
    }
    if want("Search engine") {
        b.row(
            View::Nav {
                icon: icons::SEARCH,
                title: "Search engine",
                value: de_config::prefs::search_engine_label(&f.search_engine).to_string(),
                open: open.search_engine,
            },
            Hit::SearchEngineToggle,
            ROW_H,
        );
        if open.search_engine && !filtering {
            b.divider();
            let cols = 2usize;
            let pw = (cw - CARD_PAD * 2 - (cols as i32 - 1) * PILL_GAP) / cols as i32;
            let engines = de_config::prefs::SEARCH_ENGINES;
            let rows = engines.len().div_ceil(cols);
            b.y += CARD_PAD;
            for (ri, chunk) in engines.chunks(cols).enumerate() {
                for (ci, (id, label, _)) in chunk.iter().enumerate() {
                    let idx = ri * cols + ci;
                    let rx = x + CARD_PAD + ci as i32 * (pw + PILL_GAP);
                    let ry = b.y + ri as i32 * (PILL_H + PILL_GAP);
                    b.place(
                        View::Pill {
                            label,
                            selected: f.search_engine.eq_ignore_ascii_case(id),
                        },
                        Hit::SearchEnginePick(idx),
                        Rect::new(rx, ry, pw, PILL_H),
                    );
                }
            }
            b.y += rows as i32 * PILL_H + (rows as i32 - 1) * PILL_GAP + CARD_PAD;
        }
    }
    if want("Keyboard") {
        b.row(
            View::Nav {
                icon: ICON_KEYBOARD,
                title: "Keyboard",
                value: f.keyboard_name.clone(),
                open: open.keyboard,
            },
            Hit::KeyboardToggle,
            ROW_H,
        );
        if open.keyboard && !filtering {
            b.divider();
            b.row(
                View::Switch {
                    icon: icons::PHONE_ANDROID,
                    title: "Keypress vibration",
                    sub: Some("Haptic feedback on key tap".into()),
                    on: f.osk_haptic,
                    unknown: false,
                },
                Hit::OskHaptic,
                ROW_H,
            );
            b.divider();
            b.row(
                View::Switch {
                    icon: icons::VOLUME_UP,
                    title: "Keypress sound",
                    sub: Some("Click sound on key tap".into()),
                    on: f.osk_sound,
                    unknown: false,
                },
                Hit::OskSound,
                ROW_H,
            );
            b.divider();
            b.row(
                View::Switch {
                    icon: icons::SECURITY,
                    title: "Allow 3rd-party keyboards",
                    sub: None,
                    on: f.third_party,
                    unknown: false,
                },
                Hit::ThirdParty,
                ROW_H,
            );
            for (i, k) in f.keyboards.iter().enumerate() {
                b.row(
                    View::KeyRow {
                        name: k.name.clone(),
                        installed: k.installed,
                        active: k.active,
                    },
                    Hit::KeyboardPick(i),
                    KEY_H,
                );
            }
        }
    }
    finish(&mut items, &mut y, "System", b);

    // ---- Footer notes ---------------------------------------------------
    if !filtering {
        for note in [
            "Saved prefs apply the next time the desktop starts.",
            "Brightness, volume, Wi-Fi and Bluetooth apply immediately.",
        ] {
            items.push(Item {
                view: View::Note(note),
                hit: Hit::None,
                r: Rect::new(x, y, cw, 24),
                top: false,
                bot: false,
            });
            y += 24;
        }
    }

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
                round_top: it.top,
                round_bot: it.bot,
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
            wifi_ssid: Some("HomeWiFi".into()),
            wifi_nets: Vec::new(),
            wifi_scanning: false,
            wifi_pass_ssid: None,
            wifi_pass_text: String::new(),
            wifi_pass_focused: false,
            bt: Some(false),
            bt_device: None,
            bt_devs: Vec::new(),
            bt_scanning: false,
            wallpaper: "ember".into(),
            timeout_sec: 600,
            search_engine: "google".into(),
            keyboard_name: "Rabbiz Keyboard".into(),
            osk_haptic: true,
            osk_sound: true,
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
        let (rows, _) = layout(Size::new(720, 1600), 100, 0, &Open::default(), &facts(), "");
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
                "Connectivity",
                "Notifications",
                "Sound",
                "Display",
                "Personalization",
                "System"
            ]
        );
    }

    #[test]
    fn expanded_sections_add_rows() {
        let open = Open {
            wifi: false,
            bluetooth: false,
            wallpaper: true,
            timeout: true,
            search_engine: true,
            keyboard: true,
        };
        // Tall viewport so nothing is culled — we assert on row presence.
        let (rows, content_h) = layout(Size::new(720, 4000), 100, 0, &open, &facts(), "");
        assert!(rows.iter().any(|p| matches!(p.view, View::Swatch { .. })));
        assert!(rows.iter().any(|p| matches!(p.view, View::Pill { .. })));
        assert!(rows.iter().any(|p| matches!(p.view, View::KeyRow { .. })));
        assert!(
            content_h > 1600,
            "expanded content should need scrolling on the panel"
        );
    }

    #[test]
    fn search_filters_rows_and_drops_empty_sections() {
        let f = facts();
        let (rows, _) = layout(Size::new(720, 4000), 100, 0, &Open::default(), &f, "wi");
        let titles: Vec<&str> = rows
            .iter()
            .filter_map(|p| match &p.view {
                View::Switch { title, .. }
                | View::Slider { title, .. }
                | View::Nav { title, .. } => Some(*title),
                _ => None,
            })
            .collect();
        // "wi" matches Wi-Fi only ("wallpaper" does not contain "wi").
        assert_eq!(titles, ["Wi-Fi"]);

        let (rows, _) = layout(Size::new(720, 4000), 100, 0, &Open::default(), &f, "wall");
        let titles: Vec<&str> = rows
            .iter()
            .filter_map(|p| match &p.view {
                View::Switch { title, .. }
                | View::Slider { title, .. }
                | View::Nav { title, .. } => Some(*title),
                _ => None,
            })
            .collect();
        assert_eq!(titles, ["Wallpaper"]);
        // Sections with no matching rows are dropped entirely.
        let sections: Vec<&str> = rows
            .iter()
            .filter_map(|p| match &p.view {
                View::Section(s) => Some(*s),
                _ => None,
            })
            .collect();
        assert!(!sections.contains(&"Sound"));
        assert!(!sections.contains(&"Notifications"));
        // No matches at all → no sections, no notes.
        let (rows, _) = layout(Size::new(720, 4000), 100, 0, &Open::default(), &f, "zzz");
        assert!(!rows.iter().any(|p| matches!(p.view, View::Section(_))));
        assert!(!rows.iter().any(|p| matches!(p.view, View::Note(_))));
    }

    #[test]
    fn every_card_row_has_a_leading_icon_and_card_bg() {
        let (rows, _) = layout(Size::new(720, 4000), 100, 0, &Open::default(), &facts(), "");
        for p in &rows {
            match &p.view {
                View::Switch { icon, .. } | View::Slider { icon, .. } | View::Nav { icon, .. } => {
                    assert!(!icon.is_empty(), "row missing leading icon");
                }
                _ => {}
            }
        }
        assert!(rows.iter().any(|p| matches!(p.view, View::CardBg)));
        assert!(rows.iter().any(|p| matches!(p.view, View::Hairline)));
    }

    #[test]
    fn scroll_translates_and_culls() {
        let f = facts();
        let open = Open::default();
        let (_, h0) = layout(Size::new(720, 1600), 100, 0, &open, &f, "");
        let (rows, _) = layout(Size::new(720, 1600), 100, h0, &open, &f, "");
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
