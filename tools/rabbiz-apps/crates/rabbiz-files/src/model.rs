//! Pure filesystem model for RabbizOS Files: directory listing, sorting,
//! filtering, formatting and the file operations the UI exposes. Everything
//! here is host-testable — no Wayland, no canvas.

use std::cmp::Ordering;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Cap for recursive item counting / sizing so a huge tree can't stall the UI.
pub const WALK_CAP: usize = 20_000;

/// Broad content category driving the per-type glyph. Extension-based only —
/// no content sniffing (keeps listing cheap on eMMC).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FileKind {
    Folder,
    Image,
    Audio,
    Video,
    Archive,
    Code,
    Document,
    Generic,
}

impl FileKind {
    /// Stable sort rank for `SortKey::Type`.
    pub fn rank(self) -> u8 {
        match self {
            FileKind::Folder => 0,
            FileKind::Image => 1,
            FileKind::Video => 2,
            FileKind::Audio => 3,
            FileKind::Archive => 4,
            FileKind::Document => 5,
            FileKind::Code => 6,
            FileKind::Generic => 7,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    /// Symlink (target may be dir or file; `is_dir` follows the target).
    pub is_link: bool,
    pub size: u64,
    /// Seconds since UNIX epoch.
    pub mtime: i64,
    pub kind: FileKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortKey {
    Name,
    Date,
    Size,
    Type,
}

impl SortKey {
    pub const ALL: [SortKey; 4] = [SortKey::Name, SortKey::Date, SortKey::Size, SortKey::Type];

    pub fn label(self) -> &'static str {
        match self {
            SortKey::Name => "Name",
            SortKey::Date => "Date modified",
            SortKey::Size => "Size",
            SortKey::Type => "Type",
        }
    }
}

/// Classify by lowercase extension. Unknown/none → Generic.
#[must_use]
pub fn kind_of(name: &str, is_dir: bool) -> FileKind {
    if is_dir {
        return FileKind::Folder;
    }
    let ext = name
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "svg" | "ico" | "heic" | "avif"
        | "tif" | "tiff" | "raw" | "cr2" | "nef" => FileKind::Image,
        "mp3" | "ogg" | "oga" | "wav" | "flac" | "m4a" | "aac" | "opus" | "wma" | "aiff"
        | "mid" | "midi" => FileKind::Audio,
        "mp4" | "mkv" | "avi" | "mov" | "webm" | "3gp" | "m4v" | "ts" | "mts" | "wmv" | "flv"
        | "mpg" | "mpeg" | "vob" => FileKind::Video,
        "zip" | "tar" | "gz" | "tgz" | "bz2" | "tbz2" | "xz" | "txz" | "7z" | "rar" | "deb"
        | "rpm" | "apk" | "cab" | "lz4" | "zst" => FileKind::Archive,
        "txt" | "md" | "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "csv" | "odt"
        | "ods" | "odp" | "rtf" | "epub" | "mobi" => FileKind::Document,
        "rs" | "go" | "js" | "jsx" | "tsx" | "py" | "c" | "h" | "cpp" | "hpp" | "java" | "kt"
        | "kts" | "swift" | "json" | "toml" | "yaml" | "yml" | "xml" | "html" | "htm" | "css"
        | "scss" | "sh" | "bash" | "zsh" | "sql" | "lua" | "rb" | "php" | "pl" | "r" | "scala"
        | "clj" | "ex" | "exs" | "erl" | "hs" | "ml" | "vim" | "diff" | "patch" | "cmake"
        | "mk" | "gradle" | "proto" => FileKind::Code,
        _ => FileKind::Generic,
    }
}

/// Read `dir`, returning visible+hidden entries sorted dirs-first by `key`.
/// Hidden (`.`-prefixed) entries are included; the UI filters them out unless
/// `show_hidden`.
pub fn load_dir(dir: &Path) -> io::Result<Vec<Entry>> {
    let mut out = Vec::new();
    for rd in fs::read_dir(dir)? {
        let rd = rd?;
        let name = rd.file_name().to_string_lossy().into_owned();
        // Skip . and .. (read_dir never yields them, but be explicit).
        if name == "." || name == ".." {
            continue;
        }
        let ft = match rd.file_type() {
            Ok(t) => t,
            Err(_) => continue,
        };
        let is_link = ft.is_symlink();
        // Follow links for type/size; fall back to symlink_metadata.
        let meta = fs::metadata(rd.path()).or_else(|_| rd.metadata());
        let Ok(meta) = meta else { continue };
        let is_dir = meta.is_dir();
        out.push(Entry {
            kind: kind_of(&name, is_dir),
            name,
            path: rd.path(),
            is_dir,
            is_link,
            size: if is_dir { 0 } else { meta.len() },
            mtime: meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0),
        });
    }
    Ok(out)
}

