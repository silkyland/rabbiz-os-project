//! GPU Kawase blur on `GlesRenderer` (Panfrost): downscales the live scene
//! into a 1/4-resolution offscreen `GlesTexture` and runs 3 separable-diagonal
//! Kawase passes (`fbo_a` ↔ `fbo_b`), avoiding CPU blur and 4.6 MB uploads.

use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::element::texture::{TextureBuffer, TextureRenderElement};
use smithay::backend::renderer::element::{Kind, RenderElement};
use smithay::backend::renderer::gles::{
    GlesRenderer, GlesTexProgram, GlesTexture, Uniform, UniformName, UniformType,
};
use smithay::backend::renderer::utils::draw_render_elements;
use smithay::backend::renderer::{Bind, Frame, Offscreen, Renderer};
use smithay::utils::{
    Buffer as BufferCoord, Logical, Physical, Point, Rectangle, Scale, Size, Transform,
};

use crate::RenderError;

const DOWNSCALE: i32 = 4;

const KAWASE_FRAG: &str = r#"
//_DEFINES_
#if defined(EXTERNAL)
#extension GL_OES_EGL_image_external : require
#endif
precision mediump float;
#if defined(EXTERNAL)
uniform samplerExternalOES tex;
#else
uniform sampler2D tex;
#endif
uniform float alpha;
uniform vec2 texel_offset;
varying vec2 v_coords;

void main() {
    vec4 c = texture2D(tex, v_coords + vec2(-texel_offset.x, -texel_offset.y));
    c += texture2D(tex, v_coords + vec2( texel_offset.x, -texel_offset.y));
    c += texture2D(tex, v_coords + vec2(-texel_offset.x,  texel_offset.y));
    c += texture2D(tex, v_coords + vec2( texel_offset.x,  texel_offset.y));
    c *= 0.25;
#if defined(NO_ALPHA)
    c = vec4(c.rgb, 1.0);
#endif
    gl_FragColor = c * alpha;
}
"#;

/// Reusable 1/4-resolution GPU Kawase blur pipeline for `GlesRenderer`.
pub struct GpuBlur {
    kawase: GlesTexProgram,
    fbo_a: GlesTexture,
    fbo_b: GlesTexture,
    small_size: Size<i32, BufferCoord>,
    full_size: Size<i32, Physical>,
    blurred: Option<TextureBuffer<GlesTexture>>,
}

impl GpuBlur {
    /// Allocate the shader program and two 1/4-res ping-pong textures.
    pub fn new(renderer: &mut GlesRenderer, full_w: i32, full_h: i32) -> Result<Self, RenderError> {
        let kawase = renderer
            .compile_custom_texture_shader(
                KAWASE_FRAG,
                &[UniformName::new("texel_offset", UniformType::_2f)],
            )
            .map_err(|e| RenderError::Gl(format!("kawase shader: {e}")))?;
        let small_size = Size::from(((full_w / DOWNSCALE).max(1), (full_h / DOWNSCALE).max(1)));
        let fbo_a = renderer
            .create_buffer(Fourcc::Abgr8888, small_size)
            .map_err(|e| RenderError::Gl(format!("blur fbo_a: {e}")))?;
        let fbo_b = renderer
            .create_buffer(Fourcc::Abgr8888, small_size)
            .map_err(|e| RenderError::Gl(format!("blur fbo_b: {e}")))?;
        Ok(Self {
            kawase,
            fbo_a,
            fbo_b,
            small_size,
            full_size: Size::from((full_w.max(1), full_h.max(1))),
            blurred: None,
        })
    }

