//! Bounded, session-only freedesktop notification server. No payloads are logged
//! or written to disk. The compositor consumes snapshots without D-Bus calls.
use std::collections::{HashMap, VecDeque};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};
use zbus::zvariant::OwnedValue;

const LIMIT: usize = 64;
const MAX_ACTIONS: usize = 3;
const PATH: &str = "/org/freedesktop/Notifications";
const IFACE: &str = "org.freedesktop.Notifications";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Urgency {
    Low = 0,
    #[default]
    Normal = 1,
    Critical = 2,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotificationAction {
    /// Wire key returned via `ActionInvoked(id, key)` D-Bus signal.
    pub key: String,
    /// Human-readable button label (bounded to 32 chars).
    pub label: String,
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub id: u32,
    pub app: String,
    pub desktop_entry: Option<String>,
    pub category: Option<String>,
    pub title: String,
    pub body: String,
    pub urgency: Urgency,
    pub resident: bool,
    pub transient: bool,
    pub actions: Vec<NotificationAction>,
    pub created: Instant,
    expires: Option<Instant>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct ParsedHints {
    desktop_entry: Option<String>,
    category: Option<String>,
    urgency: Urgency,
    resident: bool,
    transient: bool,
}

fn parse_actions(raw: &[String]) -> Vec<NotificationAction> {
    let mut out = Vec::new();
    for pair in raw.chunks_exact(2) {
        if out.len() >= MAX_ACTIONS {
            break;
        }
        let key = bounded(pair[0].trim(), 64);
        let label = bounded(pair[1].trim(), 32);
        if key.is_empty() || key == "default" || label.is_empty() {
            continue;
        }
        out.push(NotificationAction { key, label });
    }
    out
}

fn parse_hints(hints: &HashMap<String, OwnedValue>) -> ParsedHints {
    let urgency = match hints.get("urgency").and_then(|v| u8::try_from(v).ok()) {
        Some(0) => Urgency::Low,
        Some(2) => Urgency::Critical,
        _ => Urgency::Normal,
    };
    let desktop_entry = hints
        .get("desktop-entry")
        .and_then(|v| <&str>::try_from(v).ok())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| bounded(s, 128));
    let category = hints
        .get("category")
        .and_then(|v| <&str>::try_from(v).ok())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| bounded(s, 64));
    let resident = hints
        .get("resident")
        .and_then(|v| bool::try_from(v).ok())
        .unwrap_or(false);
    let transient = hints
        .get("transient")
        .and_then(|v| bool::try_from(v).ok())
        .unwrap_or(false);
    ParsedHints {
        desktop_entry,
        category,
        urgency,
        resident,
        transient,
    }
}

