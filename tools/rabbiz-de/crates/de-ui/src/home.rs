//! P2 home skeleton (§7.18): status bar, hero widget 672×184 @ y=88, 4×4
//! icon grid from y=312, search pill @ y=1242, dock @ y=1392, nav pill.

use de_api::{Point, Rect, Size};
use de_design::tokens::{self, Theme};
use de_paint::{text, Canvas};

use crate::chrome::{self, style};
use crate::i18n::Strings;

/// Grid cell size per spec.
const CELL_W: i32 = 168;
const CELL_H: i32 = 160;
const GRID_Y: i32 = 312;
const SEARCH_Y: i32 = 1242;
const DOCK_Y: i32 = 1392;

/// Render the home screen with `apps` (desktop ids + labels) on page 0.
pub fn render(size: Size, s: &Strings, t: &Theme, clock: &str, battery: &str, apps: &[(String, String)]) -> de_api::Pixels {
    let mut c = Canvas::filled(size, t.bg);
    chrome::status_bar(&mut c, s, t, clock, battery);
    // Hero stack (Smart Widget) 672×184.
    let hero = Rect::new(chrome::MARGIN, 88, chrome::content_w(size), 184);
    c.fill_rounded_rect(hero, tokens::RADIUS_CARD, t.glass);
    chrome::draw(&mut c, clock, &style::h2(t.text), hero.x + 24, hero.y + 24, None);
    // 4×4 grid.
    let grid_w = CELL_W * 4;
    let gx = (size.w - grid_w) / 2;
    for (i, (_id, label)) in apps.iter().take(16).enumerate() {
        let (col, row) = ((i % 4) as i32, (i / 4) as i32);
        let (cx, cy) = (gx + col * CELL_W, GRID_Y + row * CELL_H);
        draw_icon(&mut c, t, label, cx, cy);
    }
    // Search pill 672×64.
    let sp = Rect::new(chrome::MARGIN, SEARCH_Y.min(size.h - 420), chrome::content_w(size), 64);
    c.fill_rounded_rect(sp, tokens::RADIUS_PILL, t.surface2);
    chrome::draw(&mut c, s.get("home.search_hint"), &style::label(t.text3), sp.x + 28, sp.y + 18, None);
    // Dock 672×120 with four 88 px icons.
    let dock = Rect::new(chrome::MARGIN, DOCK_Y.min(size.h - 240), chrome::content_w(size), 120);
    c.fill_rounded_rect(dock, tokens::RADIUS_SHEET, t.glass);
    for i in 0..4 {
        let ix = dock.x + 40 + i as i32 * ((dock.w - 80 - 88) / 3);
        icon_tile(&mut c, t, Rect::new(ix, dock.y + 16, 88, 88));
    }
    chrome::nav_pill(&mut c, t);
    c.into_pixels()
}

fn icon_tile(c: &mut Canvas, t: &Theme, r: Rect) {
    c.fill_rounded_rect(r, tokens::ICON_TILE_RADIUS * (r.w as f32 / 96.0), t.icon_tile);
}

fn draw_icon(c: &mut Canvas, t: &Theme, label: &str, x: i32, y: i32) {
    icon_tile(c, t, Rect::new(x + (CELL_W - 96) / 2, y + 8, 96, 96));
    if let Some(px) = text::label_clamped(label, style::label(t.text), 150, 2) {
        let (w, h) = (px.width() as i32, px.height() as i32);
        c.blit(&de_api::Pixels::new(Size::new(w, h), px.data().to_vec()), Point::new(x + (CELL_W - w) / 2, y + 108));
    }
}
