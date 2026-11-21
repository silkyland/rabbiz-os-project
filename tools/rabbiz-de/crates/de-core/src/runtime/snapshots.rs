//! Frozen Recents thumbnails. Each mapped window owns its last successful
//! capture; background commits never replace it, and there is no disk cache.

use std::cell::RefCell;
use std::sync::Arc;

use de_api::{Pixels, Size};
use de_render::DeRenderer;
use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::element::surface::{
    render_elements_from_surface_tree, WaylandSurfaceRenderElement,
};
use smithay::backend::renderer::element::{AsRenderElements, Kind, RenderElement};
use smithay::backend::renderer::gles::GlesTexture;
use smithay::backend::renderer::utils::draw_render_elements;
use smithay::backend::renderer::{ExportMem, Frame, ImportAll, Offscreen};
use smithay::desktop::Window;
use smithay::reexports::pixman::Image;
use smithay::utils::{Physical, Point, Rectangle, Scale, Transform};
use smithay::wayland::compositor::with_states;
use smithay::wayland::shell::xdg::{ToplevelSurface, XdgToplevelSurfaceData};

use crate::state::De;

#[derive(Default)]
struct Thumbnail(RefCell<Option<Arc<Pixels>>>);

pub(crate) fn snapshot(window: &Window) -> Option<Arc<Pixels>> {
    window.user_data().get::<Thumbnail>()?.0.borrow().clone()
}

/// A toplevel's xdg role app id (empty if unset).
pub fn window_app_id(toplevel: &ToplevelSurface) -> String {
    with_states(toplevel.wl_surface(), |states| {
        states
            .data_map
            .get::<XdgToplevelSurfaceData>()
            .and_then(|d| d.lock().ok().and_then(|r| r.app_id.clone()))
            .unwrap_or_default()
    })
}

impl De {
    /// Use the same stack order for drawing cards and resolving their hits.
    pub(crate) fn recent_windows(&self) -> Vec<Window> {
        self.space
            .elements()
            .rev()
            .filter(|w| {
                w.toplevel()
                    .is_some_and(|t| crate::comp::has_buffer(t.wl_surface()))
            })
            .cloned()
            .collect()
    }

    pub(crate) fn capture_topmost(&mut self) {
        if let Some(window) = self.recent_windows().into_iter().next() {
            self.capture_recent_window(&window);
        }
    }

    pub(crate) fn capture_recent_window(&mut self, window: &Window) {
        let output = Size::new(self.output.width as i32, self.output.height as i32);
        let card = de_ui::recents::card_rect(output);
        // Keep GPU readback and per-window memory bounded even on a large output.
        let downsample = (360.0 / card.w.max(1) as f32)
            .min(800.0 / card.h.max(1) as f32)
            .min(1.0);
        let size = Size::new(
            (card.w as f32 * downsample).round().max(1.0) as i32,
            (card.h as f32 * downsample).round().max(1.0) as i32,
        );
        let result = match &mut self.renderer {
            DeRenderer::Gl(r) => capture::<_, GlesTexture>(r.as_mut(), window, output, size)
                .map_err(|e| e.to_string()),
            DeRenderer::Soft(r) => capture::<_, Image<'static, 'static>>(r, window, output, size)
                .map_err(|e| e.to_string()),
        };
        match result {
            Ok(Some(px)) => {
                window.user_data().insert_if_missing(Thumbnail::default);
                if let Some(thumb) = window.user_data().get::<Thumbnail>() {
                    thumb
                        .0
                        .replace(Some(Arc::new(de_ui::recents::round_snapshot(px, card))));
                }
            }
            Ok(None) => {}
            // A failed import/readback must not erase a previous good frame.
            Err(e) => eprintln!("recents snapshot: {e}"),
        }
    }
}

