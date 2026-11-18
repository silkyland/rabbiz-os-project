//! Bounded, session-only freedesktop notification server. No payloads are logged
//! or written to disk. The compositor consumes snapshots without D-Bus calls.
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};
use zbus::zvariant::OwnedValue;

const LIMIT: usize = 64;
const PATH: &str = "/org/freedesktop/Notifications";
const IFACE: &str = "org.freedesktop.Notifications";

#[derive(Clone, Debug)]
pub struct Entry {
    pub id: u32,
    pub app: String,
    pub title: String,
    pub body: String,
    pub created: Instant,
    expires: Option<Instant>,
}
#[derive(Default)]
struct Store { entries: VecDeque<Entry>, next: u32, generation: u64 }
impl Store {
    fn notify(&mut self, app: &str, replaces: u32, title: &str, body: &str,
        timeout: i32, critical: bool) -> u32 {
        let existing = self.entries.iter().position(|e| e.id == replaces);
        let id = if existing.is_some() { replaces } else {
            loop {
                self.next = self.next.wrapping_add(1).max(1);
                if !self.entries.iter().any(|e| e.id == self.next) { break self.next; }
            }
        };
        if let Some(i) = existing { self.entries.remove(i); }
        let now = Instant::now();
        // Persistent notifications and critical alarms remain until dismissed.
        let expires = if timeout == 0 || critical { None } else {
            Some(now + Duration::from_millis(if timeout < 0 { 8000 } else { timeout as u64 }))
        };
        self.entries.push_front(Entry { id, app: bounded(app, 128), title: bounded(title, 256),
            body: plain_body(body), created: now, expires });
        self.entries.truncate(LIMIT);
        self.generation = self.generation.wrapping_add(1);
        id
    }
    fn remove(&mut self, id: u32) -> bool {
        let before = self.entries.len();
        self.entries.retain(|e| e.id != id);
        let changed = self.entries.len() != before;
        if changed { self.generation = self.generation.wrapping_add(1); }
        changed
    }
}
fn bounded(s: &str, n: usize) -> String { s.chars().take(n).collect() }
fn plain_body(body: &str) -> String {
    let mut tag = false;
    body.chars().take(8192).filter(|c| {
        if *c == '<' { tag = true; false } else if *c == '>' { tag = false; false }
        else { !tag }
    }).take(2048).collect()
}
struct Service { store: Arc<Mutex<Store>> }
#[zbus::interface(name = "org.freedesktop.Notifications")]
impl Service {
    fn get_capabilities(&self) -> Vec<&str> { vec!["body"] }
    fn get_server_information(&self) -> (&str, &str, &str, &str) {
        ("RabbizOS", "Devded", env!("CARGO_PKG_VERSION"), "1.3")
    }
    #[allow(clippy::too_many_arguments)]
    fn notify(&self, app_name: &str, replaces_id: u32, _app_icon: &str, summary: &str,
        body: &str, _actions: Vec<String>, hints: HashMap<String, OwnedValue>, expire_timeout: i32) -> u32 {
        let critical = hints.get("urgency").and_then(|v| u8::try_from(v).ok()) == Some(2);
        self.store.lock().unwrap_or_else(|e| e.into_inner())
            .notify(app_name, replaces_id, summary, body, expire_timeout, critical)
    }
    async fn close_notification(&self, id: u32,
        #[zbus(signal_emitter)] emitter: zbus::object_server::SignalEmitter<'_>) -> zbus::fdo::Result<()> {
        let removed = self.store.lock().unwrap_or_else(|e| e.into_inner()).remove(id);
        if removed { Self::notification_closed(&emitter, id, 3).await?; }
        Ok(())
    }
    #[zbus(signal)]
    async fn notification_closed(emitter: &zbus::object_server::SignalEmitter<'_>,
        id: u32, reason: u32) -> zbus::Result<()>;
}

pub struct Server {
    store: Arc<Mutex<Store>>,
    dismiss: mpsc::Sender<u32>,
    available: Arc<std::sync::atomic::AtomicBool>,
}
impl Server {
    pub fn start() -> Self {
        let store = Arc::new(Mutex::new(Store::default()));
        let available = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (tx, rx) = mpsc::channel::<u32>();
        let shared = store.clone();
        let ready = available.clone();
        std::thread::spawn(move || {
            let connection = zbus::blocking::connection::Builder::session()
                .and_then(|b| b.name(IFACE))
                .and_then(|b| b.serve_at(PATH, Service { store: shared.clone() }))
                .and_then(|b| b.build());
            let Ok(connection) = connection else { return; };
            ready.store(true, std::sync::atomic::Ordering::Relaxed);
            loop {
                let command = match rx.recv_timeout(Duration::from_millis(100)) {
                    Ok(id) => Some(id),
                    Err(mpsc::RecvTimeoutError::Timeout) => None,
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                };
                let mut closed = Vec::new();
                {
                    let mut s = shared.lock().unwrap_or_else(|e| e.into_inner());
                    if let Some(id) = command {
                        let ids: Vec<_> = s.entries.iter().filter(|e| id == 0 || id == e.id).map(|e| e.id).collect();
                        for id in ids { if s.remove(id) { closed.push((id, 2u32)); } }
                    }
                    let now = Instant::now();
                    let expired: Vec<_> = s.entries.iter()
                        .filter(|e| e.expires.is_some_and(|t| now >= t)).map(|e| e.id).collect();
                    for id in expired { if s.remove(id) { closed.push((id, 1u32)); } }
                }
                for (id, reason) in closed {
                    let _ = connection.emit_signal(None::<&str>, PATH, IFACE, "NotificationClosed", &(id, reason));
                }
            }
        });
        Self { store, dismiss: tx, available }
    }
    pub fn snapshot(&self) -> (u64, Vec<Entry>) {
        let s = self.store.lock().unwrap_or_else(|e| e.into_inner());
        (s.generation, s.entries.iter().cloned().collect())
    }
    pub fn available(&self) -> bool { self.available.load(std::sync::atomic::Ordering::Relaxed) }
    /// Zero means dismiss all; IDs allocated to clients are always nonzero.
    pub fn dismiss(&self, id: u32) { let _ = self.dismiss.send(id); }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn replacement_preserves_id_and_unknown_replacement_allocates() {
        let mut s = Store::default();
        let id = s.notify("app", 0, "one", "", 0, false);
        assert_eq!(s.notify("app", id, "two", "", 0, false), id);
        assert_eq!(s.entries.len(), 1);
        assert_eq!(s.entries[0].title, "two");
        assert_ne!(s.notify("app", 900, "three", "", 0, false), 900);
        assert!(s.remove(id));
        assert!(!s.remove(id));
    }
    #[test]
    fn bounded_plain_text_store_and_critical_timeout() {
        let mut s = Store::default();
        for _ in 0..100 { s.notify("app", 0, "title", "<b>hello</b>", 1, false); }
        assert_eq!(s.entries.len(), LIMIT);
        assert_eq!(s.entries[0].body, "hello");
        s.notify("alarm", 0, "wake", "", 10, true);
        assert!(s.entries[0].expires.is_none());
        s.next = u32::MAX;
        let id = s.notify("app", 0, "wrap", "", 0, false);
        assert_ne!(id, 0);
        assert_eq!(s.entries.iter().filter(|e| e.id == id).count(), 1);
    }
}
