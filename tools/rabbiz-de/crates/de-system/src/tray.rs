//! `org.kde.StatusNotifierWatcher` D-Bus server for Desktop Mode system tray
//! icons (Phase D3).
//!
//! Tracks registered `StatusNotifierItem` services in memory without blocking
//! the compositor event loop.

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

const BUS_NAME: &str = "org.kde.StatusNotifierWatcher";
const PATH: &str = "/StatusNotifierWatcher";

/// Pure thread-safe store of registered `StatusNotifierItem` identifiers.
#[derive(Debug, Default)]
pub struct TrayStore {
    items: BTreeSet<String>,
    hosts: BTreeSet<String>,
}

impl TrayStore {
    /// Register a tray item (`service` or `/object/path`); returns `true` if newly added.
    pub fn register_item(&mut self, service: &str) -> bool {
        let trimmed = service.trim();
        if trimmed.is_empty() {
            return false;
        }
        self.items.insert(trimmed.to_string())
    }

    /// Unregister a tray item (e.g., on `NameOwnerChanged` vanish).
    pub fn unregister_item(&mut self, service: &str) -> bool {
        self.items.remove(service.trim())
    }

    /// Register a `StatusNotifierHost` (e.g., `rabbiz-de` taskbar).
    pub fn register_host(&mut self, service: &str) -> bool {
        let trimmed = service.trim();
        if trimmed.is_empty() {
            return false;
        }
        self.hosts.insert(trimmed.to_string())
    }

    /// Snapshot of currently registered item IDs.
    #[must_use]
    pub fn items(&self) -> Vec<String> {
        self.items.iter().cloned().collect()
    }

    /// Number of registered tray items.
    #[must_use]
    pub fn count(&self) -> usize {
        self.items.len()
    }

    /// Whether at least one host is registered (always true when `rabbiz-de` runs).
    #[must_use]
    pub fn host_registered(&self) -> bool {
        !self.hosts.is_empty()
    }
}

struct WatcherIface {
    store: Arc<Mutex<TrayStore>>,
}

#[zbus::interface(name = "org.kde.StatusNotifierWatcher")]
impl WatcherIface {
    async fn register_status_notifier_item(
        &self,
        service: &str,
        #[zbus(signal_emitter)] ctxt: zbus::object_server::SignalEmitter<'_>,
    ) {
        let added = {
            let mut g = self.store.lock().unwrap_or_else(|e| e.into_inner());
            g.register_item(service)
        };
        if added {
            let _ = Self::status_notifier_item_registered(&ctxt, service).await;
        }
    }

    async fn register_status_notifier_host(
        &self,
        service: &str,
        #[zbus(signal_emitter)] ctxt: zbus::object_server::SignalEmitter<'_>,
    ) {
        let added = {
            let mut g = self.store.lock().unwrap_or_else(|e| e.into_inner());
            g.register_host(service)
        };
        if added {
            let _ = Self::status_notifier_host_registered(&ctxt).await;
        }
    }

    #[zbus(property)]
    fn registered_status_notifier_items(&self) -> Vec<String> {
        self.store.lock().unwrap_or_else(|e| e.into_inner()).items()
    }

    #[zbus(property)]
    fn is_status_notifier_host_registered(&self) -> bool {
        self.store
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .host_registered()
    }

    #[zbus(property)]
    fn protocol_version(&self) -> i32 {
        0
    }

    #[zbus(signal)]
    async fn status_notifier_item_registered(
        ctxt: &zbus::object_server::SignalEmitter<'_>,
        service: &str,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn status_notifier_item_unregistered(
        ctxt: &zbus::object_server::SignalEmitter<'_>,
        service: &str,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn status_notifier_host_registered(
        ctxt: &zbus::object_server::SignalEmitter<'_>,
    ) -> zbus::Result<()>;
}

/// Handle to the background `StatusNotifierWatcher` D-Bus service.
#[derive(Clone)]
pub struct StatusNotifierWatcher {
    store: Arc<Mutex<TrayStore>>,
}

impl StatusNotifierWatcher {
    /// Start the `org.kde.StatusNotifierWatcher` service on the session D-Bus
    /// in a background thread. Never blocks the caller if no D-Bus session exists.
    #[must_use]
    pub fn start() -> Self {
        let store = Arc::new(Mutex::new(TrayStore::default()));
        {
            let mut g = store.lock().unwrap_or_else(|e| e.into_inner());
            g.register_host("org.rabbiz.Taskbar");
        }
        let thread_store = store.clone();
        std::thread::Builder::new()
            .name("rabbiz-sni-watcher".to_string())
            .spawn(move || {
                let Ok(conn) = zbus::blocking::Connection::session() else {
                    return;
                };
                if conn.object_server().at(PATH, WatcherIface { store: thread_store }).is_err() {
                    return;
                }
                if conn.request_name(BUS_NAME).is_err() {
                    return;
                }
                loop {
                    std::thread::sleep(std::time::Duration::from_secs(60));
                }
            })
            .ok();
        Self { store }
    }

    /// Current number of registered tray items.
    #[must_use]
    pub fn count(&self) -> usize {
        self.store.lock().unwrap_or_else(|e| e.into_inner()).count()
    }

    /// Snapshot of registered tray item service names.
    #[must_use]
    pub fn items(&self) -> Vec<String> {
        self.store.lock().unwrap_or_else(|e| e.into_inner()).items()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tray_store_registers_dedupes_and_unregisters() {
        let mut s = TrayStore::default();
        assert!(!s.host_registered());
        assert!(s.register_host("org.rabbiz.Taskbar"));
        assert!(s.host_registered());

        assert!(!s.register_item("   "));
        assert!(s.register_item(":1.42/org/ayatana/NotificationItem/nm_applet"));
        assert!(!s.register_item(":1.42/org/ayatana/NotificationItem/nm_applet"));
        assert_eq!(s.count(), 1);

        assert!(s.unregister_item(":1.42/org/ayatana/NotificationItem/nm_applet"));
        assert_eq!(s.count(), 0);
    }
}