/// Case-insensitive name compare with a lowercase tiebreak so ordering is
/// deterministic across filesystems.
fn cmp_name(a: &str, b: &str) -> Ordering {
    a.to_lowercase()
        .cmp(&b.to_lowercase())
        .then_with(|| a.cmp(b))
}

fn cmp_entry(a: &Entry, b: &Entry, key: SortKey) -> Ordering {
    // Folders always float above files (Files-by-Google / Dolphin default).
    match a.is_dir.cmp(&b.is_dir).reverse() {
        Ordering::Equal => {}
        o => return o,
    }
    match key {
        SortKey::Name => cmp_name(&a.name, &b.name),
        // Newest first for date/size; alphabetical for name/type.
        SortKey::Date => b
            .mtime
            .cmp(&a.mtime)
            .then_with(|| cmp_name(&a.name, &b.name)),
        SortKey::Size => b.size.cmp(&a.size).then_with(|| cmp_name(&a.name, &b.name)),
        SortKey::Type => a
            .kind
            .rank()
            .cmp(&b.kind.rank())
            .then_with(|| cmp_name(&a.name, &b.name)),
    }
}

/// Sort `entries` in place per `key`.
pub fn sort_entries(entries: &mut [Entry], key: SortKey) {
    entries.sort_by(|a, b| cmp_entry(a, b, key));
}

/// Filename substring filter (case-insensitive). Empty needle matches all.
#[must_use]
pub fn matches(entry: &Entry, needle: &str) -> bool {
    needle.is_empty() || entry.name.to_lowercase().contains(needle)
}

/// Entries to display: hidden files dropped unless `show_hidden`, then filtered
/// by `needle`, sorted by `key`.
#[must_use]
pub fn visible_entries(
    entries: &[Entry],
    key: SortKey,
    show_hidden: bool,
    needle: &str,
) -> Vec<Entry> {
    let mut v: Vec<Entry> = entries
        .iter()
        .filter(|e| show_hidden || !e.name.starts_with('.'))
        .filter(|e| matches(e, needle))
        .cloned()
        .collect();
    sort_entries(&mut v, key);
    v
}

/// `"12 B"`, `"1.4 KB"`, `"230 MB"`, `"1.2 GB"` — 1024-based, one decimal max.
#[must_use]
pub fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut v = bytes as f64;
    let mut u = 0;
    while v >= 1024.0 && u < UNITS.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    if v >= 100.0 {
        format!("{v:.0} {}", UNITS[u])
    } else {
        format!("{v:.1} {}", UNITS[u])
    }
}

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// `"18 Nov 2026, 14:32"` local time. Falls back to the raw epoch on failure.
/// Uses `libc::localtime_r` — no chrono dependency allowed in this workspace.
#[must_use]
pub fn format_time(secs: i64) -> String {
    #[cfg(unix)]
    #[allow(unsafe_code)]
    {
        let t = secs as libc::time_t;
        // SAFETY: `tm` is a zeroed, owned `libc::tm`; `localtime_r` is reentrant
        // and only reads `&t` and writes into `tm`. Null return is checked below.
        let mut tm: libc::tm = unsafe { std::mem::zeroed() };
        if unsafe { libc::localtime_r(&t, &mut tm) }.is_null() {
            return secs.to_string();
        }
        let mon = MONTHS
            .get(tm.tm_mon.clamp(0, 11) as usize)
            .copied()
            .unwrap_or("?");
        format!(
            "{} {} {}, {:02}:{:02}",
            tm.tm_mday,
            mon,
            tm.tm_year + 1900,
            tm.tm_hour,
            tm.tm_min
        )
    }
    #[cfg(not(unix))]
    {
        secs.to_string()
    }
}

