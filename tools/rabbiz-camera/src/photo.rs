use crate::preview::HEIGHT;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn next_path(folder: &Path) -> Result<PathBuf> {
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
    Ok(folder.join(format!("Rabbiz-{timestamp}.png")))
}

pub fn save(path: &Path, frame: &[u8], width: usize) -> Result<()> {
    let folder = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut temporary = tempfile::NamedTempFile::new_in(folder)
        .with_context(|| format!("open capture folder {}", folder.display()))?;
    let mut png = png::Encoder::new(temporary.as_file_mut(), width as u32, HEIGHT as u32);
    png.set_color(png::ColorType::Rgb);
    let mut writer = png.write_header().context("write PNG header")?;
    writer
        .write_image_data(frame)
        .context("encode still photo")?;
    writer.finish().context("finish still photo")?;
    temporary
        .persist_noclobber(path)
        .with_context(|| format!("save {}", path.display()))?;
    Ok(())
}
