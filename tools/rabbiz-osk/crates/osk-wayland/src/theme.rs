use osk_render::Theme;

pub(crate) fn load_theme(path: &std::path::Path) -> Option<Theme> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut dark = true;
    let mut accent: Option<(u8, u8, u8)> = None;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim().trim_matches('"');
        match key {
            "mode" => dark = value != "light",
            "accent" => accent = parse_hex(value),
            _ => {}
        }
    }
    let mut theme = if dark { Theme::dark() } else { Theme::light() };
    if let Some(accent) = accent {
        theme.accent = accent;
    }
    Some(theme)
}

fn parse_hex(value: &str) -> Option<(u8, u8, u8)> {
    let hex = value.strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some((r, g, b))
}

pub(crate) fn theme_path() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/home/rabbiz".into());
    std::path::Path::new(&home).join(".config/rabbiz-osk/theme.toml")
}
