use gtk4::gdk;
use gtk4::glib::translate::IntoGlib;

/// Converts a GDK keyval into an X11 keysym suitable for RFB KeyEvent.
/// GDK keyvals match X11 keysyms directly; we only filter out VoidSymbol (0xffffff).
pub fn keysym_from_gdk(keyval: gdk::Key) -> Option<u32> {
    let raw = keyval.into_glib();
    if raw == 0 || raw == 0x00FF_FFFF {
        return None;
    }
    Some(raw)
}
