//! Scene assembly and frame submission (`DeElement`, `render_with`, panel GPU
//! blur, and per-layer `push_*` helpers).

use std::time::{Duration, Instant};

use de_protocols::layer_shell::{origin, ABOVE_WINDOWS, BELOW_WINDOWS};
use smithay::backend::drm::compositor::{FrameFlags, PrimaryPlaneElement};
use smithay::backend::renderer::element::render_elements;
use smithay::backend::renderer::element::solid::SolidColorRenderElement;
use smithay::backend::renderer::element::surface::{
    render_elements_from_surface_tree, WaylandSurfaceRenderElement,
};
use smithay::backend::renderer::element::texture::TextureRenderElement;
use smithay::backend::renderer::element::{AsRenderElements, Kind};
use smithay::backend::renderer::{ImportAll, ImportMem, Renderer, Texture};
use smithay::desktop::utils::send_frames_surface_tree;
use smithay::desktop::{layer_map_for_output, LayerMap};
use smithay::utils::{Physical, Point, Scale};
use smithay::wayland::shell::wlr_layer::Layer;

use crate::morph::{Phase, WindowMorph};
use crate::runtime::textures::{
    sync_drag_textures, sync_home_textures, tex_el, tex_el_sized, texture_buffer, upload_any,
};

render_elements! {
    /// Elements submitted to `DrmCompositor::render_frame` each frame.
    pub DeElement<R> where R: ImportAll + ImportMem, R::TextureId: Texture + Clone + 'static;
    Texture = TextureRenderElement<R::TextureId>,
    Surface = WaylandSurfaceRenderElement<R>,
    Solid = SolidColorRenderElement,
}

/// Field-split view of `De` for [`render_with`], so the renderer can be
/// borrowed mutably at the same time as the scene state.
pub(super) struct FrameParts<'a> {
    pub(super) compositor: &'a mut de_backend::probe::DeCompositor,
    pub(super) space: &'a mut smithay::desktop::Space<smithay::desktop::Window>,
    pub(super) shell_tex: &'a mut Option<Box<dyn std::any::Any>>,
    pub(super) shell_px: &'a mut Option<de_api::Pixels>,
    pub(super) gesture_bar: &'a mut Option<Box<dyn std::any::Any>>,
    pub(super) output: &'a de_backend::drm::OutputSelection,
    pub(super) output_obj: &'a smithay::output::Output,
    pub(super) clock: &'a smithay::utils::Clock<smithay::utils::Monotonic>,
    pub(super) primary_composite: &'a mut u64,
    pub(super) direct_scanout: &'a mut u64,
    pub(super) foreground: crate::state::Foreground,
    pub(super) morph: &'a mut Option<WindowMorph>,
    pub(super) home: &'a mut crate::home_ui::HomeUi,
    pub(super) panel: &'a mut crate::panel_ui::PanelUi,
    pub(super) navigation: &'a mut crate::navigation::Navigation,
    pub(super) lock: &'a mut crate::lock_ui::LockUi,
    pub(super) shot_fx: &'a mut crate::shot_ui::ShotFx,
    pub(super) osd_fx: &'a mut crate::osd_ui::OsdFx,
    pub(super) hun_fx: &'a mut crate::hun_ui::HunFx,
    pub(super) power: &'a mut crate::power::Power,
    /// Draw the home screen from `home` layers instead of `shell_tex`.
    pub(super) layered: bool,
    /// Pointer state for the on-screen cursor sprite (topmost element).
    pub(super) pointer: &'a smithay::input::pointer::PointerHandle<crate::state::De>,
    pub(super) pointer_active: bool,
    pub(super) cursor_px: &'a mut Option<de_api::Pixels>,
    pub(super) cursor_tex: &'a mut Option<Box<dyn std::any::Any>>,
    pub(super) now: Instant,
}

