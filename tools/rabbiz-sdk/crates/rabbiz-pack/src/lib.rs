//! `rabbiz.toml` manifest schema, Freedesktop `.desktop` generator, and
//! deterministic `.rbz` application bundle packer, SHA-256 verifier, unpacker,
//! and local XDG installer.

#![forbid(unsafe_code)]

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub mod sha256;
pub use sha256::{sha256_bytes, sha256_hex};

/// Magic header for `.rbz` v1 application bundles (`"RBZ1\0"`).
pub const RBZ_MAGIC: [u8; 5] = *b"RBZ1\0";

/// Application runtime kind declared in `rabbiz.toml`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AppKind {
    #[default]
    Rust,
    C,
    Wasm,
    Flutter,
    Wayland,
}

impl AppKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::C => "c",
            Self::Wasm => "wasm",
            Self::Flutter => "flutter",
            Self::Wayland => "wayland",
        }
    }
}

/// `[app]` table in `rabbiz.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppSection {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub kind: AppKind,
    pub exec: String,
    #[serde(default = "default_icon")]
    pub icon: String,
    #[serde(default = "default_categories")]
    pub categories: Vec<String>,
}

fn default_icon() -> String {
    "icon.svg".to_string()
}

fn default_categories() -> Vec<String> {
    vec!["Utility".to_string()]
}

/// `[runtime]` table in `rabbiz.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeSection {
    #[serde(default = "default_sdk_version")]
    pub sdk_version: String,
    #[serde(default = "default_arch")]
    pub arch: String,
    #[serde(default = "default_form_factors")]
    pub form_factors: Vec<String>,
}

fn default_sdk_version() -> String {
    "0.1".to_string()
}

fn default_arch() -> String {
    "aarch64".to_string()
}

fn default_form_factors() -> Vec<String> {
    vec!["handset".to_string(), "tablet".to_string()]
}

impl Default for RuntimeSection {
    fn default() -> Self {
        Self {
            sdk_version: default_sdk_version(),
            arch: default_arch(),
            form_factors: default_form_factors(),
        }
    }
}

/// `[permissions]` table in `rabbiz.toml`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionsSection {
    #[serde(default)]
    pub network: bool,
    #[serde(default)]
    pub notifications: bool,
    #[serde(default)]
    pub intents: bool,
    #[serde(default = "default_true")]
    pub storage: bool,
}

const fn default_true() -> bool {
    true
}

/// `[mcp]` table in `rabbiz.toml`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpSection {
    #[serde(default)]
    pub enabled: bool,
}

/// Top-level `rabbiz.toml` manifest structure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppManifest {
    pub app: AppSection,
    #[serde(default)]
    pub runtime: RuntimeSection,
    #[serde(default)]
    pub permissions: PermissionsSection,
    #[serde(default)]
    pub mcp: McpSection,
}

