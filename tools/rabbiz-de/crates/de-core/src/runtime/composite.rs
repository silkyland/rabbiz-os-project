//! CPU composite of the on-screen frame — shared by the debug frame dump
//! and the wlr-screencopy shm path.
//!
//! Only `wl_shm` buffers are readable here: dmabuf/EGL client content is
//! skipped (it needs a renderer readback, which does not exist yet). The
//! blit order mirrors the element list `render_with` submits — elements are
//! front-to-back, so drawing runs in reverse: below-layers, gesture bar
//! (behind app/shell, same as the submitted order), shell-or-app, then
//! above-layers on top.

use de_api::{Pixels, Point as ApiPoint, Size};
use de_paint::canvas::Canvas;
use smithay::backend::renderer::utils::with_renderer_surface_state;
use smithay::desktop::{layer_map_for_output, LayerMap, LayerSurface};
use smithay::reexports::wayland_server::protocol::{
    wl_buffer::WlBuffer, wl_shm, wl_surface::WlSurface,
};
use smithay::wayland::compositor::{
    with_states, with_surface_tree_downward, SubsurfaceCachedState, TraversalAction,
};
use smithay::wayland::seat::WaylandFocus;
use smithay::wayland::shm;

use crate::state::{De, Foreground};

/// Output clear colour, kept in sync with `frame::render_with`.
const CLEAR: [u8; 4] = [22, 16, 26, 255];