pub(super) fn render_with<R>(de: &mut FrameParts<'_>, renderer: &mut R) -> Result<bool, String>
where
    R: Renderer
        + ImportAll
        + ImportMem
        + smithay::backend::renderer::Bind<smithay::backend::allocator::dmabuf::Dmabuf>
        + 'static,
    R::TextureId: smithay::backend::renderer::Texture + Clone + 'static,
{
    // Upload the latest shell frame as a texture (re-rastered upstream in
    // `present_once` on dirty/minute change — the cached texture otherwise
    // produces no damage and the frame is simply EmptyFrame).
    if de.shell_tex.is_none() {
        if let Some(px) = de.shell_px.as_ref() {
            *de.shell_tex = upload_any(renderer, px);
        }
    }
    if de.gesture_bar.is_none() {
        let px = de_ui::nav_bar::gesture_bar(de_api::Size::new(
            de.output.width as i32,
            de.output.height as i32,
        ));
        *de.gesture_bar = upload_any(renderer, &px);
    }

    // 1. Assemble the backdrop/content slice (front-to-back) so GPU panel blur
    //    samples the exact scene underneath panels/overlays.
    let mut backdrop: Vec<DeElement<R>> = Vec::new();
    let map = layer_map_for_output(de.output_obj);
    push_layer_elements(&mut backdrop, renderer, &map, ABOVE_WINDOWS);
    let transitioning = de.morph.is_some();
    let show_app = !transitioning
        && de.foreground == crate::state::Foreground::App
        && de.space.elements().next().is_some();
    if let Some(morph) = de.morph.as_mut() {
        let output = de_api::Rect::new(0, 0, de.output.width as i32, de.output.height as i32);
        push_morph_elements(&mut backdrop, renderer, morph, output, de.now);
    }
    if show_app {
        push_window_elements(&mut backdrop, renderer, de.space);
    } else if de.layered && de.home.layers_key().is_some() {
        sync_home_textures(renderer, de.home);
        push_home_layers::<R>(
            &mut backdrop,
            de.home,
            de_api::Size::new(de.output.width as i32, de.output.height as i32),
        );
    } else if let Some(tex) = texture_buffer::<R>(de.shell_tex.as_ref()) {
        let slide_y = if de.navigation.drawer_slide.value > 0.5 {
            de.navigation.drawer_slide.value
        } else {
            de.navigation.drawer_pull_y
        };
        if let Some(fade) = de.navigation.recents_fade.as_ref() {
            if de.navigation.home_underlay_tex.is_none() {
                if let Some(px) = de.navigation.home_underlay_px.as_ref() {
                    de.navigation.home_underlay_tex = upload_any(renderer, px);
                }
            }
            let mix = fade.mix(de.now);
            let lift = f64::from(de.output.height) * 0.025 * f64::from(1.0 - mix);
            backdrop.push(DeElement::from(TextureRenderElement::from_texture_buffer(
                (0.0, lift.round()),
                tex,
                Some(mix),
                None,
                None,
                Kind::Unspecified,
            )));
            if let Some(home_tex) = texture_buffer::<R>(de.navigation.home_underlay_tex.as_ref()) {
                backdrop.push(DeElement::from(tex_el::<R>(home_tex, (0.0, 0.0).into())));
            }
        } else if slide_y > 0.5 {
            let h = f32::from(de.output.height.max(1) as u16);
            let alpha = (1.0 - slide_y / h).clamp(0.0, 1.0);
            backdrop.push(DeElement::from(TextureRenderElement::from_texture_buffer(
                Point::<f64, Physical>::from((0.0, f64::from(slide_y.round()))),
                tex,
                Some(alpha),
                None,
                None,
                Kind::Unspecified,
            )));
            if de.navigation.home_underlay_tex.is_none() {
                if let Some(px) = de.navigation.home_underlay_px.as_ref() {
                    de.navigation.home_underlay_tex = upload_any(renderer, px);
                }
            }
            if let Some(home_tex) = texture_buffer::<R>(de.navigation.home_underlay_tex.as_ref()) {
                backdrop.push(DeElement::from(tex_el::<R>(
                    home_tex,
                    Point::<f64, Physical>::from((0.0, 0.0)),
                )));
            }
        } else {
            backdrop.push(DeElement::from(tex_el::<R>(
                tex,
                Point::<f64, Physical>::from((0.0, 0.0)),
            )));
        }
    }
    // The lifted icon rides on top of the home frame as its own texture.
    push_drag_overlay::<R>(&mut backdrop, renderer, de.home, de.output);
    push_layer_elements(&mut backdrop, renderer, &map, BELOW_WINDOWS);
    prepare_panel_gpu_blur(renderer, de.panel, &backdrop, de.output);

    // 2. Bucket backdrop and system overlays into semantic DisplayArea bands.
    let mut stack = crate::display_stack::RenderStack::new();
    stack.extend_front_to_back(de_api::DisplayArea::App, backdrop);

    // Gesture bar in SystemBars (above App/Home, below Shade/Keyguard/Cursor).
    let bar_y = f64::from(de.output.height) - f64::from(de_ui::chrome::NAV_H);
    if let Some(tex) = texture_buffer::<R>(de.gesture_bar.as_ref()) {
        stack.push(
            de_api::DisplayArea::SystemBars,
            DeElement::from(tex_el::<R>(tex, Point::<f64, Physical>::from((0.0, bar_y)))),
        );
    }

    // Back-edge indicator and screenshot feedback in Overlay.
    if let Some((px, at)) = de.navigation.back.as_ref() {
        if de.navigation.back_texture.is_none() {
            de.navigation.back_texture = upload_any(renderer, px);
        }
        if let Some(tex) = texture_buffer::<R>(de.navigation.back_texture.as_ref()) {
            stack.push(
                de_api::DisplayArea::Overlay,
                DeElement::from(tex_el::<R>(
                    tex,
                    Point::<f64, Physical>::from((f64::from(at.x), f64::from(at.y))),
                )),
            );
        }
    }
    let mut shot_els = Vec::new();
    push_shot::<R>(&mut shot_els, renderer, de.shot_fx, de.output, de.now);
    stack.extend_front_to_back(de_api::DisplayArea::Overlay, shot_els);

    let mut osd_els = Vec::new();
    push_osd::<R>(&mut osd_els, renderer, de.osd_fx, de.output, de.now);
    stack.extend_front_to_back(de_api::DisplayArea::Overlay, osd_els);

    // Heads-up notification banner in Overlay (above OSD/shot toast, below Shade).
    if de.hun_fx.is_visible() {
        let mut hun_els = Vec::new();
        push_hun::<R>(&mut hun_els, renderer, de.hun_fx, de.output);
        stack.extend_front_to_back(de_api::DisplayArea::Overlay, hun_els);
    } else {
        de.hun_fx.tex = None;
        de.hun_fx.tex_generation = u64::MAX;
    }

    // Notification / Quick-Actions panels in Shade.
    let mut panel_els = Vec::new();
    push_panel::<R>(&mut panel_els, renderer, de.panel, de.output);
    stack.extend_front_to_back(de_api::DisplayArea::Shade, panel_els);

    // Lock screen in Keyguard.
    let mut lock_els = Vec::new();
    push_lock::<R>(&mut lock_els, renderer, de.lock, de.output, de.now);
    stack.extend_front_to_back(de_api::DisplayArea::Keyguard, lock_els);

    // Power menu, screen-off fade, and pointer cursor in Critical (topmost).
    let mut critical_els = Vec::new();
    push_menu_over::<R>(&mut critical_els, renderer, de.power, de.output);
    push_fade::<R>(&mut critical_els, de.power, de.output, de.now);
    stack.extend_front_to_back(de_api::DisplayArea::Critical, critical_els);

    if de.pointer_active {
        if de.cursor_px.is_none() {
            *de.cursor_px = Some(crate::cursor::cursor_pixels());
        }
        if de.cursor_tex.is_none() {
            if let Some(px) = de.cursor_px.as_ref() {
                *de.cursor_tex = upload_any(renderer, px);
            }
        }
        if let Some(tex) = texture_buffer::<R>(de.cursor_tex.as_ref()) {
            let scale = de.output_obj.current_scale().fractional_scale();
            let loc = de.pointer.current_location();
            let (x, y) = (
                loc.x * scale - crate::cursor::CURSOR_HOTSPOT.0 * scale,
                loc.y * scale - crate::cursor::CURSOR_HOTSPOT.1 * scale,
            );
            let side = crate::cursor::CURSOR_PX as f64 * scale;
            stack.push(
                de_api::DisplayArea::Critical,
                DeElement::from(tex_el_sized::<R>(
                    tex,
                    de_api::Rect::new(
                        x.round() as i32,
                        y.round() as i32,
                        side.round() as i32,
                        side.round() as i32,
                    ),
                )),
            );
        }
    }

    // Keep unlocked during interactive lock slide so home peeks cleanly beneath.
    let strictly_locked =
        de.lock.shown() && !de.lock.is_unlocking() && de.lock.progress() <= f32::EPSILON;
    let elements = stack.into_smithay_elements(strictly_locked);

    let res = de
        .compositor
        .render_frame(
            renderer,
            &elements,
            [0.086, 0.063, 0.102, 1.0],
            if transitioning {
                FrameFlags::empty()
            } else {
                FrameFlags::DEFAULT
            },
        )
        .map_err(|e| format!("render: {e:?}"))?;

    if let Err(e) = de.compositor.queue_frame(()) {
        if !format!("{e:?}").contains("EmptyFrame") {
            return Err(format!("queue: {e:?}"));
        }
        return Ok(false);
    }

    // M1-T4 counters from the observed result, not a guess.
    match res.primary_element {
        PrimaryPlaneElement::Swapchain(_) => *de.primary_composite += 1,
        PrimaryPlaneElement::Element(_) => *de.direct_scanout += 1,
    }

    // Frame callbacks for the freshly-committed surfaces.
    let time = de.clock.now();
    let out = de.output_obj.clone();
    for layer in map.layers() {
        layer.send_frame(&out, time, Some(Duration::from_millis(16)), |_, _| {
            Some(out.clone())
        });
    }
    for window in de.space.elements() {
        window.send_frame(&out, time, Some(Duration::from_millis(16)), |_, _| {
            Some(out.clone())
        });
        for (popup, _) in crate::popups::popups_of(window) {
            send_frames_surface_tree(
                popup.wl_surface(),
                &out,
                time,
                Some(Duration::from_millis(16)),
                |_, _| Some(out.clone()),
            );
        }
    }
    Ok(true)
}

/// Elements of every mapped app window, each preceded by its popups (earlier elements are in front).
fn push_window_elements<R: Renderer + ImportAll + ImportMem>(
    elements: &mut Vec<DeElement<R>>,
    renderer: &mut R,
    space: &smithay::desktop::Space<smithay::desktop::Window>,
) where
    R::TextureId: Texture + Clone + 'static,
{
    for window in space.elements().rev() {
        let loc = space
            .element_location(window)
            .unwrap_or_default()
            .to_physical(1);
        for (popup, offset) in crate::popups::popups_of(window) {
            elements.extend(render_elements_from_surface_tree::<R, DeElement<R>>(
                renderer,
                popup.wl_surface(),
                loc + offset.to_physical(1),
                Scale::from(1.0),
                1.0,
                Kind::Unspecified,
            ));
        }
        elements.extend(AsRenderElements::<R>::render_elements::<DeElement<R>>(
            window,
            renderer,
            loc,
            Scale::from(1.0),
            1.0,
        ));
    }
}

fn push_layer_elements<R: Renderer + ImportAll + ImportMem>(
    elements: &mut Vec<DeElement<R>>,
    renderer: &mut R,
    map: &LayerMap,
    levels: [Layer; 2],
) where
    R::TextureId: Texture + Clone + 'static,
{
    for level in levels {
        for layer in map.layers_on(level).rev() {
            if let Some(loc) = origin(map, layer) {
                elements.extend(layer.render_elements::<DeElement<R>>(
                    renderer,
                    loc.to_physical(1),
                    Scale::from(1.0),
                    1.0,
                ));
            }
        }
    }
}

fn push_morph_elements<R: Renderer + ImportAll + ImportMem>(
    elements: &mut Vec<DeElement<R>>,
    renderer: &mut R,
    morph: &mut WindowMorph,
    output: de_api::Rect,
    now: Instant,
) where
    R::TextureId: Texture + Clone + 'static,
{
    let pose = morph.pose(output, now);
    // Smithay consumes front-to-back elements. The launcher texture sits
    // behind the solid backing, the scaled app and the cached icon.
    if pose.icon_alpha > 0.0 {
        if morph.icon_texture.is_none() {
            morph.icon_texture = upload_any(renderer, &morph.origin.icon);
        }
        if let Some(icon) = texture_buffer::<R>(morph.icon_texture.as_ref()) {
            elements.push(DeElement::from(TextureRenderElement::from_texture_buffer(
                (f64::from(pose.icon.x), f64::from(pose.icon.y)),
                icon,
                Some(pose.icon_alpha),
                None,
                None,
                Kind::Unspecified,
            )));
        }
    }
    let loc = Point::<i32, Physical>::from((pose.card.x, pose.card.y));
    if let Some(snapshot) = &morph.snapshot {
        if morph.snapshot_texture.is_none() {
            morph.snapshot_texture = upload_any(renderer, snapshot);
        }
        if let Some(tex) = texture_buffer::<R>(morph.snapshot_texture.as_ref()) {
            elements.push(DeElement::from(TextureRenderElement::from_texture_buffer(
                (f64::from(loc.x), f64::from(loc.y)),
                tex,
                Some(morph.snapshot_alpha()),
                None,
                Some((pose.card.w, pose.card.h).into()),
                Kind::Unspecified,
            )));
        }
    }
    if let Some(window) = morph.window.as_ref().filter(|_| pose.app_alpha > 0.0) {
        let scale = Scale::from((
            f64::from(pose.card.w) / f64::from(output.w),
            f64::from(pose.card.h) / f64::from(output.h),
        ));
        elements.extend(AsRenderElements::<R>::render_elements::<DeElement<R>>(
            window,
            renderer,
            loc,
            scale,
            pose.app_alpha,
        ));
    }
    if pose.card_alpha > 0.0 {
        morph.backing.resize((pose.card.w, pose.card.h));
        elements.push(DeElement::from(SolidColorRenderElement::from_buffer(
            &morph.backing,
            loc,
            1.0,
            pose.card_alpha,
            Kind::Unspecified,
        )));
    }
    if let Some(px) = morph
        .backdrop
        .as_ref()
        .filter(|_| morph.phase != Phase::Recents)
    {
        if morph.backdrop_texture.is_none() {
            morph.backdrop_texture = upload_any(renderer, px);
        }
        if let Some(tex) = texture_buffer::<R>(morph.backdrop_texture.as_ref()) {
            elements.push(DeElement::from(TextureRenderElement::from_texture_buffer(
                (0.0, 0.0),
                tex,
                Some(morph.backdrop_alpha()),
                None,
                None,
                Kind::Unspecified,
            )));
        }
    }
}

/// Capture and blur the live scene on `GlesRenderer` (1/4-res FBO + 3 Kawase
/// passes) when a panel pull begins. No-op on `Soft`.
fn prepare_panel_gpu_blur<R: Renderer + ImportAll + ImportMem + 'static>(
    renderer: &mut R,
    panel: &mut crate::panel_ui::PanelUi,
    scene: &Vec<DeElement<R>>,
    output: &de_backend::drm::OutputSelection,
) where
    R::TextureId: smithay::backend::renderer::Texture + Clone + 'static,
{
    if !panel.use_gpu_blur || !panel.pull.shown() {
        return;
    }
    let Some(gles) = (&mut *renderer as &mut dyn std::any::Any)
        .downcast_mut::<smithay::backend::renderer::gles::GlesRenderer>()
    else {
        return;
    };
    let Some(scene_gles) = (scene as &dyn std::any::Any)
        .downcast_ref::<Vec<DeElement<smithay::backend::renderer::gles::GlesRenderer>>>()
    else {
        return;
    };
    if panel.gpu_blur.is_none() {
        match de_render::blur::GpuBlur::new(gles, output.width as i32, output.height as i32) {
            Ok(b) => panel.gpu_blur = Some(b),
            Err(e) => {
                eprintln!("gpu_blur init: {e}");
                panel.use_gpu_blur = false;
                return;
            }
        }
    }
    let Some(blur) = panel.gpu_blur.as_mut() else {
        return;
    };
    if panel.gpu_blur_dirty || !blur.has_backdrop() {
        if let Err(e) = blur.blur_elements(gles, scene_gles, [0.086, 0.063, 0.102, 1.0]) {
            eprintln!("gpu_blur: {e}");
        }
        panel.gpu_blur_dirty = false;
    }
}

