//! `de-paint` — rasterize visual units into premultiplied RGBA8 [`Pixels`].
//!
//! M0 surface: [`Canvas`] (fill / rounded rect / blit), [`image`] PNG
//! decode/encode, [`text::label`] cosmic-text rasterization with the bundled
//! Noto/Material fonts. The layer/damage/animation cache arrives in M2-T4.

pub mod animation;
pub mod canvas;
pub mod damage;
pub mod fonts;
pub mod glass;
pub mod image;
pub mod layer;
pub mod region_cache;
pub mod svg;
pub mod text;
pub mod wallpaper;

pub use canvas::Canvas;
pub use image::{decode_png, encode_png, ImageError};
pub use region_cache::{RegionCache, RegionId};
pub use text::{draw_label, label, LabelStyle};
