//! Fast background web search worker for `rabbiz-search`.
//!
//! Uses DuckDuckGo Lite HTML (direct URLs + snippets, zero browser startup)
//! with Wikipedia OpenSearch fallback for Thai/English instant results.

use std::process::Command;
use std::sync::{mpsc, Arc, Mutex};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebHit {
    pub title: String,
    pub url: String,
    pub domain: String,
    pub snippet: String,
}

#[derive(Debug, Clone, Default)]
pub struct WebState {
    pub query: String,
    pub loading: bool,
    pub hits: Vec<WebHit>,
    pub error: Option<String>,
    pub seq: u64,
}

pub struct WebWorker {
    tx: mpsc::Sender<(u64, String)>,
    state: Arc<Mutex<WebState>>,
    next_seq: u64,
}

impl WebWorker {
    pub fn start() -> Self {
        let (tx, rx) = mpsc::channel::<(u64, String)>();
        let state = Arc::new(Mutex::new(WebState::default()));
        let shared = Arc::clone(&state);
        let _ = std::thread::Builder::new()
            .name("rabbiz-search-web".into())
            .spawn(move || run_worker(rx, shared));
        Self {
            tx,
            state,
            next_seq: 0,
        }
    }

    pub fn search(&mut self, query: &str) {
        let q = query.trim().to_string();
        self.next_seq = self.next_seq.wrapping_add(1);
        let seq = self.next_seq;
        if let Ok(mut s) = self.state.lock() {
            s.query.clone_from(&q);
            s.seq = seq;
            s.error = None;
            if q.is_empty() {
                s.loading = false;
                s.hits.clear();
                return;
            }
            s.loading = true;
        }
        let _ = self.tx.send((seq, q));
    }

    pub fn clear(&mut self) {
        self.search("");
    }

    pub fn snapshot(&self) -> WebState {
        self.state
            .lock()
            .map(|s| s.clone())
            .unwrap_or_default()
    }
}

fn run_worker(rx: mpsc::Receiver<(u64, String)>, shared: Arc<Mutex<WebState>>) {
    while let Ok(first) = rx.recv() {
        // Coalesce burst of queries to the latest one.
        let mut latest = first;
        while let Ok(next) = rx.try_recv() {
            latest = next;
        }
        let (seq, query) = latest;
        if query.is_empty() {
            continue;
        }
        let result = fetch_web_hits(&query);
        if let Ok(mut s) = shared.lock() {
            if s.seq != seq {
                continue;
            }
            s.loading = false;
            match result {
                Ok(hits) => {
                    s.hits = hits;
                    s.error = None;
                }
                Err(err) => {
                    s.hits.clear();
                    s.error = Some(err);
                }
            }
        }
    }
}

pub fn fetch_web_hits(query: &str) -> Result<Vec<WebHit>, String> {
    let encoded = percent_encode(query);
    let out = Command::new("curl")
        .args([
            "-sS",
            "--max-time",
            "6",
            "-A",
            "RabbizSearch/1.0",
            "-d",
            &format!("q={encoded}"),
            "https://lite.duckduckgo.com/lite/",
        ])
        .output()
        .map_err(|e| format!("Network unavailable ({e})"))?;

    if !out.status.success() {
        return Err("Unable to reach search service".into());
    }
    let html = String::from_utf8_lossy(&out.stdout);
    let mut hits = parse_ddg_lite_html(&html);
    if hits.is_empty() {
        hits = fetch_wikipedia_fallback(query).unwrap_or_default();
    }
    hits.truncate(10);
    Ok(hits)
}

fn fetch_wikipedia_fallback(query: &str) -> Option<Vec<WebHit>> {
    let has_thai = query.chars().any(|c| ('\u{0E00}'..='\u{0E7F}').contains(&c));
    let host = if has_thai {
        "th.wikipedia.org"
    } else {
        "en.wikipedia.org"
    };
    let url = format!(
        "https://{host}/w/api.php?action=opensearch&search={}&limit=6&format=json",
        percent_encode(query)
    );
    let out = Command::new("curl")
        .args(["-sS", "--max-time", "4", "-A", "RabbizSearch/1.0", &url])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    let titles = v.get(1)?.as_array()?;
    let descs = v.get(2).and_then(|d| d.as_array());
    let urls = v.get(3)?.as_array()?;
    let mut out_hits = Vec::new();
    for (i, t) in titles.iter().enumerate() {
        let title = t.as_str().unwrap_or_default().trim();
        let link = urls.get(i).and_then(|u| u.as_str()).unwrap_or_default();
        if title.is_empty() || link.is_empty() {
            continue;
        }
        let snippet = descs
            .and_then(|d| d.get(i))
            .and_then(|s| s.as_str())
            .unwrap_or("Wikipedia article")
            .trim()
            .to_string();
        out_hits.push(WebHit {
            title: title.to_string(),
            url: link.to_string(),
            domain: host.to_string(),
            snippet,
        });
    }
    Some(out_hits)
}

