//! P15 Keyboard & 3rd-party input method settings subpage.

use de_api::{Point, Rect, Size};
use de_design::tokens::{self, Theme};
use de_paint::{text, Canvas};

use crate::chrome::{self, style};
use crate::i18n::Strings;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyboardOption {
    pub id: String,
    pub name: String,
    pub third_party: bool,
    pub installed: bool,
    pub active: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyboardHit {
    Back,
    ToggleThirdParty,
    Select(usize),
}

const ROW_H: i32 = 88;

fn back_rect() -> Rect {
    Rect::new(chrome::MARGIN - 8, chrome::STATUS_H + 16, 64, 56)
}

fn toggle_row_rect(size: Size) -> Rect {
    Rect::new(
        chrome::MARGIN,
        chrome::STATUS_H + 96,
        chrome::content_w(size),
        ROW_H,
    )
}

fn option_row_rect(size: Size, index: usize) -> Rect {
    let base_y = chrome::STATUS_H + 96 + ROW_H + 56;
    Rect::new(
        chrome::MARGIN,
        base_y + index as i32 * ROW_H,
        chrome::content_w(size),
        ROW_H,
    )
}

/// Hit-test the Keyboard settings subpage.
#[must_use]
pub fn hit_test(size: Size, options: &[KeyboardOption], pos: Point) -> Option<KeyboardHit> {
    if back_rect().contains(pos) {
        return Some(KeyboardHit::Back);
    }
    if toggle_row_rect(size).contains(pos) {
        return Some(KeyboardHit::ToggleThirdParty);
    }
    for (i, opt) in options.iter().enumerate() {
        let r = option_row_rect(size, i);
        if r.y + r.h > size.h - chrome::NAV_H - 16 {
            break;
        }
        if r.contains(pos) && opt.installed {
            return Some(KeyboardHit::Select(i));
        }
    }
    None
}

/// Render the Keyboard settings subpage.
#[must_use]
pub fn render(
    size: Size,
    s: &Strings,
    t: &Theme,
    clock: &str,
    allow_third_party: bool,
    options: &[KeyboardOption],
) -> de_api::Pixels {
    let mut c = Canvas::filled(size, t.bg);
    chrome::status_bar(&mut c, t, &chrome::StatusInfo::new(clock, ""));

    // Header: back arrow + title.
    let br = back_rect();
    chrome::draw_ink_centered(
        &mut c,
        "\u{e5c4}",
        &text::LabelStyle::icon(28.0, t.text),
        br,
    );
    chrome::draw(
        &mut c,
        s.get("settings.keyboard"),
        &style::h2(t.text),
        br.x + br.w + 4,
        chrome::STATUS_H + 24,
        None,
    );

    // 3rd-party keyboard policy toggle card.
    let tr = toggle_row_rect(size);
    c.fill_rounded_rect(tr, tokens::RADIUS_CARD, t.surface1);
    chrome::draw(
        &mut c,
        "Allow 3rd-party keyboards",
        &style::body(t.text),
        tr.x + 24,
        tr.y + 26,
        Some(tr.w - 120),
    );
    draw_switch(
        &mut c,
        t,
        tr.x + tr.w - 24 - 52,
        tr.y + (ROW_H - 32) / 2,
        allow_third_party,
    );

    // Section header.
    let sec_y = tr.y + ROW_H + 22;
    chrome::draw(
        &mut c,
        "ACTIVE KEYBOARD",
        &style::caption(t.text3),
        chrome::MARGIN + 8,
        sec_y,
        None,
    );

    // Keyboard list card.
    let visible_count = options
        .iter()
        .enumerate()
        .take_while(|(i, _)| {
            let r = option_row_rect(size, *i);
            r.y + r.h <= size.h - chrome::NAV_H - 16
        })
        .count();

    for (i, opt) in options.iter().take(visible_count).enumerate() {
        let r = option_row_rect(size, i);
        let first = i == 0;
        let last = i + 1 == visible_count;
        let reach = tokens::RADIUS_CARD as i32;
        if first && !last {
            c.fill_rounded_rect(
                Rect::new(r.x, r.y, r.w, r.h + reach),
                tokens::RADIUS_CARD,
                t.surface1,
            );
        } else if last && !first {
            c.fill_rounded_rect(
                Rect::new(r.x, r.y - reach, r.w, r.h + reach),
                tokens::RADIUS_CARD,
                t.surface1,
            );
        } else if first && last {
            c.fill_rounded_rect(r, tokens::RADIUS_CARD, t.surface1);
        } else {
            c.fill_rect(r, t.surface1);
        }
        if !first {
            c.fill_rect(Rect::new(r.x + 16, r.y, r.w - 32, 1), t.line);
        }

        let name_color = if opt.installed { t.text } else { t.text3 };
        let status = if opt.active {
            "Active"
        } else if opt.installed {
            if opt.third_party {
                "3rd-party"
            } else {
                "Built-in"
            }
        } else {
            "Not installed"
        };
        let status_color = if opt.active { t.accent } else { t.text3 };

        if let Some(px) = text::label(status, style::caption(status_color), Some(180)) {
            let (sw, sh) = (px.width() as i32, px.height() as i32);
            c.blit(
                &de_api::Pixels::new(Size::new(sw, sh), px.data().to_vec()),
                Point::new(r.x + r.w - 24 - sw, r.y + (ROW_H - sh) / 2),
            );
        }
        chrome::draw(
            &mut c,
            &opt.name,
            &style::body(name_color),
            r.x + 24,
            r.y + 26,
            Some(r.w - 220),
        );
    }

    chrome::nav_pill(&mut c, t);
    c.into_pixels()
}

fn draw_switch(c: &mut Canvas, t: &Theme, x: i32, y: i32, on: bool) {
    let track = Rect::new(x, y, 52, 32);
    let track_color = if on { t.accent } else { t.surface2 };
    c.fill_rounded_rect(track, 16.0, track_color);
    let knob_d = 24;
    let knob_x = if on { x + 52 - 4 - knob_d } else { x + 4 };
    let knob_y = y + 4;
    c.fill_rounded_rect(
        Rect::new(knob_x, knob_y, knob_d, knob_d),
        12.0,
        if on { t.text_on_accent } else { t.text3 },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hit_test_selects_only_installed_keyboards() {
        let size = Size::new(720, 1600);
        let opts = vec![
            KeyboardOption {
                id: "rabbiz-osk".into(),
                name: "Rabbiz Keyboard".into(),
                third_party: false,
                installed: true,
                active: true,
            },
            KeyboardOption {
                id: "squeekboard".into(),
                name: "Squeekboard".into(),
                third_party: true,
                installed: false,
                active: false,
            },
        ];
        let r0 = option_row_rect(size, 0);
        let r1 = option_row_rect(size, 1);
        assert_eq!(
            hit_test(size, &opts, Point::new(r0.x + 20, r0.y + 20)),
            Some(KeyboardHit::Select(0))
        );
        assert_eq!(
            hit_test(size, &opts, Point::new(r1.x + 20, r1.y + 20)),
            None
        );
        let tr = toggle_row_rect(size);
        assert_eq!(
            hit_test(size, &opts, Point::new(tr.x + 20, tr.y + 20)),
            Some(KeyboardHit::ToggleThirdParty)
        );
    }
}