impl AppManifest {
    /// Parse and validate an [`AppManifest`] from a TOML string.
    pub fn from_toml_str(s: &str) -> Result<Self, String> {
        let manifest: Self =
            toml::from_str(s).map_err(|e| format!("parse rabbiz.toml: {e}"))?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// Load an [`AppManifest`] from a file path.
    pub fn from_path(path: &Path) -> Result<Self, String> {
        let raw = std::fs::read_to_string(path)
            .map_err(|e| format!("read {}: {e}", path.display()))?;
        Self::from_toml_str(&raw)
    }

    /// Serialize this manifest to a formatted TOML string.
    #[must_use]
    pub fn to_toml_string(&self) -> String {
        toml::to_string_pretty(self).unwrap_or_default()
    }

    /// Validate required fields (`app.id`, `app.name`, `app.version`, `app.exec`).
    pub fn validate(&self) -> Result<(), String> {
        let id = self.app.id.trim();
        if id.is_empty() {
            return Err("app.id must not be empty".to_string());
        }
        if id.contains('/') || id.contains('\\') || id.chars().any(char::is_whitespace) {
            return Err(format!("invalid app.id {id:?}: must not contain slashes or whitespace"));
        }
        if self.app.name.trim().is_empty() {
            return Err("app.name must not be empty".to_string());
        }
        if self.app.version.trim().is_empty() {
            return Err("app.version must not be empty".to_string());
        }
        if self.app.exec.trim().is_empty() {
            return Err("app.exec must not be empty".to_string());
        }
        Ok(())
    }

    /// Generate a valid Freedesktop `.desktop` file compatible with `de-apps/src/catalog.rs`.
    #[must_use]
    pub fn generate_desktop_entry(&self, install_bin_path: &Path) -> String {
        let mut cats = self.app.categories.join(";");
        if !cats.is_empty() && !cats.ends_with(';') {
            cats.push(';');
        }
        if cats.is_empty() {
            cats = "Utility;".to_string();
        }
        let icon_val = if let Some(parent) = install_bin_path.parent() {
            let candidate = parent.join(&self.app.icon);
            if candidate.exists() {
                candidate.display().to_string()
            } else {
                self.app.icon.clone()
            }
        } else {
            self.app.icon.clone()
        };
        format!(
            "[Desktop Entry]\nType=Application\nName={}\nExec={}\nIcon={}\nCategories={}\nTerminal=false\nStartupWMClass={}\nX-Rabbiz-Kind={}\nX-Rabbiz-MCP={}\n",
            self.app.name,
            install_bin_path.display(),
            icon_val,
            cats,
            self.app.id,
            self.app.kind.as_str(),
            self.mcp.enabled
        )
    }
}

/// Summary of one file stored inside an `.rbz` bundle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RbzFileEntry {
    pub rel_path: String,
    pub size: u64,
    pub sha256: String,
    pub executable: bool,
}

/// Verified metadata for a packed `.rbz` archive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RbzMetadata {
    pub manifest: AppManifest,
    pub files: Vec<RbzFileEntry>,
    pub bundle_sha256: String,
}

fn collect_relative_files(root: &Path, current: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = std::fs::read_dir(current)
        .map_err(|e| format!("read_dir {}: {e}", current.display()))?;
    let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    paths.sort();

    for path in paths {
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if name == ".git" || name == "target" || name.ends_with(".rbz") {
            continue;
        }
        if path.is_dir() {
            collect_relative_files(root, &path, out)?;
        } else if path.is_file() {
            let rel = path
                .strip_prefix(root)
                .map_err(|e| e.to_string())?
                .to_path_buf();
            out.push(rel);
        }
    }
    Ok(())
}

/// Pack `source_dir` (which must contain `rabbiz.toml`) into a deterministic `.rbz` bundle at `out_file`.
pub fn pack_rbz(source_dir: &Path, out_file: &Path) -> Result<RbzMetadata, String> {
    let manifest_path = source_dir.join("rabbiz.toml");
    let manifest = AppManifest::from_path(&manifest_path)?;

    let mut rel_paths = Vec::new();
    collect_relative_files(source_dir, source_dir, &mut rel_paths)?;
    rel_paths.sort();

    let mut file_records: Vec<(RbzFileEntry, Vec<u8>)> = Vec::with_capacity(rel_paths.len() + 1);
    for rel in rel_paths {
        let full = source_dir.join(&rel);
        let bytes = std::fs::read(&full).map_err(|e| format!("read {}: {e}", full.display()))?;
        let rel_str = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/");
        let digest = sha256_hex(&bytes);
        let is_exec = rel_str == manifest.app.exec || is_executable_file(&full);
        file_records.push((
            RbzFileEntry {
                rel_path: rel_str,
                size: bytes.len() as u64,
                sha256: digest,
                executable: is_exec,
            },
            bytes,
        ));
    }

    // Auto-include `<id>.desktop` template if not already in the directory.
    let desktop_rel = format!("{}.desktop", manifest.app.id);
    if !file_records.iter().any(|(e, _)| e.rel_path == desktop_rel) {
        let placeholder_bin = PathBuf::from(&manifest.app.exec);
        let desktop_str = manifest.generate_desktop_entry(&placeholder_bin);
        let bytes = desktop_str.into_bytes();
        let digest = sha256_hex(&bytes);
        file_records.push((
            RbzFileEntry {
                rel_path: desktop_rel,
                size: bytes.len() as u64,
                sha256: digest,
                executable: false,
            },
            bytes,
        ));
    }

    // Build deterministic binary container:
    // [5 bytes: "RBZ1\0"]
    // [4 bytes BE: manifest_toml_len] [manifest_toml_bytes]
    // [4 bytes BE: file_count]
    // For each file:
    //   [2 bytes BE: path_len] [path_utf8]
    //   [1 byte: executable]
    //   [32 bytes: sha256_raw]
    //   [8 bytes BE: data_len] [data_bytes]
    let manifest_toml = manifest.to_toml_string();
    let manifest_bytes = manifest_toml.as_bytes();

    let mut payload = Vec::new();
    payload.extend_from_slice(&RBZ_MAGIC);
    payload.extend_from_slice(&(manifest_bytes.len() as u32).to_be_bytes());
    payload.extend_from_slice(manifest_bytes);
    payload.extend_from_slice(&(file_records.len() as u32).to_be_bytes());

    let mut entries_meta = Vec::with_capacity(file_records.len());
    for (entry, data) in file_records {
        let path_bytes = entry.rel_path.as_bytes();
        payload.extend_from_slice(&(path_bytes.len() as u16).to_be_bytes());
        payload.extend_from_slice(path_bytes);
        payload.push(u8::from(entry.executable));
        let raw_hash = sha256_bytes(&data);
        payload.extend_from_slice(&raw_hash);
        payload.extend_from_slice(&(data.len() as u64).to_be_bytes());
        payload.extend_from_slice(&data);
        entries_meta.push(entry);
    }

    let bundle_sha256 = sha256_hex(&payload);
    if let Some(parent) = out_file.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("mkdir {}: {e}", parent.display()))?;
        }
    }
    let mut f = std::fs::File::create(out_file)
        .map_err(|e| format!("create {}: {e}", out_file.display()))?;
    f.write_all(&payload)
        .map_err(|e| format!("write {}: {e}", out_file.display()))?;

    Ok(RbzMetadata {
        manifest,
        files: entries_meta,
        bundle_sha256,
    })
}

