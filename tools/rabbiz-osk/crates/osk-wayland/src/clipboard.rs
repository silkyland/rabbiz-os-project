use std::io::{BufRead, BufReader};
use std::process::{Command as ProcessCommand, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// K5: watch the selection with `wl-paste --watch` (wl-clipboard uses data-control, so this
/// works without keyboard focus) and keep the last 20 text snippets in memory.
pub(crate) fn spawn_clipboard_watch(items: Arc<Mutex<Vec<String>>>, changed: Arc<AtomicBool>) {
    std::thread::Builder::new()
        .name("osk-clipboard".into())
        .spawn(move || {
            let child = ProcessCommand::new("wl-paste")
                .args(["--watch", "--no-newline"])
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn();
            let Ok(mut child) = child else {
                eprintln!("rabbiz-osk: wl-paste not available; clipboard history disabled");
                return;
            };
            let Some(stdout) = child.stdout.take() else {
                return;
            };
            for line in BufReader::new(stdout).lines() {
                let Ok(text) = line else { break };
                if text.trim().is_empty() {
                    continue;
                }
                if let Ok(mut items) = items.lock() {
                    items.retain(|old| old != &text);
                    items.insert(0, text);
                    items.truncate(20);
                }
                changed.store(true, Ordering::SeqCst);
            }
            // wl-paste exited (or its stdout failed) — reap it so the child
            // does not linger as a zombie under rabbiz-osk.
            let _ = child.wait();
        })
        .ok();
}
