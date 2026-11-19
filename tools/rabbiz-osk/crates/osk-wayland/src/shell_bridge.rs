use std::io::{Read, Write};
use std::os::unix::net::UnixStream;

/// Optional compositor bridge for shell-owned text inputs (`Screen::Search`, `Screen::Drawer`).
pub(crate) struct ShellImClient {
    stream: UnixStream,
    buf: String,
}

impl ShellImClient {
    pub(crate) fn connect_from_env() -> Option<Self> {
        let path = std::env::var("RABBIZ_OSK_IM_SOCK").ok()?;
        let stream = UnixStream::connect(path).ok()?;
        let _ = stream.set_nonblocking(true);
        Some(Self {
            stream,
            buf: String::new(),
        })
    }

    pub(crate) fn poll_events(&mut self) -> Vec<String> {
        let mut tmp = [0u8; 512];
        loop {
            match self.stream.read(&mut tmp) {
                Ok(0) => break,
                Ok(n) => self.buf.push_str(&String::from_utf8_lossy(&tmp[..n])),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(_) => break,
            }
        }
        let mut out = Vec::new();
        while let Some(nl) = self.buf.find('\n') {
            let line = self.buf[..nl].trim_end_matches('\r').to_string();
            self.buf.drain(..=nl);
            if !line.is_empty() {
                out.push(line);
            }
        }
        out
    }

    pub(crate) fn send_line(&mut self, line: &str) {
        let _ = self.stream.write_all(line.as_bytes());
        let _ = self.stream.write_all(b"\n");
        let _ = self.stream.flush();
    }
}