/// The notification/quick panel: one texture slid by the pull fraction, in
/// front of everything (apps included). Fades in as it comes down.
fn push_panel<R>(
    elements: &mut Vec<DeElement<R>>,
    renderer: &mut R,
    panel: &mut crate::panel_ui::PanelUi,
    output: &de_backend::drm::OutputSelection,
) where
    R: Renderer + ImportAll + ImportMem + 'static,
    R::TextureId: smithay::backend::renderer::Texture + Clone + 'static,
{
    if !panel.pull.shown() {
        panel.tex = None;
        panel.tex_generation = u64::MAX;
        return;
    }
    if panel.tex.is_none()
        || panel.tex_generation != panel.generation
        || panel.tex_kind != panel.kind()
    {
        let Some(px) = panel.active_pixels() else {
            return;
        };
        panel.tex = upload_any(renderer, px);
        panel.tex_generation = panel.generation;
        panel.tex_kind = panel.kind();
    }
    let (Some(tex), Some(px)) = (
        texture_buffer::<R>(panel.tex.as_ref()),
        panel.active_pixels(),
    ) else {
        return;
    };
    let f = panel.pull.frac().clamp(0.0, 1.0);
    let y = -(1.0 - f) * px.size.h as f32;
    let alpha = (f * 1.6).min(1.0);
    if panel.use_gpu_blur {
        if let Some(blur_el) = panel
            .gpu_blur
            .as_ref()
            .and_then(|b| b.element(alpha))
            .and_then(|el| {
                (Box::new(el) as Box<dyn std::any::Any>)
                    .downcast::<TextureRenderElement<R::TextureId>>()
                    .ok()
            })
        {
            elements.insert(0, DeElement::from(*blur_el));
        }
        let scrim = de_design::tokens::DEFAULT
            .scrim
            .map(|v| f32::from(v) / 255.0);
        let buf = panel.scrim_buf.get_or_insert_with(|| {
            smithay::backend::renderer::element::solid::SolidColorBuffer::new(
                (output.width as i32, output.height as i32),
                scrim,
            )
        });
        buf.resize((output.width as i32, output.height as i32));
        elements.insert(
            0,
            DeElement::from(SolidColorRenderElement::from_buffer(
                buf,
                Point::<i32, Physical>::from((0, 0)),
                1.0,
                alpha,
                Kind::Unspecified,
            )),
        );
    }
    elements.insert(
        0,
        DeElement::from(TextureRenderElement::from_texture_buffer(
            Point::<f64, Physical>::from((0.0, f64::from(y.round()))),
            tex,
            Some(alpha),
            None,
            None,
            Kind::Unspecified,
        )),
    );
}

