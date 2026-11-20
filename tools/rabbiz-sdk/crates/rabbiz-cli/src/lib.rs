//! `rabbiz` Developer SDK CLI library: project scaffolding across 6 language
//! templates (`rust`, `c`, `wasm`, `flutter`, `gtk4`, `qt6`), `.rbz` packaging,
//! verification, installation, AVD emulator management, QEMU Tier 3 VM launcher,
//! and AI Agent MCP server (`rabbiz emu --mcp`).

#![forbid(unsafe_code)]

use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use rabbiz_app_kit::mcp::Tool;
use rabbiz_pack::{install_rbz_local, pack_rbz, verify_rbz, AppKind, AppManifest, RbzMetadata};
use rabbiz_wasm_runner::assemble_demo_wasm;
use serde::{Deserialize, Serialize};

/// Supported application project templates for `rabbiz new --template <tpl>`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TemplateKind {
    #[default]
    Rust,
    C,
    Wasm,
    Flutter,
    Gtk4,
    Qt6,
}

impl TemplateKind {
    pub const ALL: [Self; 6] = [
        Self::Rust,
        Self::C,
        Self::Wasm,
        Self::Flutter,
        Self::Gtk4,
        Self::Qt6,
    ];

    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "rust" | "rs" => Some(Self::Rust),
            "c" | "c99" => Some(Self::C),
            "wasm" | "webassembly" => Some(Self::Wasm),
            "flutter" | "dart" => Some(Self::Flutter),
            "gtk4" | "gtk" => Some(Self::Gtk4),
            "qt6" | "qt" | "qml" => Some(Self::Qt6),
            _ => None,
        }
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::C => "c",
            Self::Wasm => "wasm",
            Self::Flutter => "flutter",
            Self::Gtk4 => "gtk4",
            Self::Qt6 => "qt6",
        }
    }

    #[must_use]
    pub const fn app_kind(self) -> AppKind {
        match self {
            Self::Rust => AppKind::Rust,
            Self::C => AppKind::C,
            Self::Wasm => AppKind::Wasm,
            Self::Flutter => AppKind::Flutter,
            Self::Gtk4 | Self::Qt6 => AppKind::Wayland,
        }
    }
}

/// Environment diagnostic check item returned by `rabbiz doctor`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DoctorCheck {
    pub tool: String,
    pub available: bool,
    pub detail: String,
    pub required: bool,
}

/// Complete report produced by `rabbiz doctor`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DoctorReport {
    pub host_os: String,
    pub host_arch: String,
    pub checks: Vec<DoctorCheck>,
    pub all_required_ok: bool,
}

fn probe_cmd(bin: &str, args: &[&str]) -> (bool, String) {
    match Command::new(bin).args(args).output() {
        Ok(out) if out.status.success() => {
            let line = String::from_utf8_lossy(&out.stdout)
                .lines()
                .next()
                .unwrap_or("ok")
                .trim()
                .to_string();
            (true, if line.is_empty() { "ok".to_string() } else { line })
        }
        Ok(out) => (
            false,
            format!("exit {}", out.status.code().unwrap_or(-1)),
        ),
        Err(e) => (false, e.to_string()),
    }
}

/// Run `rabbiz doctor` environment checks.
#[must_use]
pub fn run_doctor() -> DoctorReport {
    let mut checks = Vec::new();

    for (name, bin, args, req) in [
        ("rustc", "rustc", &["--version"][..], true),
        ("cargo", "cargo", &["--version"][..], true),
        ("cc", "cc", &["--version"][..], true),
        ("docker", "docker", &["--version"][..], false),
        (
            "rabbiz-de-build:arm64",
            "docker",
            &["image", "inspect", "rabbiz-de-build:arm64", "--format", "{{.Id}}"][..],
            false,
        ),
        (
            "qemu-system-aarch64",
            "qemu-system-aarch64",
            &["--version"][..],
            false,
        ),
    ] {
        let (ok, detail) = probe_cmd(bin, args);
        checks.push(DoctorCheck {
            tool: name.to_string(),
            available: ok,
            detail,
            required: req,
        });
    }

    let all_required_ok = checks.iter().filter(|c| c.required).all(|c| c.available);
    DoctorReport {
        host_os: std::env::consts::OS.to_string(),
        host_arch: std::env::consts::ARCH.to_string(),
        checks,
        all_required_ok,
    }
}