/// POSIX permission string `"rwxr-xr-x"` (plus `l` prefix handled by caller
/// via `is_link` if wanted — we keep it plain mode bits only).
#[must_use]
pub fn mode_string(mode: u32) -> String {
    let mut s = String::with_capacity(9);
    for shift in [6, 3, 0] {
        let bits = (mode >> shift) & 0o7;
        s.push(if bits & 0o4 != 0 { 'r' } else { '-' });
        s.push(if bits & 0o2 != 0 { 'w' } else { '-' });
        s.push(if bits & 0o1 != 0 { 'x' } else { '-' });
    }
    s
}

/// Free/total bytes of the filesystem containing `path` via `statvfs`.
/// Returns `None` on error (caller shows "—").
#[must_use]
#[allow(unsafe_code)]
pub fn free_space(path: &Path) -> Option<(u64, u64)> {
    use std::os::unix::ffi::OsStrExt;
    let c = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
    // SAFETY: `st` is a zeroed, owned `libc::statvfs` C struct; `c.as_ptr()` is
    // a valid NUL-terminated path and `&mut st` is valid for the call duration.
    let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(c.as_ptr(), &mut st) } != 0 {
        return None;
    }
    let total = st.f_blocks as u64 * st.f_frsize as u64;
    let free = st.f_bavail as u64 * st.f_frsize as u64;
    Some((free, total))
}

/// Recursive stats for a directory: `(entries_seen, bytes)` capped at
/// `WALK_CAP` entries so a giant tree can't hang the UI. Symlinks are not
/// followed (counted as 0 bytes, 1 entry).
pub fn dir_stats(path: &Path) -> io::Result<(usize, u64)> {
    let mut count = 0usize;
    let mut bytes = 0u64;
    let mut stack = vec![path.to_path_buf()];
    while let Some(dir) = stack.pop() {
        if count >= WALK_CAP {
            break;
        }
        let rd = match fs::read_dir(&dir) {
            Ok(r) => r,
            Err(_) => continue,
        };
        for ent in rd.flatten() {
            if count >= WALK_CAP {
                break;
            }
            count += 1;
            let ft = match ent.file_type() {
                Ok(t) => t,
                Err(_) => continue,
            };
            if ft.is_dir() {
                stack.push(ent.path());
            } else if ft.is_file() {
                bytes += ent.metadata().map(|m| m.len()).unwrap_or(0);
            }
        }
    }
    Ok((count, bytes))
}

/// Count entries that a recursive delete would remove (for the confirm
/// dialog), capped at `WALK_CAP`.
#[must_use]
pub fn count_tree(path: &Path) -> usize {
    if path.is_file() {
        return 1;
    }
    dir_stats(path).map(|(n, _)| n).unwrap_or(1)
}

/// Create a directory. Errors surface to the UI as a status message.
pub fn make_dir(path: &Path) -> io::Result<()> {
    fs::create_dir(path)
}

/// Rename/move within the filesystem. Cross-device moves fall back to
/// copy+delete so "Move" works between e.g. /data and /sdcard.
pub fn move_entry(from: &Path, to: &Path) -> io::Result<()> {
    match fs::rename(from, to) {
        Ok(()) => Ok(()),
        Err(e) if e.raw_os_error() == Some(libc::EXDEV) => {
            // Cross-device: copy the tree, then delete the source.
            copy_entry(from, to)?;
            remove_entry(from)
        }
        Err(e) => Err(e),
    }
}

