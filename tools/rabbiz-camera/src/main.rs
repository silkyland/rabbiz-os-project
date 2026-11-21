mod photo;
mod preview;
mod video;
mod viewfinder;
mod window;

use anyhow::{Context, Result};
use clap::Parser;
use preview::{Framing, Preview, Source};
use std::path::PathBuf;
use std::time::Duration;

#[derive(Parser)]
#[command(about = "Preview a camera and capture photos or video")]
struct Options {
    /// Loop a sample image or video. The default source is an animated test pattern.
    #[arg(long, conflicts_with = "device")]
    input: Option<PathBuf>,
    /// Linux V4L2 device path or macOS AVFoundation video device index.
    #[arg(long)]
    device: Option<String>,
    /// Second camera device for the Flip button.
    #[arg(long, conflicts_with = "alternate_input")]
    alternate_device: Option<String>,
    /// Second sample file for testing the Flip button.
    #[arg(long)]
    alternate_input: Option<PathBuf>,
    /// Folder for shutter captures.
    #[arg(long, default_value = ".")]
    output_dir: PathBuf,
    /// Save one frame as PNG and exit without opening a window.
    #[arg(long, value_name = "PNG")]
    capture: Option<PathBuf>,
}

fn main() -> Result<()> {
    let options = Options::parse();
    let source = match (options.input, options.device) {
        (Some(path), _) => Source::File(path),
        (_, Some(device)) => Source::Device(device),
        _ => Source::Demo,
    };

    let framing = if options.capture.is_some() {
        Framing::Full
    } else {
        Framing::Portrait
    };
    let preview = Preview::start(&source, framing)?;
    if let Some(path) = options.capture {
        let frame = preview
            .frames
            .recv_timeout(Duration::from_secs(10))
            .context("no complete frame received from FFmpeg")?;
        photo::save(&path, &frame, preview::WIDTH)?;
        println!("{}", path.display());
        Ok(())
    } else {
        let alternate = options
            .alternate_device
            .map(Source::Device)
            .or_else(|| options.alternate_input.map(Source::File));
        window::run(preview, source, alternate, &options.output_dir)
    }
}
