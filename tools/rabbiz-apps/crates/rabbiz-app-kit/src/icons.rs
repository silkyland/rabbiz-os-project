//! Shared Material Icons Round codepoints (verified against the bundled
//! `MaterialIconsRound-Regular.ttf` cmap in `tools/rabbiz-de/assets/fonts/`).
//!
//! Centralizes commonly duplicated icon constants across `rabbiz-*` apps.

pub const BACK: &str = "\u{e5c4}";
pub const CLOSE: &str = "\u{e5cd}";
pub const SEARCH: &str = "\u{e8b6}";
pub const ADD: &str = "\u{e145}";
pub const DELETE: &str = "\u{e872}";
pub const EDIT: &str = "\u{e3c9}";
pub const CHECK: &str = "\u{e5ca}";
pub const CALL: &str = "\u{e0b0}";
pub const MESSAGE: &str = "\u{e0c9}";
pub const PLAY: &str = "\u{e037}";
pub const PAUSE: &str = "\u{e034}";
pub const SKIP_NEXT: &str = "\u{e044}";
pub const SKIP_PREV: &str = "\u{e045}";
pub const FOLDER: &str = "\u{e2c7}";
pub const FILE: &str = "\u{e24d}";
pub const SETTINGS: &str = "\u{e8b8}";
pub const SHARE: &str = "\u{e80d}";
pub const STAR: &str = "\u{e838}";
pub const STAR_OUTLINE: &str = "\u{e83a}";
pub const MENU: &str = "\u{e5d2}";
pub const CHEVRON_RIGHT: &str = "\u{e5cc}";
pub const CHEVRON_LEFT: &str = "\u{e5cb}";
pub const MORE_VERT: &str = "\u{e5d4}";
pub const REFRESH: &str = "\u{e5d5}";
pub const WARNING: &str = "\u{e002}";
pub const INFO: &str = "\u{e88e}";
pub const ERROR: &str = "\u{e000}";
pub const CHECK_CIRCLE: &str = "\u{e86c}";
pub const HISTORY: &str = "\u{e889}";
pub const SEND: &str = "\u{e163}";
pub const BACKSPACE: &str = "\u{e14a}";
pub const CONTENT_COPY: &str = "\u{e14d}";
pub const SAVE: &str = "\u{e161}";
pub const NOTE: &str = "\u{e06f}";
pub const DESCRIPTION: &str = "\u{e873}";
pub const SHUFFLE: &str = "\u{e043}";
pub const REPEAT: &str = "\u{e040}";
pub const REPEAT_ONE: &str = "\u{e041}";
pub const STOP: &str = "\u{e047}";
pub const MUSIC_NOTE: &str = "\u{e405}";
pub const ALBUM: &str = "\u{e019}";
pub const QUEUE_MUSIC: &str = "\u{e03d}";
pub const HEADSET: &str = "\u{e310}";
pub const AUDIO_FILE: &str = "\u{eb82}";
pub const MOVIE: &str = "\u{e02c}";
pub const CALL_END: &str = "\u{e0b1}";
pub const MIC: &str = "\u{e029}";
pub const MIC_OFF: &str = "\u{e02b}";
/// `dialpad` glyph (the on-screen keypad grid). NB: U+E0CF is
/// `contact_phone`, not the dialpad.
pub const DIALPAD: &str = "\u{e0bc}";
pub const PERSON: &str = "\u{e7fd}";
pub const GROUP: &str = "\u{e7ef}";
pub const IMAGE: &str = "\u{e3f4}";
pub const PHOTO: &str = "\u{e410}";
pub const PHOTO_LIBRARY: &str = "\u{e413}";
pub const CAMERA: &str = "\u{e3b0}";
pub const VIDEOCAM: &str = "\u{e04b}";
pub const GRID_VIEW: &str = "\u{e9b0}";
pub const LIST: &str = "\u{e896}";
pub const EXPAND_MORE: &str = "\u{e5cf}";
pub const EXPAND_LESS: &str = "\u{e5ce}";
pub const ARROW_UPWARD: &str = "\u{e5d8}";
pub const ARROW_DOWNWARD: &str = "\u{e5db}";
pub const VOLUME_UP: &str = "\u{e050}";
pub const VOLUME_DOWN: &str = "\u{e04d}";
pub const VOLUME_OFF: &str = "\u{e04f}";
pub const FOLDER_OPEN: &str = "\u{e2c8}";
/// `file_download` (tray + arrow). NB: U+E090 does not exist in the bundled font.
pub const DOWNLOAD: &str = "\u{e2c4}";
pub const ZOOM_IN: &str = "\u{e8ff}";
pub const ZOOM_OUT: &str = "\u{e900}";
pub const TIMER: &str = "\u{e425}";
pub const ALARM: &str = "\u{e855}";
/// `av_timer` — the closest stopwatch-shaped glyph in the bundled font
/// (there is no `stopwatch`; U+E01C is `closed_caption`).
pub const STOPWATCH: &str = "\u{e01b}";
pub const SCHEDULE: &str = "\u{e8b5}";
pub const SNOOZE: &str = "\u{e046}";
pub const WIFI: &str = "\u{e63e}";
pub const BLUETOOTH: &str = "\u{e1a7}";
/// `signal_cellular_alt` — handset-style signal bars.
pub const SIGNAL_CELLULAR: &str = "\u{e202}";
pub const AIRPLANE_MODE: &str = "\u{e195}";
pub const BRIGHTNESS: &str = "\u{e1ac}";
pub const FLASHLIGHT: &str = "\u{f00b}";
pub const DARK_MODE: &str = "\u{e51c}";
pub const LIGHT_MODE: &str = "\u{e518}";
pub const DO_NOT_DISTURB: &str = "\u{e612}";
pub const WALLPAPER: &str = "\u{e1bc}";
pub const LOCK: &str = "\u{e897}";
pub const NOTIFICATIONS: &str = "\u{e7f4}";
pub const STORAGE: &str = "\u{e1db}";
pub const SECURITY: &str = "\u{e32a}";
pub const ACCESSIBILITY: &str = "\u{e84e}";
/// `system_update` (circular arrows). NB: U+E8D7 is `system_update_alt`.
pub const SYSTEM_UPDATE: &str = "\u{e62a}";
pub const APPS: &str = "\u{e5c3}";
pub const PHONE_ANDROID: &str = "\u{e324}";
pub const SIM_CARD: &str = "\u{e32b}";
pub const LANGUAGE: &str = "\u{e894}";
/// `battery_full` — the filled battery glyph.
pub const BATTERY: &str = "\u{e1a4}";
pub const BATTERY_ALERT: &str = "\u{e19c}";
pub const PIN_DROP: &str = "\u{e55e}";
pub const PUSH_PIN: &str = "\u{f045}";
pub const LOCATION: &str = "\u{e0c8}";
pub const CALENDAR: &str = "\u{e935}";
pub const HOME: &str = "\u{e88a}";
pub const VISIBILITY: &str = "\u{e8f4}";
pub const VISIBILITY_OFF: &str = "\u{e8f5}";
pub const POWER_SETTINGS_NEW: &str = "\u{e8ac}";
pub const RESTART_ALT: &str = "\u{f053}";
pub const FILTER_LIST: &str = "\u{e152}";
pub const SORT: &str = "\u{e164}";
pub const RADIO_BUTTON_CHECKED: &str = "\u{e837}";
pub const RADIO_BUTTON_UNCHECKED: &str = "\u{e836}";

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: &[(&str, &str)] = &[
        ("BACK", BACK),
        ("CLOSE", CLOSE),
        ("SEARCH", SEARCH),
        ("ADD", ADD),
        ("DELETE", DELETE),
        ("EDIT", EDIT),
        ("CHECK", CHECK),
        ("CALL", CALL),
        ("MESSAGE", MESSAGE),
        ("PLAY", PLAY),
        ("PAUSE", PAUSE),
        ("SKIP_NEXT", SKIP_NEXT),
        ("SKIP_PREV", SKIP_PREV),
        ("FOLDER", FOLDER),
        ("FILE", FILE),
        ("SETTINGS", SETTINGS),
        ("SHARE", SHARE),
        ("STAR", STAR),
        ("STAR_OUTLINE", STAR_OUTLINE),
        ("MENU", MENU),
        ("CHEVRON_RIGHT", CHEVRON_RIGHT),
        ("CHEVRON_LEFT", CHEVRON_LEFT),
        ("MORE_VERT", MORE_VERT),
        ("REFRESH", REFRESH),
        ("WARNING", WARNING),
        ("INFO", INFO),
        ("ERROR", ERROR),
        ("CHECK_CIRCLE", CHECK_CIRCLE),
        ("HISTORY", HISTORY),
        ("SEND", SEND),
        ("BACKSPACE", BACKSPACE),
        ("CONTENT_COPY", CONTENT_COPY),
        ("SAVE", SAVE),
        ("NOTE", NOTE),
        ("DESCRIPTION", DESCRIPTION),
        ("SHUFFLE", SHUFFLE),
        ("REPEAT", REPEAT),
        ("REPEAT_ONE", REPEAT_ONE),
        ("STOP", STOP),
        ("MUSIC_NOTE", MUSIC_NOTE),
        ("ALBUM", ALBUM),
        ("QUEUE_MUSIC", QUEUE_MUSIC),
        ("HEADSET", HEADSET),
        ("AUDIO_FILE", AUDIO_FILE),
        ("MOVIE", MOVIE),
        ("CALL_END", CALL_END),
        ("MIC", MIC),
        ("MIC_OFF", MIC_OFF),
        ("DIALPAD", DIALPAD),
        ("PERSON", PERSON),
        ("GROUP", GROUP),
        ("IMAGE", IMAGE),
        ("PHOTO", PHOTO),
        ("PHOTO_LIBRARY", PHOTO_LIBRARY),
        ("CAMERA", CAMERA),
        ("VIDEOCAM", VIDEOCAM),
        ("GRID_VIEW", GRID_VIEW),
        ("LIST", LIST),
        ("EXPAND_MORE", EXPAND_MORE),
        ("EXPAND_LESS", EXPAND_LESS),
        ("ARROW_UPWARD", ARROW_UPWARD),
        ("ARROW_DOWNWARD", ARROW_DOWNWARD),
        ("VOLUME_UP", VOLUME_UP),
        ("VOLUME_DOWN", VOLUME_DOWN),
        ("VOLUME_OFF", VOLUME_OFF),
        ("FOLDER_OPEN", FOLDER_OPEN),
        ("DOWNLOAD", DOWNLOAD),
        ("ZOOM_IN", ZOOM_IN),
        ("ZOOM_OUT", ZOOM_OUT),
        ("TIMER", TIMER),
        ("ALARM", ALARM),
        ("STOPWATCH", STOPWATCH),
        ("SCHEDULE", SCHEDULE),
        ("SNOOZE", SNOOZE),
        ("WIFI", WIFI),
        ("BLUETOOTH", BLUETOOTH),
        ("SIGNAL_CELLULAR", SIGNAL_CELLULAR),
        ("AIRPLANE_MODE", AIRPLANE_MODE),
        ("BRIGHTNESS", BRIGHTNESS),
        ("FLASHLIGHT", FLASHLIGHT),
        ("DARK_MODE", DARK_MODE),
        ("LIGHT_MODE", LIGHT_MODE),
        ("DO_NOT_DISTURB", DO_NOT_DISTURB),
        ("WALLPAPER", WALLPAPER),
        ("LOCK", LOCK),
        ("NOTIFICATIONS", NOTIFICATIONS),
        ("STORAGE", STORAGE),
        ("SECURITY", SECURITY),
        ("ACCESSIBILITY", ACCESSIBILITY),
        ("SYSTEM_UPDATE", SYSTEM_UPDATE),
        ("APPS", APPS),
        ("PHONE_ANDROID", PHONE_ANDROID),
        ("SIM_CARD", SIM_CARD),
        ("LANGUAGE", LANGUAGE),
        ("BATTERY", BATTERY),
        ("BATTERY_ALERT", BATTERY_ALERT),
        ("PIN_DROP", PIN_DROP),
        ("PUSH_PIN", PUSH_PIN),
        ("LOCATION", LOCATION),
        ("CALENDAR", CALENDAR),
        ("HOME", HOME),
        ("VISIBILITY", VISIBILITY),
        ("VISIBILITY_OFF", VISIBILITY_OFF),
        ("POWER_SETTINGS_NEW", POWER_SETTINGS_NEW),
        ("RESTART_ALT", RESTART_ALT),
        ("FILTER_LIST", FILTER_LIST),
        ("SORT", SORT),
        ("RADIO_BUTTON_CHECKED", RADIO_BUTTON_CHECKED),
        ("RADIO_BUTTON_UNCHECKED", RADIO_BUTTON_UNCHECKED),
    ];

    #[test]
    fn all_icons_rasterize_real_ink() {
        // Every codepoint was verified against the bundled TTF cmap, but a
        // PUA scalar that maps to `.notdef` would still pass the range test —
        // rasterize each and demand visible ink.
        for &(name, glyph) in ALL {
            let px = crate::text::icon(glyph, 24.0, [255, 255, 255, 255])
                .unwrap_or_else(|| panic!("{name} failed to rasterize"));
            let ink = px.data().chunks(4).filter(|c| c[3] > 10).count();
            assert!(ink > 20, "{name} rasterized with no ink ({ink})");
        }
    }

    #[test]
    fn all_icons_are_single_scalar_material_pua_codepoints() {
        for &(name, glyph) in ALL {
            assert!(!glyph.is_empty(), "{name} must not be empty");
            let mut chars = glyph.chars();
            let ch = chars.next().expect("non-empty icon");
            assert!(
                chars.next().is_none(),
                "{name} must be a single Unicode scalar"
            );
            let cp = u32::from(ch);
            assert!(
                (0xE000..=0xF8FF).contains(&cp),
                "{name} (U+{cp:04X}) must be in the Material PUA range U+E000..=U+F8FF"
            );
        }
    }
}
