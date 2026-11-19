//! Headless modes for the same binary: `--cli <cmd>` text commands and the
//! `--mcp` stdio server (tool table + handlers live here; `main.rs` only
//! dispatches on argv). Nothing in this module touches Wayland.
//!
//! Library scans reuse `photos::scan`/`default_dirs` and image dimensions
//! reuse `decode::rasterize_content`, so CLI/MCP results always match what
//! the GUI grid and viewer would show.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rabbiz_app_kit::mcp::{serve, Tool};
use serde_json::{json, Value};

use crate::decode;
use crate::photos::{self, Photo};
use crate::viewer::VIEW_DECODE_W;

const HELP: &str = "\
rabbiz-photos — RabbizOS Photos

USAGE:
    rabbiz-photos                 launch the GUI
    rabbiz-photos <file-or-dir>   headless: a directory is listed, a file's
                                  resolved path is printed
    rabbiz-photos --cli <cmd>     text CLI (commands below)
    rabbiz-photos --mcp           MCP server on stdio

COMMANDS:
    list [dir]      photos, newest first: day, size, path
    albums [dir]    photos grouped by day with counts (alias: days)
    info <path>     size, mtime, day label, decoded dimensions
    open <path>     print the resolved absolute path
    help            this text

With no dir, ~/Pictures and ~/Downloads are scanned (non-recursive).
";

/// Usage text for `--help` / bare `help` invocations.
pub fn help_text() -> &'static str {
    HELP
}

/// A parsed `--cli` invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    List { dir: Option<PathBuf> },
    Albums { dir: Option<PathBuf> },
    Info { path: PathBuf },
    Open { path: PathBuf },
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
    if args.len() > i + 1 {
        return Err(format!("unexpected argument '{}'", args[i + 1]));
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
        "albums" | "days" => Ok(Command::Albums {
            dir: opt_dir(args, 1)?,
        }),
        "info" => Ok(Command::Info {
            path: need_path(args, 1, "info")?,
        }),
        "open" => Ok(Command::Open {
            path: need_path(args, 1, "open")?,
        }),
        "help" | "--help" | "-h" => Ok(Command::Help),
        other => Err(format!("unknown command '{other}'")),
    }
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
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

fn scan_opt(dir: &Option<PathBuf>) -> Vec<Photo> {
    match dir {
        Some(d) => photos::scan(std::slice::from_ref(d)),
        None => photos::scan(&photos::default_dirs()),
    }
}

/// `list` output: one `day ⇥ size ⇥ path` line per photo, newest first.
fn list_text(list: &[Photo], now: i64) -> String {
    if list.is_empty() {
        return "No photos found.".to_string();
    }
    let mut s = format!("{} photo(s)\n", list.len());
    for p in list {
        s.push_str(&format!(
            "{}\t{}\t{}\n",
            photos::format_day(p.mtime, now),
            photos::format_bytes(p.bytes),
            p.path.display()
        ));
    }
    s
}

/// Photos arrive mtime-sorted, so equal days are contiguous — same grouping
/// rule `PhotosApp::new` uses for the grid headers.
fn day_groups(list: &[Photo], now: i64) -> Vec<(String, usize, u64)> {
    let mut out: Vec<(String, usize, u64)> = Vec::new();
    for p in list {
        let label = photos::format_day(p.mtime, now);
        match out.last_mut() {
            Some(g) if g.0 == label => {
                g.1 += 1;
                g.2 += p.bytes;
            }
            _ => out.push((label, 1, p.bytes)),
        }
    }
    out
}

fn albums_text(list: &[Photo], now: i64) -> String {
    let groups = day_groups(list, now);
    if groups.is_empty() {
        return "No photos found.".to_string();
    }
    let mut s = format!("{} day(s), {} photo(s)\n", groups.len(), list.len());
    for (label, count, bytes) in groups {
        s.push_str(&format!(
            "{}\t{} photo(s)\t{}\n",
            label,
            count,
            photos::format_bytes(bytes)
        ));
    }
    s
}

