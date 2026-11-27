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
    let strictly_locked =
        de.lock.shown() && !de.lock.is_unlocking() && de.lock.slide_px() <= f32::EPSILON;
    if strictly_locked {
        let now = std::time::Instant::now();
        if let Some(px) = de.lock.px.as_ref() {
            let x = de.lock.shake_x(now).round() as i32;
            canvas.blit_scaled(px, de_api::Rect::new(x, 0, px.size.w, px.size.h), 1.0);
        }
        if de.power.menu_over_lock {
            if let Some(px) = de.power.menu_px.as_ref() {
                canvas.blit(px, ApiPoint::ZERO);
            }
        }
        return Some(canvas.into_pixels());
    }
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
            if de.desktop_mode {
                let geo = window.geometry();
                let title = window
                    .toplevel()
                    .map(|t| crate::snapshots::window_app_id(t))
                    .unwrap_or_else(|| "Application".to_string());
                let focused = de
                    .seat
                    .get_keyboard()
                    .and_then(|k| k.current_focus())
                    .zip(window.wl_surface())
                    .is_some_and(|(f, s)| f == *s);
                let tb = de_ui::desktop_chrome::render_titlebar(
                    geo.size.w.max(240),
                    &de.theme,
                    &de_ui::desktop_chrome::WindowChromeModel {
                        title: &title,
                        focused,
                        maximized: false,
                        hovered: None,
                    },
                );
                canvas.blit(
                    &tb,
                    ApiPoint::new(loc.x, loc.y - de_ui::desktop_chrome::TITLEBAR_H),
                );
            }
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
                    // Same card motion as the GPU path (`push_recents_layers`):
                    // the dismiss drag/fly-off offsets and fades the lifted
                    // card, and cards behind a just-removed slot slide over.
                    let lift = de.card_lift(&de.recent_windows());
                    let shift = de
                        .navigation
                        .recents_layout_shift
                        .as_ref()
                        .map(|(slot, spring, _)| (*slot, spring.value));
                    for (i, cache) in layers.cards.iter().enumerate() {
                        let mut card = de_ui::recents::card_at(out_size, i, pos);
                        let mut alpha = pull;
                        if let Some(l) = lift.filter(|l| l.index == i) {
                            card.y += l.dy.round() as i32;
                            alpha *= l.alpha(out_size);
                        }
                        if let Some((slot, dx)) = shift {
                            if i >= slot {
                                card.x += dx.round() as i32;
                            }
                        }
                        if alpha <= 0.0 {
                            continue;
                        }
                        if let Some(px) = cache.px.as_ref() {
                            canvas.blit_scaled(
                                px,
                                de_api::Rect::new(
                                    card.x,
                                    card.y - de_ui::recents::CARD_HEADER_H + card_rise,
                                    px.size.w,
                                    px.size.h,
                                ),
                                alpha,
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
    // SystemBars: centered gesture pill sits above App/OSK and below Panels.
    let bar = de_ui::nav_bar::gesture_bar_at(
        canvas.size(),
        &de.theme,
        de.navigation.nav_bar_active.value,
    );
    canvas.blit(&bar, ApiPoint::new(0, canvas.size().h - bar.size.h));
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
            let (dx, alpha) = de.osd_fx.transform(now);
            if alpha > 0.004 {
                canvas.blit_scaled(
                    px,
                    de_api::Rect::new(r.x + dx.round() as i32, r.y, px.size.w, px.size.h),
                    alpha,
                );
            }
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
    if !de.rotation.policy.auto && !de.desktop_mode && !de.lock.shown() {
        if de.rotation.sensed.is_some_and(|s| s != de.rotation.current) {
            let pill = de_ui::rotate_pill::render(&de.theme);
            let r = de_ui::rotate_pill::pill_rect(out_size);
            canvas.blit(&pill, ApiPoint::new(r.x, r.y));
        }
    }
    if de.desktop_mode && !de.lock.shown() {
        let app_labels: Vec<String> = de
            .space
            .elements()
            .map(|w| {
                w.toplevel()
                    .map(|t| crate::snapshots::window_app_id(t))
                    .unwrap_or_else(|| "App".to_string())
            })
            .collect();
        let focused_idx = de.seat.get_keyboard().and_then(|k| k.current_focus()).and_then(|f| {
            de.space
                .elements()
                .position(|w| w.wl_surface().is_some_and(|s| *s == f))
        });
        let apps: Vec<de_ui::taskbar::TaskbarApp<'_>> = app_labels
            .iter()
            .enumerate()
            .map(|(i, lbl)| de_ui::taskbar::TaskbarApp {
                label: lbl,
                focused: focused_idx == Some(i),
                minimized: false,
            })
            .collect();
        let mins = crate::runtime::frame_io::minute_of_day();
        let clock = format!("{:02}:{:02}", (mins / 60) % 24, mins % 60);
        let bar = de_ui::taskbar::render(
            out_size,
            &de.theme,
            &de_ui::taskbar::TaskbarModel {
                apps: &apps,
                tray_count: de.tray_watcher.count(),
                clock: &clock,
                launcher_open: de.foreground == Foreground::Shell
                    && de.nav.current == de_ui::nav::Screen::Drawer,
                hovered: None,
            },
        );
        canvas.blit(
            &bar,
            ApiPoint::new(0, out_size.h - de_ui::taskbar::TASKBAR_H),
        );
    }
    if de.power.menu_over_lock {
        if let Some(px) = de.power.menu_px.as_ref() {
            canvas.blit(px, ApiPoint::ZERO);
        }
    }
    if de.desktop_mode || de.pointer_active {
        let cur = de
            .cursor_px
            .as_ref()
            .cloned()
            .unwrap_or_else(crate::cursor::cursor_pixels);
        let loc = de.pointer.current_location();
        canvas.blit(
            &cur,
            ApiPoint::new(
                (loc.x - crate::cursor::CURSOR_HOTSPOT.0).round() as i32,
                (loc.y - crate::cursor::CURSOR_HOTSPOT.1).round() as i32,
            ),
        );
    }
    Some(canvas.into_pixels())
}

/// Whether an external capture client (screenshot tool, mirror, recorder) must get a blank frame: the lock
/// screen is a security boundary, so nothing is captured while it is showing.
#[must_use]
pub fn capture_is_blank(screen: de_ui::nav::Screen) -> bool {
    screen == de_ui::nav::Screen::Lock
}

/// Whether external capture must return a blank frame given Keyguard state,
/// external `ext-session-lock-v1` state, and the current navigation screen (`CHAL-002`).
#[must_use]
pub fn should_blank_capture(
    keyguard_locked: bool,
    session_locked: bool,
    screen: de_ui::nav::Screen,
) -> bool {
    keyguard_locked || session_locked || capture_is_blank(screen)
}

/// What an external capture client may see: the composited output, or a blank frame on the lock screen.
#[must_use]
pub fn capture_output(de: &De) -> Option<Pixels> {
    if should_blank_capture(de.lock.shown(), de.is_session_locked(), de.nav.current) {
        let size = Size::new(de.output.width as i32, de.output.height as i32);
        if size.w <= 0 || size.h <= 0 {
            return None;
        }
        return Some(Canvas::filled(size, CLEAR).into_pixels());
    }
    composite_output(de)
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
    const MAX_SHM_DIM: usize = 16_384;
    if ptr.is_null() {
        return None;
    }
    let (r, g, b, a) = match data.format {
        wl_shm::Format::Argb8888 | wl_shm::Format::Xrgb8888 => (2usize, 1usize, 0usize, 3usize),
        wl_shm::Format::Abgr8888 | wl_shm::Format::Xbgr8888 => (0, 1, 2, 3),
        _ => return None,
    };
    let a = match data.format {
        wl_shm::Format::Argb8888 | wl_shm::Format::Abgr8888 => a,
        _ => NO_ALPHA,
    };
    if data.width <= 0 || data.height <= 0 || data.offset < 0 || data.stride <= 0 {
        return None;
    }
    let w = usize::try_from(data.width).ok()?;
    let h = usize::try_from(data.height).ok()?;
    if w > MAX_SHM_DIM || h > MAX_SHM_DIM {
        return None;
    }
    let offset = usize::try_from(data.offset).ok()?;
    let stride = usize::try_from(data.stride).ok()?;
    let row_bytes = w.checked_mul(4)?;
    if stride < row_bytes {
        return None;
    }
    let last_row = offset.checked_add(h.checked_sub(1)?.checked_mul(stride)?)?;
    let need = last_row.checked_add(row_bytes)?;
    if need > len {
        return None;
    }
    let out_len = w.checked_mul(h)?.checked_mul(4)?;
    // SAFETY: `ptr` is non-null and valid for `len` bytes for the duration of
    // Smithay's `with_buffer_contents` callback; `need <= len` is verified above.
    let src = unsafe { std::slice::from_raw_parts(ptr, len) };
    let mut out = vec![0u8; out_len];
    for y in 0..h {
        let src_start = offset.checked_add(y.checked_mul(stride)?)?;
        let src_row = src.get(src_start..src_start.checked_add(row_bytes)?)?;
        let dst_start = y.checked_mul(row_bytes)?;
        let dst_row = out.get_mut(dst_start..dst_start.checked_add(row_bytes)?)?;
        for (s_px, d_px) in src_row.chunks_exact(4).zip(dst_row.chunks_exact_mut(4)) {
            d_px[0] = s_px[r];
            d_px[1] = s_px[g];
            d_px[2] = s_px[b];
            d_px[3] = if a == NO_ALPHA { 255 } else { s_px[a] };
        }
    }
    Some(Pixels {
        size: Size::new(data.width, data.height),
        data: out,
    })
}

#[cfg(test)]
mod capture_policy_tests {
    use super::{capture_is_blank, read_shm, should_blank_capture};
    use de_ui::nav::Screen;
    use smithay::reexports::wayland_server::protocol::wl_shm;
    use smithay::wayland::shm;

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

    #[test]
    fn locked_keyguard_or_session_lock_always_blanks_capture() {
        // CHAL-002: even when nav.current stays Screen::Home, active Keyguard or
        // external session lock must blank all external captures.
        assert!(should_blank_capture(true, false, Screen::Home));
        assert!(should_blank_capture(false, true, Screen::Home));
        assert!(should_blank_capture(true, true, Screen::Drawer));
        assert!(should_blank_capture(false, false, Screen::Lock));
        assert!(!should_blank_capture(false, false, Screen::Home));
    }

    #[test]
    fn read_shm_rejects_overflow_and_oversized_dimensions() {
        let buf = [0u8; 64];
        // Crafted i32 overflow dimensions (CHAL-001)
        let overflow = shm::BufferData {
            offset: 0,
            width: 32_768,
            height: 32_768,
            stride: 32_768 * 4,
            format: wl_shm::Format::Argb8888,
        };
        assert!(read_shm(buf.as_ptr(), usize::MAX, overflow).is_none());

        let wrap_stride = shm::BufferData {
            offset: 0,
            width: 0x4000_0000,
            height: 1,
            stride: 4,
            format: wl_shm::Format::Argb8888,
        };
        assert!(read_shm(buf.as_ptr(), buf.len(), wrap_stride).is_none());

        let valid = shm::BufferData {
            offset: 0,
            width: 2,
            height: 1,
            stride: 8,
            format: wl_shm::Format::Xrgb8888,
        };
        let src = [0x11, 0x22, 0x33, 0x00, 0x44, 0x55, 0x66, 0x00];
        let px = read_shm(src.as_ptr(), src.len(), valid).expect("valid shm");
        assert_eq!(px.data, vec![0x33, 0x22, 0x11, 255, 0x66, 0x55, 0x44, 255]);
    }
}
