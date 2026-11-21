//! PNG decode/encode into the premultiplied RGBA8 `Pixels` contract.

use de_api::{Pixels, Size};
use png::{BitDepth, ColorType, Decoder, Encoder};

/// Errors from image IO.
#[derive(Debug)]
pub enum ImageError {
    Png(png::DecodingError),
    Encode(png::EncodingError),
    Unsupported {
        color_type: ColorType,
        bit_depth: BitDepth,
    },
}

impl std::fmt::Display for ImageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Png(e) => write!(f, "png decode: {e}"),
            Self::Encode(e) => write!(f, "png encode: {e}"),
            Self::Unsupported {
                color_type,
                bit_depth,
            } => {
                write!(f, "unsupported png format {color_type:?}/{bit_depth:?}")
            }
        }
    }
}

impl std::error::Error for ImageError {}

/// Decode a PNG into premultiplied RGBA8 pixels.
///
/// Any input is expanded to 8-bit RGBA; alpha is premultiplied to match the
/// `de-api` contract.
pub fn decode_png(bytes: &[u8]) -> Result<Pixels, ImageError> {
    let decoder = Decoder::new(std::io::Cursor::new(bytes));
    decode_png_pixels(decoder.read_info().map_err(ImageError::Png)?)
}

fn decode_png_pixels(
    mut reader: png::Reader<std::io::Cursor<&[u8]>>,
) -> Result<Pixels, ImageError> {
    let mut buf = vec![0; reader.output_buffer_size().unwrap_or(0)];
    let info = reader.next_frame(&mut buf).map_err(ImageError::Png)?;
    buf.truncate(info.buffer_size());
    let (ct, bd) = (info.color_type, info.bit_depth);
    if bd != BitDepth::Eight {
        return Err(ImageError::Unsupported {
            color_type: ct,
            bit_depth: bd,
        });
    }
    let size = Size::new(info.width as i32, info.height as i32);
    let mut data = Vec::with_capacity((size.area() * 4) as usize);
    match ct {
        ColorType::Rgba => {
            for px in buf.as_chunks::<4>().0 {
                let (r, g, b, a) = (
                    u32::from(px[0]),
                    u32::from(px[1]),
                    u32::from(px[2]),
                    u32::from(px[3]),
                );
                data.extend_from_slice(&[
                    ((r * a + 127) / 255) as u8,
                    ((g * a + 127) / 255) as u8,
                    ((b * a + 127) / 255) as u8,
                    a as u8,
                ]);
            }
        }
        ColorType::Rgb => {
            for px in buf.as_chunks::<3>().0 {
                data.extend_from_slice(&[px[0], px[1], px[2], 255]);
            }
        }
        ColorType::GrayscaleAlpha => {
            for px in buf.as_chunks::<2>().0 {
                let (g, a) = (u32::from(px[0]), u32::from(px[1]));
                let gp = ((g * a + 127) / 255) as u8;
                data.extend_from_slice(&[gp, gp, gp, a as u8]);
            }
        }
        ColorType::Grayscale => {
            for g in buf {
                data.extend_from_slice(&[g, g, g, 255]);
            }
        }
        ColorType::Indexed => {
            return Err(ImageError::Unsupported {
                color_type: ct,
                bit_depth: bd,
            });
        }
    }
    Ok(Pixels { size, data })
}

/// Decode local album artwork, including JPEG covers and palette PNGs.
#[must_use]
pub fn decode_album_art(bytes: &[u8]) -> Option<Pixels> {
    const MAX_SIDE: usize = 2048;
    if bytes.starts_with(&[0xFF, 0xD8]) {
        use zune_jpeg::zune_core::{colorspace::ColorSpace, options::DecoderOptions};
        let options = DecoderOptions::default()
            .set_max_width(MAX_SIDE)
            .set_max_height(MAX_SIDE)
            .jpeg_set_out_colorspace(ColorSpace::RGBA);
        let mut decoder =
            zune_jpeg::JpegDecoder::new_with_options(std::io::Cursor::new(bytes), options);
        decoder.decode_headers().ok()?;
        if decoder.output_colorspace()? != ColorSpace::RGBA {
            return None;
        }
        let info = decoder.info()?;
        let size = Size::new(i32::from(info.width), i32::from(info.height));
        return Some(Pixels::new(size, decoder.decode().ok()?));
    }
    let mut decoder = Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let reader = decoder.read_info().ok()?;
    let info = reader.info();
    if info.width > MAX_SIDE as u32 || info.height > MAX_SIDE as u32 {
        return None;
    }
    decode_png_pixels(reader).ok()
}

/// Encode premultiplied pixels to PNG (un-premultiplies alpha first).
#[allow(clippy::manual_checked_ops)] // alpha-zero guard reads better than checked_div per channel
pub fn encode_png(pixels: &Pixels) -> Result<Vec<u8>, ImageError> {
    let mut data = Vec::with_capacity(pixels.data.len());
    for px in pixels.data.as_chunks::<4>().0 {
        let (r, g, b, a) = (
            u32::from(px[0]),
            u32::from(px[1]),
            u32::from(px[2]),
            u32::from(px[3]),
        );
        if a == 0 {
            data.extend_from_slice(&[0, 0, 0, 0]);
        } else {
            data.extend_from_slice(&[
                ((r * 255 + a / 2) / a).min(255) as u8,
                ((g * 255 + a / 2) / a).min(255) as u8,
                ((b * 255 + a / 2) / a).min(255) as u8,
                a as u8,
            ]);
        }
    }
    let mut out = Vec::new();
    let mut enc = Encoder::new(&mut out, pixels.size.w as u32, pixels.size.h as u32);
    enc.set_color(ColorType::Rgba);
    enc.set_depth(BitDepth::Eight);
    let mut writer = enc.write_header().map_err(ImageError::Encode)?;
    writer.write_image_data(&data).map_err(ImageError::Encode)?;
    drop(writer);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let size = Size::new(3, 2);
        let mut px = Pixels::empty(size);
        // Half-transparent red at (1,0).
        let i = 4_usize;
        px.data[i] = 128;
        px.data[i + 3] = 128;
        let png = encode_png(&px).unwrap();
        let back = decode_png(&png).unwrap();
        assert_eq!(back.size, size);
        assert!((i32::from(back.data[i]) - 128).abs() <= 2);
        assert_eq!(back.data[i + 3], 128);
    }
}
