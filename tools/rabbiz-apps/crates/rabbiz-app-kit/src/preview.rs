//! A bounded cache for previews decoded off the UI thread.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, TryRecvError};

use de_api::Pixels;

pub struct PreviewCache {
    decode: fn(&Path) -> Option<Pixels>,
    capacity: usize,
    entries: HashMap<PathBuf, Option<Pixels>>,
    oldest_first: VecDeque<PathBuf>,
    pending: Option<(PathBuf, Receiver<Option<Pixels>>)>,
    discard_pending: bool,
}

impl PreviewCache {
    #[must_use]
    pub fn new(capacity: usize, decode: fn(&Path) -> Option<Pixels>) -> Self {
        Self {
            decode,
            capacity: capacity.max(1),
            entries: HashMap::new(),
            oldest_first: VecDeque::new(),
            pending: None,
            discard_pending: false,
        }
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.oldest_first.clear();
        self.discard_pending = self.pending.is_some();
    }

    pub fn get(&self, path: &Path) -> Option<&Pixels> {
        self.entries.get(path).and_then(Option::as_ref)
    }

    /// Start at most one decode at a time. Request in priority order and call
    /// again after polling a completed job; old directories add no backlog.
    pub fn request(&mut self, path: &Path) {
        if self.pending.is_some() || self.entries.contains_key(path) {
            return;
        }
        let source = path.to_owned();
        let decode = self.decode;
        let (tx, rx) = mpsc::sync_channel(1);
        let worker = std::thread::Builder::new().name("rabbiz-preview".into());
        if worker.spawn(move || { let _ = tx.send(decode(&source)); }).is_ok() {
            self.pending = Some((path.to_owned(), rx));
        }
    }

    pub fn is_loading(&self) -> bool {
        self.pending.is_some()
    }

    /// Returns true when a job finishes, even without an image. The caller can
    /// then draw fallbacks and request another visible preview.
    pub fn poll(&mut self) -> bool {
        let Some((_, rx)) = &self.pending else {
            return false;
        };
        let result = match rx.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return false,
            Err(TryRecvError::Disconnected) => None,
        };
        let (path, _) = self.pending.take().expect("polled preview job");
        if !std::mem::take(&mut self.discard_pending) {
            if self.entries.len() >= self.capacity {
                if let Some(oldest) = self.oldest_first.pop_front() {
                    self.entries.remove(&oldest);
                }
            }
            self.oldest_first.push_back(path.clone());
            self.entries.insert(path, result);
        }
        true
    }
}