/// Lock overlay texture: slides up with the unlock drag and fades out;
/// fp-fail shakes it horizontally. Inserted at the front (topmost).
fn push_lock<R>(
    elements: &mut Vec<DeElement<R>>,
    renderer: &mut R,
    lock: &mut crate::lock_ui::LockUi,
    output: &de_backend::drm::OutputSelection,
    now: Instant,
) where
    R: Renderer + ImportAll + ImportMem,
    R::TextureId: smithay::backend::renderer::Texture + Clone + 'static,
{
    if !lock.shown() {
        lock.tex = None;
        lock.tex_generation = u64::MAX;
        return;
    }
    if lock.tex.is_none() || lock.tex_generation != lock.generation {
        let Some(px) = lock.px.as_ref() else { return };
        lock.tex = upload_any(renderer, px);
        lock.tex_generation = lock.generation;
    }
    let Some(tex) = texture_buffer::<R>(lock.tex.as_ref()) else {
        return;
    };
    let _ = output;
    let x = lock.shake_x(now);
    let y = -f64::from(lock.slide_px());
    let alpha = 1.0 - lock.progress() * 0.55;
    elements.insert(
        0,
        DeElement::from(TextureRenderElement::from_texture_buffer(
            Point::<f64, Physical>::from((x, y.round())),
            tex,
            Some(alpha),
            None,
            None,
            Kind::Unspecified,
        )),
    );
}