/// Compose the full output (physical pixels), or `None` before the shell
/// produced its first frame.
#[must_use]
pub fn composite_output(de: &De) -> Option<Pixels> {
    de.shell_px.as_ref()?;
    let mut canvas = Canvas::filled(
        Size::new(de.output.width as i32, de.output.height as i32),
        CLEAR,
    );
    let map = layer_map_for_output(&de.output_obj);
    blit_layers(
        &mut canvas,
        &map,
        de_protocols::layer_shell::BELOW_WINDOWS.iter().rev(),
    );

    if matches!(de.foreground, Foreground::App)
        && de.morph.is_none()
        && de.space.elements().next().is_some()
    {
        // `Space::elements()` yields bottom-to-top (back-to-front), matching
        // painter's-algorithm `canvas.blit()` so the topmost focused window
        // paints last on top of any background windows.
        for window in de.space.elements() {
            let loc = de.space.element_location(window).unwrap_or_default();
            let Some(surface) = window.wl_surface().map(std::borrow::Cow::into_owned) else {
                continue;
            };
            blit_surface(&mut canvas, &surface, ApiPoint::new(loc.x, loc.y));
            for (popup, offset) in crate::popups::popups_of(window) {
                blit_surface(
                    &mut canvas,
                    popup.wl_surface(),
                    ApiPoint::new(loc.x + offset.x, loc.y + offset.y),
                );
            }
        }
    } else if let Some(shell) = de.shell_px.as_ref() {
        let slide_y = if de.navigation.drawer_slide.value > 0.5 {
            de.navigation.drawer_slide.value
        } else {
            de.navigation.drawer_pull_y
        };
        if de.recents_layers_active() {
            let pull = if de.navigation.nav_recents_active {
                de.navigation.nav_recents_pull.value.clamp(0.0, 1.0)
            } else {
                1.0
            };
            if pull < 0.999 {
                if let Some(under) = de.navigation.home_underlay_px.as_ref() {
                    canvas.blit(under, ApiPoint::ZERO);
                } else {
                    canvas.blit(shell, ApiPoint::ZERO);
                }
            }
            if let Some(layers) = de.navigation.recents_layers.as_ref() {
                let out_size = canvas.size();
                let card_rise = ((1.0 - pull) * (out_size.h as f32 * 0.22)).round() as i32;
                if pull > 0.0 {
                    if let Some(bg) = layers.bg_px.as_ref() {
                        canvas.blit_scaled(
                            bg,
                            de_api::Rect::new(0, 0, bg.size.w, bg.size.h),
                            pull,
                        );
                    }
                    let pos = de.navigation.recents_pager.pos();
                    for (i, cache) in layers.cards.iter().enumerate() {
                        let card = de_ui::recents::card_at(out_size, i, pos);
                        if let Some(px) = cache.px.as_ref() {
                            canvas.blit_scaled(
                                px,
                                de_api::Rect::new(
                                    card.x,
                                    card.y - de_ui::recents::CARD_HEADER_H + card_rise,
                                    px.size.w,
                                    px.size.h,
                                ),
                                pull,
                            );
                        }
                    }
                }
            }
        } else if de.nav.current == de_ui::nav::Screen::Drawer && slide_y > 0.5 {
            if let Some(under) = de.navigation.home_underlay_px.as_ref() {
                canvas.blit(under, ApiPoint::ZERO);
            }
            canvas.blit(shell, ApiPoint::new(0, slide_y.round() as i32));
        } else {
            canvas.blit(shell, ApiPoint::ZERO);
        }
        if let Some(morph) = de.morph.as_ref() {
            let out_rect = de_api::Rect::new(0, 0, canvas.size().w, canvas.size().h);
            let pose = morph.pose(out_rect, std::time::Instant::now());
            if pose.card_alpha > 0.0 {
                let mut bg = de.theme.bg;
                bg[3] = (f32::from(bg[3]) * pose.card_alpha.clamp(0.0, 1.0)).round() as u8;
                canvas.fill_rounded_rect(pose.card, 28.0, bg);
            }
            if let Some(snapshot) = &morph.snapshot {
                canvas.blit_scaled(snapshot, pose.card, morph.snapshot_alpha());
            }
            if let Some(win) = morph.window.as_ref().filter(|_| pose.app_alpha > 0.0) {
                if let Some(surface) = win.wl_surface().map(std::borrow::Cow::into_owned) {
                    if let Some((px, _)) = surface_pixels(&surface) {
                        canvas.blit_scaled(&px, pose.card, pose.app_alpha);
                    }
                }
            }
            if pose.icon_alpha > 0.0 {
                canvas.blit_scaled(&morph.origin.icon, pose.icon, pose.icon_alpha);
            }
        }
    }

    // SystemBars: gesture bar sits above App/Shell and below ABOVE_WINDOWS/OSK/Panels.
    let bar = de_ui::nav_bar::gesture_bar_at(
        canvas.size(),
        &de.theme,
        de.navigation.nav_bar_active.value,
    );
    canvas.blit(&bar, ApiPoint::new(0, canvas.size().h - bar.size.h));

    // Overlays the renderer draws as textures: the paging home layers, the
    // notification/quick panel. Captures must show what is on the display.
    if de.home_layers_active() {
        if let Some(px) = de.home_layers_composed() {
            canvas.blit(&px, ApiPoint::ZERO);
        }
    }
    if de.media_on_home() {
        if let Some(px) = de.media.surface_px.as_ref() {
            let r = de.media.rect(canvas.size());
            canvas.blit(px, ApiPoint::new(r.x, r.y));
        }
    }
    blit_layers(
        &mut canvas,
        &map,
        de_protocols::layer_shell::ABOVE_WINDOWS.iter().rev(),
    );
    if let (true, Some(px)) = (de.panels.pull.shown(), de.panels.active_pixels()) {
        let f = de.panels.pull.frac().clamp(0.0, 1.0);
        let y = (-(1.0 - f) * px.size.h as f32).round() as i32;
        if de.panels.use_gpu_blur {
            // On GL the panel image holds only the controls; the display gets
            // its frosted sheet (blurred scene + scrim) from separate GPU
            // elements. Draw the same sheet here so captures match the screen.
            let alpha = (f * 1.6).min(1.0);
            let blurred = de_paint::glass::blurred_backdrop(&canvas.to_pixels());
            canvas.blit_scaled(
                &blurred,
                de_api::Rect::new(0, 0, blurred.size.w, blurred.size.h),
                alpha,
            );
            let mut scrim = de.theme.scrim;
            scrim[3] = (f32::from(scrim[3]) * alpha).round() as u8;
            canvas.fill_rect(
                de_api::Rect::new(0, 0, blurred.size.w, blurred.size.h),
                scrim,
            );
        }
        canvas.blit_scaled(
            px,
            de_api::Rect::new(0, y, px.size.w, px.size.h),
            (f * 1.6).min(1.0),
        );
    }
    if let Some((px, at)) = de.navigation.back.as_ref() {
        canvas.blit(px, *at);
    }
    let now = std::time::Instant::now();
    let out_size = canvas.size();
    if de.osd_fx.active(now) {
        if let Some(px) = de.osd_fx.px.as_ref() {
            let r = crate::osd_ui::OsdFx::rect(out_size);
            canvas.blit(px, ApiPoint::new(r.x, r.y));
        }
    }
    if de.hun_fx.is_visible() {
        if let (Some(px), Some(r)) = (de.hun_fx.px.as_ref(), de.hun_fx.banner_rect(out_size)) {
            canvas.blit(px, ApiPoint::new(r.x, r.y));
        }
    }

    if de.lock.shown() {
        if let Some(px) = de.lock.px.as_ref() {
            let x = de.lock.shake_x(now).round() as i32;
            let y = -(de.lock.slide_px().round() as i32);
            let alpha = (1.0 - de.lock.progress() * 0.55).clamp(0.0, 1.0);
            canvas.blit_scaled(px, de_api::Rect::new(x, y, px.size.w, px.size.h), alpha);
        }
    }
    if de.power.menu_over_lock {
        if let Some(px) = de.power.menu_px.as_ref() {
            canvas.blit(px, ApiPoint::ZERO);
        }
    }
    Some(canvas.into_pixels())
}