struct ParsedRbz {
    metadata: RbzMetadata,
    files: Vec<(RbzFileEntry, Vec<u8>)>,
}

fn parse_and_verify_rbz_bytes(raw: &[u8]) -> Result<ParsedRbz, String> {
    if raw.len() < 13 || raw[..5] != RBZ_MAGIC {
        return Err("invalid .rbz archive magic (expected RBZ1)".to_string());
    }
    let bundle_sha256 = sha256_hex(raw);
    let mut pos = 5usize;

    let manifest_len = read_u32_be(raw, &mut pos)? as usize;
    let manifest_slice = read_slice(raw, &mut pos, manifest_len)?;
    let manifest_str = std::str::from_utf8(manifest_slice)
        .map_err(|e| format!("invalid manifest UTF-8: {e}"))?;
    let manifest = AppManifest::from_toml_str(manifest_str)?;

    let file_count = read_u32_be(raw, &mut pos)? as usize;
    if file_count > 16384 {
        return Err(format!("excessive file count in .rbz: {file_count}"));
    }

    let mut entries = Vec::with_capacity(file_count);
    let mut files = Vec::with_capacity(file_count);

    for _ in 0..file_count {
        let path_len = read_u16_be(raw, &mut pos)? as usize;
        let path_slice = read_slice(raw, &mut pos, path_len)?;
        let rel_path = std::str::from_utf8(path_slice)
            .map_err(|e| format!("invalid file path UTF-8: {e}"))?
            .to_string();
        if rel_path.contains("..") || rel_path.starts_with('/') {
            return Err(format!("unsafe relative path in .rbz: {rel_path:?}"));
        }
        let exec_byte = read_slice(raw, &mut pos, 1)?[0];
        let expected_hash_bytes = read_slice(raw, &mut pos, 32)?;
        let data_len = read_u64_be(raw, &mut pos)? as usize;
        let data = read_slice(raw, &mut pos, data_len)?.to_vec();

        let actual_hash_bytes = sha256_bytes(&data);
        if actual_hash_bytes.as_slice() != expected_hash_bytes {
            return Err(format!(
                "SHA-256 integrity mismatch for file {rel_path:?} in .rbz archive"
            ));
        }
        let entry = RbzFileEntry {
            rel_path,
            size: data.len() as u64,
            sha256: sha256_hex(&data),
            executable: exec_byte != 0,
        };
        entries.push(entry.clone());
        files.push((entry, data));
    }

    Ok(ParsedRbz {
        metadata: RbzMetadata {
            manifest,
            files: entries,
            bundle_sha256,
        },
        files,
    })
}