/// Everything `info` reports. `dimensions` are the true pixel size sniffed
/// from the image header (PNG IHDR / GIF header / WebP chunks / JPEG SOF);
/// when the header can't be read but the image still decodes, the decoded
/// content size is reported instead — that one is scaled to fit a
/// `VIEW_DECODE_W` viewport, so only its aspect ratio is meaningful.
#[derive(Debug)]
pub struct PhotoInfo {
    pub name: String,
    pub path: PathBuf,
    pub bytes: u64,
    pub mtime: i64,
    pub day: String,
    /// `(width, height)` in source pixels when `exact`, else decoded px.
    pub dimensions: Option<(i32, i32)>,
    /// `true` when `dimensions` came from the file header.
    pub exact: bool,
    /// Whether the resvg decode pipeline can rasterize the file.
    pub decodable: bool,
}

fn be16(b: &[u8], i: usize) -> u32 {
    u16::from_be_bytes([b[i], b[i + 1]]) as u32
}

fn le16(b: &[u8], i: usize) -> u32 {
    u16::from_le_bytes([b[i], b[i + 1]]) as u32
}

fn le24(b: &[u8], i: usize) -> u32 {
    u32::from(b[i]) | (u32::from(b[i + 1]) << 8) | (u32::from(b[i + 2]) << 16)
}

fn le32(b: &[u8], i: usize) -> u32 {
    u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]])
}

fn be32(b: &[u8], i: usize) -> u32 {
    u32::from_be_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]])
}

fn png_size(b: &[u8]) -> Option<(u32, u32)> {
    const SIG: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    if b.len() < 24 || b[..8] != SIG || &b[12..16] != b"IHDR" {
        return None;
    }
    let (w, h) = (be32(b, 16), be32(b, 20));
    (w > 0 && h > 0).then_some((w, h))
}

fn gif_size(b: &[u8]) -> Option<(u32, u32)> {
    if b.len() < 10 || (&b[..6] != b"GIF87a" && &b[..6] != b"GIF89a") {
        return None;
    }
    let (w, h) = (le16(b, 6), le16(b, 8));
    (w > 0 && h > 0).then_some((w, h))
}

fn webp_size(b: &[u8]) -> Option<(u32, u32)> {
    if b.len() < 30 || &b[..4] != b"RIFF" || &b[8..12] != b"WEBP" {
        return None;
    }
    match &b[12..16] {
        // VP8X: 24-bit LE canvas size minus one, at bytes 24 and 27.
        b"VP8X" => Some((le24(b, 24) + 1, le24(b, 27) + 1)),
        // VP8L: signature 0x2F then 14-bit LE width/height minus one.
        b"VP8L" => {
            if b[20] != 0x2F {
                return None;
            }
            let bits = le32(b, 21);
            Some(((bits & 0x3FFF) + 1, ((bits >> 14) & 0x3FFF) + 1))
        }
        // VP8 (lossy): frame start code 9D 01 2A, then 14-bit dims.
        b"VP8 " => {
            if b[23..26] != [0x9D, 0x01, 0x2A] {
                return None;
            }
            let (w, h) = (le16(b, 26) & 0x3FFF, le16(b, 28) & 0x3FFF);
            (w > 0 && h > 0).then_some((w, h))
        }
        _ => None,
    }
}