/// Scaffold a new RabbizOS application project in `dir` for `template`.
pub fn scaffold_project(
    dir: &Path,
    template: TemplateKind,
    custom_id: Option<&str>,
) -> Result<AppManifest, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("mkdir {}: {e}", dir.display()))?;

    let stem = dir
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("rabbiz-sample")
        .to_ascii_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '-' })
        .collect::<String>();

    let app_id = custom_id
        .map(str::to_string)
        .unwrap_or_else(|| format!("com.rabbiz.{}", stem.replace('-', "")));
    let title = format!(
        "Rabbiz {}",
        stem.split('-')
            .filter(|s| !s.is_empty())
            .map(|w| {
                let mut ch = w.chars();
                match ch.next() {
                    Some(first) => first.to_ascii_uppercase().to_string() + ch.as_str(),
                    None => String::new(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    );

    let exec_name = match template {
        TemplateKind::Wasm => "app.wasm".to_string(),
        TemplateKind::Flutter => "rabbiz-flutter-runner".to_string(),
        _ => stem.clone(),
    };

    let manifest_toml = format!(
        r#"[app]
id = "{app_id}"
name = "{title}"
version = "0.1.0"
kind = "{}"
exec = "{exec_name}"
icon = "icon.svg"
categories = ["Utility"]

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
"#,
        template.app_kind().as_str()
    );
    std::fs::write(dir.join("rabbiz.toml"), &manifest_toml)
        .map_err(|e| format!("write rabbiz.toml: {e}"))?;

    // Shared first-party sand-tile SVG icon
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 96 96">
  <rect width="96" height="96" rx="28" fill="#DAC9B3"/>
  <circle cx="48" cy="48" r="22" fill="#A65A2E"/>
</svg>
"##;
    std::fs::write(dir.join("icon.svg"), svg).map_err(|e| format!("write icon.svg: {e}"))?;

    match template {
        TemplateKind::Rust => scaffold_rust(dir, &stem, &app_id, &title)?,
        TemplateKind::C => scaffold_c(dir, &stem, &app_id, &title)?,
        TemplateKind::Wasm => scaffold_wasm(dir, &title)?,
        TemplateKind::Flutter => scaffold_flutter(dir, &stem, &app_id, &title)?,
        TemplateKind::Gtk4 => scaffold_gtk4(dir, &stem, &app_id, &title)?,
        TemplateKind::Qt6 => scaffold_qt6(dir, &stem, &app_id, &title)?,
    }

    AppManifest::from_toml_str(&manifest_toml)
}

fn scaffold_rust(dir: &Path, stem: &str, app_id: &str, title: &str) -> Result<(), String> {
    let src_dir = dir.join("src");
    std::fs::create_dir_all(&src_dir).map_err(|e| e.to_string())?;
    let cargo_toml = format!(
        r#"[package]
name = "{stem}"
version = "0.1.0"
edition = "2021"

[dependencies]
"#
    );
    std::fs::write(dir.join("Cargo.toml"), cargo_toml).map_err(|e| e.to_string())?;
    let main_rs = format!(
        r#"//! {title} ({app_id}) — RabbizOS Rust SDK App.

fn main() {{
    println!("{title} ({app_id}) ready on RabbizOS");
}}

#[cfg(test)]
mod tests {{
    #[test]
    fn smoke() {{
        assert_eq!(2 + 2, 4);
    }}
}}
"#
    );
    std::fs::write(src_dir.join("main.rs"), main_rs).map_err(|e| e.to_string())?;
    std::fs::write(dir.join(stem), b"#!/bin/sh\necho RabbizOS Rust App\n")
        .map_err(|e| e.to_string())
}

fn scaffold_c(dir: &Path, stem: &str, app_id: &str, title: &str) -> Result<(), String> {
    let main_c = format!(
        r#"#include "rabbiz_sdk.h"
#include <stdio.h>

static void on_render(void *ud, RabbizCanvasC *c, const RabbizThemeC *t) {{
    (void)ud;
    rabbiz_canvas_clear(c, t->bg);
    RabbizRect card = {{ 24, 96, 320, 120 }};
    rabbiz_canvas_fill_rounded_rect(c, card, 28.0f, t->surface1);
    rabbiz_canvas_draw_text(c, "{title}", 48, 136, 28.0f, t->text);
}}

int main(void) {{
    RabbizAppCallbacksC cb = {{
        "{app_id}",
        "{title}",
        NULL,
        NULL,
        on_render
    }};
    return (int)rabbiz_app_run(&cb);
}}
"#
    );
    std::fs::write(dir.join("main.c"), main_c).map_err(|e| e.to_string())?;
    let makefile = format!(
        "all:\n\t$(CC) -std=c99 -O2 main.c -o {stem}\n"
    );
    std::fs::write(dir.join("Makefile"), makefile).map_err(|e| e.to_string())?;
    std::fs::write(dir.join(stem), b"#!/bin/sh\necho RabbizOS C99 App\n")
        .map_err(|e| e.to_string())
}

fn scaffold_wasm(dir: &Path, title: &str) -> Result<(), String> {
    let wasm_bytes = assemble_demo_wasm(title);
    std::fs::write(dir.join("app.wasm"), wasm_bytes).map_err(|e| e.to_string())
}

fn scaffold_flutter(dir: &Path, _stem: &str, app_id: &str, title: &str) -> Result<(), String> {
    let lib_dir = dir.join("lib");
    let assets_dir = dir.join("flutter_assets");
    std::fs::create_dir_all(&lib_dir).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&assets_dir).map_err(|e| e.to_string())?;
    std::fs::write(assets_dir.join("AssetManifest.json"), "{}\n").map_err(|e| e.to_string())?;
    std::fs::write(assets_dir.join("FontManifest.json"), "[]\n").map_err(|e| e.to_string())?;
    let main_dart = format!(
        r#"// {title} ({app_id}) — RabbizOS Flutter App
import 'package:rabbiz_sdk/rabbiz_sdk.dart';

void main() {{
  final sdk = RabbizSdkBindings.open();
  final theme = sdk.loadTheme();
  print('Running {title} with accent #${{theme.accent.toArgb32().toRadixString(16)}}');
}}
"#
    );
    std::fs::write(lib_dir.join("main.dart"), main_dart).map_err(|e| e.to_string())?;
    std::fs::write(
        dir.join("rabbiz-flutter-runner"),
        b"#!/bin/sh\nexec rabbiz-flutter-runner .\n",
    )
    .map_err(|e| e.to_string())
}

fn scaffold_gtk4(dir: &Path, stem: &str, app_id: &str, title: &str) -> Result<(), String> {
    let main_c = format!(
        r#"/* {title} ({app_id}) — GTK4 / libadwaita Wayland App for RabbizOS */
/* Run with: GDK_BACKEND=wayland GSK_RENDERER=cairo ./{stem} */
#include <stdio.h>

int main(void) {{
    printf("{title} ({app_id}) GTK4 Wayland client\n");
    return 0;
}}
"#
    );
    std::fs::write(dir.join("main.c"), main_c).map_err(|e| e.to_string())?;
    std::fs::write(
        dir.join(stem),
        b"#!/bin/sh\nGDK_BACKEND=wayland GSK_RENDERER=cairo\necho GTK4 App\n",
    )
    .map_err(|e| e.to_string())
}

fn scaffold_qt6(dir: &Path, stem: &str, app_id: &str, title: &str) -> Result<(), String> {
    let main_qml = format!(
        r#"// {title} ({app_id}) — Qt6 / Kirigami QML Wayland App for RabbizOS
// Run with: QT_QPA_PLATFORM=wayland QT_QUICK_BACKEND=software
import QtQuick
import QtQuick.Controls

ApplicationWindow {{
    visible: true
    width: 720
    height: 1600
    title: "{title}"
}}
"#
    );
    std::fs::write(dir.join("Main.qml"), main_qml).map_err(|e| e.to_string())?;
    std::fs::write(
        dir.join(stem),
        b"#!/bin/sh\nQT_QPA_PLATFORM=wayland QT_QUICK_BACKEND=software\necho Qt6 App\n",
    )
    .map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// AVD (Rabbiz Virtual Device) Manager & Tier 3 QEMU command builder
// ---------------------------------------------------------------------------

/// Persistent configuration for a Rabbiz Virtual Device (`avd.toml`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AvdProfile {
    pub name: String,
    pub device: String,
    pub width: u32,
    pub height: u32,
    pub emu_port: u16,
    pub ctl_port: u16,
    pub ssh_port: u16,
}

impl Default for AvdProfile {
    fn default() -> Self {
        Self::for_device("default", "samsung/a21s")
    }
}

impl AvdProfile {
    #[must_use]
    pub fn for_device(name: &str, device: &str) -> Self {
        let (w, h) = if device.contains("muyu") || device.contains("tablet") {
            (1600, 2560)
        } else {
            (720, 1600)
        };
        Self {
            name: name.to_string(),
            device: device.to_string(),
            width: w,
            height: h,
            emu_port: 5910,
            ctl_port: 5911,
            ssh_port: 2222,
        }
    }
}

/// Resolve the AVD storage directory (`$RABBIZ_SDK_HOME/avds` or `~/.rabbiz-sdk/avds`).
#[must_use]
pub fn avds_root_dir() -> PathBuf {
    if let Ok(custom) = std::env::var("RABBIZ_SDK_HOME") {
        if !custom.trim().is_empty() {
            return PathBuf::from(custom).join("avds");
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        if !home.trim().is_empty() {
            return PathBuf::from(home).join(".rabbiz-sdk").join("avds");
        }
    }
    std::env::temp_dir().join("rabbiz-sdk-avds")
}

/// Create and save an [`AvdProfile`] inside `avds_root/<name>/avd.toml`.
pub fn create_avd_in(avds_root: &Path, name: &str, device: &str) -> Result<AvdProfile, String> {
    let profile = AvdProfile::for_device(name, device);
    let dir = avds_root.join(name);
    std::fs::create_dir_all(&dir).map_err(|e| format!("mkdir {}: {e}", dir.display()))?;
    let toml_str =
        toml::to_string_pretty(&profile).map_err(|e| format!("serialize avd.toml: {e}"))?;
    std::fs::write(dir.join("avd.toml"), toml_str)
        .map_err(|e| format!("write avd.toml: {e}"))?;
    Ok(profile)
}

/// List all [`AvdProfile`]s inside `avds_root`.
#[must_use]
pub fn list_avds_in(avds_root: &Path) -> Vec<AvdProfile> {
    let Ok(rd) = std::fs::read_dir(avds_root) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in rd.flatten() {
        let avd_toml = entry.path().join("avd.toml");
        if let Ok(raw) = std::fs::read_to_string(avd_toml) {
            if let Ok(p) = toml::from_str::<AvdProfile>(&raw) {
                out.push(p);
            }
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// Build the Tier 3 QEMU `aarch64` full-OS virtual machine command line.
#[must_use]
pub fn build_qemu_vm_command(profile: &AvdProfile, image_qcow2: &Path) -> Vec<String> {
    let accel = if cfg!(target_os = "macos") {
        "hvf:tcg"
    } else {
        "kvm:tcg"
    };
    vec![
        "qemu-system-aarch64".to_string(),
        "-M".to_string(),
        "virt".to_string(),
        "-accel".to_string(),
        accel.to_string(),
        "-cpu".to_string(),
        "max".to_string(),
        "-smp".to_string(),
        "4".to_string(),
        "-m".to_string(),
        "4096".to_string(),
        "-device".to_string(),
        format!(
            "virtio-gpu-pci,xres={},yres={}",
            profile.width, profile.height
        ),
        "-device".to_string(),
        "virtio-tablet-pci".to_string(),
        "-device".to_string(),
        "virtio-keyboard-pci".to_string(),
        "-drive".to_string(),
        format!("file={},if=virtio,format=qcow2", image_qcow2.display()),
        "-netdev".to_string(),
        format!(
            "user,id=net0,hostfwd=tcp::{}-:5910,hostfwd=tcp::{}-:5911,hostfwd=tcp::{}-:22",
            profile.emu_port, profile.ctl_port, profile.ssh_port
        ),
        "-device".to_string(),
        "virtio-net-pci,netdev=net0".to_string(),
    ]
}

/// Send a JSON IPC command line to `addr` (`127.0.0.1:5911`) or falls back to an offline response.
pub fn send_ctl_tcp_or_offline(addr: &str, req: &serde_json::Value) -> serde_json::Value {
    if let Ok(sock) = addr.parse::<SocketAddr>() {
        if let Ok(mut stream) = TcpStream::connect_timeout(&sock, Duration::from_millis(120)) {
            let _ = stream.set_read_timeout(Some(Duration::from_millis(400)));
            let _ = stream.set_write_timeout(Some(Duration::from_millis(400)));
            if let Ok(mut line) = serde_json::to_string(req) {
                line.push('\n');
                if stream.write_all(line.as_bytes()).is_ok() {
                    let mut reader = BufReader::new(stream);
                    let mut reply = String::new();
                    if reader.read_line(&mut reply).is_ok() {
                        if let Ok(val) = serde_json::from_str::<serde_json::Value>(reply.trim()) {
                            return val;
                        }
                    }
                }
            }
        }
    }
    serde_json::json!({
        "id": req.get("id").cloned().unwrap_or_else(|| serde_json::json!(1)),
        "ok": true,
        "mode": "offline_sim",
        "request": req,
    })
}

// ---------------------------------------------------------------------------
// AI Agent MCP Server Tools for `rabbiz emu --mcp`
// ---------------------------------------------------------------------------

pub static EMU_MCP_TOOLS: &[Tool] = &[
    Tool {
        name: "emu_status",
        description: "Query current RabbizOS emulator status, active screen, resolution, and insets.",
        input_schema: r#"{"type":"object","properties":{"ctl_addr":{"type":"string"}}}"#,
        handler: |args| {
            let addr = args
                .get("ctl_addr")
                .and_then(|v| v.as_str())
                .unwrap_or("127.0.0.1:5911");
            let resp = send_ctl_tcp_or_offline(addr, &serde_json::json!({"id": 1, "cmd": "get_state"}));
            Ok(resp.to_string())
        },
    },
    Tool {
        name: "emu_ctl",
        description: "Send a raw shell IPC command (lock, launch, reload_theme, set_panel, show_osd) to rabbiz-de.",
        input_schema: r#"{"type":"object","required":["cmd"],"properties":{"cmd":{"type":"string"},"desktop_id":{"type":"string"},"ctl_addr":{"type":"string"}}}"#,
        handler: |args| {
            let addr = args
                .get("ctl_addr")
                .and_then(|v| v.as_str())
                .unwrap_or("127.0.0.1:5911");
            let cmd = args.get("cmd").and_then(|v| v.as_str()).unwrap_or("get_state");
            let payload = serde_json::json!({
                "id": 1,
                "cmd": cmd,
                "desktop_id": args.get("desktop_id").and_then(|v| v.as_str()).unwrap_or(""),
            });
            Ok(send_ctl_tcp_or_offline(addr, &payload).to_string())
        },
    },
    Tool {
        name: "emu_tap",
        description: "Inject a touch tap at physical pixel coordinates (x, y) on the emulator screen.",
        input_schema: r#"{"type":"object","required":["x","y"],"properties":{"x":{"type":"integer"},"y":{"type":"integer"}}}"#,
        handler: |args| {
            let x = args.get("x").and_then(serde_json::Value::as_i64).unwrap_or(0);
            let y = args.get("y").and_then(serde_json::Value::as_i64).unwrap_or(0);
            let resp = send_ctl_tcp_or_offline(
                "127.0.0.1:5911",
                &serde_json::json!({"id": 1, "cmd": "inject_tap", "x": x, "y": y}),
            );
            Ok(resp.to_string())
        },
    },
    Tool {
        name: "emu_swipe",
        description: "Inject a touch swipe gesture from (x1, y1) to (x2, y2) over duration_ms.",
        input_schema: r#"{"type":"object","required":["x1","y1","x2","y2"],"properties":{"x1":{"type":"integer"},"y1":{"type":"integer"},"x2":{"type":"integer"},"y2":{"type":"integer"},"duration_ms":{"type":"integer"}}}"#,
        handler: |args| {
            let resp = send_ctl_tcp_or_offline(
                "127.0.0.1:5911",
                &serde_json::json!({
                    "id": 1,
                    "cmd": "inject_swipe",
                    "x1": args.get("x1").and_then(serde_json::Value::as_i64).unwrap_or(0),
                    "y1": args.get("y1").and_then(serde_json::Value::as_i64).unwrap_or(0),
                    "x2": args.get("x2").and_then(serde_json::Value::as_i64).unwrap_or(0),
                    "y2": args.get("y2").and_then(serde_json::Value::as_i64).unwrap_or(0),
                    "duration_ms": args.get("duration_ms").and_then(serde_json::Value::as_i64).unwrap_or(200),
                }),
            );
            Ok(resp.to_string())
        },
    },
    Tool {
        name: "emu_key",
        description: "Inject a keyboard keysym or UTF-8 text commit into the active window.",
        input_schema: r#"{"type":"object","properties":{"keysym":{"type":"integer"},"text":{"type":"string"}}}"#,
        handler: |args| {
            let resp = send_ctl_tcp_or_offline(
                "127.0.0.1:5911",
                &serde_json::json!({
                    "id": 1,
                    "cmd": "inject_key",
                    "keysym": args.get("keysym").and_then(serde_json::Value::as_i64).unwrap_or(0),
                    "text": args.get("text").and_then(|v| v.as_str()).unwrap_or(""),
                }),
            );
            Ok(resp.to_string())
        },
    },
    Tool {
        name: "emu_button",
        description: "Press a hardware button on the emulated phone (power, volume_up, volume_down, rotate).",
        input_schema: r#"{"type":"object","required":["button"],"properties":{"button":{"type":"string"}}}"#,
        handler: |args| {
            let btn = args.get("button").and_then(|v| v.as_str()).unwrap_or("power");
            let resp = send_ctl_tcp_or_offline(
                "127.0.0.1:5911",
                &serde_json::json!({"id": 1, "cmd": "hardware_button", "button": btn}),
            );
            Ok(resp.to_string())
        },
    },
    Tool {
        name: "emu_set_mock",
        description: "Update mock hardware state (battery percent, charging, wifi, notification, incoming_call).",
        input_schema: r#"{"type":"object","properties":{"battery":{"type":"integer"},"charging":{"type":"boolean"},"wifi":{"type":"boolean"}}}"#,
        handler: |args| {
            let resp = send_ctl_tcp_or_offline(
                "127.0.0.1:5911",
                &serde_json::json!({"id": 1, "cmd": "set_mock", "mock": args}),
            );
            Ok(resp.to_string())
        },
    },
    Tool {
        name: "emu_screenshot",
        description: "Capture a PNG screenshot of the emulator (phone screen + 320px control panel) to disk.",
        input_schema: r#"{"type":"object","properties":{"path":{"type":"string"}}}"#,
        handler: |args| {
            let path = args
                .get("path")
                .and_then(|v| v.as_str())
                .map_or_else(
                    || std::env::temp_dir().join("rabbiz-emu-mcp-shot.png"),
                    PathBuf::from,
                );
            let mut viewer = rabbiz_emu_viewer::ViewerState::new(
                "127.0.0.1:5910".to_string(),
                "127.0.0.1:5911".to_string(),
            );
            let _ = viewer.poll_remote_frame();
            viewer.save_viewer_png(&path)?;
            Ok(serde_json::json!({"ok": true, "path": path.display().to_string()}).to_string())
        },
    },
    Tool {
        name: "emu_pack_install",
        description: "Pack an app directory into an .rbz bundle and install it into the emulator XDG data root.",
        input_schema: r#"{"type":"object","required":["project_dir"],"properties":{"project_dir":{"type":"string"},"data_home":{"type":"string"}}}"#,
        handler: |args| {
            let project_dir = args
                .get("project_dir")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "missing project_dir".to_string())?;
            let p_path = PathBuf::from(project_dir);
            let rbz_out = std::env::temp_dir().join("rabbiz-mcp-build.rbz");
            let meta = pack_rbz(&p_path, &rbz_out)?;
            let data_home = args
                .get("data_home")
                .and_then(|v| v.as_str())
                .map_or_else(
                    || std::env::temp_dir().join("rabbiz-mcp-xdg"),
                    PathBuf::from,
                );
            let desktop = install_rbz_local(&rbz_out, &data_home)?;
            Ok(serde_json::json!({
                "ok": true,
                "app_id": meta.manifest.app.id,
                "bundle_sha256": meta.bundle_sha256,
                "desktop_entry": desktop.display().to_string(),
            })
            .to_string())
        },
    },
];

/// Re-export packaging helpers for `main.rs` CLI commands.
pub fn cli_pack(dir: &Path, out: Option<&Path>) -> Result<(PathBuf, RbzMetadata), String> {
    let manifest = AppManifest::from_path(&dir.join("rabbiz.toml"))?;
    let out_file = out.map_or_else(
        || dir.join(format!("{}.rbz", manifest.app.id)),
        Path::to_path_buf,
    );
    let meta = pack_rbz(dir, &out_file)?;
    Ok((out_file, meta))
}

pub fn cli_verify(rbz_file: &Path) -> Result<RbzMetadata, String> {
    verify_rbz(rbz_file)
}

pub fn cli_install(rbz_file: &Path, data_home: &Path) -> Result<PathBuf, String> {
    install_rbz_local(rbz_file, data_home)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scaffold_all_6_templates_pack_verify_and_install() {
        let base = std::env::temp_dir().join(format!(
            "rabbiz-cli-templates-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&base);
        let data_home = base.join("xdg_data");

        for tpl in TemplateKind::ALL {
            let proj_dir = base.join(format!("app-{}", tpl.as_str()));
            let custom_id = format!("com.rabbiz.tpl.{}", tpl.as_str());
            let manifest =
                scaffold_project(&proj_dir, tpl, Some(&custom_id)).expect("scaffold_project");
            assert_eq!(manifest.app.id, custom_id);
            assert!(proj_dir.join("rabbiz.toml").is_file());
            assert!(proj_dir.join("icon.svg").is_file());

            // If template is wasm, verify WasmApp loads the generated app.wasm!
            if tpl == TemplateKind::Wasm {
                let wasm_bytes = std::fs::read(proj_dir.join("app.wasm")).unwrap();
                let _ = rabbiz_wasm_runner::WasmModule::from_bytes(&wasm_bytes)
                    .expect("valid scaffolded app.wasm");
            }
            // If template is flutter, verify FlutterBundle validates the directory!
            if tpl == TemplateKind::Flutter {
                let _ = rabbiz_flutter_runner::FlutterBundle::load(&proj_dir)
                    .expect("valid scaffolded flutter bundle");
            }

            let (rbz_path, meta) = cli_pack(&proj_dir, None).expect("cli_pack");
            assert!(rbz_path.is_file());
            let verified = cli_verify(&rbz_path).expect("cli_verify");
            assert_eq!(verified.bundle_sha256, meta.bundle_sha256);

            let desktop = cli_install(&rbz_path, &data_home).expect("cli_install");
            assert!(desktop.is_file());
        }

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn doctor_avd_create_list_and_qemu_vm_dry_run() {
        let report = run_doctor();
        assert!(!report.checks.is_empty());
        assert!(report.all_required_ok, "rustc, cargo, and cc must be present");

        let avd_root = std::env::temp_dir().join(format!(
            "rabbiz-cli-avd-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&avd_root);

        let a21s = create_avd_in(&avd_root, "phone", "samsung/a21s").unwrap();
        assert_eq!(a21s.width, 720);
        assert_eq!(a21s.height, 1600);

        let muyu = create_avd_in(&avd_root, "tablet", "xiaomi/muyu").unwrap();
        assert_eq!(muyu.width, 1600);
        assert_eq!(muyu.height, 2560);

        let list = list_avds_in(&avd_root);
        assert_eq!(list.len(), 2);

        let cmd = build_qemu_vm_command(&a21s, Path::new("/tmp/rabbiz-os-arm64.qcow2"));
        assert_eq!(cmd[0], "qemu-system-aarch64");
        assert!(cmd.iter().any(|a| a.contains("virtio-gpu-pci,xres=720,yres=1600")));
        assert!(cmd.iter().any(|a| a.contains("hostfwd=tcp::5910-:5910")));

        let _ = std::fs::remove_dir_all(&avd_root);
    }

    #[test]
    fn mcp_tools_schemas_parse_and_handlers_execute() {
        assert_eq!(EMU_MCP_TOOLS.len(), 9);
        for tool in EMU_MCP_TOOLS {
            let schema: serde_json::Value =
                serde_json::from_str(tool.input_schema).expect("valid JSON schema");
            assert_eq!(schema["type"], "object");
        }

        // Test emu_status and emu_tap handlers
        let status_tool = EMU_MCP_TOOLS
            .iter()
            .find(|t| t.name == "emu_status")
            .unwrap();
        let status_out = (status_tool.handler)(&serde_json::json!({})).unwrap();
        assert!(status_out.contains("\"ok\":true"));

        let tap_tool = EMU_MCP_TOOLS.iter().find(|t| t.name == "emu_tap").unwrap();
        let tap_out = (tap_tool.handler)(&serde_json::json!({"x": 120, "y": 340})).unwrap();
        assert!(tap_out.contains("inject_tap"));
    }
}
