//! Wire protocol for `rabbiz-emu` frame streaming, dirty-tile damage detection,
//! and input/control communication between `rabbiz-de --emu-listen` and
//! `rabbiz-emu-viewer` / `rabbiz emu`.

#![forbid(unsafe_code)]

use std::io::{BufRead, BufReader, Read, Write};

use de_api::{Pixels, Point, Rect, Size};
use de_paint::{decode_png, encode_png, Canvas};
use serde::{Deserialize, Serialize};

/// Default 64x64 tile size for dirty-region damage checks.
pub const DEFAULT_TILE_SIZE: i32 = 64;

/// Maximum allowed single frame payload (32 MiB) to guard against malformed streams.
pub const MAX_FRAME_BYTES: usize = 32 * 1024 * 1024;

/// Initial handshake from an emulator compositor endpoint (`rabbiz-de --emu-listen`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmuHello {
    pub width: u32,
    pub height: u32,
    pub refresh_hz: f64,
    pub form_factor: String,
}

impl Default for EmuHello {
    fn default() -> Self {
        Self {
            width: 720,
            height: 1600,
            refresh_hz: 60.0,
            form_factor: "handset".to_string(),
        }
    }
}

/// One damaged tile (or full-screen region) inside an [`EmuFrame`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameTile {
    pub rect: Rect,
    pub png_or_rgba: Vec<u8>,
    pub is_png: bool,
}

/// A streamed emulator frame with one or more [`FrameTile`] updates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmuFrame {
    pub seq: u64,
    pub timestamp_ms: u64,
    pub full_size: Size,
    pub tiles: Vec<FrameTile>,
}

impl EmuFrame {
    /// Encode a complete [`Pixels`] buffer as a single full-screen PNG [`FrameTile`].
    pub fn encode_full_png(seq: u64, timestamp_ms: u64, pixels: &Pixels) -> Result<Self, String> {
        if pixels.size.is_empty() {
            return Err("cannot encode empty pixels".to_string());
        }
        let png = encode_png(pixels).map_err(|e| format!("encode png: {e}"))?;
        let tile = FrameTile {
            rect: Rect::from_origin_size(Point::ZERO, pixels.size),
            png_or_rgba: png,
            is_png: true,
        };
        Ok(Self {
            seq,
            timestamp_ms,
            full_size: pixels.size,
            tiles: vec![tile],
        })
    }

    /// Encode only the dirty tiles between `prev` and `curr` as raw RGBA8 tiles
    /// (or full frame if dimensions changed).
    pub fn encode_dirty_tiles(
        seq: u64,
        timestamp_ms: u64,
        prev: &Pixels,
        curr: &Pixels,
        tile_size: i32,
    ) -> Result<Self, String> {
        if curr.size.is_empty() {
            return Err("cannot encode empty current pixels".to_string());
        }
        let dirty_rects = compute_dirty_tiles(prev, curr, tile_size);
        let mut tiles = Vec::with_capacity(dirty_rects.len());
        for rect in dirty_rects {
            let rgba = extract_rect_rgba(curr, rect);
            tiles.push(FrameTile {
                rect,
                png_or_rgba: rgba,
                is_png: false,
            });
        }
        Ok(Self {
            seq,
            timestamp_ms,
            full_size: curr.size,
            tiles,
        })
    }

    /// Decode and composite all tiles in this frame into `target`, resizing
    /// `target` if its dimensions do not match `self.full_size`.
    ///
    /// Returns the list of updated [`Rect`] regions.
    pub fn decode_into(&self, target: &mut Pixels) -> Result<Vec<Rect>, String> {
        if self.full_size.is_empty() {
            return Err("invalid empty frame full_size".to_string());
        }
        if target.size != self.full_size
            || target.data.len() != (self.full_size.area() * 4) as usize
        {
            *target = Pixels::empty(self.full_size);
        }

        let mut updated = Vec::with_capacity(self.tiles.len());
        for tile in &self.tiles {
            if tile.rect.is_empty() {
                continue;
            }
            let tile_px = if tile.is_png {
                decode_png(&tile.png_or_rgba).map_err(|e| format!("decode png tile: {e}"))?
            } else {
                let expected = (tile.rect.size().area() * 4) as usize;
                if tile.png_or_rgba.len() != expected {
                    return Err(format!(
                        "raw tile byte count mismatch: got {}, expected {expected}",
                        tile.png_or_rgba.len()
                    ));
                }
                Pixels::new(tile.rect.size(), tile.png_or_rgba.clone())
            };

            if tile_px.size != tile.rect.size() {
                return Err(format!(
                    "decoded tile size {}x{} != rect {}x{}",
                    tile_px.size.w, tile_px.size.h, tile.rect.w, tile.rect.h
                ));
            }

            copy_tile_into(target, &tile_px, tile.rect.origin())?;
            updated.push(tile.rect);
        }

        Ok(updated)
    }
}

