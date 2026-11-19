//! D-Bus compatibility (`sm.puri.OSK0` and `org.rabbiz.OSK1`).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

struct Osk0Service {
    visible: Arc<AtomicBool>,
}

#[zbus::interface(name = "sm.puri.OSK0")]
impl Osk0Service {
    fn set_visible(&self, visible: bool) {
        self.visible.store(visible, Ordering::SeqCst);
    }

    #[zbus(property)]
    fn visible(&self) -> bool {
        self.visible.load(Ordering::SeqCst)
    }
}

struct Osk1Service {
    reload_theme: Arc<AtomicBool>,
    next_language: Arc<AtomicBool>,
}

#[zbus::interface(name = "org.rabbiz.OSK1")]
impl Osk1Service {
    /// Re-read ~/.config/rabbiz-osk/theme.toml and repaint without restarting.
    fn reload_theme(&self) {
        self.reload_theme.store(true, Ordering::SeqCst);
    }

    /// Quick Settings "language" tile: cycle EN → TH without gsettings.
    fn next_language(&self) {
        self.next_language.store(true, Ordering::SeqCst);
    }
}

pub(crate) fn spawn_dbus(
    visible: Arc<AtomicBool>,
    reload_theme: Arc<AtomicBool>,
    next_language: Arc<AtomicBool>,
) {
    std::thread::Builder::new()
        .name("osk-dbus".into())
        .spawn(move || {
            let built = zbus::blocking::connection::Builder::session()
                .and_then(|b| b.name("sm.puri.OSK0"))
                .and_then(|b| {
                    b.serve_at(
                        "/sm/puri/OSK0",
                        Osk0Service {
                            visible: visible.clone(),
                        },
                    )
                })
                .and_then(|b| {
                    b.serve_at(
                        "/org/rabbiz/OSK1",
                        Osk1Service {
                            reload_theme: reload_theme.clone(),
                            next_language: next_language.clone(),
                        },
                    )
                });
            match built.and_then(|b| b.build()) {
                Ok(_conn) => loop {
                    std::thread::park();
                },
                Err(err) => {
                    eprintln!("rabbiz-osk: D-Bus unavailable ({err}); compat methods disabled")
                }
            }
        })
        .ok();
}