/// Verify the cryptographic SHA-256 integrity of every file in `rbz_file` and return its [`RbzMetadata`].
pub fn verify_rbz(rbz_file: &Path) -> Result<RbzMetadata, String> {
    let mut raw = Vec::new();
    std::fs::File::open(rbz_file)
        .and_then(|mut f| f.read_to_end(&mut raw))
        .map_err(|e| format!("read {}: {e}", rbz_file.display()))?;
    Ok(parse_and_verify_rbz_bytes(&raw)?.metadata)
}

/// Verify and unpack `rbz_file` into `dest_dir`, writing a `SHA256SUMS` manifest alongside the files.
pub fn unpack_rbz(rbz_file: &Path, dest_dir: &Path) -> Result<AppManifest, String> {
    let raw = std::fs::read(rbz_file)
        .map_err(|e| format!("read {}: {e}", rbz_file.display()))?;
    let parsed = parse_and_verify_rbz_bytes(&raw)?;
    std::fs::create_dir_all(dest_dir)
        .map_err(|e| format!("mkdir {}: {e}", dest_dir.display()))?;

    let mut sums_lines = String::new();
    for (entry, data) in &parsed.files {
        let out_path = dest_dir.join(&entry.rel_path);
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("mkdir {}: {e}", parent.display()))?;
        }
        std::fs::write(&out_path, data)
            .map_err(|e| format!("write {}: {e}", out_path.display()))?;
        if entry.executable {
            set_executable_bit(&out_path);
        }
        sums_lines.push_str(&format!("{}  {}\n", entry.sha256, entry.rel_path));
    }
    std::fs::write(dest_dir.join("SHA256SUMS"), sums_lines)
        .map_err(|e| format!("write SHA256SUMS: {e}"))?;

    Ok(parsed.metadata.manifest)
}

/// Install `rbz_file` into `<data_home>/rabbiz/apps/<id>/` and register `<data_home>/applications/<id>.desktop`.
///
/// Returns the path to the generated `.desktop` file.
pub fn install_rbz_local(rbz_file: &Path, data_home: &Path) -> Result<PathBuf, String> {
    let raw = std::fs::read(rbz_file)
        .map_err(|e| format!("read {}: {e}", rbz_file.display()))?;
    let parsed = parse_and_verify_rbz_bytes(&raw)?;
    let app_id = &parsed.metadata.manifest.app.id;

    let install_dir = data_home.join("rabbiz").join("apps").join(app_id);
    let manifest = unpack_rbz(rbz_file, &install_dir)?;

    let bin_path = install_dir.join(&manifest.app.exec);
    if bin_path.exists() {
        set_executable_bit(&bin_path);
    }

    let apps_dir = data_home.join("applications");
    std::fs::create_dir_all(&apps_dir)
        .map_err(|e| format!("mkdir {}: {e}", apps_dir.display()))?;
    let desktop_path = apps_dir.join(format!("{app_id}.desktop"));
    let desktop_content = manifest.generate_desktop_entry(&bin_path);
    std::fs::write(&desktop_path, desktop_content)
        .map_err(|e| format!("write {}: {e}", desktop_path.display()))?;

    Ok(desktop_path)
}

fn read_slice<'a>(raw: &'a [u8], pos: &mut usize, len: usize) -> Result<&'a [u8], String> {
    let end = pos
        .checked_add(len)
        .filter(|&e| e <= raw.len())
        .ok_or_else(|| "truncated .rbz archive".to_string())?;
    let s = &raw[*pos..end];
    *pos = end;
    Ok(s)
}

fn read_u16_be(raw: &[u8], pos: &mut usize) -> Result<u16, String> {
    let b = read_slice(raw, pos, 2)?;
    Ok(u16::from_be_bytes([b[0], b[1]]))
}

fn read_u32_be(raw: &[u8], pos: &mut usize) -> Result<u32, String> {
    let b = read_slice(raw, pos, 4)?;
    Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}

fn read_u64_be(raw: &[u8], pos: &mut usize) -> Result<u64, String> {
    let b = read_slice(raw, pos, 8)?;
    Ok(u64::from_be_bytes([
        b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
    ]))
}