/// Compare `prev` and `curr` on a `tile_size x tile_size` grid (default 64x64)
/// and return the bounding [`Rect`] of every tile where at least one pixel changed.
///
/// If `prev.size != curr.size`, returns all tiles covering `curr.size`.
#[must_use]
pub fn compute_dirty_tiles(prev: &Pixels, curr: &Pixels, tile_size: i32) -> Vec<Rect> {
    if curr.size.is_empty() {
        return Vec::new();
    }
    let step = tile_size.max(1);
    let w = curr.size.w;
    let h = curr.size.h;
    let size_mismatch = prev.size != curr.size
        || prev.data.len() != (prev.size.area() * 4) as usize
        || curr.data.len() != (curr.size.area() * 4) as usize;

    let mut dirty = Vec::new();
    let stride = (w * 4) as usize;

    let mut ty = 0;
    while ty < h {
        let th = (h - ty).min(step);
        let mut tx = 0;
        while tx < w {
            let tw = (w - tx).min(step);
            let rect = Rect::new(tx, ty, tw, th);
            let is_dirty = if size_mismatch {
                true
            } else {
                tile_differs(&prev.data, &curr.data, stride, rect)
            };
            if is_dirty {
                dirty.push(rect);
            }
            tx += step;
        }
        ty += step;
    }

    dirty
}

fn tile_differs(prev: &[u8], curr: &[u8], stride: usize, rect: Rect) -> bool {
    let row_bytes = (rect.w * 4) as usize;
    let x_offset = (rect.x * 4) as usize;
    for r in 0..rect.h as usize {
        let start = (rect.y as usize + r) * stride + x_offset;
        let end = start + row_bytes;
        if prev[start..end] != curr[start..end] {
            return true;
        }
    }
    false
}

fn extract_rect_rgba(src: &Pixels, rect: Rect) -> Vec<u8> {
    let stride = (src.size.w * 4) as usize;
    let row_bytes = (rect.w * 4) as usize;
    let x_offset = (rect.x * 4) as usize;
    let mut out = vec![0u8; (rect.w * rect.h * 4) as usize];
    for r in 0..rect.h as usize {
        let s_start = (rect.y as usize + r) * stride + x_offset;
        let d_start = r * row_bytes;
        out[d_start..d_start + row_bytes].copy_from_slice(&src.data[s_start..s_start + row_bytes]);
    }
    out
}

fn copy_tile_into(dst: &mut Pixels, tile: &Pixels, origin: Point) -> Result<(), String> {
    let bounds = Rect::from_origin_size(Point::ZERO, dst.size);
    let tile_rect = Rect::from_origin_size(origin, tile.size);
    let Some(clamped) = tile_rect.intersect(bounds) else {
        return Ok(());
    };
    let dst_stride = (dst.size.w * 4) as usize;
    let src_stride = (tile.size.w * 4) as usize;
    let copy_bytes = (clamped.w * 4) as usize;

    for r in 0..clamped.h as usize {
        let dy = (clamped.y as usize + r) * dst_stride + (clamped.x as usize * 4);
        let sy = ((clamped.y - origin.y) as usize + r) * src_stride
            + ((clamped.x - origin.x) as usize * 4);
        dst.data[dy..dy + copy_bytes].copy_from_slice(&tile.data[sy..sy + copy_bytes]);
    }
    Ok(())
}

/// Write an [`EmuHello`] handshake line (`HELLO <w> <h> <hz> <form_factor>\n`).
pub fn write_hello<W: Write>(writer: &mut W, hello: &EmuHello) -> Result<(), String> {
    writeln!(
        writer,
        "HELLO {} {} {:.2} {}",
        hello.width, hello.height, hello.refresh_hz, hello.form_factor
    )
    .map_err(|e| format!("write hello: {e}"))?;
    writer.flush().map_err(|e| format!("flush hello: {e}"))
}