fn jpeg_size(b: &[u8]) -> Option<(u32, u32)> {
    if b.len() < 4 || b[0] != 0xFF || b[1] != 0xD8 {
        return None;
    }
    let mut i = 2;
    // Walk marker segments until a Start-Of-Frame (SOF0..SOF15, excluding
    // DHT/DAC/JPG/RST which share the C4/C8/CC range).
    while i + 9 <= b.len() {
        if b[i] != 0xFF {
            i += 1;
            continue;
        }
        let m = b[i + 1];
        if m == 0x00 {
            i += 2;
            continue;
        }
        // Standalone markers carry no length field.
        if m == 0x01 || (0xD0..=0xD9).contains(&m) {
            if m == 0xD9 {
                break; // EOI
            }
            i += 2;
            continue;
        }
        let len = be16(b, i + 2) as usize;
        if len < 2 {
            return None;
        }
        if (0xC0..=0xCF).contains(&m) && m != 0xC4 && m != 0xC8 && m != 0xCC {
            // len(2) precision(1) height(2) width(2) after the marker.
            let (h, w) = (be16(b, i + 5), be16(b, i + 7));
            return (w > 0 && h > 0).then_some((w, h));
        }
        i += 2 + len;
    }
    None
}

/// Pixel dimensions sniffed from the file header, no decode needed.
/// Covers every format `photos::IMAGE_EXTS` accepts.
#[must_use]
pub fn sniff_size(bytes: &[u8]) -> Option<(u32, u32)> {
    png_size(bytes)
        .or_else(|| gif_size(bytes))
        .or_else(|| webp_size(bytes))
        .or_else(|| jpeg_size(bytes))
}

/// Header-sniffed dimensions for an image file on disk.
pub fn image_dimensions(path: &Path) -> Option<(i32, i32)> {
    let bytes = fs::read(path).ok()?;
    sniff_size(&bytes).map(|(w, h)| (w as i32, h as i32))
}

pub fn inspect(path: &Path) -> Result<PhotoInfo, String> {
    let abs = resolve(path)?;
    let md = fs::metadata(&abs).map_err(|e| format!("{}: {e}", abs.display()))?;
    if !md.is_file() {
        return Err(format!("{}: not a file", abs.display()));
    }
    let name = abs
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("photo")
        .to_string();
    let mtime = md
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    // Header sniff first (exact pixels); decode only proves rasterizability
    // and is the fallback source of dimensions.
    let sniffed = image_dimensions(&abs);
    let decoded = decode::rasterize_content(&abs, VIEW_DECODE_W)
        .ok()
        .map(|px| (px.size.w, px.size.h));
    Ok(PhotoInfo {
        name,
        path: abs,
        bytes: md.len(),
        mtime,
        day: photos::format_day(mtime, now_secs()),
        dimensions: sniffed.or(decoded),
        exact: sniffed.is_some(),
        decodable: decoded.is_some(),
    })
}

fn info_text(path: &Path) -> Result<String, String> {
    let info = inspect(path)?;
    let mut s = format!("{}\n", info.path.display());
    s.push_str(&format!(
        "  size:     {} ({} bytes)\n",
        photos::format_bytes(info.bytes),
        info.bytes
    ));
    s.push_str(&format!(
        "  modified: {} (mtime {})\n",
        info.day, info.mtime
    ));
    match (info.dimensions, info.exact, info.decodable) {
        (Some((w, h)), true, _) => s.push_str(&format!("  image:    {w}x{h} px\n")),
        (Some((w, h)), false, true) => s.push_str(&format!(
            "  image:    {w}x{h} px decoded (long side ≤ {VIEW_DECODE_W} px; header unreadable)\n"
        )),
        (None, _, true) => s.push_str("  image:    decodable, dimensions unknown\n"),
        _ => s.push_str("  image:    not decodable\n"),
    }
    Ok(s)
}

fn open_text(path: &Path) -> Result<String, String> {
    let abs = resolve(path)?;
    let md = fs::metadata(&abs).map_err(|e| format!("{}: {e}", abs.display()))?;
    if !md.is_file() {
        return Err(format!("{}: not a file", abs.display()));
    }
    // Headless has no viewer; the resolved path is the contract (callers can
    // hand it to the GUI or another viewer).
    Ok(format!("{}\n", abs.display()))
}

