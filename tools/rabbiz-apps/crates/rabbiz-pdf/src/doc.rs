//! PDF document access on top of `hayro` (pure-Rust CPU rasterizer).
//!
//! `Doc` owns the parsed `hayro_syntax::Pdf`; pages are rasterized on demand
//! into premultiplied-RGBA `Pixels` at a caller-chosen scale. Rendering is
//! synchronous and expensive, so callers must go through a queue and render
//! at most one page per UI event/tick.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use de_api::{Pixels, Size};
use hayro::hayro_interpret::InterpreterSettings;
use hayro::hayro_syntax::Pdf;
use hayro::vello_cpu::color::palette::css::WHITE;
use hayro::{PixmapSettings, RenderCache, RenderSettings};

/// One file found by the picker scan.
#[derive(Debug, Clone)]
pub struct PdfFile {
    pub path: PathBuf,
    pub name: String,
    /// Seconds since the Unix epoch (for sorting / "recent" order).
    pub mtime: i64,
    pub bytes: u64,
}

#[must_use]
pub fn is_pdf_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("pdf"))
        .unwrap_or(false)
}

/// Extract the file argument passed by the desktop entry (`Exec=rabbiz-pdf %f`).
/// The Wayland runner does not consume argv, so `main` calls this before
/// `run_app`. Returns the first argument that is not a flag and exists as a
/// file; `None` when the app was launched without a document.
#[must_use]
pub fn file_arg<I, S>(args: I) -> Option<PathBuf>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    args.into_iter()
        .filter_map(|a| {
            let a = a.as_ref();
            if a.starts_with('-') {
                None
            } else {
                Some(PathBuf::from(a))
            }
        })
        .find(|p| p.is_file())
}

/// Default roots scanned by the picker, in priority order.
#[must_use]
pub fn default_dirs() -> Vec<PathBuf> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"));
    vec![
        home.join("Documents"),
        home.join("Downloads"),
        home.clone(),
        home.join("Desktop"),
    ]
}

/// Scan `dirs` (in order, non-recursive) for `*.pdf`, newest first.
/// A file reachable from several roots appears once (first root wins).
#[must_use]
pub fn scan(dirs: &[PathBuf]) -> Vec<PdfFile> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for dir in dirs {
        let Ok(rd) = fs::read_dir(dir) else {
            continue;
        };
        for entry in rd.flatten() {
            let p = entry.path();
            if !p.is_file() || !is_pdf_file(&p) {
                continue;
            }
            // Canonical-ish dedup: same file listed via ~ and ~/Downloads.
            let key = p.to_string_lossy().into_owned();
            if !seen.insert(key) {
                continue;
            }
            let Ok(md) = entry.metadata() else {
                continue;
            };
            let mtime = md
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            out.push(PdfFile {
                name: p
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("document.pdf")
                    .to_string(),
                path: p,
                mtime,
                bytes: md.len(),
            });
        }
    }
    out.sort_by(|a, b| b.mtime.cmp(&a.mtime).then_with(|| a.name.cmp(&b.name)));
    out
}

/// Human file size for the picker ("1.2 MB").
#[must_use]
pub fn format_bytes(n: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut v = n as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{n} B")
    } else {
        format!("{v:.1} {}", UNITS[i])
    }
}

/// Document-level metadata from hayro's info dictionary (`Pdf::metadata`).
/// Every field is `None` when the PDF doesn't carry it — callers should
/// report "unavailable" rather than invent values.
#[derive(Debug, Default)]
pub struct DocMetadata {
    /// `Pdf::version()` as a short string (e.g. "1.7").
    pub version: Option<String>,
    pub title: Option<String>,
    pub author: Option<String>,
    pub subject: Option<String>,
    pub keywords: Option<String>,
    pub creator: Option<String>,
    pub producer: Option<String>,
    /// ISO-ish "YYYY-MM-DD HH:MM:SS" as written in the file (offset ignored).
    pub created: Option<String>,
    pub modified: Option<String>,
}

/// A loaded PDF document. Rendering goes through [`Doc::render_page`]; the
/// caller owns caching/queuing because hayro renders synchronously.
pub struct Doc {
    pdf: Pdf,
    pub name: String,
}

/// Why a document could not be opened or a page could not be rendered.
#[derive(Debug)]
pub enum DocError {
    /// File could not be read or parsed (corrupt, unsupported, or
    /// password-protected — hayro has no encryption support).
    Open(String),
    /// Page index out of range or page render failed.
    Render(String),
}

impl std::fmt::Display for DocError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Open(m) | Self::Render(m) => f.write_str(m),
        }
    }
}

