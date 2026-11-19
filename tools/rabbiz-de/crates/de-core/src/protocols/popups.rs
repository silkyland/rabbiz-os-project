//! xdg popups: the surfaces apps use for context menus, drop-downs and tooltips.
//!
//! Placement is kept on the screen, an interactive popup takes a grab (keyboard focus, and a click outside
//! dismisses it), touches outside dismiss it too, and popups are drawn above their window and included in
//! captures. See `docs/DE-DESKTOP-MODE.md`, Phase A.

use smithay::desktop::{
    find_popup_root_surface, get_popup_toplevel_coords, PopupGrab, PopupKeyboardGrab, PopupKind,
    PopupManager, PopupPointerGrab, PopupUngrabStrategy, Window,
};
use smithay::input::pointer::Focus;
use smithay::input::Seat;
use smithay::reexports::wayland_server::protocol::{wl_seat, wl_surface::WlSurface};
use smithay::utils::{Logical, Point, Rectangle, Serial, Size};
use smithay::wayland::compositor::get_parent;
use smithay::wayland::shell::xdg::PopupSurface;

use crate::input::SurfaceHit;
use crate::state::De;

/// The rectangle a popup must stay inside, in the popup's parent's coordinates: the whole output, shifted by
/// where the parent sits relative to the output and by the popup's offset from its toplevel.
#[must_use]
pub(crate) fn popup_target(
    output: Size<i32, Logical>,
    popup_in_toplevel: Point<i32, Logical>,
    window_origin: Point<i32, Logical>,
) -> Rectangle<i32, Logical> {
    Rectangle::new(
        Point::from((
            -popup_in_toplevel.x - window_origin.x,
            -popup_in_toplevel.y - window_origin.y,
        )),
        output,
    )
}

/// Popups of `window` with the offset at which each one is drawn, relative to the window's drawing origin.
pub(crate) fn popups_of(window: &Window) -> Vec<(PopupKind, Point<i32, Logical>)> {
    let Some(root) = window.toplevel().map(|t| t.wl_surface().clone()) else {
        return Vec::new();
    };
    PopupManager::popups_for_surface(&root)
        .map(|(popup, at)| {
            let offset = window.geometry().loc + at - popup.geometry().loc;
            (popup, offset)
        })
        .collect()
}

impl De {
    /// Slide or flip a popup so it stays on the output (the client's positioner says how).
    pub(crate) fn unconstrain_popup(&self, popup: &PopupSurface) {
        let kind = PopupKind::from(popup.clone());
        let Ok(root) = find_popup_root_surface(&kind) else {
            return;
        };
        let Some(window) = self.window_for_surface(&root) else {
            return;
        };
        let origin = self
            .space
            .element_geometry(&window)
            .map_or_else(Point::default, |g| g.loc);
        let target = popup_target(
            Size::from((self.output.width as i32, self.output.height as i32)),
            get_popup_toplevel_coords(&kind),
            origin,
        );
        popup.with_pending_state(|state| {
            state.geometry = state.positioner.get_unconstrained_geometry(target);
        });
    }

    /// An interactive popup (a menu) asked for a grab: give it the keyboard and let a click outside dismiss it.
    pub(crate) fn grab_popup(
        &mut self,
        surface: PopupSurface,
        seat: &wl_seat::WlSeat,
        serial: Serial,
    ) {
        let Some(seat) = Seat::<De>::from_resource(seat) else {
            return;
        };
        let kind = PopupKind::Xdg(surface);
        let Ok(root) = find_popup_root_surface(&kind) else {
            return;
        };
        let Ok(mut grab) = self.popups.grab_popup::<De>(root, kind, &seat, serial) else {
            return;
        };
        if let Some(keyboard) = seat.get_keyboard() {
            if keyboard.is_grabbed()
                && !(keyboard.has_grab(serial)
                    || keyboard.has_grab(grab.previous_serial().unwrap_or(serial)))
            {
                grab.ungrab(PopupUngrabStrategy::All);
                return;
            }
            keyboard.set_focus(self, grab.current_grab(), serial);
            keyboard.set_grab(self, PopupKeyboardGrab::new(&grab), serial);
        }
        if let Some(pointer) = seat.get_pointer() {
            if pointer.is_grabbed()
                && !(pointer.has_grab(serial)
                    || pointer.has_grab(grab.previous_serial().unwrap_or_else(|| grab.serial())))
            {
                grab.ungrab(PopupUngrabStrategy::All);
                return;
            }
            pointer.set_grab(self, PopupPointerGrab::new(&grab), serial, Focus::Keep);
        }
        self.popup_grab = Some(grab);
        self.needs_present = true;
    }

    /// A touch landed while a popup grab may be active. Smithay has no touch popup grab, so a touch outside the
    /// popup tree dismisses the menu and is consumed (it does not also activate what was underneath).
    /// Returns `true` when the touch was consumed.
    pub(crate) fn dismiss_popup_for_touch(&mut self, hit: Option<&SurfaceHit>) -> bool {
        if self.popup_grab.as_ref().is_some_and(PopupGrab::has_ended) {
            self.popup_grab = None;
        }
        if self.popup_grab.is_none() {
            return false;
        }
        if hit.is_some_and(|h| h.above_windows || self.in_popup_tree(&h.surface)) {
            return false;
        }
        if let Some(mut grab) = self.popup_grab.take() {
            grab.ungrab(PopupUngrabStrategy::All);
        }
        self.needs_present = true;
        true
    }

    /// Whether `surface` belongs to a popup (or one of its subsurfaces).
    fn in_popup_tree(&self, surface: &WlSurface) -> bool {
        let mut root = surface.clone();
        while let Some(parent) = get_parent(&root) {
            root = parent;
        }
        self.popups.find_popup(&root).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_target_is_the_output_in_the_parents_coordinates() {
        let out = Size::from((720, 1600));
        // popup at (100, 200) inside a window drawn at the output origin
        let t = popup_target(out, Point::from((100, 200)), Point::from((0, 0)));
        assert_eq!(
            (t.loc.x, t.loc.y, t.size.w, t.size.h),
            (-100, -200, 720, 1600)
        );
    }

    #[test]
    fn a_window_that_is_not_at_the_origin_shifts_the_target_the_other_way() {
        let t = popup_target(
            Size::from((720, 1600)),
            Point::from((0, 0)),
            Point::from((30, 40)),
        );
        assert_eq!((t.loc.x, t.loc.y), (-30, -40));
    }
}
