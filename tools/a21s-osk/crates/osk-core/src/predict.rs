//! On-device next-word prediction (plan K6, tiers T0/T1 without a neural model).
//!
//! There is no shipped corpus: the seed list is a small hand-written set of very common
//! English and Thai words, and everything else is learned from what the user actually types
//! (counts + bigrams), persisted under `$XDG_STATE_HOME/a21s-osk/predictor.tsv`. That keeps
//! the data licence-free and the keyboard private by construction.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Hand-written seeds: no third-party corpus, so nothing to license or attribute.
const SEED_EN: &[&str] = &[
    "the", "and", "for", "you", "that", "with", "this", "have", "from", "not", "are", "but",
    "what", "all", "when", "can", "there", "use", "your", "how", "will", "about", "out", "who",
    "get", "which", "go", "me", "like", "time", "just", "know", "take", "people", "into", "year",
    "good", "some", "could", "them", "see", "other", "than", "then", "now", "look", "only",
    "come", "over", "think", "also", "back", "after", "use", "two", "how", "our", "work", "first",
    "well", "way", "even", "new", "want", "because", "any", "these", "give", "day", "most", "us",
    "hello", "thanks", "please", "sorry", "yes", "no", "okay", "ok", "good", "morning", "night",
    "today", "tomorrow", "yesterday", "home", "phone", "message", "email", "name", "help", "need",
    "make", "made", "said", "say", "going", "got", "let", "put", "keep", "thing", "things",
    "still", "again", "never", "always", "much", "many", "more", "very", "really", "right",
    "wrong", "sure", "maybe", "here", "where", "why", "who", "what", "when", "while", "before",
];

const SEED_TH: &[&str] = &[
    "ที่", "และ", "ของ", "ใน", "ได้", "ไม่", "ให้", "มี", "ไป", "มา", "จะ", "ทำ", "อยู่", "คน",
    "นี้", "นั้น", "กับ", "การ", "ความ", "วัน", "เวลา", "ครับ", "ค่ะ", "นะ", "เลย", "แล้ว",
    "กำลัง", "ต้อง", "อยาก", "ชอบ", "กิน", "ข้าว", "น้ำ", "บ้าน", "งาน", "เรียน", "หนังสือ",
    "โทรศัพท์", "ข้อความ", "ขอบคุณ", "ขอโทษ", "สวัสดี", "ราคา", "เงิน", "ร้าน", "อาหาร",
];

pub struct Predictor {
    /// word → times committed (seed words start at 1).
    unigrams: HashMap<String, u32>,
    /// (previous word, word) → times seen.
    bigrams: HashMap<(String, String), u32>,
    last_word: Option<String>,
    dirty: bool,
}