/// Whether an external capture client (screenshot tool, mirror, recorder) must get a blank frame: the lock
/// screen is a security boundary, so nothing is captured while it is showing.
#[must_use]
pub fn capture_is_blank(screen: de_ui::nav::Screen) -> bool {
    screen == de_ui::nav::Screen::Lock
}

/// What an external capture client may see: the composited output, or a blank frame on the lock screen.
#[must_use]
pub fn capture_output(de: &De) -> Option<Pixels> {
    let full = composite_output(de)?;
    if capture_is_blank(de.nav.current) {
        return Some(Canvas::filled(full.size, CLEAR).into_pixels());
    }
    Some(full)
}

/// Crop `px` to `rect` (clamped to the image).
#[must_use]
pub fn crop(px: &Pixels, rect: de_api::Rect) -> Option<Pixels> {
    let bounds = de_api::Rect::from_origin_size(ApiPoint::ZERO, px.size);
    let r = rect.intersect(bounds)?;
    if r.is_empty() {
        return None;
    }
    let mut out = vec![0u8; (r.w * r.h * 4) as usize];
    for y in 0..r.h {
        let s = ((r.y + y) * px.size.w + r.x) as usize * 4;
        let d = (y * r.w) as usize * 4;
        out[d..d + r.w as usize * 4].copy_from_slice(&px.data[s..s + r.w as usize * 4]);
    }
    Some(Pixels {
        size: Size::new(r.w, r.h),
        data: out,
    })
}

/// Blit every mapped layer surface on `levels` bottom-to-top. `levels` must
/// already be in draw order (lowest level first); within a level
/// `layers_on` returns bottom-to-top.
fn blit_layers<'a>(
    canvas: &mut Canvas,
    map: &LayerMap,
    levels: impl Iterator<Item = &'a smithay::wayland::shell::wlr_layer::Layer>,
) {
    for level in levels {
        for layer in map.layers_on(*level) {
            blit_layer(canvas, map, layer);
        }
    }
}

fn blit_layer(canvas: &mut Canvas, map: &LayerMap, layer: &LayerSurface) {
    let Some(loc) = de_protocols::layer_shell::origin(map, layer) else {
        return;
    };
    blit_surface(canvas, layer.wl_surface(), ApiPoint::new(loc.x, loc.y));
}

