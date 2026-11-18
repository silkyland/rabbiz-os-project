use anyhow::{Context, Result};
use std::io::{BufReader, Read};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, TrySendError};
use std::thread::{self, JoinHandle};

pub const WIDTH: usize = 640;
pub const HEIGHT: usize = 480;
pub const PORTRAIT_WIDTH: usize = 360;

pub enum Framing {
    Full,
    Portrait,
}

impl Framing {
    fn width(&self) -> usize {
        match self {
            Self::Full => WIDTH,
            Self::Portrait => PORTRAIT_WIDTH,
        }
    }

    fn filter(&self) -> &'static str {
        match self {
            Self::Full => "fps=24,scale=640:480:force_original_aspect_ratio=decrease,pad=640:480:(ow-iw)/2:(oh-ih)/2,setsar=1",
            Self::Portrait => "fps=24,scale=360:480:force_original_aspect_ratio=increase,crop=360:480,setsar=1",
        }
    }
}

pub enum Source {
    Demo,
    File(PathBuf),
    Device(String),
}

impl Source {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Demo => "Test pattern",
            Self::File(_) => "Sample file",
            Self::Device(_) => "Camera input",
        }
    }

    fn input(&self, ffmpeg: &mut Command) -> Result<()> {
        match self {
            Self::Demo => {
                ffmpeg.args(["-re", "-f", "lavfi", "-i", "testsrc2=size=640x480:rate=24"]);
            }
            Self::File(path) => {
                ffmpeg
                    .args(["-re", "-stream_loop", "-1", "-i"])
                    .arg(path.canonicalize().context("open sample file")?);
            }
            Self::Device(device) => {
                #[cfg(target_os = "linux")]
                ffmpeg.args(["-f", "v4l2", "-i", device]);
                #[cfg(target_os = "macos")]
                ffmpeg.args([
                    "-f",
                    "avfoundation",
                    "-framerate",
                    "30",
                    "-i",
                    &format!("{device}:none"),
                ]);
                #[cfg(not(any(target_os = "linux", target_os = "macos")))]
                anyhow::bail!("Camera input is supported on Linux and macOS");
            }
        }
        Ok(())
    }
}

pub struct Preview {
    pub frames: Receiver<Vec<u8>>,
    child: Child,
    reader: Option<JoinHandle<()>>,
}

impl Preview {
    pub fn start(source: &Source, framing: Framing) -> Result<Self> {
        let mut ffmpeg = Command::new("ffmpeg");
        ffmpeg.args(["-hide_banner", "-loglevel", "error", "-nostdin"]);
        source.input(&mut ffmpeg)?;
        let mut child = ffmpeg
            .args([
                "-map",
                "0:v:0",
                "-vf",
                framing.filter(),
                "-c:v",
                "rawvideo",
                "-threads",
                "1",
                "-pix_fmt",
                "rgb24",
                "-f",
                "rawvideo",
                "pipe:1",
            ])
            .stdout(Stdio::piped())
            .spawn()
            .context("start FFmpeg (install ffmpeg and make it available on PATH)")?;
        let stdout = child
            .stdout
            .take()
            .context("FFmpeg did not open its output pipe")?;
        let (send, frames) = mpsc::sync_channel(1);
        let frame_bytes = framing.width() * HEIGHT * 3;
        let reader = thread::spawn(move || {
            let mut stdout = BufReader::new(stdout);
            let mut frame = vec![0; frame_bytes];
            while stdout.read_exact(&mut frame).is_ok() {
                match send.try_send(frame) {
                    Ok(()) => frame = vec![0; frame_bytes],
                    Err(TrySendError::Full(pending)) => frame = pending,
                    Err(TrySendError::Disconnected(_)) => break,
                }
            }
        });
        Ok(Self {
            frames,
            child,
            reader: Some(reader),
        })
    }
}

impl Drop for Preview {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}
