//! P16 screenshot skeleton: 160×96 thumbnail toast bottom-right for 3 s.

use de_api::{Rect, Size};
use de_design::tokens::{self, Theme};
use de_paint::Canvas;

use crate::chrome;
use crate::i18n::Strings;

pub fn render(size: Size, s: &Strings, t: &Theme) -> de_api::Pixels {
    let mut c = Canvas::filled(size, t.bg);
    chrome::status_bar(&mut c, t, &chrome::StatusInfo::default());
    let toast = Rect::new(size.w - 160 - 24, size.h - 96 - 120, 160, 96);
    c.fill_rounded_rect(toast, tokens::RADIUS_CHIP, t.surface3);
    c.fill_rect(Rect::new(toast.x + 2, toast.y + 2, toast.w - 4, toast.h - 4), t.surface2);
    let _ = s;
    c.into_pixels()
}
