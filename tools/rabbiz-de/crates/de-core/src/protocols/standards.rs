//! Stock freedesktop Wayland globals beyond the core shell set
//! (docs/AUDIT-DE-STANDARDS-2026-11-21 §2): every entry is a smithay-provided
//! state + delegate so a conformant third-party client finds the same API it
//! would on GNOME/KDE. None of these are privileged — they expose no input,
//! capture or shell surface.
//!
//! Deliberately absent (tracked in the audit): `ext_idle_notify_v1` needs
//! `IdleNotifierState<De>` timers on an `EventLoop<De>` (ours is
//! `EventLoop<Run>`), `zwp_pointer_gestures_v1` has no libinput gesture
//! forwarding yet, and `wp_security_context_v1` belongs to the M5 sandbox
//! work. `wp_presentation` is registered here; feedback is emitted next to
//! `send_frames_surface_tree` in `frame/render.rs`.

use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::wayland::keyboard_shortcuts_inhibit::{
    KeyboardShortcutsInhibitHandler, KeyboardShortcutsInhibitState, KeyboardShortcutsInhibitor,
};
use smithay::wayland::tablet_manager::TabletSeatHandler;
use smithay::wayland::xdg_foreign::{XdgForeignHandler, XdgForeignState};
use smithay::wayland::xdg_system_bell::XdgSystemBellHandler;
use smithay::wayland::xdg_toplevel_icon::XdgToplevelIconHandler;
use smithay::wayland::xdg_toplevel_tag::XdgToplevelTagHandler;

use crate::state::De;

// `wp_single_pixel_buffer_manager_v1` — 1×1 solid-colour buffers; no handler.
smithay::delegate_single_pixel_buffer!(De);

// `wp_cursor_shape_manager_v1` — clients name standard pointer shapes
// (desktop-mode pointer UX); state+delegate only, no handler trait.
smithay::delegate_cursor_shape!(De);

// Required by the cursor-shape dispatch chain (no tablet devices yet; all
// `TabletSeatHandler` methods have empty defaults).
impl TabletSeatHandler for De {}

// `zxdg_exporter_v1`/`zxdg_importer_v1` — portal parent-window handles.
impl XdgForeignHandler for De {
    fn xdg_foreign_state(&mut self) -> &mut XdgForeignState {
        &mut self.xdg_foreign_state
    }
}
smithay::delegate_xdg_foreign!(De);

// `wp_content_type_manager_v1` — per-surface video/photo/game hints
// (future adaptive-refresh/scanout input).
smithay::delegate_content_type!(De);

// `xdg_toplevel_icon_manager_v1` — client-provided window icons
// (`ToplevelIconCachedState` holds them for the recents/task surfaces).
impl XdgToplevelIconHandler for De {}
smithay::delegate_xdg_toplevel_icon!(De);

// `xdg_toplevel_tag_manager_v1` — client-supplied semantic tags
// (e.g. picture-in-picture); stored on the toplevel for shell policy.
impl XdgToplevelTagHandler for De {}
smithay::delegate_xdg_toplevel_tag!(De);

// `zwp_keyboard_shortcuts_inhibit_manager_v1` — games/remoting hold the
// keyboard. v1 policy: grant on request (a foreground-only window can never
// trap system gestures — our global shortcuts already only fire for the
// shell's own surfaces).
impl KeyboardShortcutsInhibitHandler for De {
    fn keyboard_shortcuts_inhibit_state(&mut self) -> &mut KeyboardShortcutsInhibitState {
        &mut self.kbd_shortcuts_inhibit_state
    }
    fn new_inhibitor(&mut self, inhibitor: KeyboardShortcutsInhibitor) {
        inhibitor.activate();
    }
}
smithay::delegate_keyboard_shortcuts_inhibit!(De);

// `xdg_system_bell_v1` — terminal/editor bell → soft haptic tick (the phone
// has no PC speaker; silent on hosts without the vibrator node).
impl XdgSystemBellHandler for De {
    fn ring(&mut self, _surface: Option<WlSurface>) {
        crate::runtime::haptics::pulse(crate::runtime::haptics::HapticKind::Bell);
    }
}
smithay::delegate_xdg_system_bell!(De);

// `wp_presentation` — frame feedback clock is CLOCK_MONOTONIC (clk_id=1),
// matching `Clock<Monotonic>` used by `send_frames_surface_tree`.
smithay::delegate_presentation!(De);
