//! Headless modes for the same binary: `--cli <cmd>` text commands and the
//! `--mcp` stdio server (tool table + handlers live here; `main.rs` only
//! dispatches on argv). Nothing in this module touches Wayland.
//!
//! Library scans reuse `doc::scan`/`doc::default_dirs` and document access
//! reuses `doc::Doc`, so CLI/MCP results always match what the GUI picker
//! and viewer would show.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use rabbiz_app_kit::mcp::{serve, Tool};
use serde_json::{json, Value};

use crate::doc::{self, Doc, DocError, PdfFile};

const HELP: &str = "\
rabbiz-pdf — RabbizOS PDF reader

USAGE:
    rabbiz-pdf                    launch the GUI
    rabbiz-pdf <file.pdf>         open the document in the GUI
    rabbiz-pdf --cli <cmd>        text CLI (commands below)
    rabbiz-pdf --mcp              MCP server on stdio

COMMANDS:
    list [dir]        PDFs, newest first: name, size, mtime, path
    info <path>       pages, size, PDF version, info-dictionary metadata,
                      per-page dimensions in points
    render <path> <page> [out.png]
                      rasterize one page (1-based) to PNG at ~144 dpi;
                      default output is <name>-p<page>.png in the cwd
    help              this text

With no dir, ~/Documents, ~/Downloads, ~ and ~/Desktop are scanned
(non-recursive).

EXIT CODES: 0 ok · 1 command failed · 2 usage error
";

/// Usage text for `--help` / bare `help` invocations.
pub fn help_text() -> &'static str {
    HELP
}

/// A parsed `--cli` invocation. `page` in [`Command::Render`] is 1-based,
/// matching what a user reads off the page.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    List {
        dir: Option<PathBuf>,
    },
    Info {
        path: PathBuf,
    },
    Render {
        path: PathBuf,
        page: usize,
        out: Option<PathBuf>,
    },
    Help,
}

fn opt_dir(args: &[String], i: usize) -> Result<Option<PathBuf>, String> {
    match args.get(i) {
        None => Ok(None),
        Some(a) if a.starts_with('-') => Err(format!("unexpected argument '{a}'")),
        Some(a) => {
            if args.len() > i + 1 {
                return Err(format!("unexpected argument '{}'", args[i + 1]));
            }
            Ok(Some(PathBuf::from(a)))
        }
    }
}

fn need_path(args: &[String], i: usize, cmd: &str) -> Result<PathBuf, String> {
    let Some(a) = args.get(i) else {
        return Err(format!("'{cmd}' needs a path"));
    };
    if a.starts_with('-') {
        return Err(format!("'{cmd}' needs a path, got flag '{a}'"));
    }
    Ok(PathBuf::from(a))
}

/// Parse `--cli` arguments (everything after the literal `--cli`).
pub fn parse(args: &[String]) -> Result<Command, String> {
    let Some(cmd) = args.first() else {
        return Ok(Command::Help);
    };
    match cmd.as_str() {
        "list" => Ok(Command::List {
            dir: opt_dir(args, 1)?,
        }),
        "info" => {
            let path = need_path(args, 1, "info")?;
            if args.len() > 2 {
                return Err(format!("unexpected argument '{}'", args[2]));
            }
            Ok(Command::Info { path })
        }
        "render" => {
            let path = need_path(args, 1, "render")?;
            let page_s = args
                .get(2)
                .ok_or_else(|| "'render' needs a page number".to_string())?;
            let page: usize = page_s.parse().ok().filter(|p| *p >= 1).ok_or_else(|| {
                format!("'render' page must be a positive integer, got '{page_s}'")
            })?;
            if args.len() > 4 {
                return Err(format!("unexpected argument '{}'", args[4]));
            }
            let out = args.get(3).map(PathBuf::from);
            Ok(Command::Render { path, page, out })
        }
        "help" | "--help" | "-h" => Ok(Command::Help),
        other => Err(format!("unknown command '{other}'")),
    }
}

/// Absolute path for `p`, resolving `..`/`.` via canonicalize when possible.
/// Missing paths are returned as-is so callers get a proper fs error.
fn resolve(p: &Path) -> Result<PathBuf, String> {
    let abs = if p.is_absolute() {
        p.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| format!("cwd: {e}"))?
            .join(p)
    };
    Ok(abs.canonicalize().unwrap_or(abs))
}