/// Parse an [`EmuHello`] from a `"HELLO ..."` line or JSON payload.
pub fn parse_hello_line(line: &str) -> Result<EmuHello, String> {
    let trimmed = line.trim();
    if trimmed.starts_with('{') {
        return serde_json::from_str(trimmed).map_err(|e| format!("invalid JSON hello: {e}"));
    }
    let parts: Vec<&str> = trimmed.split_whitespace().collect();
    if parts.len() < 4 || parts[0] != "HELLO" {
        return Err(format!("invalid HELLO line: {trimmed:?}"));
    }
    let width: u32 = parts[1]
        .parse()
        .map_err(|e| format!("invalid hello width: {e}"))?;
    let height: u32 = parts[2]
        .parse()
        .map_err(|e| format!("invalid hello height: {e}"))?;
    let refresh_hz: f64 = parts[3]
        .parse()
        .map_err(|e| format!("invalid hello refresh_hz: {e}"))?;
    let form_factor = if parts.len() >= 5 {
        parts[4].to_string()
    } else {
        "handset".to_string()
    };
    Ok(EmuHello {
        width,
        height,
        refresh_hz,
        form_factor,
    })
}

/// Read an [`EmuHello`] handshake line from `reader`.
pub fn read_hello<R: BufRead>(reader: &mut R) -> Result<EmuHello, String> {
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .map_err(|e| format!("read hello: {e}"))?;
    if line.is_empty() {
        return Err("unexpected EOF reading HELLO".to_string());
    }
    parse_hello_line(&line)
}

/// Write a frame packet to `writer`.
///
/// If the frame has a single full-screen PNG tile, writes the canonical wire format
/// spoken by `rabbiz-de --emu-listen`:
/// `FRAME <seq> <w> <h> <len>\n<png_bytes>`
/// Otherwise writes a multi-tile packet:
/// `TILES <seq> <ts_ms> <w> <h> <count>\n` followed by `TILE <x> <y> <w> <h> <is_png> <len>\n<bytes>`.
pub fn write_frame_packet<W: Write>(writer: &mut W, frame: &EmuFrame) -> Result<(), String> {
    if frame.tiles.len() == 1
        && frame.tiles[0].is_png
        && frame.tiles[0].rect == Rect::from_origin_size(Point::ZERO, frame.full_size)
    {
        let bytes = &frame.tiles[0].png_or_rgba;
        writeln!(
            writer,
            "FRAME {} {} {} {}",
            frame.seq,
            frame.full_size.w,
            frame.full_size.h,
            bytes.len()
        )
        .map_err(|e| format!("write frame header: {e}"))?;
        writer
            .write_all(bytes)
            .map_err(|e| format!("write frame payload: {e}"))?;
        return writer.flush().map_err(|e| format!("flush frame: {e}"));
    }

    writeln!(
        writer,
        "TILES {} {} {} {} {}",
        frame.seq,
        frame.timestamp_ms,
        frame.full_size.w,
        frame.full_size.h,
        frame.tiles.len()
    )
    .map_err(|e| format!("write tiles header: {e}"))?;
    for tile in &frame.tiles {
        writeln!(
            writer,
            "TILE {} {} {} {} {} {}",
            tile.rect.x,
            tile.rect.y,
            tile.rect.w,
            tile.rect.h,
            u8::from(tile.is_png),
            tile.png_or_rgba.len()
        )
        .map_err(|e| format!("write tile header: {e}"))?;
        writer
            .write_all(&tile.png_or_rgba)
            .map_err(|e| format!("write tile bytes: {e}"))?;
    }
    writer.flush().map_err(|e| format!("flush tiles: {e}"))
}

