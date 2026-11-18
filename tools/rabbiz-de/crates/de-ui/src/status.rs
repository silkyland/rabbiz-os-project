//! Status bar skeleton (standalone scene): the 56 px top bar by itself over
//! the app canvas — used to verify cutout insets and clock/battery metrics.

use de_api::Size;
use de_design::tokens::Theme;
use de_paint::Canvas;

use crate::chrome;
use crate::i18n::Strings;

pub fn render(size: Size, s: &Strings, t: &Theme, clock: &str, battery: &str) -> de_api::Pixels {
    let mut c = Canvas::filled(size, t.bg);
    chrome::status_bar(&mut c, s, t, clock, battery);
    chrome::nav_pill(&mut c, t);
    c.into_pixels()
}