fn capture<R, T>(
    renderer: &mut R,
    window: &Window,
    output: Size,
    size: Size,
) -> Result<Option<Pixels>, R::Error>
where
    R: ImportAll + Offscreen<T> + ExportMem,
    R::TextureId: Clone + 'static,
{
    let scale = Scale::from((1.0, 1.0));
    let root = AsRenderElements::<R>::render_elements::<WaylandSurfaceRenderElement<R>>(
        window,
        renderer,
        Point::from((0, 0)),
        scale,
        1.0,
    );
    if root.is_empty() {
        return Ok(None);
    }
    let mut elements = Vec::new();
    for (popup, at) in crate::popups::popups_of(window) {
        elements.extend(render_elements_from_surface_tree::<
            R,
            WaylandSurfaceRenderElement<R>,
        >(
            renderer,
            popup.wl_surface(),
            at.to_physical_precise_round(scale),
            scale,
            1.0,
            Kind::Unspecified,
        ));
    }
    elements.extend(root);
    let full = raster_snapshot::<R, T, _>(renderer, &elements, output)?;
    if size == output {
        return Ok(Some(full));
    }
    let mut canvas = de_paint::Canvas::filled(size, de_design::DEFAULT.surface1);
    canvas.blit_scaled(&full, de_api::Rect::new(0, 0, size.w, size.h), 1.0);
    Ok(Some(canvas.into_pixels()))
}

fn raster_snapshot<R, T, E>(
    renderer: &mut R,
    elements: &[E],
    size: Size,
) -> Result<Pixels, R::Error>
where
    R: Offscreen<T> + ExportMem,
    R::TextureId: 'static,
    E: RenderElement<R>,
{
    let region = Rectangle::<i32, Physical>::from_size((size.w, size.h).into());
    let mut buffer = renderer.create_buffer(Fourcc::Abgr8888, (size.w, size.h).into())?;
    let mut target = renderer.bind(&mut buffer)?;
    let mut frame = renderer.render(&mut target, region.size, Transform::Normal)?;
    frame.clear(
        de_design::DEFAULT
            .surface1
            .map(|v| f32::from(v) / 255.0)
            .into(),
        &[region],
    )?;
    draw_render_elements(&mut frame, 1.0, elements, &[region])?;
    let _ = frame.finish()?;
    let mapping = renderer.copy_framebuffer(
        &target,
        Rectangle::from_size((size.w, size.h).into()),
        Fourcc::Abgr8888,
    )?;
    // Smithay's GLES offscreen projection already puts the logical top row
    // first in readback, just like Pixman. Do not flip the mapping again.
    Ok(Pixels::new(size, renderer.map_texture(&mapping)?.to_vec()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use smithay::backend::renderer::element::solid::{SolidColorBuffer, SolidColorRenderElement};
    use smithay::backend::renderer::pixman::PixmanRenderer;

    fn assert_readback<R, T>(renderer: &mut R)
    where
        R: Offscreen<T> + ExportMem,
        R::TextureId: 'static,
    {
        let top = SolidColorBuffer::new((3, 2), [1.0, 0.0, 0.0, 1.0]);
        let bottom = SolidColorBuffer::new((3, 2), [0.0, 0.0, 1.0, 1.0]);
        let elements = [
            SolidColorRenderElement::from_buffer(&top, (0, 0), 1.0, 1.0, Kind::Unspecified),
            SolidColorRenderElement::from_buffer(&bottom, (0, 2), 1.0, 1.0, Kind::Unspecified),
        ];
        let px = raster_snapshot::<R, T, _>(renderer, &elements, Size::new(3, 4)).unwrap();
        assert_eq!(&px.data[..24], &[255, 0, 0, 255].repeat(6));
        assert_eq!(&px.data[24..], &[0, 0, 255, 255].repeat(6));
    }

    #[test]
    fn offscreen_capture_preserves_rgba_and_row_order() {
        assert_readback::<_, Image<'static, 'static>>(&mut PixmanRenderer::new().unwrap());
    }

    #[test]
    #[ignore = "requires an EGL device with GLES readback support"]
    #[allow(unsafe_code)]
    fn gles_offscreen_capture_preserves_rgba_and_row_order() {
        use smithay::backend::egl::{EGLDevice, EGLDisplay};
        let device = EGLDevice::enumerate()
            .unwrap()
            .next()
            .expect("no EGL device");
        // SAFETY: EGLDevice is the native display handle returned by EGL.
        let display = unsafe { EGLDisplay::new(device) }.unwrap();
        let mut renderer = de_render::gl::create(&display).unwrap();
        assert_readback::<_, GlesTexture>(&mut renderer);
    }
}
