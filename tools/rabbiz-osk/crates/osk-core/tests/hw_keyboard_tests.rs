use osk_core::hw_keyboard_mode;

/// `RABBIZ_HW_KEYBOARD` presence (any value, including empty) selects hardware-keyboard
/// mode; absence selects the on-screen window. One test so env mutation stays serialized.
#[test]
fn hw_keyboard_mode_follows_env_presence() {
    std::env::remove_var("RABBIZ_HW_KEYBOARD");
    assert!(!hw_keyboard_mode());

    std::env::set_var("RABBIZ_HW_KEYBOARD", "1");
    assert!(hw_keyboard_mode());

    // Presence, not value, is the contract: an empty value still counts as set.
    std::env::set_var("RABBIZ_HW_KEYBOARD", "");
    assert!(hw_keyboard_mode());

    std::env::remove_var("RABBIZ_HW_KEYBOARD");
    assert!(!hw_keyboard_mode());
}