fn scan_opt(dir: &Option<PathBuf>) -> Vec<PdfFile> {
    match dir {
        Some(d) => doc::scan(std::slice::from_ref(d)),
        None => doc::scan(&doc::default_dirs()),
    }
}

/// `list` output: one `name⇥size⇥mtime⇥path` line per PDF, newest first —
/// the same order and fields the picker shows.
fn list_text(list: &[PdfFile]) -> String {
    if list.is_empty() {
        return "No PDFs found.".to_string();
    }
    let mut s = String::new();
    for f in list {
        s.push_str(&format!(
            "{}\t{}\t{}\t{}\n",
            f.name,
            f.bytes,
            f.mtime,
            f.path.display()
        ));
    }
    s
}

/// Everything `info` reports, collected once so CLI text and MCP JSON share
/// the same data.
#[derive(Debug)]
pub struct PdfInfo {
    pub name: String,
    pub path: PathBuf,
    pub bytes: u64,
    pub mtime: i64,
    pub pages: usize,
    /// `(width_pt, height_pt)` per page — `render_dimensions`, so rotation
    /// and crop boxes are already applied. No rendering involved.
    pub page_sizes: Vec<(f32, f32)>,
    pub meta: doc::DocMetadata,
}

fn inspect(path: &Path) -> Result<PdfInfo, String> {
    let abs = resolve(path)?;
    let md = fs::metadata(&abs).map_err(|e| format!("{}: {e}", abs.display()))?;
    if !md.is_file() {
        return Err(format!("{}: not a file", abs.display()));
    }
    if !doc::is_pdf_file(&abs) {
        return Err(format!("{}: not a .pdf file", abs.display()));
    }
    let doc = Doc::open(&abs).map_err(|e| match e {
        DocError::Open(m) | DocError::Render(m) => m,
    })?;
    let pages = doc.page_count();
    let page_sizes = (0..pages)
        .map(|i| doc.page_size(i).unwrap_or((0.0, 0.0)))
        .collect();
    Ok(PdfInfo {
        name: doc.name.clone(),
        path: abs,
        bytes: md.len(),
        mtime: md
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0),
        pages,
        page_sizes,
        meta: doc.metadata(),
    })
}

/// Max page-dimension lines printed by `info` text; the full list is still
/// in the MCP JSON. Keeps a 1000-page doc from flooding the terminal.
const INFO_PAGE_LINES: usize = 50;

fn info_text(path: &Path) -> Result<String, String> {
    let info = inspect(path)?;
    let mut s = format!("{}\n", info.path.display());
    s.push_str(&format!(
        "  size:     {} ({} bytes)\n",
        doc::format_bytes(info.bytes),
        info.bytes
    ));
    s.push_str(&format!(
        "  modified: {} (mtime {})\n",
        info.mtime, info.mtime
    ));
    s.push_str(&format!("  pages:    {}\n", info.pages));
    if let Some(v) = &info.meta.version {
        s.push_str(&format!("  version:  PDF {v}\n"));
    }
    for (label, val) in [
        ("title", &info.meta.title),
        ("author", &info.meta.author),
        ("subject", &info.meta.subject),
        ("keywords", &info.meta.keywords),
        ("creator", &info.meta.creator),
        ("producer", &info.meta.producer),
        ("created", &info.meta.created),
        ("modified-date", &info.meta.modified),
    ] {
        if let Some(v) = val {
            s.push_str(&format!("  {label}: {v}\n"));
        }
    }
    if info.page_sizes.is_empty() {
        s.push_str("  page size: unavailable\n");
    } else {
        let shown = info.page_sizes.len().min(INFO_PAGE_LINES);
        for (i, (w, h)) in info.page_sizes.iter().take(shown).enumerate() {
            s.push_str(&format!("  page {}:   {:.0} x {:.0} pt\n", i + 1, w, h));
        }
        if info.page_sizes.len() > shown {
            s.push_str(&format!(
                "  … {} more page(s) not shown\n",
                info.page_sizes.len() - shown
            ));
        }
    }
    Ok(s)
}

/// Pixels per PDF point for `render`: ~144 dpi, the same ballpark as a
/// printed page. `Doc::render_page` still applies its own safety caps.
const RENDER_PX_PER_PT: f32 = 2.0;