#[derive(Default)]
struct Store {
    entries: VecDeque<Entry>,
    next: u32,
    generation: u64,
}
impl Store {
    #[allow(clippy::too_many_arguments)]
    fn notify(
        &mut self,
        app: &str,
        replaces: u32,
        title: &str,
        body: &str,
        actions: Vec<NotificationAction>,
        hints: ParsedHints,
        timeout: i32,
    ) -> u32 {
        let existing = self.entries.iter().position(|e| e.id == replaces);
        let id = if existing.is_some() {
            replaces
        } else {
            loop {
                self.next = self.next.wrapping_add(1).max(1);
                if !self.entries.iter().any(|e| e.id == self.next) {
                    break self.next;
                }
            }
        };
        if let Some(i) = existing {
            self.entries.remove(i);
        }
        let now = Instant::now();
        // Persistent, resident, and critical notifications remain until dismissed.
        let expires = if timeout == 0 || hints.urgency == Urgency::Critical || hints.resident {
            None
        } else {
            Some(now + Duration::from_millis(if timeout < 0 { 8000 } else { timeout as u64 }))
        };
        self.entries.push_front(Entry {
            id,
            app: bounded(app, 128),
            desktop_entry: hints.desktop_entry,
            category: hints.category,
            title: bounded(title, 256),
            body: plain_body(body),
            urgency: hints.urgency,
            resident: hints.resident,
            transient: hints.transient,
            actions,
            created: now,
            expires,
        });
        self.entries.truncate(LIMIT);
        self.generation = self.generation.wrapping_add(1);
        id
    }
    fn remove(&mut self, id: u32) -> bool {
        let before = self.entries.len();
        self.entries.retain(|e| e.id != id);
        let changed = self.entries.len() != before;
        if changed {
            self.generation = self.generation.wrapping_add(1);
        }
        changed
    }
}
fn bounded(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}
fn plain_body(body: &str) -> String {
    let mut tag = false;
    body.chars()
        .take(8192)
        .filter(|c| {
            if *c == '<' {
                tag = true;
                false
            } else if *c == '>' {
                tag = false;
                false
            } else {
                !tag
            }
        })
        .take(2048)
        .collect()
}
struct Service {
    store: Arc<Mutex<Store>>,
}
#[zbus::interface(name = "org.freedesktop.Notifications")]
impl Service {
    fn get_capabilities(&self) -> Vec<&str> {
        vec!["body", "actions", "persistence", "action-icons"]
    }
    fn get_server_information(&self) -> (&str, &str, &str, &str) {
        ("RabbizOS", "Devded", env!("CARGO_PKG_VERSION"), "1.3")
    }
    #[allow(clippy::too_many_arguments)]
    fn notify(
        &self,
        app_name: &str,
        replaces_id: u32,
        _app_icon: &str,
        summary: &str,
        body: &str,
        actions: Vec<String>,
        hints: HashMap<String, OwnedValue>,
        expire_timeout: i32,
    ) -> u32 {
        let parsed_actions = parse_actions(&actions);
        let parsed_hints = parse_hints(&hints);
        self.store.lock().unwrap_or_else(|e| e.into_inner()).notify(
            app_name,
            replaces_id,
            summary,
            body,
            parsed_actions,
            parsed_hints,
            expire_timeout,
        )
    }
    async fn close_notification(
        &self,
        id: u32,
        #[zbus(signal_emitter)] emitter: zbus::object_server::SignalEmitter<'_>,
    ) -> zbus::fdo::Result<()> {
        let removed = self
            .store
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(id);
        if removed {
            Self::notification_closed(&emitter, id, 3).await?;
        }
        Ok(())
    }
    #[zbus(signal)]
    async fn notification_closed(
        emitter: &zbus::object_server::SignalEmitter<'_>,
        id: u32,
        reason: u32,
    ) -> zbus::Result<()>;
    #[allow(dead_code)]
    #[zbus(signal)]
    async fn action_invoked(
        emitter: &zbus::object_server::SignalEmitter<'_>,
        id: u32,
        action_key: &str,
    ) -> zbus::Result<()>;
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
                .and_then(|b| {
                    b.serve_at(
                        PATH,
                        Service {
                            store: shared.clone(),
                        },
                    )
                })
                .and_then(|b| b.build());
            let Ok(connection) = connection else {
                return;
            };
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
                        let ids: Vec<_> = s
                            .entries
                            .iter()
                            .filter(|e| id == 0 || id == e.id)
                            .map(|e| e.id)
                            .collect();
                        for id in ids {
                            if s.remove(id) {
                                closed.push((id, 2u32));
                            }
                        }
                    }
                    let now = Instant::now();
                    let expired: Vec<_> = s
                        .entries
                        .iter()
                        .filter(|e| e.expires.is_some_and(|t| now >= t))
                        .map(|e| e.id)
                        .collect();
                    for id in expired {
                        if s.remove(id) {
                            closed.push((id, 1u32));
                        }
                    }
                }
                for (id, reason) in closed {
                    let _ = connection.emit_signal(
                        None::<&str>,
                        PATH,
                        IFACE,
                        "NotificationClosed",
                        &(id, reason),
                    );
                }
            }
        });
        Self {
            store,
            dismiss: tx,
            available,
        }
    }
    pub fn snapshot(&self) -> (u64, Vec<Entry>) {
        let s = self.store.lock().unwrap_or_else(|e| e.into_inner());
        (s.generation, s.entries.iter().cloned().collect())
    }
    pub fn available(&self) -> bool {
        self.available.load(std::sync::atomic::Ordering::Relaxed)
    }
    /// Zero means dismiss all; IDs allocated to clients are always nonzero.
    pub fn dismiss(&self, id: u32) {
        let _ = self.dismiss.send(id);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use zbus::zvariant::Value;

    #[test]
    fn replacement_preserves_id_and_unknown_replacement_allocates() {
        let mut s = Store::default();
        let id = s.notify("app", 0, "one", "", Vec::new(), ParsedHints::default(), 0);
        assert_eq!(
            s.notify("app", id, "two", "", Vec::new(), ParsedHints::default(), 0),
            id
        );
        assert_eq!(s.entries.len(), 1);
        assert_eq!(s.entries[0].title, "two");
        assert_ne!(
            s.notify(
                "app",
                900,
                "three",
                "",
                Vec::new(),
                ParsedHints::default(),
                0
            ),
            900
        );
        assert!(s.remove(id));
        assert!(!s.remove(id));
    }

    #[test]
    fn bounded_plain_text_store_and_64_entry_cap() {
        let mut s = Store::default();
        for i in 0..100 {
            s.notify(
                "app",
                0,
                &format!("title-{i}"),
                "<b>hello</b>",
                Vec::new(),
                ParsedHints::default(),
                1,
            );
        }
        assert_eq!(s.entries.len(), LIMIT);
        assert_eq!(s.entries[0].title, "title-99");
        assert_eq!(s.entries[0].body, "hello");
        s.next = u32::MAX;
        let id = s.notify("app", 0, "wrap", "", Vec::new(), ParsedHints::default(), 0);
        assert_ne!(id, 0);
        assert_eq!(s.entries.iter().filter(|e| e.id == id).count(), 1);
    }

    #[test]
    fn action_pair_parsing_skips_default_and_caps_at_three() {
        let long_key = "k".repeat(80);
        let long_label = "L".repeat(50);
        let raw = vec![
            "default".to_string(),
            "Open".to_string(),
            String::new(),
            "EmptyKey".to_string(),
            "empty-label".to_string(),
            "   ".to_string(),
            long_key,
            long_label,
            "reply".to_string(),
            "Reply".to_string(),
            "archive".to_string(),
            "Archive".to_string(),
            "extra".to_string(),
            "Ignored".to_string(),
            "dangling".to_string(),
        ];
        let parsed = parse_actions(&raw);
        assert_eq!(parsed.len(), MAX_ACTIONS);
        assert_eq!(parsed[0].key.len(), 64);
        assert_eq!(parsed[0].label.len(), 32);
        assert_eq!(
            parsed[1],
            NotificationAction {
                key: "reply".into(),
                label: "Reply".into(),
            }
        );
        assert_eq!(
            parsed[2],
            NotificationAction {
                key: "archive".into(),
                label: "Archive".into(),
            }
        );
    }

    #[test]
    fn hint_parsing_extracts_all_supported_fields() {
        let mut hints = HashMap::new();
        hints.insert(
            "urgency".to_string(),
            OwnedValue::try_from(Value::from(2u8)).unwrap(),
        );
        hints.insert(
            "desktop-entry".to_string(),
            OwnedValue::try_from(Value::from("org.rabbiz.Messages")).unwrap(),
        );
        hints.insert(
            "category".to_string(),
            OwnedValue::try_from(Value::from("im.received")).unwrap(),
        );
        hints.insert(
            "resident".to_string(),
            OwnedValue::try_from(Value::from(true)).unwrap(),
        );
        hints.insert(
            "transient".to_string(),
            OwnedValue::try_from(Value::from(true)).unwrap(),
        );
        let parsed = parse_hints(&hints);
        assert_eq!(parsed.urgency, Urgency::Critical);
        assert_eq!(parsed.desktop_entry.as_deref(), Some("org.rabbiz.Messages"));
        assert_eq!(parsed.category.as_deref(), Some("im.received"));
        assert!(parsed.resident);
        assert!(parsed.transient);

        let mut low_hints = HashMap::new();
        low_hints.insert(
            "urgency".to_string(),
            OwnedValue::try_from(Value::from(0u8)).unwrap(),
        );
        low_hints.insert(
            "desktop-entry".to_string(),
            OwnedValue::try_from(Value::from("   ")).unwrap(),
        );
        let low_parsed = parse_hints(&low_hints);
        assert_eq!(low_parsed.urgency, Urgency::Low);
        assert_eq!(low_parsed.desktop_entry, None);
        assert!(!low_parsed.resident);
        assert!(!low_parsed.transient);

        let default_parsed = parse_hints(&HashMap::new());
        assert_eq!(default_parsed.urgency, Urgency::Normal);
    }

    #[test]
    fn urgency_and_resident_expiration_rules() {
        let mut s = Store::default();

        // Critical notifications never auto-expire even with positive timeout.
        let critical = ParsedHints {
            urgency: Urgency::Critical,
            ..ParsedHints::default()
        };
        s.notify("alarm", 0, "wake", "", Vec::new(), critical, 10);
        assert!(s.entries[0].expires.is_none());
        assert_eq!(s.entries[0].urgency, Urgency::Critical);

        // Resident notifications never auto-expire even at Normal urgency.
        let resident = ParsedHints {
            urgency: Urgency::Normal,
            resident: true,
            ..ParsedHints::default()
        };
        s.notify("music", 0, "playing", "", Vec::new(), resident, 5000);
        assert!(s.entries[0].expires.is_none());
        assert!(s.entries[0].resident);

        // Normal non-resident with positive timeout expires.
        s.notify(
            "chat",
            0,
            "hi",
            "",
            Vec::new(),
            ParsedHints::default(),
            3000,
        );
        assert!(s.entries[0].expires.is_some());

        // Normal non-resident with timeout == 0 persists.
        s.notify(
            "chat",
            0,
            "sticky",
            "",
            Vec::new(),
            ParsedHints::default(),
            0,
        );
        assert!(s.entries[0].expires.is_none());

        // Low urgency with default (-1) timeout expires.
        let low = ParsedHints {
            urgency: Urgency::Low,
            ..ParsedHints::default()
        };
        s.notify("sync", 0, "done", "", Vec::new(), low, -1);
        assert!(s.entries[0].expires.is_some());
    }

    #[test]
    fn capabilities_advertise_actions_and_persistence() {
        let svc = Service {
            store: Arc::new(Mutex::new(Store::default())),
        };
        assert_eq!(
            svc.get_capabilities(),
            vec!["body", "actions", "persistence", "action-icons"]
        );
    }
}
