//! Raster decode for arbitrary photo files.
//!
//! There is no JPEG decoder in the app-kit surface, but `de-paint` already
//! pulls in `resvg` with its `raster-images` feature (zune-jpeg, image-webp,
//! gif, tiny-skia PNG). usvg's default `href` resolver reads raster files
//! straight from disk, so wrapping the path in a one-image SVG gives us
//! JPEG/PNG/WebP/GIF decode with zero new dependencies.
//!
//! Cost note: resvg decodes the *full-resolution* image regardless of the
//! requested output size, so each call is bounded by decode time (~100-300 ms
//! for a 12 MP phone photo) and transient RAM (~4 bytes/px of the source).
//! Callers must decode lazily and cache — see `ThumbCache` in `main.rs`.

use std::path::Path;

use de_api::Pixels;
use de_paint::svg;

/// How the source image fits the SVG viewport.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fit {
    /// Scale to fit entirely inside the viewport (letterbox, transparent bars).
    Meet,
    /// Scale to cover the viewport, cropping overflow (center crop).
    Slice,
}

impl Fit {
    fn as_str(self) -> &'static str {
        match self {
            Self::Meet => "xMidYMid meet",
            Self::Slice => "xMidYMid slice",
        }
    }
}

/// MIME hint for the data URL. usvg's default data resolver matches known
/// image MIMEs directly and only magic-byte-sniffs `text/plain`, so unknown
/// extensions must fall back to `text/plain`, not octet-stream.
fn mime_for(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .as_deref()
    {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        Some("gif") => "image/gif",
        _ => "text/plain",
    }
}

/// Build the wrapper SVG. `href` uses a `data:` URL so the file bytes go
/// through usvg's data-URL resolver — the same code path resvg uses for
/// embedded raster assets, and immune to odd characters in file paths.
fn svg_for(path: &Path, w: i32, h: i32, fit: Fit) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let b64 = base64_encode(&bytes);
    let href = format!("data:{};base64,{}", mime_for(path), b64);
    // The SVG is built from a base64 data URL, so no user-controlled text
    // reaches the markup — no escaping needed.
    Ok(format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" \
         viewBox=\"0 0 {w} {h}\"><image href=\"{href}\" x=\"0\" y=\"0\" \
         width=\"{w}\" height=\"{h}\" preserveAspectRatio=\"{}\"/></svg>",
        fit.as_str()
    ))
}

/// Minimal standard-base64 encoder (avoids a new dependency for ~15 lines).
fn base64_encode(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len() / 3 * 4 + 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            T[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            T[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// Rasterize `path` into a `w`×`h` premultiplied-RGBA pixmap.
pub fn rasterize(path: &Path, w: i32, h: i32, fit: Fit) -> Result<Pixels, String> {
    if w <= 0 || h <= 0 {
        return Err("empty target".to_string());
    }
    let svg = svg_for(path, w, h, fit)?;
    svg::rasterize(svg.as_bytes(), w as u32).map_err(|e| e.to_string())
}

/// Rasterize for the viewer: decode at `target_w` wide with a square
/// `meet` viewport, then crop the transparent letterbox so the returned
/// `Pixels` are exactly the photo's content — giving us the true aspect
/// ratio without needing a separate header parser.
pub fn rasterize_content(path: &Path, target_w: i32) -> Result<Pixels, String> {
    let px = rasterize(path, target_w, target_w, Fit::Meet)?;
    crop_opaque(&px).ok_or_else(|| "empty image".to_string())
}

/// Bounding box of pixels with alpha > 0, cropped out of `px`.
/// Returns `None` for a fully transparent image.
fn crop_opaque(px: &Pixels) -> Option<Pixels> {
    let (w, h) = (px.size.w as usize, px.size.h as usize);
    if w == 0 || h == 0 {
        return None;
    }
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0usize, 0usize);
    for y in 0..h {
        for x in 0..w {
            if px.data[(y * w + x) * 4 + 3] > 0 {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x + 1);
                y1 = y1.max(y + 1);
            }
        }
    }
    if x0 >= x1 || y0 >= y1 {
        return None;
    }
    let cw = (x1 - x0) as i32;
    let ch = (y1 - y0) as i32;
    let mut data = Vec::with_capacity((cw * ch * 4) as usize);
    for y in y0..y1 {
        let si = (y * w + x0) * 4;
        let ei = (y * w + x1) * 4;
        data.extend_from_slice(&px.data[si..ei]);
    }
    Some(Pixels::new(de_api::Size::new(cw, ch), data))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_vectors() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(base64_encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn rasterizes_png_roundtrip() {
        let dir = std::env::temp_dir().join(format!("rabbiz-photos-decode-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("a&b.png");
        // 2x3 opaque red PNG generated through de-paint's own encoder.
        let mut px = Pixels::empty(de_api::Size::new(2, 3));
        for i in (0..px.data.len()).step_by(4) {
            px.data[i] = 200;
            px.data[i + 3] = 255;
        }
        let png = de_paint::encode_png(&px).unwrap();
        std::fs::write(&p, &png).unwrap();
        let out = rasterize(&p, 32, 32, Fit::Slice).expect("decode png");
        assert_eq!(out.size, de_api::Size::new(32, 32));
        let content = rasterize_content(&p, 16).expect("content crop");
        // meet inside a 16x16 viewport: 2:3 image → ~11x16 content.
        assert_eq!(content.size.h, 16);
        assert!((9..=13).contains(&content.size.w), "{:?}", content.size);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
