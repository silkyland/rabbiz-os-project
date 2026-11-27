/// ZRLE tile decoding (RFC 6143 §7.7.6, tiles encoded as TRLE §7.7.5).
///
/// With the negotiated pixel format (32 bpp, depth 24, little-endian, shifts 16/8/0),
/// a CPIXEL is 3 bytes stored B, G, R — matching BGRA8888 (`cairo::Format::ARgb32`
/// on little-endian) with alpha = 0xFF.

#[derive(Debug)]
pub struct DecodeError(pub String);

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for DecodeError {}

#[inline]
fn write_pixel(frame: &mut [u8], byte_offset: usize, r: u8, g: u8, b: u8) {
    frame[byte_offset] = b;
    frame[byte_offset + 1] = g;
    frame[byte_offset + 2] = r;
    frame[byte_offset + 3] = 255;
}

pub fn decode(
    data: &[u8],
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    frame: &mut [u8],
    frame_width: usize,
) -> Result<(), DecodeError> {
    if width == 0 || height == 0 {
        return Ok(());
    }
    if data.is_empty() {
        return Err(DecodeError("empty ZRLE block".to_string()));
    }

    let mut cursor = 0usize;
    let mut palette = [0u8; 16 * 3];

    let next_byte = |cur: &mut usize| -> Result<u8, DecodeError> {
        if *cur >= data.len() {
            return Err(DecodeError("ZRLE overrun".to_string()));
        }
        let b = data[*cur];
        *cur += 1;
        Ok(b)
    };

    let next_cpixel = |cur: &mut usize| -> Result<(u8, u8, u8), DecodeError> {
        if *cur + 3 > data.len() {
            return Err(DecodeError("ZRLE overrun in colour".to_string()));
        }
        let b = data[*cur];
        let g = data[*cur + 1];
        let r = data[*cur + 2];
        *cur += 3;
        Ok((r, g, b))
    };

    let next_run_length = |cur: &mut usize| -> Result<usize, DecodeError> {
        let mut length = 1usize;
        loop {
            let b = next_byte(cur)?;
            length += b as usize;
            if b != 255 {
                return Ok(length);
            }
        }
    };

    let tiles_x = width.div_ceil(64);
    let tiles_y = height.div_ceil(64);

    for tile_y in 0..tiles_y {
        for tile_x in 0..tiles_x {
            let tile_w = (width - tile_x * 64).min(64);
            let tile_h = (height - tile_y * 64).min(64);
            let pixel_count = tile_w * tile_h;
            let origin_x = x + tile_x * 64;
            let origin_y = y + tile_y * 64;
            let subencoding = next_byte(&mut cursor)?;

            match subencoding {
                0 => {
                    // Raw CPIXELs
                    for row in 0..tile_h {
                        let mut offset = ((origin_y + row) * frame_width + origin_x) * 4;
                        for _ in 0..tile_w {
                            let (r, g, b) = next_cpixel(&mut cursor)?;
                            write_pixel(frame, offset, r, g, b);
                            offset += 4;
                        }
                    }
                }
                1 => {
                    // Solid
                    let (r, g, b) = next_cpixel(&mut cursor)?;
                    for row in 0..tile_h {
                        let mut offset = ((origin_y + row) * frame_width + origin_x) * 4;
                        for _ in 0..tile_w {
                            write_pixel(frame, offset, r, g, b);
                            offset += 4;
                        }
                    }
                }
                2..=16 => {
                    // Packed palette
                    let palette_size = subencoding as usize;
                    for idx in 0..palette_size {
                        let (r, g, b) = next_cpixel(&mut cursor)?;
                        palette[idx * 3] = r;
                        palette[idx * 3 + 1] = g;
                        palette[idx * 3 + 2] = b;
                    }
                    let bits = if palette_size == 2 {
                        1
                    } else if palette_size <= 4 {
                        2
                    } else {
                        4
                    };
                    let row_bytes = (tile_w * bits).div_ceil(8);
                    let mask = ((1u16 << bits) - 1) as u8;

                    for row in 0..tile_h {
                        if cursor + row_bytes > data.len() {
                            return Err(DecodeError("ZRLE packed palette overrun".to_string()));
                        }
                        let row_slice = &data[cursor..cursor + row_bytes];
                        let mut offset = ((origin_y + row) * frame_width + origin_x) * 4;
                        let mut bit_index = 0usize;
                        for _ in 0..tile_w {
                            let byte = row_slice[bit_index >> 3];
                            let shift = 8 - bits - (bit_index & 7);
                            let idx = ((byte >> shift) & mask) as usize;
                            write_pixel(
                                frame,
                                offset,
                                palette[idx * 3],
                                palette[idx * 3 + 1],
                                palette[idx * 3 + 2],
                            );
                            offset += 4;
                            bit_index += bits;
                        }
                        cursor += row_bytes;
                    }
                }
                128 => {
                    // Plain RLE
                    let mut done = 0usize;
                    while done < pixel_count {
                        let (r, g, b) = next_cpixel(&mut cursor)?;
                        let run = next_run_length(&mut cursor)?;
                        let count = run.min(pixel_count - done);
                        for step in 0..count {
                            let pixel = done + step;
                            let offset = ((origin_y + pixel / tile_w) * frame_width
                                + origin_x
                                + pixel % tile_w)
                                * 4;
                            write_pixel(frame, offset, r, g, b);
                        }
                        done += count;
                    }
                }
                130..=255 => {
                    // Palette RLE
                    let palette_size = (subencoding - 128) as usize;
                    for idx in 0..palette_size {
                        let (r, g, b) = next_cpixel(&mut cursor)?;
                        palette[idx * 3] = r;
                        palette[idx * 3 + 1] = g;
                        palette[idx * 3 + 2] = b;
                    }
                    let mut done = 0usize;
                    while done < pixel_count {
                        let byte = next_byte(&mut cursor)?;
                        let (idx, run) = if byte & 0x80 != 0 {
                            ((byte & 0x7F) as usize, next_run_length(&mut cursor)?)
                        } else {
                            (byte as usize, 1usize)
                        };
                        if idx >= palette_size {
                            return Err(DecodeError("ZRLE palette index out of range".to_string()));
                        }
                        let count = run.min(pixel_count - done);
                        for step in 0..count {
                            let pixel = done + step;
                            let offset = ((origin_y + pixel / tile_w) * frame_width
                                + origin_x
                                + pixel % tile_w)
                                * 4;
                            write_pixel(
                                frame,
                                offset,
                                palette[idx * 3],
                                palette[idx * 3 + 1],
                                palette[idx * 3 + 2],
                            );
                        }
                        done += count;
                    }
                }
                other => {
                    return Err(DecodeError(format!(
                        "unsupported ZRLE subencoding {other}"
                    )));
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solid_tile() {
        let mut frame = vec![0u8; 4 * 4 * 4];
        // subencoding 1 (solid), CPIXEL B=0x11, G=0x22, R=0x33
        decode(&[1, 0x11, 0x22, 0x33], 1, 1, 2, 2, &mut frame, 4).unwrap();
        let off = (4 + 1) * 4;
        assert_eq!(&frame[off..off + 4], &[0x11, 0x22, 0x33, 255]);
        let off2 = (2 * 4 + 2) * 4;
        assert_eq!(&frame[off2..off2 + 4], &[0x11, 0x22, 0x33, 255]);
    }

    #[test]
    fn packed_palette_1bit() {
        let mut frame = vec![0u8; 2 * 2 * 4];
        // subencoding 2 (2 colors): color0=(0,0,0), color1=(10,20,30)
        // row0: 0b10_000000 = 0x80 (pixel 0=1, pixel 1=0)
        // row1: 0b01_000000 = 0x40 (pixel 0=0, pixel 1=1)
        let data = [2, 0, 0, 0, 10, 20, 30, 0x80, 0x40];
        decode(&data, 0, 0, 2, 2, &mut frame, 2).unwrap();
        assert_eq!(&frame[0..4], &[10, 20, 30, 255]);
        assert_eq!(&frame[4..8], &[0, 0, 0, 255]);
        assert_eq!(&frame[8..12], &[0, 0, 0, 255]);
        assert_eq!(&frame[12..16], &[10, 20, 30, 255]);
    }

    #[test]
    fn plain_rle() {
        let mut frame = vec![0u8; 2 * 2 * 4];
        // subencoding 128: 3 pixels of (1,2,3) -> run byte 2 (1+2=3), 1 pixel of (4,5,6) -> run byte 0 (1+0=1)
        let data = [128, 1, 2, 3, 2, 4, 5, 6, 0];
        decode(&data, 0, 0, 2, 2, &mut frame, 2).unwrap();
        assert_eq!(&frame[0..4], &[1, 2, 3, 255]);
        assert_eq!(&frame[8..12], &[1, 2, 3, 255]);
        assert_eq!(&frame[12..16], &[4, 5, 6, 255]);
    }
}