fn percent_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len() * 3);
    for &b in input.trim().as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            b' ' => out.push('+'),
            _ => {
                const HEX: &[u8; 16] = b"0123456789ABCDEF";
                out.push('%');
                out.push(HEX[(b >> 4) as usize] as char);
                out.push(HEX[(b & 0x0F) as usize] as char);
            }
        }
    }
    out
}

/// Parse DuckDuckGo Lite HTML results into structured native cards.
pub fn parse_ddg_lite_html(html: &str) -> Vec<WebHit> {
    let mut hits = Vec::new();
    let mut rest = html;
    while let Some(pos) = rest.find("class='result-link'") {
        let before = &rest[..pos];
        let a_start = before.rfind("<a ").unwrap_or(0);
        let tag_head = &rest[a_start..pos];
        let url = extract_attr(tag_head, "href").unwrap_or_default();
        let after_cls = &rest[pos + "class='result-link'".len()..];
        let Some(gt) = after_cls.find('>') else {
            break;
        };
        let after_gt = &after_cls[gt + 1..];
        let Some(a_end) = after_gt.find("</a>") else {
            break;
        };
        let title = clean_html_text(&after_gt[..a_end]);
        rest = &after_gt[a_end + 4..];

        // Look ahead for the snippet & display domain before the next result-link.
        let next_link = rest.find("class='result-link'").unwrap_or(rest.len());
        let chunk = &rest[..next_link];

        let snippet = chunk
            .find("class='result-snippet'")
            .and_then(|s| {
                let s_after = &chunk[s..];
                let s_gt = s_after.find('>')?;
                let s_body = &s_after[s_gt + 1..];
                let s_end = s_body.find("</td>")?;
                Some(clean_html_text(&s_body[..s_end]))
            })
            .unwrap_or_default();

        let domain = chunk
            .find("class='link-text'")
            .and_then(|d| {
                let d_after = &chunk[d..];
                let d_gt = d_after.find('>')?;
                let d_body = &d_after[d_gt + 1..];
                let d_end = d_body.find("</span>")?;
                Some(clean_html_text(&d_body[..d_end]))
            })
            .filter(|d| !d.is_empty())
            .unwrap_or_else(|| domain_of(&url));

        if !title.is_empty() && (url.starts_with("http://") || url.starts_with("https://")) {
            hits.push(WebHit {
                title,
                url: decode_html_entities(&url),
                domain,
                snippet,
            });
        }
    }
    hits
}

fn extract_attr(tag: &str, attr: &str) -> Option<String> {
    let double = format!("{attr}=\"");
    if let Some(i) = tag.find(&double) {
        let s = &tag[i + double.len()..];
        let end = s.find('"')?;
        return Some(s[..end].to_string());
    }
    let single = format!("{attr}='");
    if let Some(i) = tag.find(&single) {
        let s = &tag[i + single.len()..];
        let end = s.find('\'')?;
        return Some(s[..end].to_string());
    }
    None
}

fn domain_of(url: &str) -> String {
    let stripped = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url);
    stripped.split('/').next().unwrap_or(stripped).to_string()
}

fn clean_html_text(raw: &str) -> String {
    let mut no_tags = String::with_capacity(raw.len());
    let mut in_tag = false;
    for ch in raw.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => no_tags.push(ch),
            _ => {}
        }
    }
    let decoded = decode_html_entities(&no_tags);
    decoded.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn decode_html_entities(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let after = &rest[amp + 1..];
        if let Some(semi) = after.find(';').filter(|&i| i <= 10) {
            let ent = &after[..semi];
            let decoded = match ent {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" | "#39" | "#x27" | "#x27;" => Some('\''),
                "nbsp" => Some(' '),
                _ if ent.starts_with("#x") || ent.starts_with("#X") => {
                    u32::from_str_radix(&ent[2..], 16)
                        .ok()
                        .and_then(char::from_u32)
                }
                _ if ent.starts_with('#') => ent[1..].parse::<u32>().ok().and_then(char::from_u32),
                _ => None,
            };
            if let Some(ch) = decoded {
                out.push(ch);
                rest = &after[semi + 1..];
                continue;
            }
        }
        out.push('&');
        rest = after;
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ddg_lite_results_and_decodes_entities() {
        let html = r#"
            <tr><td>
              <a rel="nofollow" href="https://github.com/Smithay/smithay" class='result-link'>Smithay &amp; Wayland</a>
            </td></tr>
            <tr><td class='result-snippet'>
              A <b>smithy</b> for rusty Wayland compositors &#x27;fast&#x27;.
            </td></tr>
            <tr><td><span class='link-text'>github.com/Smithay/smithay</span></td></tr>
        "#;
        let hits = parse_ddg_lite_html(html);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].title, "Smithay & Wayland");
        assert_eq!(hits[0].url, "https://github.com/Smithay/smithay");
        assert_eq!(hits[0].domain, "github.com/Smithay/smithay");
        assert_eq!(
            hits[0].snippet,
            "A smithy for rusty Wayland compositors 'fast'."
        );
    }
}