/// Power menu floating above the lock (long-press combo while locked).
fn push_menu_over<R>(
    elements: &mut Vec<DeElement<R>>,
    renderer: &mut R,
    power: &mut crate::power::Power,
    _output: &de_backend::drm::OutputSelection,
) where
    R: Renderer + ImportAll + ImportMem,
    R::TextureId: smithay::backend::renderer::Texture + Clone + 'static,
{
    if !power.menu_over_lock {
        power.menu_tex = None;
        return;
    }
    if power.menu_tex.is_none() {
        let Some(px) = power.menu_px.as_ref() else {
            return;
        };
        power.menu_tex = upload_any(renderer, px);
    }
    let Some(tex) = texture_buffer::<R>(power.menu_tex.as_ref()) else {
        return;
    };
    elements.insert(
        0,
        DeElement::from(TextureRenderElement::from_texture_buffer(
            Point::<f64, Physical>::from((0.0, 0.0)),
            tex,
            Some(1.0),
            None,
            None,
            Kind::Unspecified,
        )),
    );
}

/// Screenshot feedback: toast card bottom-right + the white flash (drawn
/// after the capture, so the flash never photographs itself).
fn push_shot<R>(
    elements: &mut Vec<DeElement<R>>,
    renderer: &mut R,
    shot: &mut crate::shot_ui::ShotFx,
    output: &de_backend::drm::OutputSelection,
    now: Instant,
) where
    R: Renderer + ImportAll + ImportMem,
    R::TextureId: smithay::backend::renderer::Texture + Clone + 'static,
{
    if !shot.toast_active(now) {
        shot.tex = None;
        shot.tex_generation = u64::MAX;
    } else {
        if shot.tex.is_none() || shot.tex_generation != shot.generation {
            if let Some(px) = shot.px.as_ref() {
                shot.tex = upload_any(renderer, px);
                shot.tex_generation = shot.generation;
            }
        }
        if let Some(tex) = texture_buffer::<R>(shot.tex.as_ref()) {
            let r = crate::shot_ui::ShotFx::toast_rect(de_api::Size::new(
                output.width as i32,
                output.height as i32,
            ));
            elements.insert(
                0,
                DeElement::from(TextureRenderElement::from_texture_buffer(
                    Point::<f64, Physical>::from((f64::from(r.x), f64::from(r.y))),
                    tex,
                    Some(1.0),
                    None,
                    None,
                    Kind::Unspecified,
                )),
            );
        }
    }
    if shot.flash_active(now) {
        let buf = smithay::backend::renderer::element::solid::SolidColorBuffer::new(
            (output.width as i32, output.height as i32),
            [1.0, 0.972, 0.941, 1.0],
        );
        elements.insert(
            0,
            DeElement::from(SolidColorRenderElement::from_buffer(
                &buf,
                Point::<i32, Physical>::from((0, 0)),
                1.0,
                1.0,
                Kind::Unspecified,
            )),
        );
    }
}