/// Remove a file or a whole directory tree. Uses `remove_dir_all` — this is
/// a PERMANENT delete (no freedesktop trash implementation; documented).
pub fn remove_entry(path: &Path) -> io::Result<()> {
    let meta = fs::symlink_metadata(path)?;
    if meta.is_dir() && !meta.file_type().is_symlink() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
}

/// Recursive copy preserving the entry name under `dest_dir`.
/// Only exercised by the unit tests — the UI copy op picks `dst` itself.
#[cfg(test)]
pub fn copy_into(src: &Path, dest_dir: &Path) -> io::Result<PathBuf> {
    let name = src
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "bad source name"))?;
    let dst = dest_dir.join(name);
    copy_entry(src, &dst)?;
    Ok(dst)
}

/// Copy `src` to exactly `dst` (file or directory tree).
pub fn copy_entry(src: &Path, dst: &Path) -> io::Result<()> {
    let meta = fs::symlink_metadata(src)?;
    if meta.file_type().is_symlink() {
        // Copy the link itself, not the target.
        let target = fs::read_link(src)?;
        #[cfg(unix)]
        std::os::unix::fs::symlink(&target, dst)?;
        return Ok(());
    }
    if meta.is_dir() {
        fs::create_dir_all(dst)?;
        for rd in fs::read_dir(src)? {
            let rd = rd?;
            copy_entry(&rd.path(), &dst.join(rd.file_name()))?;
        }
        Ok(())
    } else {
        fs::copy(src, dst).map(|_| ())
    }
}

/// Free name in `dir` for `name`: "name", "name (1)", "name (2)", …
#[must_use]
pub fn unique_name(dir: &Path, name: &str) -> String {
    if !dir.join(name).exists() {
        return name.to_string();
    }
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() => (s.to_string(), format!(".{e}")),
        _ => (name.to_string(), String::new()),
    };
    for i in 1..1000 {
        let cand = format!("{stem} ({i}){ext}");
        if !dir.join(&cand).exists() {
            return cand;
        }
    }
    format!("{stem} (copy){ext}")
}

