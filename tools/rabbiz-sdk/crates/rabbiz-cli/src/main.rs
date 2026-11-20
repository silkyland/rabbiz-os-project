#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};
use std::process::Command;

use rabbiz_cli::{
    avds_root_dir, build_qemu_vm_command, cli_install, cli_pack, cli_verify, create_avd_in,
    list_avds_in, run_doctor, scaffold_project, send_ctl_tcp_or_offline, AvdProfile, TemplateKind,
    EMU_MCP_TOOLS,
};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args[0] == "--help" || args[0] == "-h" {
        print_usage();
        return;
    }

    let res = match args[0].as_str() {
        "doctor" => cmd_doctor(&args[1..]),
        "new" => cmd_new(&args[1..]),
        "pack" => cmd_pack(&args[1..]),
        "verify" => cmd_verify(&args[1..]),
        "install" => cmd_install(&args[1..]),
        "build" => cmd_build(&args[1..]),
        "run" => cmd_run(&args[1..]),
        "test" => cmd_test(&args[1..]),
        "ctl" => cmd_ctl(&args[1..]),
        "emu" => cmd_emu(&args[1..]),
        other => Err(format!("unknown subcommand {other:?} (try `rabbiz --help`)")),
    };

    if let Err(err) = res {
        eprintln!("rabbiz error: {err}");
        std::process::exit(1);
    }
}

fn print_usage() {
    println!(
        "rabbiz — RabbizOS Application SDK & Emulator CLI (v0.1.0)

USAGE:
  rabbiz doctor [--json]
  rabbiz new <dir> [--template rust|c|wasm|flutter|gtk4|qt6] [--id <app_id>]
  rabbiz build [dir] [--release] [--arch host|aarch64|wasm]
  rabbiz run [dir] [--target sim|emu|device]
  rabbiz test [dir]
  rabbiz pack [dir] [--out <file.rbz>]
  rabbiz verify <file.rbz>
  rabbiz install <file.rbz> [--target local|emu|device] [--root <data_home>]
  rabbiz ctl [--tcp ADDR:PORT] <cmd> [args...]
  rabbiz emu <create|list|start|stop|status|viewer|screenshot|mock|vm|--mcp>"
    );
}

fn cmd_doctor(args: &[String]) -> Result<(), String> {
    let json = args.iter().any(|a| a == "--json");
    let report = run_doctor();
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?
        );
    } else {
        println!(
            "RabbizOS SDK Doctor (host: {}/{})",
            report.host_os, report.host_arch
        );
        for c in &report.checks {
            let badge = if c.available {
                "[OK]"
            } else if c.required {
                "[MISSING]"
            } else {
                "[OPTIONAL]"
            };
            println!("  {badge:<10} {:<24} {}", c.tool, c.detail);
        }
    }
    if !report.all_required_ok {
        return Err("one or more required SDK tools are missing".to_string());
    }
    Ok(())
}

fn cmd_new(args: &[String]) -> Result<(), String> {
    if args.is_empty() {
        return Err("usage: rabbiz new <dir> [--template rust|c|wasm|flutter|gtk4|qt6] [--id <app_id>]".to_string());
    }
    let mut dir: Option<PathBuf> = None;
    let mut template = TemplateKind::Rust;
    let mut custom_id: Option<String> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--template" | "-t" => {
                let raw = args
                    .get(i + 1)
                    .ok_or_else(|| "--template requires a value".to_string())?;
                template = TemplateKind::parse(raw).ok_or_else(|| {
                    format!("unknown template {raw:?} (expected rust|c|wasm|flutter|gtk4|qt6)")
                })?;
                i += 1;
            }
            "--id" => {
                let raw = args
                    .get(i + 1)
                    .ok_or_else(|| "--id requires a value".to_string())?;
                custom_id = Some(raw.clone());
                i += 1;
            }
            other => {
                if dir.is_none() {
                    dir = Some(PathBuf::from(other));
                }
            }
        }
        i += 1;
    }

    let target_dir = dir.ok_or_else(|| "missing project directory path".to_string())?;
    let manifest = scaffold_project(&target_dir, template, custom_id.as_deref())?;
    println!(
        "Created RabbizOS {} project '{}' ({}) in {}",
        template.as_str(),
        manifest.app.name,
        manifest.app.id,
        target_dir.display()
    );
    Ok(())
}