/// Volume / brightness OSD pill in `DisplayArea::Overlay`.
fn push_osd<R>(
    elements: &mut Vec<DeElement<R>>,
    renderer: &mut R,
    osd: &mut crate::osd_ui::OsdFx,
    output: &de_backend::drm::OutputSelection,
    now: Instant,
) where
    R: Renderer + ImportAll + ImportMem,
    R::TextureId: smithay::backend::renderer::Texture + Clone + 'static,
{
    if !osd.active(now) {
        osd.tex = None;
        osd.tex_generation = u64::MAX;
        return;
    }
    if osd.tex.is_none() || osd.tex_generation != osd.generation {
        if let Some(px) = osd.px.as_ref() {
            osd.tex = upload_any(renderer, px);
            osd.tex_generation = osd.generation;
        }
    }
    if let Some(tex) = texture_buffer::<R>(osd.tex.as_ref()) {
        let r = crate::osd_ui::OsdFx::rect(de_api::Size::new(
            output.width as i32,
            output.height as i32,
        ));
        elements.insert(
            0,
            DeElement::from(TextureRenderElement::from_texture_buffer(
                Point::<f64, Physical>::from((f64::from(r.x), f64::from(r.y))),
                tex,
                Some(1.0),
                None,
                None,
                Kind::Unspecified,
            )),
        );
    }
}