impl Predictor {
    pub fn new() -> Predictor {
        let mut unigrams = HashMap::new();
        for word in SEED_EN.iter().chain(SEED_TH.iter()) {
            unigrams.insert((*word).to_string(), 1);
        }
        Predictor {
            unigrams,
            bigrams: HashMap::new(),
            last_word: None,
            dirty: false,
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn mark_clean(&mut self) {
        self.dirty = false;
    }

    /// Feed text the user committed; words are split on whitespace. Thai has no spaces, so
    /// Thai words only become predictable after the user types them with a separator.
    pub fn learn(&mut self, text: &str) {
        let mut last = self.last_word.clone();
        for raw in text.split_whitespace() {
            let word: String = raw
                .trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase();
            if word.is_empty() {
                continue;
            }
            *self.unigrams.entry(word.clone()).or_insert(0) += 1;
            if let Some(prev) = last.take() {
                *self.bigrams.entry((prev, word.clone())).or_insert(0) += 1;
            }
            last = Some(word);
        }
        self.last_word = last;
        self.dirty = true;
    }

    /// Up to three suggestions for the current text: completions of the word being typed,
    /// then next-word candidates (bigram first, then unigram frequency).
    pub fn suggestions(&self, text: &str) -> Vec<String> {
        let trimmed = text.trim_end();
        let typing = !text.ends_with(' ') && !text.is_empty();
        let prefix: &str = if typing {
            trimmed.rsplit([' ', '\n']).next().unwrap_or("")
        } else {
            ""
        };
        let prefix_lower = prefix.to_lowercase();
        let previous = if typing {
            let before = trimmed[..trimmed.len().saturating_sub(prefix.len())].trim_end();
            before.rsplit([' ', '\n']).next().filter(|s| !s.is_empty())
        } else {
            trimmed.rsplit([' ', '\n']).next().filter(|s| !s.is_empty())
        };

        let mut out: Vec<String> = Vec::new();
        if !prefix_lower.is_empty() {
            let mut completions: Vec<(&String, u32)> = self
                .unigrams
                .iter()
                .filter(|(word, _)| word.starts_with(&prefix_lower) && word.len() > prefix_lower.len())
                .map(|(word, count)| (word, *count))
                .collect();
            // Shorter completions are usually the intended word; frequency breaks ties.
            completions.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.len().cmp(&b.0.len())).then(a.0.cmp(b.0)));
            for (word, _) in completions.into_iter().take(2) {
                push_unique(&mut out, word.clone(), prefix);
            }
        }

        if let Some(previous) = previous {
            let previous_lower = previous.to_lowercase();
            let mut next: Vec<(&String, u32)> = self
                .bigrams
                .iter()
                .filter(|((prev, _), _)| *prev == previous_lower)
                .map(|((_, word), count)| (word, *count))
                .collect();
            next.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
            for (word, _) in next.into_iter() {
                if out.len() >= 3 {
                    break;
                }
                push_unique(&mut out, word.clone(), prefix_lower.as_str());
            }
        }

        if out.len() < 3 && !prefix_lower.is_empty() {
            let mut frequent: Vec<(&String, u32)> = self
                .unigrams
                .iter()
                .filter(|(word, _)| word.starts_with(&prefix_lower) && word.len() > prefix_lower.len())
                .map(|(word, count)| (word, *count))
                .collect();
            frequent.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.len().cmp(&b.0.len())));
            for (word, _) in frequent.into_iter() {
                if out.len() >= 3 {
                    break;
                }
                push_unique(&mut out, word.clone(), prefix_lower.as_str());
            }
        }

        // Nothing to complete and no bigram: fall back to the most frequent seed words.
        if out.len() < 3 && prefix.is_empty() {
            let mut frequent: Vec<(&String, &u32)> = self.unigrams.iter().collect();
            frequent.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
            for (word, _) in frequent.into_iter() {
                if out.len() >= 3 {
                    break;
                }
                push_unique(&mut out, word.clone(), "");
            }
        }
        out.truncate(3);
        out
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let mut body = String::new();
        for (word, count) in &self.unigrams {
            body.push_str(&format!("u\t{word}\t{count}\n"));
        }
        for ((prev, word), count) in &self.bigrams {
            body.push_str(&format!("b\t{prev}\t{word}\t{count}\n"));
        }
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, body)?;
        std::fs::rename(&tmp, path)
    }

    pub fn load(path: &Path) -> Predictor {
        let mut predictor = Predictor::new();
        let Ok(body) = std::fs::read_to_string(path) else {
            return predictor;
        };
        for line in body.lines() {
            let parts: Vec<&str> = line.split('\t').collect();
            match parts.as_slice() {
                ["u", word, count] => {
                    if let Ok(count) = count.parse() {
                        predictor.unigrams.insert((*word).to_string(), count);
                    }
                }
                ["b", prev, word, count] => {
                    if let Ok(count) = count.parse() {
                        predictor.bigrams.insert(((*prev).to_string(), (*word).to_string()), count);
                    }
                }
                _ => {}
            }
        }
        predictor
    }
}

fn push_unique(out: &mut Vec<String>, word: String, prefix: &str) {
    if out.len() >= 3 || word == prefix || out.contains(&word) {
        return;
    }
    out.push(word);
}

/// State path shared by the keyboard process (Wayland client) and tests.
pub fn state_path() -> PathBuf {
    let base = std::env::var("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|_| std::env::var("HOME").map(|h| PathBuf::from(h).join(".local/state")))
        .unwrap_or_else(|_| PathBuf::from("/tmp"));
    base.join("a21s-osk/predictor.tsv")
}