fn execute(cmd: &Command) -> Result<String, String> {
    let now = now_secs();
    match cmd {
        Command::List { dir } => Ok(list_text(&scan_opt(dir), now)),
        Command::Albums { dir } => Ok(albums_text(&scan_opt(dir), now)),
        Command::Info { path } => info_text(path),
        Command::Open { path } => open_text(path),
        Command::Help => Ok(HELP.to_string()),
    }
}

/// Run `--cli` args; returns the process exit code.
pub fn run_cli(args: &[String]) -> i32 {
    match parse(args).and_then(|c| execute(&c)) {
        Ok(text) => {
            print!("{text}");
            if !text.ends_with('\n') {
                println!();
            }
            0
        }
        Err(e) => {
            eprintln!("error: {e}");
            2
        }
    }
}

/// `rabbiz-photos <file-or-dir>`: a directory lists itself, a file prints
/// its resolved path (same contract as `open`). Returns the exit code.
pub fn run_path_arg(path: &Path) -> i32 {
    let abs = match resolve(path) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: {e}");
            return 2;
        }
    };
    match fs::metadata(&abs) {
        Ok(md) if md.is_dir() => {
            let list = photos::scan(std::slice::from_ref(&abs));
            print!("{}", list_text(&list, now_secs()));
            0
        }
        Ok(_) => match open_text(&abs) {
            Ok(text) => {
                print!("{text}");
                0
            }
            Err(e) => {
                eprintln!("error: {e}");
                2
            }
        },
        Err(e) => {
            eprintln!("error: {}: {e}", abs.display());
            2
        }
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

fn scan_for_mcp(args: &Value) -> Vec<Photo> {
    match arg_dir(args) {
        Some(d) => photos::scan(std::slice::from_ref(&d)),
        None => photos::scan(&photos::default_dirs()),
    }
}

fn photo_json(p: &Photo, now: i64) -> Value {
    json!({
        "name": p.name,
        "path": p.path,
        "day": photos::format_day(p.mtime, now),
        "mtime": p.mtime,
        "bytes": p.bytes,
    })
}

fn mcp_list(args: &Value) -> Result<String, String> {
    let now = now_secs();
    let list: Vec<Value> = scan_for_mcp(args)
        .iter()
        .map(|p| photo_json(p, now))
        .collect();
    Ok(json!(list).to_string())
}

fn mcp_albums(args: &Value) -> Result<String, String> {
    let now = now_secs();
    let groups: Vec<Value> = day_groups(&scan_for_mcp(args), now)
        .into_iter()
        .map(|(day, count, bytes)| json!({ "day": day, "count": count, "bytes": bytes }))
        .collect();
    Ok(json!(groups).to_string())
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
        "day": info.day,
        "width": info.dimensions.map(|d| d.0),
        "height": info.dimensions.map(|d| d.1),
        "dimensions_exact": info.exact,
        "decodable": info.decodable,
    })
    .to_string())
}

/// MCP tool table served by `--mcp`.
pub static TOOLS: &[Tool] = &[
    Tool {
        name: "photos_list",
        description: "List photos (jpg/png/webp/gif) from ~/Pictures and \
                      ~/Downloads, or from 'dir' when given. Newest first; \
                      returns JSON array of {name,path,day,mtime,bytes}.",
        input_schema: r#"{"type":"object","properties":{"dir":{"type":"string","description":"Directory to scan instead of the defaults (non-recursive)"}}}"#,
        handler: mcp_list,
    },
    Tool {
        name: "photos_albums",
        description: "Photos grouped by day (same grouping as the app grid \
                      headers): JSON array of {day,count,bytes}, newest day first.",
        input_schema: r#"{"type":"object","properties":{"dir":{"type":"string","description":"Directory to scan instead of the defaults (non-recursive)"}}}"#,
        handler: mcp_albums,
    },
    Tool {
        name: "photo_info",
        description: "Metadata for one image file: size, mtime, day label, \
                      true pixel dimensions from the file header (falls back \
                      to the decode-scaled size) and decodability.",
        input_schema: r#"{"type":"object","properties":{"path":{"type":"string","description":"Image file path"}},"required":["path"]}"#,
        handler: mcp_info,
    },
];