/// Read a single [`EmuFrame`] packet (`FRAME ...` or `TILES ...`) from `reader`.
pub fn read_frame_packet<R: BufRead>(reader: &mut R) -> Result<EmuFrame, String> {
    let mut header = String::new();
    reader
        .read_line(&mut header)
        .map_err(|e| format!("read frame header: {e}"))?;
    if header.is_empty() {
        return Err("unexpected EOF reading frame header".to_string());
    }
    let parts: Vec<&str> = header.split_whitespace().collect();
    if parts.is_empty() {
        return Err("empty frame header".to_string());
    }

    match parts[0] {
        "FRAME" | "RAW" => {
            let is_png = parts[0] == "FRAME";
            if parts.len() < 5 {
                return Err(format!("malformed {} header: {:?}", parts[0], header.trim()));
            }
            let seq: u64 = parts[1].parse().map_err(|e| format!("seq: {e}"))?;
            let w: i32 = parts[2].parse().map_err(|e| format!("width: {e}"))?;
            let h: i32 = parts[3].parse().map_err(|e| format!("height: {e}"))?;
            let len: usize = parts[4].parse().map_err(|e| format!("len: {e}"))?;
            if len > MAX_FRAME_BYTES {
                return Err(format!("frame payload {len} exceeds MAX_FRAME_BYTES"));
            }
            let mut payload = vec![0u8; len];
            reader
                .read_exact(&mut payload)
                .map_err(|e| format!("read frame payload: {e}"))?;
            let full_size = Size::new(w, h);
            Ok(EmuFrame {
                seq,
                timestamp_ms: 0,
                full_size,
                tiles: vec![FrameTile {
                    rect: Rect::from_origin_size(Point::ZERO, full_size),
                    png_or_rgba: payload,
                    is_png,
                }],
            })
        }
        "TILES" => {
            if parts.len() < 6 {
                return Err(format!("malformed TILES header: {:?}", header.trim()));
            }
            let seq: u64 = parts[1].parse().map_err(|e| format!("seq: {e}"))?;
            let timestamp_ms: u64 = parts[2].parse().map_err(|e| format!("ts: {e}"))?;
            let w: i32 = parts[3].parse().map_err(|e| format!("w: {e}"))?;
            let h: i32 = parts[4].parse().map_err(|e| format!("h: {e}"))?;
            let count: usize = parts[5].parse().map_err(|e| format!("count: {e}"))?;
            if count > 4096 {
                return Err(format!("tile count {count} exceeds limit"));
            }
            let mut tiles = Vec::with_capacity(count);
            for _ in 0..count {
                let mut tline = String::new();
                reader
                    .read_line(&mut tline)
                    .map_err(|e| format!("read tile line: {e}"))?;
                let tp: Vec<&str> = tline.split_whitespace().collect();
                if tp.len() < 7 || tp[0] != "TILE" {
                    return Err(format!("malformed TILE header: {:?}", tline.trim()));
                }
                let x: i32 = tp[1].parse().map_err(|e| format!("tile x: {e}"))?;
                let y: i32 = tp[2].parse().map_err(|e| format!("tile y: {e}"))?;
                let tw: i32 = tp[3].parse().map_err(|e| format!("tile w: {e}"))?;
                let th: i32 = tp[4].parse().map_err(|e| format!("tile h: {e}"))?;
                let is_png = tp[5] != "0";
                let len: usize = tp[6].parse().map_err(|e| format!("tile len: {e}"))?;
                if len > MAX_FRAME_BYTES {
                    return Err(format!("tile payload {len} exceeds MAX_FRAME_BYTES"));
                }
                let mut buf = vec![0u8; len];
                reader
                    .read_exact(&mut buf)
                    .map_err(|e| format!("read tile bytes: {e}"))?;
                tiles.push(FrameTile {
                    rect: Rect::new(x, y, tw, th),
                    png_or_rgba: buf,
                    is_png,
                });
            }
            Ok(EmuFrame {
                seq,
                timestamp_ms,
                full_size: Size::new(w, h),
                tiles,
            })
        }
        "ERR" => Err(format!("emulator frame error: {}", header.trim())),
        other => Err(format!("unknown frame packet type: {other:?}")),
    }
}

/// Request `HELLO\n` over a stream (e.g. `TcpStream`) and parse the [`EmuHello`] response.
pub fn client_handshake<S: Read + Write>(stream: &mut S) -> Result<EmuHello, String> {
    stream
        .write_all(b"HELLO\n")
        .map_err(|e| format!("send HELLO: {e}"))?;
    stream.flush().map_err(|e| format!("flush HELLO: {e}"))?;
    let mut reader = BufReader::new(stream);
    read_hello(&mut reader)
}