fn render_text(path: &Path, page: usize, out: Option<&Path>) -> Result<String, String> {
    let abs = resolve(path)?;
    let md = fs::metadata(&abs).map_err(|e| format!("{}: {e}", abs.display()))?;
    if !md.is_file() {
        return Err(format!("{}: not a file", abs.display()));
    }
    let doc = Doc::open(&abs).map_err(|e| match e {
        DocError::Open(m) | DocError::Render(m) => m,
    })?;
    let n = doc.page_count();
    if page > n {
        return Err(format!(
            "page {page} doesn't exist (document has {n} page(s))"
        ));
    }
    let px = doc
        .render_page(page - 1, RENDER_PX_PER_PT)
        .map_err(|e| match e {
            DocError::Open(m) | DocError::Render(m) => m,
        })?;
    let png = de_paint::encode_png(&px).map_err(|e| format!("PNG encode: {e}"))?;
    let dest = match out {
        Some(o) => resolve(o)?,
        None => {
            let stem = abs.file_stem().and_then(|s| s.to_str()).unwrap_or("page");
            std::env::current_dir()
                .map_err(|e| format!("cwd: {e}"))?
                .join(format!("{stem}-p{page}.png"))
        }
    };
    fs::write(&dest, &png).map_err(|e| format!("{}: {e}", dest.display()))?;
    Ok(format!(
        "wrote {} ({} x {} px, {} bytes)\n",
        dest.display(),
        px.size.w,
        px.size.h,
        png.len()
    ))
}

fn execute(cmd: &Command) -> Result<String, String> {
    match cmd {
        Command::List { dir } => Ok(list_text(&scan_opt(dir))),
        Command::Info { path } => info_text(path),
        Command::Render { path, page, out } => render_text(path, *page, out.as_deref()),
        Command::Help => Ok(HELP.to_string()),
    }
}

/// Run `--cli` args; returns the process exit code.
/// 0 ok · 1 command failed · 2 usage error.
pub fn run_cli(args: &[String]) -> i32 {
    match parse(args) {
        Err(e) => {
            eprintln!("error: {e}");
            eprint!("{HELP}");
            2
        }
        Ok(cmd) => match execute(&cmd) {
            Ok(text) => {
                print!("{text}");
                if !text.ends_with('\n') {
                    println!();
                }
                0
            }
            Err(e) => {
                eprintln!("error: {e}");
                1
            }
        },
    }
}

// ---------------------------------------------------------------------------
// MCP server
// ---------------------------------------------------------------------------

fn arg_dir(args: &Value) -> Option<PathBuf> {
    args.get("dir")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
}

fn mcp_list(args: &Value) -> Result<String, String> {
    let list: Vec<Value> = scan_opt(&arg_dir(args))
        .iter()
        .map(|f| {
            json!({
                "name": f.name,
                "path": f.path,
                "bytes": f.bytes,
                "mtime": f.mtime,
            })
        })
        .collect();
    Ok(json!(list).to_string())
}

fn mcp_info(args: &Value) -> Result<String, String> {
    let path = args
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| "missing required argument 'path'".to_string())?;
    let info = inspect(Path::new(path))?;
    Ok(json!({
        "name": info.name,
        "path": info.path,
        "bytes": info.bytes,
        "mtime": info.mtime,
        "pages": info.pages,
        "version": info.meta.version,
        "title": info.meta.title,
        "author": info.meta.author,
        "subject": info.meta.subject,
        "keywords": info.meta.keywords,
        "creator": info.meta.creator,
        "producer": info.meta.producer,
        "created": info.meta.created,
        "modified": info.meta.modified,
        "page_sizes_pt": info.page_sizes.iter().map(|(w, h)| json!([w, h])).collect::<Vec<_>>(),
    })
    .to_string())
}

fn mcp_render(args: &Value) -> Result<String, String> {
    let path = args
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| "missing required argument 'path'".to_string())?;
    let page = args
        .get("page")
        .and_then(Value::as_u64)
        .filter(|p| *p >= 1)
        .ok_or_else(|| "missing or invalid required argument 'page' (1-based)".to_string())?;
    let out = args
        .get("out")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(PathBuf::from);
    render_text(Path::new(path), page as usize, out.as_deref())
}