impl Doc {
    /// Read and parse `path`. All hayro load failures map to [`DocError::Open`]
    /// with a user-presentable message.
    pub fn open(path: &Path) -> Result<Self, DocError> {
        let bytes = fs::read(path).map_err(|e| DocError::Open(format!("Can't read file: {e}")))?;
        let pdf = Pdf::new(bytes).map_err(|e| {
            let msg = format!("{e:?}").to_lowercase();
            if msg.contains("encrypt") || msg.contains("password") || msg.contains("crypt") {
                DocError::Open("This PDF is password-protected".to_string())
            } else {
                DocError::Open("This PDF can't be opened (corrupt or unsupported)".to_string())
            }
        })?;
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("document.pdf")
            .to_string();
        Ok(Self { pdf, name })
    }

    #[must_use]
    pub fn page_count(&self) -> usize {
        self.pdf.pages().len()
    }

    /// Page size in PDF points (1/72″) as used by the renderer, i.e. with
    /// rotation and crop-box handling already applied.
    #[must_use]
    pub fn page_size(&self, idx: usize) -> Option<(f32, f32)> {
        self.pdf.pages().get(idx).map(|p| p.render_dimensions())
    }

    /// Info-dictionary metadata + PDF version. Cheap: no page content is
    /// touched, only the trailer dictionary hayro already parsed.
    #[must_use]
    pub fn metadata(&self) -> DocMetadata {
        let text = |b: &Option<Box<[u8]>>| {
            b.as_ref()
                .map(|v| String::from_utf8_lossy(v).into_owned())
                .filter(|s| !s.is_empty())
        };
        let date = |d: &Option<hayro::hayro_syntax::object::DateTime>| {
            d.as_ref().map(|d| {
                format!(
                    "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
                    d.year, d.month, d.day, d.hour, d.minute, d.second
                )
            })
        };
        // `metadata()` returns the single info-dictionary entry hayro
        // parsed (empty fields when the document has none).
        // `PdfVersion` debugs as e.g. `Pdf14` — normalize to "1.4".
        let v = format!("{:?}", self.pdf.version());
        let version = v
            .strip_prefix("Pdf")
            .and_then(|d| (d.len() == 2).then(|| format!("{}.{}", &d[..1], &d[1..])))
            .unwrap_or(v);
        let mut meta = DocMetadata {
            version: Some(version),
            ..DocMetadata::default()
        };
        let m = self.pdf.metadata();
        meta.title = text(&m.title);
        meta.author = text(&m.author);
        meta.subject = text(&m.subject);
        meta.keywords = text(&m.keywords);
        meta.creator = text(&m.creator);
        meta.producer = text(&m.producer);
        meta.created = date(&m.creation_date);
        meta.modified = date(&m.modification_date);
        meta
    }

    /// Rasterize page `idx` to premultiplied RGBA pixels.
    ///
    /// `px_per_pt` is the output scale (pixels per PDF point). The result is
    /// capped so no side exceeds [`MAX_PIXMAP_SIDE`] (vello_cpu pixmaps are
    /// `u16`-sized) and so the buffer stays under ~48 MB.
    pub fn render_page(&self, idx: usize, px_per_pt: f32) -> Result<Pixels, DocError> {
        let page = self
            .pdf
            .pages()
            .get(idx)
            .ok_or_else(|| DocError::Render(format!("Page {} doesn't exist", idx + 1)))?;
        let (w_pt, h_pt) = page.render_dimensions();
        if w_pt <= 0.0 || h_pt <= 0.0 {
            return Err(DocError::Render("Page has no size".to_string()));
        }
        // Scale down if the naive pixmap would exceed u16 or the byte budget.
        let mut s = px_per_pt.max(0.05);
        let max_side = MAX_PIXMAP_SIDE as f32;
        s = s.min(max_side / w_pt.max(h_pt));
        let bytes = (w_pt * s) * (h_pt * s) * 4.0;
        if bytes > MAX_PIXMAP_BYTES as f32 {
            s *= (MAX_PIXMAP_BYTES as f32 / bytes).sqrt();
        }
        let cache = RenderCache::new();
        let interp = InterpreterSettings::default();
        let rs = RenderSettings::default();
        let pm = PixmapSettings {
            x_scale: s,
            y_scale: s,
            bg_color: WHITE,
        };
        let pixmap = hayro::render(page, &cache, &interp, &rs, &pm);
        let w = pixmap.width() as i32;
        let h = pixmap.height() as i32;
        if w <= 0 || h <= 0 {
            return Err(DocError::Render("Page rendered empty".to_string()));
        }
        Ok(Pixels::new(
            Size::new(w, h),
            pixmap.data_as_u8_slice().to_vec(),
        ))
    }
}