/// Blit a surface and all of its `wl_subsurface` children's committed shm buffers at `loc`.
fn blit_surface(canvas: &mut Canvas, surface: &WlSurface, loc: ApiPoint) {
    // Collect `(surface, abs_loc)` first so we do not re-enter `with_states`
    // while `with_surface_tree_downward` holds each node's `SurfaceData` lock.
    let mut nodes: Vec<(WlSurface, ApiPoint)> = Vec::new();
    with_surface_tree_downward(
        surface,
        loc,
        |child, states, &parent_loc| {
            let mut guard = states.cached_state.get::<SubsurfaceCachedState>();
            let offset = guard.current().location;
            drop(guard);
            let cur_loc = ApiPoint::new(parent_loc.x + offset.x, parent_loc.y + offset.y);
            nodes.push((child.clone(), cur_loc));
            TraversalAction::DoChildren(cur_loc)
        },
        |_, _, _| {},
        |_, _, _| true,
    );
    for (node, cur_loc) in nodes {
        let Some((px, scale)) = surface_pixels(&node) else {
            continue;
        };
        if scale <= 1 {
            canvas.blit(&px, cur_loc);
        } else {
            let dst = de_api::Rect::new(cur_loc.x, cur_loc.y, px.size.w / scale, px.size.h / scale);
            canvas.blit_scaled(&px, dst, 1.0);
        }
    }
}

/// Committed shm buffer of `surface` as premultiplied RGBA8 plus its
/// `buffer_scale`. `None` for unmapped, bufferless or dmabuf/EGL surfaces.
fn surface_pixels(surface: &WlSurface) -> Option<(Pixels, i32)> {
    let buffer = with_renderer_surface_state(surface, |state| state.buffer().cloned())??;
    let scale = with_states(surface, |states| {
        states
            .cached_state
            .get::<smithay::wayland::compositor::SurfaceAttributes>()
            .current()
            .buffer_scale
    });
    let px = shm_pixels(&buffer)?;
    Some((px, scale.max(1)))
}

/// Read a `wl_shm` buffer into premultiplied RGBA8 (R,G,B,A bytes).
/// `Argb8888`/`Xrgb8888` are B,G,R,[,A] in memory on little-endian;
/// `Abgr8888`/`Xbgr8888` are R,G,B,[,A].
fn shm_pixels(buffer: &WlBuffer) -> Option<Pixels> {
    shm::with_buffer_contents(buffer, read_shm).ok().flatten()
}

#[allow(unsafe_code)] // shm pool is only reachable through the raw pointer
fn read_shm(ptr: *const u8, len: usize, data: shm::BufferData) -> Option<Pixels> {
    const NO_ALPHA: usize = usize::MAX;
    let (r, g, b, a) = match data.format {
        wl_shm::Format::Argb8888 | wl_shm::Format::Xrgb8888 => (2usize, 1usize, 0usize, 3usize),
        wl_shm::Format::Abgr8888 | wl_shm::Format::Xbgr8888 => (0, 1, 2, 3),
        _ => return None,
    };
    let a = match data.format {
        wl_shm::Format::Argb8888 | wl_shm::Format::Abgr8888 => a,
        _ => NO_ALPHA,
    };
    let (w, h) = (data.width, data.height);
    if w <= 0 || h <= 0 || data.offset < 0 || data.stride < w * 4 {
        return None;
    }
    let need = data.offset as usize + (h as usize - 1) * data.stride as usize + w as usize * 4;
    if need > len {
        return None;
    }
    let mut out = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        let row = (data.offset + y * data.stride) as usize;
        for x in 0..w {
            let s = row + x as usize * 4;
            let d = (y * w + x) as usize * 4;
            // SAFETY: bounds checked above against the pool length `len`;
            // the client may write concurrently, which can only tear pixels.
            unsafe {
                *out.get_unchecked_mut(d) = *ptr.add(s + r);
                *out.get_unchecked_mut(d + 1) = *ptr.add(s + g);
                *out.get_unchecked_mut(d + 2) = *ptr.add(s + b);
                *out.get_unchecked_mut(d + 3) = if a == NO_ALPHA { 255 } else { *ptr.add(s + a) };
            }
        }
    }
    Some(Pixels {
        size: Size::new(w, h),
        data: out,
    })
}

#[cfg(test)]
mod capture_policy_tests {
    use super::capture_is_blank;
    use de_ui::nav::Screen;

    #[test]
    fn only_the_lock_screen_blanks_captures() {
        assert!(capture_is_blank(Screen::Lock));
        for s in [
            Screen::Home,
            Screen::Drawer,
            Screen::Recents,
            Screen::Shade,
            Screen::Power,
            Screen::Search,
            Screen::Error,
        ] {
            assert!(!capture_is_blank(s), "{s:?}");
        }
    }
}