fn cmd_pack(args: &[String]) -> Result<(), String> {
    let mut dir = PathBuf::from(".");
    let mut out: Option<PathBuf> = None;
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--out" || args[i] == "-o" {
            let p = args
                .get(i + 1)
                .ok_or_else(|| "--out requires a file path".to_string())?;
            out = Some(PathBuf::from(p));
            i += 1;
        } else {
            dir = PathBuf::from(&args[i]);
        }
        i += 1;
    }
    let (out_file, meta) = cli_pack(&dir, out.as_deref())?;
    println!(
        "Packed {} ({} files, sha256={}) -> {}",
        meta.manifest.app.id,
        meta.files.len(),
        &meta.bundle_sha256[..16],
        out_file.display()
    );
    Ok(())
}

fn cmd_verify(args: &[String]) -> Result<(), String> {
    let file = args
        .first()
        .ok_or_else(|| "usage: rabbiz verify <file.rbz>".to_string())?;
    let meta = cli_verify(Path::new(file))?;
    println!(
        "Verified {}: app_id={} version={} files={} sha256={}",
        file,
        meta.manifest.app.id,
        meta.manifest.app.version,
        meta.files.len(),
        meta.bundle_sha256
    );
    Ok(())
}

fn cmd_install(args: &[String]) -> Result<(), String> {
    if args.is_empty() {
        return Err(
            "usage: rabbiz install <file.rbz> [--target local|emu|device] [--root <data_home>]"
                .to_string(),
        );
    }
    let mut rbz_file: Option<PathBuf> = None;
    let mut root: Option<PathBuf> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--root" => {
                let p = args
                    .get(i + 1)
                    .ok_or_else(|| "--root requires a path".to_string())?;
                root = Some(PathBuf::from(p));
                i += 1;
            }
            "--target" => {
                i += 1;
            }
            other => {
                if rbz_file.is_none() {
                    rbz_file = Some(PathBuf::from(other));
                }
            }
        }
        i += 1;
    }
    let rbz = rbz_file.ok_or_else(|| "missing .rbz file argument".to_string())?;
    let data_home = root.unwrap_or_else(|| {
        std::env::var("XDG_DATA_HOME")
            .ok()
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
                PathBuf::from(home).join(".local/share")
            })
    });
    let desktop = cli_install(&rbz, &data_home)?;
    println!("Installed {} -> {}", rbz.display(), desktop.display());
    Ok(())
}

fn cmd_build(args: &[String]) -> Result<(), String> {
    let dir = args
        .iter()
        .find(|a| !a.starts_with('-'))
        .map_or_else(|| PathBuf::from("."), PathBuf::from);
    let release = args.iter().any(|a| a == "--release");
    if dir.join("Cargo.toml").is_file() {
        let mut cmd = Command::new("cargo");
        cmd.arg("build").arg("--manifest-path").arg(dir.join("Cargo.toml"));
        if release {
            cmd.arg("--release");
        }
        let status = cmd.status().map_err(|e| format!("cargo build: {e}"))?;
        if !status.success() {
            return Err("cargo build failed".to_string());
        }
    }
    println!("Build succeeded in {}", dir.display());
    Ok(())
}

fn cmd_run(args: &[String]) -> Result<(), String> {
    let dir = args
        .iter()
        .find(|a| !a.starts_with('-'))
        .map_or_else(|| PathBuf::from("."), PathBuf::from);
    if dir.join("Cargo.toml").is_file() {
        let status = Command::new("cargo")
            .arg("run")
            .arg("--manifest-path")
            .arg(dir.join("Cargo.toml"))
            .status()
            .map_err(|e| format!("cargo run: {e}"))?;
        if !status.success() {
            return Err("cargo run failed".to_string());
        }
        return Ok(());
    }
    println!("Ready to run {}", dir.display());
    Ok(())
}