/// vello_cpu pixmaps are `u16` per side.
const MAX_PIXMAP_SIDE: u32 = 60_000;
/// ~48 MB per rendered page.
const MAX_PIXMAP_BYTES: usize = 48 * 1024 * 1024;

/// Minimal valid one-page PDF (Helvetica "Hello" on A4-ish MediaBox).
/// Offsets are computed so the xref is well-formed. Shared with the
/// `headless` tests via `crate::doc::tiny_pdf`.
#[cfg(test)]
pub(crate) fn tiny_pdf() -> Vec<u8> {
    let stream = b"BT /F1 24 Tf 72 720 Td (Hello Rabbiz) Tj ET".to_vec();
    let s5 = {
        let mut s5 = format!("<< /Length {} >>\nstream\n", stream.len()).into_bytes();
        s5.extend_from_slice(&stream);
        s5.extend_from_slice(b"\nendstream");
        s5
    };
    let objs: Vec<Vec<u8>> = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>"
            .to_vec(),
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec(),
        s5,
    ];

    let mut pdf = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (i, body) in objs.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        pdf.extend_from_slice(body);
        pdf.extend_from_slice(b"\nendobj\n");
    }
    let xref_at = pdf.len();
    pdf.extend_from_slice(format!("xref\n0 {}\n", objs.len() + 1).as_bytes());
    pdf.extend_from_slice(b"0000000000 65535 f \n");
    for off in &offsets {
        pdf.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref_at}\n%%EOF",
            objs.len() + 1
        )
        .as_bytes(),
    );
    pdf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pdf_filter() {
        assert!(is_pdf_file(Path::new("/x/report.PDF")));
        assert!(is_pdf_file(Path::new("/x/a.pdf")));
        assert!(!is_pdf_file(Path::new("/x/a.txt")));
        assert!(!is_pdf_file(Path::new("/x/noext")));
    }

    #[test]
    fn file_arg_skips_flags_and_missing() {
        let args = vec!["rabbiz-pdf", "--foo", "/tmp/x.pdf"];
        // /tmp/x.pdf won't exist → None; use a real temp file instead.
        assert_eq!(file_arg(args), None);
        let f = std::env::temp_dir().join(format!("rabbiz-pdf-arg-{}", std::process::id()));
        fs::write(&f, b"x").unwrap();
        let p = file_arg(vec![
            "rabbiz-pdf".to_string(),
            "--force".to_string(),
            f.to_string_lossy().into_owned(),
        ]);
        assert_eq!(p.as_deref(), Some(f.as_path()));
        let _ = fs::remove_file(&f);
    }

    #[test]
    fn scan_finds_pdfs_sorted() {
        let dir = std::env::temp_dir().join(format!("rabbiz-pdf-scan-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("b.pdf"), b"%PDF").unwrap();
        fs::write(dir.join("a.PDF"), b"%PDF").unwrap();
        fs::write(dir.join("skip.txt"), b"x").unwrap();
        let files = scan(std::slice::from_ref(&dir));
        assert_eq!(files.len(), 2);
        assert!(files
            .iter()
            .all(|f| f.name.ends_with(".pdf") || f.name.ends_with(".PDF")));
        assert!(files.windows(2).all(|w| w[0].mtime >= w[1].mtime));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn open_rejects_garbage() {
        let f = std::env::temp_dir().join(format!("rabbiz-pdf-bad-{}", std::process::id()));
        fs::write(&f, b"this is not a pdf at all").unwrap();
        assert!(matches!(Doc::open(&f), Err(DocError::Open(_))));
        let _ = fs::remove_file(&f);
    }

    #[test]
    fn render_tiny_pdf_and_time_it() {
        let f = std::env::temp_dir().join(format!("rabbiz-pdf-tiny-{}", std::process::id()));
        fs::write(&f, tiny_pdf()).unwrap();
        let doc = Doc::open(&f).expect("tiny pdf should parse");
        assert_eq!(doc.page_count(), 1);
        let (w, h) = doc.page_size(0).unwrap();
        assert!((w - 612.0).abs() < 0.5 && (h - 792.0).abs() < 0.5);
        // ~2 px/pt ≈ A4 at ~1200 px wide — representative of on-screen scale.
        let t0 = std::time::Instant::now();
        let px = doc.render_page(0, 2.0).expect("render should succeed");
        let elapsed = t0.elapsed();
        eprintln!("hayro render 1224x1584px took {elapsed:?}");
        assert!(px.size.w > 1000 && px.size.h > 1000);
        assert_eq!(px.data.len() as i64, px.size.area() * 4);
        let _ = fs::remove_file(&f);
    }
}