/// MCP tool table served by `--mcp`.
pub static TOOLS: &[Tool] = &[
    Tool {
        name: "pdf_list",
        description: "List PDF files from ~/Documents, ~/Downloads, ~ and \
                      ~/Desktop (or 'dir' when given), newest first. \
                      Returns JSON array of {name,path,bytes,mtime}.",
        input_schema: r#"{"type":"object","properties":{"dir":{"type":"string","description":"Directory to scan instead of the defaults (non-recursive)"}},"additionalProperties":false}"#,
        handler: mcp_list,
    },
    Tool {
        name: "pdf_info",
        description: "Metadata for one PDF: file size, mtime, page count, PDF \
                      version, info-dictionary fields (title/author/… when \
                      present) and per-page dimensions in points.",
        input_schema: r#"{"type":"object","properties":{"path":{"type":"string","description":"PDF file path"}},"required":["path"],"additionalProperties":false}"#,
        handler: mcp_info,
    },
    Tool {
        name: "pdf_render",
        description: "Rasterize one page (1-based) of a PDF to PNG at ~144 \
                      dpi and write it to 'out' (default <name>-p<page>.png \
                      in the cwd). Returns the written path and pixel size.",
        input_schema: r#"{"type":"object","properties":{"path":{"type":"string","description":"PDF file path"},"page":{"type":"integer","description":"1-based page number","minimum":1},"out":{"type":"string","description":"Output PNG path (optional)"}},"required":["path","page"],"additionalProperties":false}"#,
        handler: mcp_render,
    },
];