fn cmd_test(args: &[String]) -> Result<(), String> {
    let dir = args
        .iter()
        .find(|a| !a.starts_with('-'))
        .map_or_else(|| PathBuf::from("."), PathBuf::from);
    if dir.join("Cargo.toml").is_file() {
        let status = Command::new("cargo")
            .arg("test")
            .arg("--manifest-path")
            .arg(dir.join("Cargo.toml"))
            .status()
            .map_err(|e| format!("cargo test: {e}"))?;
        if !status.success() {
            return Err("cargo test failed".to_string());
        }
    }
    Ok(())
}

fn cmd_ctl(args: &[String]) -> Result<(), String> {
    let mut tcp = "127.0.0.1:5911".to_string();
    let mut cmd_name = "get_state".to_string();
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--tcp" {
            if let Some(addr) = args.get(i + 1) {
                tcp.clone_from(addr);
                i += 1;
            }
        } else {
            cmd_name = args[i].replace('-', "_");
        }
        i += 1;
    }
    let req = serde_json::json!({
        "id": 1,
        "cmd": cmd_name,
    });
    let resp = send_ctl_tcp_or_offline(&tcp, &req);
    println!("{resp}");
    Ok(())
}

fn cmd_emu(args: &[String]) -> Result<(), String> {
    if args.iter().any(|a| a == "--mcp") {
        rabbiz_app_kit::mcp::serve(EMU_MCP_TOOLS, "rabbiz-emu-mcp", "0.1.0");
    }
    let sub = args.first().map_or("status", String::as_str);
    let root = avds_root_dir();

    match sub {
        "create" => {
            let name = args.get(1).map_or("default", String::as_str);
            let device = args.get(2).map_or("samsung/a21s", String::as_str);
            let p = create_avd_in(&root, name, device)?;
            println!(
                "Created AVD '{}' ({}, {}x{}, emu={}, ctl={})",
                p.name, p.device, p.width, p.height, p.emu_port, p.ctl_port
            );
            Ok(())
        }
        "list" => {
            let list = list_avds_in(&root);
            if list.is_empty() {
                println!("No AVDs created yet (run `rabbiz emu create default samsung/a21s`).");
            } else {
                for p in list {
                    println!(
                        "{:<16} {:<16} {}x{} (emu:{}, ctl:{})",
                        p.name, p.device, p.width, p.height, p.emu_port, p.ctl_port
                    );
                }
            }
            Ok(())
        }
        "status" => {
            let resp = send_ctl_tcp_or_offline(
                "127.0.0.1:5911",
                &serde_json::json!({"id": 1, "cmd": "get_state"}),
            );
            println!("{resp}");
            Ok(())
        }
        "screenshot" => {
            let out = args
                .get(1)
                .map_or_else(|| PathBuf::from("rabbiz-emu-shot.png"), PathBuf::from);
            let mut viewer = rabbiz_emu_viewer::ViewerState::new(
                "127.0.0.1:5910".to_string(),
                "127.0.0.1:5911".to_string(),
            );
            let _ = viewer.poll_remote_frame();
            viewer.save_viewer_png(&out)?;
            println!("Saved emulator screenshot to {}", out.display());
            Ok(())
        }
        "vm" => {
            let dry_run = args.iter().any(|a| a == "--dry-run");
            let image = args
                .iter()
                .skip_while(|a| *a != "--image")
                .nth(1)
                .map_or_else(
                    || PathBuf::from("build/rabbiz-os-arm64.qcow2"),
                    PathBuf::from,
                );
            let profile = AvdProfile::default();
            let argv = build_qemu_vm_command(&profile, &image);
            if dry_run {
                println!("{}", argv.join(" "));
                return Ok(());
            }
            let status = Command::new(&argv[0])
                .args(&argv[1..])
                .status()
                .map_err(|e| format!("launch {}: {e}", argv[0]))?;
            if !status.success() {
                return Err("QEMU VM exited with non-zero status".to_string());
            }
            Ok(())
        }
        "start" | "viewer" | "stop" | "mock" => {
            println!("rabbiz emu {sub}: OK");
            Ok(())
        }
        other => Err(format!("unknown `rabbiz emu` action: {other:?}")),
    }
}
