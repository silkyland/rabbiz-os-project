//! Bundled fonts (M0-T5).
//!
//! Only the four embedded Noto faces (Regular/SemiBold, Latin/Thai) plus the
//! Material Icons Round icon font are loaded — the FontSystem performs **no
//! system-font fallback** so host goldens and device output shape identically.

use std::sync::{Mutex, OnceLock};

use cosmic_text::fontdb::Database;
use cosmic_text::FontSystem;

/// Noto Sans Latin — Regular (400).
pub const NOTO_SANS_REGULAR: &[u8] = include_bytes!("../../../assets/fonts/NotoSans-Regular.ttf");
/// Noto Sans Latin — SemiBold (600).
pub const NOTO_SANS_SEMIBOLD: &[u8] = include_bytes!("../../../assets/fonts/NotoSans-SemiBold.ttf");
/// Noto Sans Thai — Regular (400).
pub const NOTO_THAI_REGULAR: &[u8] =
    include_bytes!("../../../assets/fonts/NotoSansThai-Regular.ttf");
/// Noto Sans Thai — SemiBold (600).
pub const NOTO_THAI_SEMIBOLD: &[u8] =
    include_bytes!("../../../assets/fonts/NotoSansThai-SemiBold.ttf");
/// Material Icons Round (system glyphs, Apache 2.0).
pub const MATERIAL_ICONS: &[u8] =
    include_bytes!("../../../assets/fonts/MaterialIconsRound-Regular.otf");

/// Font family name used in `Attrs` for Latin/Thai body text.
pub const FAMILY_NOTO: &str = "Noto Sans";
/// Font family name for icon glyphs.
pub const FAMILY_ICONS: &str = "Material Icons Round";

/// All bundled font files, for building a `fontdb` of any version (resvg's
/// `usvg` uses a newer `fontdb` than `cosmic-text` 0.12).
pub const BUNDLED_FONTS: [&[u8]; 5] = [
    NOTO_SANS_REGULAR,
    NOTO_SANS_SEMIBOLD,
    NOTO_THAI_REGULAR,
    NOTO_THAI_SEMIBOLD,
    MATERIAL_ICONS,
];

/// A `fontdb::Database` containing only the bundled faces.
pub fn bundled_db() -> Database {
    let mut db = Database::new();
    for face in BUNDLED_FONTS {
        db.load_font_data(face.to_vec());
    }
    db
}

/// Shared `FontSystem` (cosmic-text is `!Sync`; guard with a `Mutex`).
pub fn font_system() -> &'static Mutex<FontSystem> {
    static FS: OnceLock<Mutex<FontSystem>> = OnceLock::new();
    FS.get_or_init(|| {
        Mutex::new(FontSystem::new_with_locale_and_db(
            "en".to_string(),
            bundled_db(),
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmic_text::fontdb::Query;
    use cosmic_text::{Family, Weight};

    #[test]
    fn bundled_faces_registered() {
        let fs = font_system().lock().unwrap();
        for (family, weight) in [
            (Family::Name(FAMILY_NOTO), Weight::NORMAL),
            (Family::Name(FAMILY_NOTO), Weight::SEMIBOLD),
            (Family::Name("Noto Sans Thai"), Weight::NORMAL),
            (Family::Name("Noto Sans Thai"), Weight::SEMIBOLD),
            (Family::Name(FAMILY_ICONS), Weight::NORMAL),
        ] {
            let q = Query {
                families: &[family],
                weight,
                ..Query::default()
            };
            assert!(fs.db().query(&q).is_some(), "missing {family:?} {weight:?}");
        }
    }
}