/// Sanitize a user-typed name: strip `/` and NUL, trim, reject empty/dot names.
#[must_use]
pub fn clean_name(raw: &str) -> Option<String> {
    let s = raw.trim().to_string();
    if s.is_empty()
        || s == "."
        || s == ".."
        || s.contains('\0')
        || s.contains('/')
        || s.contains('\\')
    {
        return None;
    }
    Some(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tmpdir(tag: &str) -> PathBuf {
        let d =
            std::env::temp_dir().join(format!("rabbiz-files-test-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn kind_by_extension() {
        assert_eq!(kind_of("pic.PNG", false), FileKind::Image);
        assert_eq!(kind_of("song.flac", false), FileKind::Audio);
        assert_eq!(kind_of("clip.mp4", false), FileKind::Video);
        assert_eq!(kind_of("a.zip", false), FileKind::Archive);
        assert_eq!(kind_of("main.rs", false), FileKind::Code);
        assert_eq!(kind_of("book.pdf", false), FileKind::Document);
        assert_eq!(kind_of("README", false), FileKind::Generic);
        assert_eq!(kind_of("x", true), FileKind::Folder);
    }

    #[test]
    fn sort_dirs_first_then_name() {
        let mut v = vec![
            Entry {
                name: "b.txt".into(),
                path: "b.txt".into(),
                is_dir: false,
                is_link: false,
                size: 0,
                mtime: 0,
                kind: FileKind::Generic,
            },
            Entry {
                name: "Zdir".into(),
                path: "Zdir".into(),
                is_dir: true,
                is_link: false,
                size: 0,
                mtime: 0,
                kind: FileKind::Folder,
            },
            Entry {
                name: "a.txt".into(),
                path: "a.txt".into(),
                is_dir: false,
                is_link: false,
                size: 0,
                mtime: 0,
                kind: FileKind::Generic,
            },
        ];
        sort_entries(&mut v, SortKey::Name);
        assert_eq!(v[0].name, "Zdir");
        assert_eq!(v[1].name, "a.txt");
        assert_eq!(v[2].name, "b.txt");
    }

    #[test]
    fn date_and_size_desc() {
        let e = |name: &str, size: u64, mtime: i64| Entry {
            name: name.into(),
            path: name.into(),
            is_dir: false,
            is_link: false,
            size,
            mtime,
            kind: FileKind::Generic,
        };
        let mut v = vec![e("old", 10, 1), e("new", 5, 9), e("big", 99, 5)];
        sort_entries(&mut v, SortKey::Date);
        assert_eq!(v[0].name, "new");
        sort_entries(&mut v, SortKey::Size);
        assert_eq!(v[0].name, "big");
    }

    #[test]
    fn format_size_units() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(2048), "2.0 KB");
        assert_eq!(format_size(5 * 1024 * 1024), "5.0 MB");
        assert_eq!(format_size(3 * 1024 * 1024 * 1024), "3.0 GB");
    }

    #[test]
    fn format_time_nonempty() {
        let s = format_time(1_700_000_000);
        assert!(s.contains("2023"), "{s}");
    }

    #[test]
    fn mode_string_bits() {
        assert_eq!(mode_string(0o644), "rw-r--r--");
        assert_eq!(mode_string(0o755), "rwxr-xr-x");
        assert_eq!(mode_string(0o600), "rw-------");
    }

    #[test]
    fn hidden_and_filter() {
        let d = tmpdir("filter");
        fs::write(d.join("keep.txt"), b"x").unwrap();
        fs::write(d.join(".hidden"), b"x").unwrap();
        let entries = load_dir(&d).unwrap();
        let vis = visible_entries(&entries, SortKey::Name, false, "");
        assert_eq!(vis.len(), 1);
        let vis = visible_entries(&entries, SortKey::Name, true, "hid");
        assert_eq!(vis.len(), 1);
        assert_eq!(vis[0].name, ".hidden");
        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn ops_roundtrip() {
        let d = tmpdir("ops");
        let sub = d.join("sub");
        make_dir(&sub).unwrap();
        fs::write(sub.join("f.txt"), b"hello").unwrap();

        // copy_into preserves name
        let dst = d.join("copy");
        make_dir(&dst).unwrap();
        copy_into(&sub, &dst).unwrap();
        assert!(dst.join("sub/f.txt").exists());

        // unique_name
        assert_eq!(unique_name(&d, "new"), "new");
        assert_eq!(unique_name(&d, "sub"), "sub (1)");

        // move_entry
        let moved = d.join("moved");
        move_entry(&sub, &moved).unwrap();
        assert!(moved.join("f.txt").exists());
        assert!(!sub.exists());

        // remove_entry recursive
        remove_entry(&dst).unwrap();
        assert!(!dst.exists());
        assert_eq!(count_tree(&moved), 1);
        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn clean_name_rules() {
        assert_eq!(clean_name("  foo.txt "), Some("foo.txt".into()));
        assert_eq!(clean_name("a/b"), None);
        assert_eq!(clean_name(".."), None);
        assert_eq!(clean_name("   "), None);
        assert_eq!(clean_name("."), None);
    }

    #[test]
    fn free_space_reports() {
        let (free, total) = free_space(Path::new("/")).expect("statvfs /");
        assert!(total > 0);
        assert!(free <= total);
    }

    #[test]
    fn dir_stats_counts() {
        let d = tmpdir("stats");
        fs::create_dir_all(d.join("a/b")).unwrap();
        fs::write(d.join("a/b/c.txt"), b"12345").unwrap();
        let (n, bytes) = dir_stats(&d).unwrap();
        assert!(n >= 3);
        assert!(bytes >= 5);
        fs::remove_dir_all(&d).ok();
    }
}