/// Heads-up notification banner in `DisplayArea::Overlay`.
fn push_hun<R>(
    elements: &mut Vec<DeElement<R>>,
    renderer: &mut R,
    hun: &mut crate::hun_ui::HunFx,
    output: &de_backend::drm::OutputSelection,
) where
    R: Renderer + ImportAll + ImportMem,
    R::TextureId: smithay::backend::renderer::Texture + Clone + 'static,
{
    if hun.tex.is_none() || hun.tex_generation != hun.generation {
        if let Some(px) = hun.px.as_ref() {
            hun.tex = upload_any(renderer, px);
            hun.tex_generation = hun.generation;
        }
    }
    let size = de_api::Size::new(output.width as i32, output.height as i32);
    if let (Some(tex), Some(r)) = (texture_buffer::<R>(hun.tex.as_ref()), hun.banner_rect(size)) {
        elements.insert(
            0,
            DeElement::from(TextureRenderElement::from_texture_buffer(
                Point::<f64, Physical>::from((f64::from(r.x), f64::from(r.y))),
                tex,
                Some(1.0),
                None,
                None,
                Kind::Unspecified,
            )),
        );
    }
}

/// Black curtain for the sleep/wake fades (topmost of everything).
fn push_fade<R>(
    elements: &mut Vec<DeElement<R>>,
    power: &crate::power::Power,
    output: &de_backend::drm::OutputSelection,
    now: Instant,
) where
    R: Renderer + ImportAll + ImportMem,
    R::TextureId: smithay::backend::renderer::Texture + Clone + 'static,
{
    let alpha = power.fade_alpha(now);
    if alpha <= 0.0 {
        return;
    }
    let buf = smithay::backend::renderer::element::solid::SolidColorBuffer::new(
        (output.width as i32, output.height as i32),
        [0.0, 0.0, 0.0, 1.0],
    );
    elements.insert(
        0,
        DeElement::from(SolidColorRenderElement::from_buffer(
            &buf,
            Point::<i32, Physical>::from((0, 0)),
            1.0,
            alpha,
            Kind::Unspecified,
        )),
    );
}