    /// Downscale `elements` into `fbo_a` and run 3 Kawase passes on GPU.
    /// Captures both shell textures and live dmabuf/EGL client surfaces.
    pub fn blur_elements<E: RenderElement<GlesRenderer>>(
        &mut self,
        renderer: &mut GlesRenderer,
        elements: &[E],
        clear: [f32; 4],
    ) -> Result<(), RenderError> {
        let small_phys: Size<i32, Physical> = Size::from((self.small_size.w, self.small_size.h));
        let damage = Rectangle::from_size(small_phys);
        let scale = Scale::from((
            f64::from(self.small_size.w) / f64::from(self.full_size.w),
            f64::from(self.small_size.h) / f64::from(self.full_size.h),
        ));

        {
            let mut target = renderer
                .bind(&mut self.fbo_a)
                .map_err(|e| RenderError::Gl(format!("bind fbo_a: {e}")))?;
            let mut frame = renderer
                .render(&mut target, small_phys, Transform::Normal)
                .map_err(|e| RenderError::Gl(format!("render fbo_a: {e}")))?;
            frame
                .clear(clear.into(), &[damage])
                .map_err(|e| RenderError::Gl(format!("clear fbo_a: {e}")))?;
            draw_render_elements(&mut frame, scale, elements, &[damage])
                .map_err(|e| RenderError::Gl(format!("draw scene: {e}")))?;
            let _ = frame
                .finish()
                .map_err(|e| RenderError::Gl(format!("finish fbo_a: {e}")))?;
        }

        let src_rect: Rectangle<f64, BufferCoord> = Rectangle::from_size(
            (f64::from(self.small_size.w), f64::from(self.small_size.h)).into(),
        );
        let w_f = self.small_size.w as f32;
        let h_f = self.small_size.h as f32;

        // 3 Kawase passes: a -> b (1.5), b -> a (2.5), a -> b (3.5).
        for (pass, offset) in [1.5_f32, 2.5, 3.5].into_iter().enumerate() {
            let (src, dst) = if pass % 2 == 0 {
                (&self.fbo_a, &mut self.fbo_b)
            } else {
                (&self.fbo_b, &mut self.fbo_a)
            };
            let mut target = renderer
                .bind(dst)
                .map_err(|e| RenderError::Gl(format!("bind pass {pass}: {e}")))?;
            let mut frame = renderer
                .render(&mut target, small_phys, Transform::Normal)
                .map_err(|e| RenderError::Gl(format!("render pass {pass}: {e}")))?;
            let u = Uniform::new("texel_offset", [offset / w_f, offset / h_f]);
            frame
                .render_texture_from_to(
                    src,
                    src_rect,
                    damage,
                    &[damage],
                    &[],
                    Transform::Normal,
                    1.0,
                    Some(&self.kawase),
                    &[u],
                )
                .map_err(|e| RenderError::Gl(format!("kawase pass {pass}: {e}")))?;
            let _ = frame
                .finish()
                .map_err(|e| RenderError::Gl(format!("finish pass {pass}: {e}")))?;
        }

        self.blurred = Some(TextureBuffer::from_texture(
            renderer,
            self.fbo_b.clone(),
            1,
            Transform::Normal,
            None,
        ));
        Ok(())
    }

    /// Whether a blurred backdrop texture is ready to composite.
    #[must_use]
    pub fn has_backdrop(&self) -> bool {
        self.blurred.is_some()
    }

    /// Build a full-screen `TextureRenderElement` from the blurred 1/4-res
    /// texture (hardware bilinear upsample on scanout/composite).
    #[must_use]
    pub fn element(&self, alpha: f32) -> Option<TextureRenderElement<GlesTexture>> {
        let buf = self.blurred.as_ref()?;
        let src: Rectangle<f64, Logical> = Rectangle::from_size(
            (f64::from(self.small_size.w), f64::from(self.small_size.h)).into(),
        );
        let dst: Size<i32, Logical> = Size::from((self.full_size.w, self.full_size.h));
        Some(TextureRenderElement::from_texture_buffer(
            Point::<f64, Physical>::from((0.0, 0.0)),
            buf,
            Some(alpha),
            Some(src),
            Some(dst),
            Kind::Unspecified,
        ))
    }
}
