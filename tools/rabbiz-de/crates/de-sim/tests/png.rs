//! End-to-end check: the shipped binary builds a screen and writes a PNG.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn temp_out(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("de-sim-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    dir.join("out.png")
}

#[test]
fn brand_scene_writes_a_720x1600_png() {
    let out = temp_out("brand");
    let status = Command::new(env!("CARGO_BIN_EXE_de-sim"))
        .args(["--scene", "brand", "--out"])
        .arg(&out)
        .status()
        .expect("run de-sim");
    assert!(status.success(), "de-sim exited with {status}");

    let bytes = fs::read(&out).expect("read png");
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n", "not a PNG");
    let width = u32::from_be_bytes(bytes[16..20].try_into().unwrap());
    let height = u32::from_be_bytes(bytes[20..24].try_into().unwrap());
    assert_eq!((width, height), (720, 1600));
    let _ = fs::remove_dir_all(out.parent().unwrap());
}

#[test]
fn unknown_scene_fails_without_writing() {
    let out = temp_out("nope");
    let status = Command::new(env!("CARGO_BIN_EXE_de-sim"))
        .args(["--scene", "nope", "--out"])
        .arg(&out)
        .status()
        .expect("run de-sim");
    assert!(!status.success());
    assert!(!out.exists(), "a failed run must not leave a file");
}