#[cfg(unix)]
fn is_executable_file(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .map(|m| m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable_file(_path: &Path) -> bool {
    false
}

#[cfg(unix)]
fn set_executable_bit(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(meta) = std::fs::metadata(path) {
        let mut perms = meta.permissions();
        perms.set_mode(perms.mode() | 0o755);
        let _ = std::fs::set_permissions(path, perms);
    }
}

#[cfg(not(unix))]
fn set_executable_bit(_path: &Path) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fips_180_4_sha256_test_vectors() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            sha256_hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

    #[test]
    fn manifest_parse_validate_and_desktop_generation() {
        let toml_src = r#"
[app]
id = "com.example.hello"
name = "Hello Rabbiz"
version = "0.1.0"
kind = "rust"
exec = "hello-rabbiz"
icon = "icon.svg"
categories = ["Utility", "Development"]

[runtime]
sdk_version = "0.1"
arch = "aarch64"
form_factors = ["handset", "tablet"]

[permissions]
network = false
notifications = true
intents = true
storage = true

[mcp]
enabled = true
"#;
        let m = AppManifest::from_toml_str(toml_src).expect("parse manifest");
        assert_eq!(m.app.id, "com.example.hello");
        assert_eq!(m.app.kind, AppKind::Rust);
        assert!(m.permissions.notifications);
        assert!(m.mcp.enabled);

        let desktop = m.generate_desktop_entry(Path::new("/home/rabbiz/.local/share/rabbiz/apps/com.example.hello/hello-rabbiz"));
        assert!(desktop.contains("[Desktop Entry]"));
        assert!(desktop.contains("Name=Hello Rabbiz"));
        assert!(desktop.contains("StartupWMClass=com.example.hello"));
        assert!(desktop.contains("Categories=Utility;Development;"));
    }

    #[test]
    fn rbz_pack_verify_tamper_detection_unpack_and_install_roundtrip() {
        let base = std::env::temp_dir().join(format!(
            "rabbiz-pack-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&base);
        let src_dir = base.join("src_app");
        std::fs::create_dir_all(&src_dir).unwrap();

        std::fs::write(
            src_dir.join("rabbiz.toml"),
            r#"
[app]
id = "com.rabbiz.packdemo"
name = "Pack Demo"
version = "0.1.0"
kind = "rust"
exec = "pack-demo-bin"
"#,
        )
        .unwrap();
        std::fs::write(src_dir.join("pack-demo-bin"), b"#!/bin/sh\necho hi\n").unwrap();
        std::fs::write(src_dir.join("icon.svg"), b"<svg/>").unwrap();

        let rbz_path = base.join("com.rabbiz.packdemo.rbz");
        let meta = pack_rbz(&src_dir, &rbz_path).expect("pack_rbz");
        assert_eq!(meta.manifest.app.id, "com.rabbiz.packdemo");
        assert!(meta.files.iter().any(|f| f.rel_path == "pack-demo-bin"));
        assert!(meta.files.iter().any(|f| f.rel_path == "com.rabbiz.packdemo.desktop"));

        let verified = verify_rbz(&rbz_path).expect("verify_rbz");
        assert_eq!(verified.bundle_sha256, meta.bundle_sha256);

        // Tamper with the last byte of the .rbz file and verify tamper detection triggers!
        let mut tampered = std::fs::read(&rbz_path).unwrap();
        let last_idx = tampered.len() - 1;
        tampered[last_idx] ^= 0xFF;
        let tampered_path = base.join("tampered.rbz");
        std::fs::write(&tampered_path, tampered).unwrap();
        let tamper_err = verify_rbz(&tampered_path).unwrap_err();
        assert!(tamper_err.contains("SHA-256 integrity mismatch"));

        // Install into a clean XDG_DATA_HOME directory
        let data_home = base.join("xdg_data");
        let desktop_file = install_rbz_local(&rbz_path, &data_home).expect("install_rbz_local");
        assert!(desktop_file.is_file());
        assert!(data_home
            .join("rabbiz/apps/com.rabbiz.packdemo/pack-demo-bin")
            .is_file());
        assert!(data_home
            .join("rabbiz/apps/com.rabbiz.packdemo/SHA256SUMS")
            .is_file());

        let _ = std::fs::remove_dir_all(&base);
    }
}