/// Serve the MCP tool table on stdin/stdout; never returns.
pub fn serve_mcp() -> ! {
    serve(TOOLS, "rabbiz-pdf", env!("CARGO_PKG_VERSION"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static N: AtomicUsize = AtomicUsize::new(0);

    fn tmpdir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "rabbiz-pdf-cli-{tag}-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn write_pdf(dir: &Path, name: &str) -> PathBuf {
        let p = dir.join(name);
        fs::write(&p, crate::doc::tiny_pdf()).unwrap();
        p
    }

    fn s(args: &[&str]) -> Vec<String> {
        args.iter().map(|a| a.to_string()).collect()
    }

    #[test]
    fn parse_commands() {
        assert_eq!(parse(&[]).unwrap(), Command::Help);
        assert_eq!(parse(&s(&["help"])).unwrap(), Command::Help);
        assert_eq!(parse(&s(&["--help"])).unwrap(), Command::Help);
        assert_eq!(parse(&s(&["-h"])).unwrap(), Command::Help);
        assert_eq!(parse(&s(&["list"])).unwrap(), Command::List { dir: None });
        assert_eq!(
            parse(&s(&["list", "/tmp/x"])).unwrap(),
            Command::List {
                dir: Some(PathBuf::from("/tmp/x"))
            }
        );
        assert_eq!(
            parse(&s(&["info", "a.pdf"])).unwrap(),
            Command::Info {
                path: PathBuf::from("a.pdf")
            }
        );
        assert_eq!(
            parse(&s(&["render", "a.pdf", "2", "o.png"])).unwrap(),
            Command::Render {
                path: PathBuf::from("a.pdf"),
                page: 2,
                out: Some(PathBuf::from("o.png")),
            }
        );
        assert_eq!(
            parse(&s(&["render", "a.pdf", "1"])).unwrap(),
            Command::Render {
                path: PathBuf::from("a.pdf"),
                page: 1,
                out: None,
            }
        );
        // Errors: unknown command, missing args, bad page, extra args.
        assert!(parse(&s(&["bogus"])).is_err());
        assert!(parse(&s(&["info"])).is_err());
        assert!(parse(&s(&["info", "a.pdf", "extra"])).is_err());
        assert!(parse(&s(&["render", "a.pdf"])).is_err());
        assert!(parse(&s(&["render", "a.pdf", "0"])).is_err());
        assert!(parse(&s(&["render", "a.pdf", "x"])).is_err());
        assert!(parse(&s(&["list", "--bogus"])).is_err());
    }

    #[test]
    fn list_on_tempdir() {
        let dir = tmpdir("list");
        let p1 = write_pdf(&dir, "b.pdf");
        let p2 = write_pdf(&dir, "a.PDF");
        fs::write(dir.join("skip.txt"), b"x").unwrap();
        let files = doc::scan(std::slice::from_ref(&dir));
        assert_eq!(files.len(), 2);
        let text = list_text(&files);
        assert!(text.contains("b.pdf") && text.contains("a.PDF"));
        assert!(!text.contains("skip.txt"));
        // TSV contract: name, size, mtime, path.
        for line in text.lines() {
            let cols: Vec<&str> = line.split('\t').collect();
            assert_eq!(cols.len(), 4, "bad list line: {line}");
        }
        assert!(
            text.contains(&p1.display().to_string()) || text.contains(&p2.display().to_string())
        );
        assert_eq!(list_text(&[]), "No PDFs found.");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn info_on_tiny_pdf() {
        let dir = tmpdir("info");
        let p = write_pdf(&dir, "tiny.pdf");
        let info = inspect(&p).expect("tiny pdf should inspect");
        assert_eq!(info.pages, 1);
        assert_eq!(info.page_sizes.len(), 1);
        let (w, h) = info.page_sizes[0];
        assert!((w - 612.0).abs() < 0.5 && (h - 792.0).abs() < 0.5);
        assert!(info.bytes > 0);
        let text = info_text(&p).expect("info text");
        assert!(text.contains("pages:    1"));
        assert!(text.contains("612 x 792 pt"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn info_rejects_corrupt_and_non_pdf() {
        let dir = tmpdir("bad");
        let bad = dir.join("bad.pdf");
        fs::write(&bad, b"this is not a pdf at all").unwrap();
        assert!(inspect(&bad).is_err());
        assert!(inspect(&dir.join("missing.pdf")).is_err());
        let txt = dir.join("notes.txt");
        fs::write(&txt, b"hello").unwrap();
        assert!(inspect(&txt).is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn render_tiny_pdf_to_png() {
        let dir = tmpdir("render");
        let p = write_pdf(&dir, "tiny.pdf");
        let out = dir.join("out.png");
        let text = render_text(&p, 1, Some(&out)).expect("render should succeed");
        assert!(text.contains("wrote "));
        let png = fs::read(&out).unwrap();
        assert_eq!(&png[..8], &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
        // Page 2 doesn't exist.
        assert!(render_text(&p, 2, Some(&dir.join("x.png"))).is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn mcp_tools_schema_and_handlers() {
        // Every schema parses and forbids extra properties.
        for t in TOOLS {
            let v: Value = serde_json::from_str(t.input_schema).unwrap();
            assert_eq!(v["additionalProperties"], false, "{}", t.name);
        }
        assert_eq!(TOOLS.len(), 3);

        // pdf_list with explicit dir returns JSON array.
        let dir = tmpdir("mcp");
        write_pdf(&dir, "one.pdf");
        let args = json!({ "dir": dir.to_string_lossy() });
        let out: Value = serde_json::from_str(&mcp_list(&args).unwrap()).unwrap();
        assert_eq!(out.as_array().unwrap().len(), 1);
        assert_eq!(out[0]["name"], "one.pdf");

        // pdf_info happy path + missing path error.
        let p = dir.join("one.pdf");
        let info: Value =
            serde_json::from_str(&mcp_info(&json!({ "path": p.to_string_lossy() })).unwrap())
                .unwrap();
        assert_eq!(info["pages"], 1);
        assert_eq!(info["page_sizes_pt"][0][0], 612.0);
        assert!(mcp_info(&json!({})).is_err());

        // pdf_render happy path + bad page.
        let out_png = dir.join("r.png");
        let r = mcp_render(&json!({
            "path": p.to_string_lossy(),
            "page": 1,
            "out": out_png.to_string_lossy(),
        }))
        .unwrap();
        assert!(r.contains("wrote "));
        assert!(fs::metadata(&out_png).is_ok());
        assert!(mcp_render(&json!({ "path": p.to_string_lossy(), "page": 0 })).is_err());
        assert!(mcp_render(&json!({ "path": p.to_string_lossy() })).is_err());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn cli_exit_codes() {
        // Help parses and executes; run_cli returns 0.
        assert_eq!(run_cli(&s(&["help"])), 0);
        // Usage error → 2.
        assert_eq!(run_cli(&s(&["bogus"])), 2);
        assert_eq!(run_cli(&s(&["info"])), 2);
        // Command failure (missing file) → 1.
        let dir = tmpdir("exit");
        let missing = dir.join("nope.pdf");
        assert_eq!(run_cli(&s(&["info", missing.to_str().unwrap()])), 1);
        let _ = fs::remove_dir_all(&dir);
    }
}