/// Serve the MCP tool table on stdin/stdout; never returns.
pub fn serve_mcp() -> ! {
    serve(TOOLS, "rabbiz-photos", env!("CARGO_PKG_VERSION"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static N: AtomicUsize = AtomicUsize::new(0);

    fn tmpdir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "rabbiz-photos-cli-{tag}-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn write_png(dir: &Path, name: &str) -> PathBuf {
        // 2x3 opaque PNG through de-paint's encoder (same as decode tests).
        let mut px = de_api::Pixels::empty(de_api::Size::new(2, 3));
        for i in (0..px.data.len()).step_by(4) {
            px.data[i] = 200;
            px.data[i + 3] = 255;
        }
        let p = dir.join(name);
        fs::write(&p, de_paint::encode_png(&px).unwrap()).unwrap();
        p
    }

    #[test]
    fn parse_commands() {
        let v: Vec<String> = vec![];
        assert_eq!(parse(&v).unwrap(), Command::Help);
        let v: Vec<String> = ["list"].iter().map(|s| s.to_string()).collect();
        assert_eq!(parse(&v).unwrap(), Command::List { dir: None });
        let v: Vec<String> = ["list", "/tmp/x"].iter().map(|s| s.to_string()).collect();
        assert_eq!(
            parse(&v).unwrap(),
            Command::List {
                dir: Some(PathBuf::from("/tmp/x"))
            }
        );
        let v: Vec<String> = ["days", "/tmp/x"].iter().map(|s| s.to_string()).collect();
        assert!(matches!(parse(&v).unwrap(), Command::Albums { .. }));
        let v: Vec<String> = ["info", "a.png"].iter().map(|s| s.to_string()).collect();
        assert_eq!(
            parse(&v).unwrap(),
            Command::Info {
                path: PathBuf::from("a.png")
            }
        );
        assert!(parse(&["info".to_string()]).is_err());
        assert!(parse(&["bogus".to_string()]).is_err());
        assert!(parse(&["list".to_string(), "--bogus".to_string()]).is_err());
    }

    #[test]
    fn list_text_on_tempdir() {
        let dir = tmpdir("list");
        write_png(&dir, "a.png");
        write_png(&dir, "b.jpg"); // ext only matters to scan
        fs::write(dir.join("skip.txt"), b"x").unwrap();
        let list = photos::scan(std::slice::from_ref(&dir));
        assert_eq!(list.len(), 2);
        let text = list_text(&list, now_secs());
        assert!(text.contains("2 photo(s)"));
        assert!(text.contains("a.png") && text.contains("b.jpg"));
        assert!(!text.contains("skip.txt"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn albums_group_by_day() {
        let dir = tmpdir("albums");
        let now = SystemTime::now();
        for name in ["today1.png", "today2.png", "old.png"] {
            let p = write_png(&dir, name);
            let t = if name == "old.png" {
                now - std::time::Duration::from_secs(5 * 86_400)
            } else {
                now
            };
            fs::File::options()
                .write(true)
                .open(&p)
                .unwrap()
                .set_modified(t)
                .unwrap();
        }
        let list = photos::scan(std::slice::from_ref(&dir));
        let groups = day_groups(&list, now_secs());
        assert_eq!(groups.len(), 2, "{groups:?}");
        assert_eq!(groups.iter().map(|g| g.1).sum::<usize>(), 3);
        let text = albums_text(&list, now_secs());
        assert!(text.contains("2 day(s), 3 photo(s)"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn info_decodes_png_and_rejects_missing() {
        let dir = tmpdir("info");
        let p = write_png(&dir, "pic.png");
        let info = inspect(&p).unwrap();
        assert_eq!(info.dimensions, Some((2, 3)));
        assert!(info.exact);
        assert!(info.decodable);
        assert_eq!(info.name, "pic.png");
        assert!(info.bytes > 0);

        let txt = dir.join("note.txt");
        fs::write(&txt, b"hello").unwrap();
        let info = inspect(&txt).unwrap();
        assert_eq!(info.dimensions, None);
        assert!(!info.decodable);

        assert!(inspect(&dir.join("nope.png")).is_err());
        assert!(inspect(&dir).is_err()); // directory, not a file
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn open_resolves_and_lists_dir() {
        let dir = tmpdir("open");
        let p = write_png(&dir, "x.png");
        let out = open_text(&p).unwrap();
        assert!(out.contains("x.png"));
        assert!(Path::new(out.trim()).is_absolute());
        assert!(open_text(&dir).is_err()); // dir is not a file

        assert_eq!(run_path_arg(&dir), 0);
        assert_eq!(run_path_arg(&p), 0);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn mcp_handlers_return_json() {
        let dir = tmpdir("mcp");
        write_png(&dir, "m.png");
        let args = json!({ "dir": dir.to_string_lossy() });
        let list: Value = serde_json::from_str(&mcp_list(&args).unwrap()).unwrap();
        assert_eq!(list.as_array().unwrap().len(), 1);
        assert_eq!(list[0]["name"], "m.png");

        let albums: Value = serde_json::from_str(&mcp_albums(&args).unwrap()).unwrap();
        assert_eq!(albums.as_array().unwrap()[0]["count"], 1);

        let p = dir.join("m.png");
        let info: Value =
            serde_json::from_str(&mcp_info(&json!({ "path": p.to_string_lossy() })).unwrap())
                .unwrap();
        assert_eq!(info["width"], 2);
        assert_eq!(info["height"], 3);
        assert_eq!(info["dimensions_exact"], true);
        assert_eq!(info["decodable"], true);
        assert!(mcp_info(&json!({})).is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn sniff_size_headers() {
        // PNG 2x3 produced by the encoder.
        let mut px = de_api::Pixels::empty(de_api::Size::new(2, 3));
        for i in (0..px.data.len()).step_by(4) {
            px.data[i + 3] = 255;
        }
        let png = de_paint::encode_png(&px).unwrap();
        assert_eq!(sniff_size(&png), Some((2, 3)));

        // GIF89a 320x200 header.
        let mut gif = b"GIF89a".to_vec();
        gif.extend_from_slice(&320u16.to_le_bytes());
        gif.extend_from_slice(&200u16.to_le_bytes());
        gif.extend_from_slice(&[0; 20]);
        assert_eq!(sniff_size(&gif), Some((320, 200)));

        // JPEG: SOI + APP0 segment + SOF0 4032x3024.
        let mut jpg = vec![0xFF, 0xD8];
        jpg.extend_from_slice(&[0xFF, 0xE0, 0x00, 0x10]); // APP0 len 16
        jpg.extend_from_slice(&[0; 14]);
        jpg.extend_from_slice(&[0xFF, 0xC0, 0x00, 0x11, 0x08]); // SOF0 len 17
        jpg.extend_from_slice(&3024u16.to_be_bytes());
        jpg.extend_from_slice(&4032u16.to_be_bytes());
        assert_eq!(sniff_size(&jpg), Some((4032, 3024)));

        // WebP VP8X canvas 640x480 (stored minus one).
        let mut webp = b"RIFF\x00\x00\x00\x00WEBPVP8X".to_vec();
        webp.extend_from_slice(&[0, 0, 0, 0]); // chunk size
        webp.extend_from_slice(&[0; 4]); // flags + reserved
        webp.extend_from_slice(&639u32.to_le_bytes()[..3]);
        webp.extend_from_slice(&479u32.to_le_bytes()[..3]);
        assert_eq!(webp.len(), 30);
        assert_eq!(sniff_size(&webp), Some((640, 480)));

        assert_eq!(sniff_size(b"not an image"), None);
        assert_eq!(sniff_size(&[]), None);
    }
}