/// Request `FRAME\n` over a stream and decode the returned [`EmuFrame`].
pub fn client_poll_frame<S: Read + Write>(stream: &mut S) -> Result<EmuFrame, String> {
    stream
        .write_all(b"FRAME\n")
        .map_err(|e| format!("send FRAME: {e}"))?;
    stream.flush().map_err(|e| format!("flush FRAME: {e}"))?;
    let mut reader = BufReader::new(stream);
    read_frame_packet(&mut reader)
}

/// Request `RAW\n` (uncompressed RGBA8) over a stream for zero-compression 60 FPS viewing.
pub fn client_poll_raw_frame<S: Read + Write>(stream: &mut S) -> Result<EmuFrame, String> {
    stream
        .write_all(b"RAW\n")
        .map_err(|e| format!("send RAW: {e}"))?;
    stream.flush().map_err(|e| format!("flush RAW: {e}"))?;
    let mut reader = BufReader::new(stream);
    read_frame_packet(&mut reader)
}

/// Request `RAW_IF_NEWER <last_seq>\n`: returns `Ok(Some(EmuFrame))` when a new frame
/// has been composited since `last_seq`, or `Ok(None)` when the server replies `UNCHANGED`.
pub fn client_poll_raw_if_newer<S: Read + Write>(
    stream: &mut S,
    last_seq: u64,
) -> Result<Option<EmuFrame>, String> {
    let cmd = format!("RAW_IF_NEWER {last_seq}\n");
    stream
        .write_all(cmd.as_bytes())
        .map_err(|e| format!("send RAW_IF_NEWER: {e}"))?;
    stream.flush().map_err(|e| format!("flush RAW_IF_NEWER: {e}"))?;
    let mut reader = BufReader::new(stream);
    let mut header = String::new();
    reader
        .read_line(&mut header)
        .map_err(|e| format!("read RAW_IF_NEWER header: {e}"))?;
    if header.is_empty() {
        return Err("unexpected EOF reading RAW_IF_NEWER".to_string());
    }
    let parts: Vec<&str> = header.split_whitespace().collect();
    if parts.is_empty() {
        return Err("empty RAW_IF_NEWER header".to_string());
    }
    if parts[0] == "UNCHANGED" {
        return Ok(None);
    }
    if parts[0] == "RAW" || parts[0] == "FRAME" {
        let is_png = parts[0] == "FRAME";
        if parts.len() < 5 {
            return Err(format!("malformed {} header: {:?}", parts[0], header.trim()));
        }
        let seq: u64 = parts[1].parse().map_err(|e| format!("seq: {e}"))?;
        let w: i32 = parts[2].parse().map_err(|e| format!("width: {e}"))?;
        let h: i32 = parts[3].parse().map_err(|e| format!("height: {e}"))?;
        let len: usize = parts[4].parse().map_err(|e| format!("len: {e}"))?;
        if len > MAX_FRAME_BYTES {
            return Err(format!("frame payload {len} exceeds MAX_FRAME_BYTES"));
        }
        let mut payload = vec![0u8; len];
        reader
            .read_exact(&mut payload)
            .map_err(|e| format!("read frame payload: {e}"))?;
        let full_size = Size::new(w, h);
        return Ok(Some(EmuFrame {
            seq,
            timestamp_ms: 0,
            full_size,
            tiles: vec![FrameTile {
                rect: Rect::from_origin_size(Point::ZERO, full_size),
                png_or_rgba: payload,
                is_png,
            }],
        }));
    }
    Err(format!("unexpected RAW_IF_NEWER reply: {:?}", header.trim()))
}

