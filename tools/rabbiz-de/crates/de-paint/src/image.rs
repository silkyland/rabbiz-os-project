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
    let mut reader = decoder.read_info().map_err(ImageError::Png)?;
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
            for px in buf.chunks_exact(4) {
                let (r, g, b, a) = (px[0] as u32, px[1] as u32, px[2] as u32, px[3] as u32);
                data.extend_from_slice(&[
                    ((r * a + 127) / 255) as u8,
                    ((g * a + 127) / 255) as u8,
                    ((b * a + 127) / 255) as u8,
                    a as u8,
                ]);
            }
        }
        ColorType::Rgb => {
            for px in buf.chunks_exact(3) {
                data.extend_from_slice(&[px[0], px[1], px[2], 255]);
            }
        }
        ColorType::GrayscaleAlpha => {
            for px in buf.chunks_exact(2) {
                let (g, a) = (px[0] as u32, px[1] as u32);
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

/// Encode premultiplied pixels to PNG (un-premultiplies alpha first).
pub fn encode_png(pixels: &Pixels) -> Result<Vec<u8>, ImageError> {
    let mut data = Vec::with_capacity(pixels.data.len());
    for px in pixels.data.chunks_exact(4) {
        let (r, g, b, a) = (px[0] as u32, px[1] as u32, px[2] as u32, px[3] as u32);
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
        let i = (1 * 4) as usize;
        px.data[i] = 128;
        px.data[i + 3] = 128;
        let png = encode_png(&px).unwrap();
        let back = decode_png(&png).unwrap();
        assert_eq!(back.size, size);
        assert!((back.data[i] as i32 - 128).abs() <= 2);
        assert_eq!(back.data[i + 3], 128);
    }
}
