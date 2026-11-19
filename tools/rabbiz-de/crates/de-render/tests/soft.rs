//! Pixman (Soft) renderer tests — the only backend that runs headless.

use de_api::RendererKind;
use de_render::target::DeRenderer;
use de_render::texture::{upload, upload_rgba, DeTexture};
use de_render::RenderError;
use smithay::backend::allocator::{
    Format as DrmFormat, Fourcc as DrmFourcc, Modifier as DrmModifier,
};

fn soft() -> DeRenderer {
    DeRenderer::Soft(de_render::soft::create().expect("pixman renderer"))
}

#[test]
fn soft_renderer_reports_soft_kind() {
    assert_eq!(soft().kind(), RendererKind::Soft);
}

/// The pixman path advertises exactly the two shm formats; anything else
/// would have to come from the GL path's EGL queries.
#[test]
fn soft_renderer_formats_match_shm_formats() {
    let fmts = soft().renderer_formats();
    assert_eq!(fmts.len(), 2);
    for f in &fmts {
        assert_eq!(f.modifier, DrmModifier::Linear);
    }
    let codes: Vec<_> = fmts.iter().map(|f: &DrmFormat| f.code).collect();
    assert_eq!(codes, vec![DrmFourcc::Argb8888, DrmFourcc::Xrgb8888]);
}

#[test]
fn render_error_display_tags_the_stage() {
    assert!(RenderError::Gl("x".into()).to_string().starts_with("gl: "));
    assert!(RenderError::Soft("x".into())
        .to_string()
        .starts_with("soft: "));
    assert!(RenderError::Upload("x".into())
        .to_string()
        .starts_with("upload: "));
}

#[test]
fn upload_into_soft_renderer_succeeds() {
    let mut r = soft();
    let px = de_api::Pixels {
        size: de_api::Size::new(4, 4),
        data: vec![0xFF; 4 * 4 * 4],
    };
    let tex = upload(&mut r, &px).expect("pixman upload");
    assert!(matches!(tex, DeTexture::Soft(_)));
}

#[test]
fn upload_rejects_short_pixel_data() {
    let mut r = soft();
    // 4×4 RGBA needs 64 bytes; 63 must not silently succeed.
    let err = upload_rgba(&mut r, &[0u8; 63], 4, 4);
    assert!(err.is_err());
}