/// Lifted icon + landing highlight, in front of everything else.
fn push_drag_overlay<R>(
    elements: &mut Vec<DeElement<R>>,
    renderer: &mut R,
    home: &mut crate::home_ui::HomeUi,
    output: &de_backend::drm::OutputSelection,
) where
    R: Renderer + ImportAll + ImportMem,
    R::TextureId: smithay::backend::renderer::Texture + Clone + 'static,
{
    if home.drag.is_none() {
        return;
    }
    let size = de_api::Size::new(output.width as i32, output.height as i32);
    sync_drag_textures(renderer, home);
    let Some((at, hl)) = home.drag_overlay(size) else {
        return;
    };
    if let (Some(rect), Some(tex)) = (hl, texture_buffer::<R>(home.hl_tex.as_ref())) {
        elements.insert(0, DeElement::from(tex_el_sized::<R>(tex, rect)));
    }
    if let Some(tex) = texture_buffer::<R>(home.drag_tex.as_ref()) {
        elements.insert(
            0,
            DeElement::from(tex_el::<R>(
                tex,
                Point::<f64, Physical>::from((f64::from(at.x), f64::from(at.y))),
            )),
        );
    }
}

/// Push the home layers front to back: dots, page grids at their slid
/// positions, then the base. Positions are whole pixels.
fn push_home_layers<R: ImportAll>(
    elements: &mut Vec<DeElement<R>>,
    h: &crate::home_ui::HomeUi,
    size: de_api::Size,
) where
    R::TextureId: smithay::backend::renderer::Texture + Clone + 'static,
{
    let width = size.w;
    let pos = h.pos();
    let near = pos.round().max(0.0) as usize;
    if let Some(tex) = texture_buffer::<R>(h.dots_tex.get(&near)) {
        let y = de_ui::home::dots_y(size);
        elements.push(DeElement::from(tex_el::<R>(
            tex,
            Point::<f64, Physical>::from((0.0, f64::from(y))),
        )));
    }
    let mut pages: Vec<usize> = h.page_tex.keys().copied().collect();
    pages.sort_unstable();
    for p in pages {
        let x = ((p as f32 - pos) * width as f32).round();
        if x <= -(width as f32) || x >= width as f32 {
            continue;
        }
        if let Some(tex) = texture_buffer::<R>(h.page_tex.get(&p)) {
            elements.push(DeElement::from(tex_el::<R>(
                tex,
                Point::<f64, Physical>::from((f64::from(x), f64::from(de_ui::home::grid_band_y()))),
            )));
        }
    }
    if let Some(tex) = texture_buffer::<R>(h.base_tex.as_ref()) {
        elements.push(DeElement::from(tex_el::<R>(
            tex,
            Point::<f64, Physical>::from((0.0, 0.0)),
        )));
    }
}
