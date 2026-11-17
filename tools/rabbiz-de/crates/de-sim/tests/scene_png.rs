//! End-to-end checks for `de-sim`: run the real binary and inspect the PNG.
//!
//! These live in `tests/` because they cross the process boundary; the scene
//! composition itself is unit-tested in `de-ui`.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// Command for the binary cargo built for this test run.
fn de_sim() -> Command {
    Command::new(env!("CARGO_BIN_EXE_de-sim"))
}

/// Per-test scratch directory, removed by the caller.
fn scratch(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!("de-sim-{tag}-{}", std::process::id()))
}

fn run(args: &[&str]) -> Output {
    de_sim()
        .args(args)
        .output()
        .expect("de-sim binary should be runnable")
}

/// PNG dimensions from the IHDR chunk, asserting the signature first.
fn png_size(path: &Path) -> (u32, u32) {
    let bytes = std::fs::read(path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()));
    assert!(
        bytes.len() >= 24,
        "{} is too short to be a PNG",
        path.display()
    );
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n", "not a PNG");
    assert_eq!(&bytes[12..16], b"IHDR", "first chunk is not IHDR");
    let width = u32::from_be_bytes(bytes[16..20].try_into().unwrap());
    let height = u32::from_be_bytes(bytes[20..24].try_into().unwrap());
    (width, height)
}

#[test]
fn lock_scene_writes_a_720x1600_png() {
    let dir = scratch("lock");
    let _ = std::fs::remove_dir_all(&dir);
    let out = dir.join("lock.png");

    let output = run(&[
        "--scene",
        "lock",
        "--out",
        out.to_str().expect("temp path is UTF-8"),
    ]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "de-sim failed: {stderr}");
    assert_eq!(png_size(&out), (720, 1600));

    // The runner reports the scene it painted, for scripts and goldens.
    let stdout = String::from_utf8(output.stdout).expect("stdout is JSON");
    assert!(stdout.contains("\"scene\":\"lock\""), "got {stdout:?}");
    assert!(stdout.contains("\"width\":720"), "got {stdout:?}");
    assert!(stdout.contains("\"height\":1600"), "got {stdout:?}");

    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn unknown_scene_exits_2_without_writing_a_file() {
    let dir = scratch("unknown");
    let _ = std::fs::remove_dir_all(&dir);
    let out = dir.join("nope.png");

    let output = run(&[
        "--scene",
        "nope",
        "--out",
        out.to_str().expect("temp path is UTF-8"),
    ]);
    assert_eq!(output.status.code(), Some(2));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("'nope'"), "stderr: {stderr}");
    assert!(stderr.contains("brand"), "stderr: {stderr}");
    assert!(stderr.contains("lock"), "stderr: {stderr}");
    assert!(!out.exists(), "a rejected scene must not write output");

    let _ = std::fs::remove_dir_all(&dir);
}