/// Helper to create a test [`Pixels`] canvas filled with `color`.
#[must_use]
pub fn solid_pixels(size: Size, color: [u8; 4]) -> Pixels {
    Canvas::filled(size, color).into_pixels()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn dirty_tile_detection_64x64_skips_identical_and_pinpoints_changed_tile() {
        let size = Size::new(192, 192);
        let prev = solid_pixels(size, [0x20, 0x30, 0x40, 0xFF]);
        let mut curr = prev.clone();

        // Identical buffers -> 0 dirty tiles.
        let none = compute_dirty_tiles(&prev, &curr, DEFAULT_TILE_SIZE);
        assert!(none.is_empty(), "identical frames must emit 0 dirty tiles");

        // Mutate a single pixel at (75, 130) -> falls inside tile (64, 128, 64, 64).
        let idx = ((130 * size.w + 75) * 4) as usize;
        curr.data[idx] = 0xFF;

        let dirty = compute_dirty_tiles(&prev, &curr, DEFAULT_TILE_SIZE);
        assert_eq!(dirty, vec![Rect::new(64, 128, 64, 64)]);
    }

    #[test]
    fn dirty_tiles_encode_and_decode_into_reconstructs_exact_frame() {
        let size = Size::new(140, 130); // non-multiple of 64 to exercise edge tiles
        let prev = solid_pixels(size, [10, 20, 30, 255]);
        let mut curr = prev.clone();

        // Change pixel in bottom-right partial tile (135, 129) -> tile (128, 128, 12, 2).
        let idx = ((129 * size.w + 135) * 4) as usize;
        curr.data[idx + 1] = 240;

        let frame =
            EmuFrame::encode_dirty_tiles(7, 1234, &prev, &curr, DEFAULT_TILE_SIZE).unwrap();
        assert_eq!(frame.tiles.len(), 1);
        assert_eq!(frame.tiles[0].rect, Rect::new(128, 128, 12, 2));

        let mut target = prev.clone();
        let updated = frame.decode_into(&mut target).unwrap();
        assert_eq!(updated, vec![Rect::new(128, 128, 12, 2)]);
        assert_eq!(target.data, curr.data);
    }

    #[test]
    fn full_png_frame_roundtrip_and_wire_stream_framing() {
        let mut c = Canvas::filled(Size::new(128, 96), [0x16, 0x10, 0x1A, 0xFF]);
        c.fill_rect(Rect::new(16, 16, 48, 32), [0xD0, 0x8A, 0x5A, 0xFF]);
        let px = c.into_pixels();

        let frame = EmuFrame::encode_full_png(42, 999, &px).expect("encode_full_png");
        let mut wire = Vec::new();
        write_frame_packet(&mut wire, &frame).expect("write_frame_packet");

        // Verify canonical "FRAME 42 128 96 <len>\n" prefix.
        let header_end = wire.iter().position(|&b| b == b'\n').unwrap();
        let header_str = std::str::from_utf8(&wire[..header_end]).unwrap();
        assert!(header_str.starts_with("FRAME 42 128 96 "));

        let mut reader = Cursor::new(wire);
        let decoded_frame = read_frame_packet(&mut reader).expect("read_frame_packet");
        assert_eq!(decoded_frame.seq, 42);
        assert_eq!(decoded_frame.full_size, Size::new(128, 96));

        let mut out_px = Pixels::empty(Size::ZERO);
        let rects = decoded_frame.decode_into(&mut out_px).expect("decode_into");
        assert_eq!(rects, vec![Rect::new(0, 0, 128, 96)]);
        assert_eq!(out_px.size, px.size);
        assert_eq!(out_px.data, px.data);
    }

    #[test]
    fn hello_and_multi_tile_stream_framing_roundtrip() {
        let hello = EmuHello {
            width: 720,
            height: 1600,
            refresh_hz: 60.0,
            form_factor: "handset".to_string(),
        };
        let mut buf = Vec::new();
        write_hello(&mut buf, &hello).unwrap();
        let mut reader = Cursor::new(buf);
        let parsed = read_hello(&mut reader).unwrap();
        assert_eq!(parsed, hello);

        let size = Size::new(128, 128);
        let prev = solid_pixels(size, [1, 2, 3, 255]);
        let mut curr = prev.clone();
        curr.data[0] = 99;
        curr.data[((90 * 128 + 90) * 4) as usize] = 77;

        let tile_frame = EmuFrame::encode_dirty_tiles(9, 500, &prev, &curr, 64).unwrap();
        assert_eq!(tile_frame.tiles.len(), 2);

        let mut tile_wire = Vec::new();
        write_frame_packet(&mut tile_wire, &tile_frame).unwrap();
        let mut tile_reader = Cursor::new(tile_wire);
        let read_back = read_frame_packet(&mut tile_reader).unwrap();
        assert_eq!(read_back, tile_frame);
    }
}
