use crate::preview::{HEIGHT, PORTRAIT_WIDTH};
use anyhow::{ensure, Context, Result};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, SyncSender, TrySendError};
use std::thread::{self, JoinHandle};

pub struct Recording {
    frames: SyncSender<Vec<u8>>,
    encoder: JoinHandle<Result<PathBuf>>,
}

impl Recording {
    pub fn start(folder: &Path, first: &[u8]) -> Result<Self> {
        let path = crate::photo::next_path(folder)?.with_extension("mp4");
        let pending =
            tempfile::NamedTempFile::new_in(folder).context("open video output folder")?;
        let mut child = Command::new("ffmpeg")
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-nostdin",
                "-y",
                "-f",
                "rawvideo",
                "-pixel_format",
                "rgb24",
                "-video_size",
                &format!("{PORTRAIT_WIDTH}x{HEIGHT}"),
                "-framerate",
                "24",
                "-i",
                "pipe:0",
                "-an",
                "-c:v",
                "libx264",
                "-preset",
                "ultrafast",
                "-pix_fmt",
                "yuv420p",
                "-movflags",
                "+faststart",
                "-f",
                "mp4",
            ])
            .arg(pending.path())
            .stdin(Stdio::piped())
            .spawn()
            .context("start FFmpeg video encoder")?;
        let mut stdin = child.stdin.take().context("open FFmpeg video input pipe")?;
        let (frames, queue) = mpsc::sync_channel::<Vec<u8>>(12);
        frames
            .send(first.to_vec())
            .context("queue first video frame")?;
        let encoder = thread::spawn(move || {
            let encoded = queue
                .into_iter()
                .try_for_each(|frame| stdin.write_all(&frame));
            drop(stdin);
            if encoded.is_err() {
                let _ = child.kill();
            }
            let status = child.wait().context("wait for FFmpeg video encoder")?;
            encoded.context("write video frame")?;
            ensure!(
                status.success(),
                "FFmpeg video encoder exited with {status}"
            );
            pending
                .persist_noclobber(&path)
                .with_context(|| format!("save {}", path.display()))?;
            Ok(path)
        });

        Ok(Self { frames, encoder })
    }

    pub fn add(&self, frame: &[u8]) -> Result<()> {
        match self.frames.try_send(frame.to_vec()) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(_)) => anyhow::bail!("video encoder could not keep up"),
            Err(TrySendError::Disconnected(_)) => anyhow::bail!("video encoder stopped"),
        }
    }

    pub fn finish(self) -> Result<PathBuf> {
        drop(self.frames);
        self.encoder
            .join()
            .map_err(|_| anyhow::anyhow!("video encoder worker panicked"))?
    }
}
