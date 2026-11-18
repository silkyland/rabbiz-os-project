//! M3-T7: prefs file behaviour — legacy seeding, atomic save, backup, no
//! legacy writes.

use std::path::PathBuf;

use de_config::prefs::Prefs;

fn tmpdir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("rabbiz-de-prefs-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn defaults_match_decision_14() {
    let p = Prefs::default();
    assert_eq!(p.accent, "copper");
    assert_eq!(p.wallpaper_id, "ember");
    assert_eq!(p.theme_mode, "light");
    assert!(p.hidden_apps.is_empty() && p.dock.is_empty());
}

#[test]
fn seeds_from_legacy_once_and_never_writes_it() {
    let dir = tmpdir("legacy");
    let legacy = dir.join("a21s-shell").join("prefs.json");
    std::fs::create_dir_all(legacy.parent().unwrap()).unwrap();
    std::fs::write(
        &legacy,
        r#"{"wallpaperId":"dusk","accentId":"crimson","lightTheme":false}"#,
    )
    .unwrap();

    let own = dir.join("rabbiz-de").join("prefs.json");
    let p = Prefs::load_from(&own, &legacy);
    assert_eq!(p.wallpaper_id, "dusk");
    assert_eq!(p.accent, "crimson");
    // Legacy dark default does NOT override the Cream Glass light default.
    assert_eq!(p.theme_mode, "light");

    // Saving the DE prefs must not touch the legacy file.
    let before = std::fs::read_to_string(&legacy).unwrap();
    let mut p2 = p.clone();
    p2.pin_dock("a21s-calculator");
    p2.save_to(&own).unwrap();
    assert_eq!(std::fs::read_to_string(&legacy).unwrap(), before);
}

#[test]
fn unknown_wallpaper_id_passes_through_loader_falls_back() {
    let dir = tmpdir("unknown-wp");
    let legacy = dir.join("prefs.json");
    std::fs::write(&legacy, r#"{"wallpaperId":"nebula"}"#).unwrap();
    let p = Prefs::load_from(&dir.join("own.json"), &legacy);
    assert_eq!(p.wallpaper_id, "nebula"); // loader (de-paint) falls back to ember
}

#[test]
fn save_is_atomic_and_backs_up() {
    let dir = tmpdir("atomic");
    let own = dir.join("prefs.json");

    let mut p = Prefs::default();
    p.pin_dock("a21s-files");
    p.save_to(&own).unwrap();
    assert!(own.exists());
    assert!(!own.with_extension("json.tmp").exists());
    // First write has nothing to back up.
    assert!(!own.with_extension("json.bak").exists());

    p.set_hidden("org.kde.dolphin", true);
    p.save_to(&own).unwrap();
    let bak = own.with_extension("json.bak");
    assert!(bak.exists());
    let restored = Prefs::from_json(&std::fs::read_to_string(&bak).unwrap()).unwrap();
    assert_eq!(restored.dock, vec!["a21s-files"]);
    assert!(!restored.is_hidden("org.kde.dolphin"));

    let again = Prefs::from_json(&std::fs::read_to_string(&own).unwrap()).unwrap();
    assert!(again.is_hidden("org.kde.dolphin"));
}

#[test]
fn dock_pin_dedupes_and_caps_at_six() {
    let mut p = Prefs::default();
    for i in 0..8 {
        p.pin_dock(&format!("app{i}"));
    }
    // Full dock refuses new pins (you unpin first, like iOS/Android).
    assert_eq!(p.dock.len(), 6);
    assert_eq!(p.dock[0], "app0");
    assert_eq!(p.dock[5], "app5");
    assert!(!p.dock.iter().any(|d| d == "app7"));

    // Re-pin an existing entry moves it to the end.
    p.pin_dock("app1");
    assert_eq!(p.dock.iter().filter(|d| *d == "app1").count(), 1);
    assert_eq!(p.dock[5], "app1");

    // Unpin frees a slot.
    p.unpin_dock("app0");
    p.pin_dock("app9");
    assert_eq!(p.dock[5], "app9");
}

#[test]
fn json_roundtrip_uses_de_keys() {
    let mut p = Prefs::default();
    p.pin_dock("a21s-calculator");
    p.set_hidden("foo", true);
    let text = p.to_json();
    assert!(text.contains("\"deAccent\""));
    assert!(text.contains("\"deWallpaperId\""));
    assert!(text.contains("\"hiddenApps\""));
    let back = Prefs::from_json(&text).unwrap();
    assert_eq!(back, p);
}
