//! Status bar scene (standalone): the 56 px top bar by itself over the
//! app canvas — exercises every `StatusInfo` element (clock, privacy dots,
//! radio glyphs, battery) for the golden diff.

use de_api::Size;
use de_design::tokens::Theme;
use de_paint::Canvas;

use crate::chrome::{self, StatusInfo};
use crate::i18n::Strings;

pub fn render(size: Size, _s: &Strings, t: &Theme, status: &StatusInfo) -> de_api::Pixels {
    let mut c = Canvas::filled(size, t.bg);
    chrome::status_bar(&mut c, t, status);
    chrome::nav_pill(&mut c, t);
    c.into_pixels()
}
