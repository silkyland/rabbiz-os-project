//! Server-side window decorations (SSD titlebar + resize hit-testing) for Desktop Mode.

use de_api::{Pixels, Point, Rect, Size};
use de_design::tokens::Theme;
use de_paint::{text::LabelStyle, Canvas};

use crate::chrome;

/// Height of the compositor-drawn window titlebar in logical px.
pub const TITLEBAR_H: i32 = 36;
/// Width/height of each window control button (Minimize / Maximize / Close).
pub const BUTTON_SIZE: i32 = 36;
/// Outer resize grab margin around floating windows.
pub const RESIZE_MARGIN: i32 = 8;

/// Edges/corners that can be dragged to resize a floating window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResizeEdge {
    Top,
    Bottom,
    Left,
    Right,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

/// What a pointer press on a window titlebar hit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TitlebarHit {
    Move,
    Minimize,
    Maximize,
    Close,
}

/// Visual state for rendering one window's titlebar.
#[derive(Debug, Clone, Copy)]
pub struct WindowChromeModel<'a> {
    pub title: &'a str,
    pub focused: bool,
    pub maximized: bool,
    pub hovered: Option<TitlebarHit>,
}

/// Geometry of the 3 window buttons inside a `titlebar_rect`.
#[must_use]
pub fn button_rects(titlebar_rect: Rect) -> [Rect; 3] {
    let close = Rect::new(
        titlebar_rect.x + titlebar_rect.w - BUTTON_SIZE,
        titlebar_rect.y,
        BUTTON_SIZE,
        titlebar_rect.h,
    );
    let max = Rect::new(
        close.x - BUTTON_SIZE,
        titlebar_rect.y,
        BUTTON_SIZE,
        titlebar_rect.h,
    );
    let min = Rect::new(
        max.x - BUTTON_SIZE,
        titlebar_rect.y,
        BUTTON_SIZE,
        titlebar_rect.h,
    );
    [min, max, close]
}

/// Hit-test a pointer position inside `titlebar_rect`.
#[must_use]
pub fn hit_titlebar(titlebar_rect: Rect, pos: Point) -> Option<TitlebarHit> {
    if !titlebar_rect.contains(pos) {
        return None;
    }
    let [min, max, close] = button_rects(titlebar_rect);
    if close.contains(pos) {
        Some(TitlebarHit::Close)
    } else if max.contains(pos) {
        Some(TitlebarHit::Maximize)
    } else if min.contains(pos) {
        Some(TitlebarHit::Minimize)
    } else {
        Some(TitlebarHit::Move)
    }
}

/// Hit-test the outer border of `win_rect` (excluding its interior) for edge/corner resize.
#[must_use]
pub fn hit_resize_edge(win_rect: Rect, pos: Point, margin: i32) -> Option<ResizeEdge> {
    let outer = Rect::new(
        win_rect.x - margin,
        win_rect.y - margin,
        win_rect.w + margin * 2,
        win_rect.h + margin * 2,
    );
    if !outer.contains(pos) || win_rect.contains(pos) {
        return None;
    }
    let left = pos.x < win_rect.x;
    let right = pos.x >= win_rect.x + win_rect.w;
    let top = pos.y < win_rect.y;
    let bottom = pos.y >= win_rect.y + win_rect.h;
    match (top, bottom, left, right) {
        (true, false, true, false) => Some(ResizeEdge::TopLeft),
        (true, false, false, true) => Some(ResizeEdge::TopRight),
        (false, true, true, false) => Some(ResizeEdge::BottomLeft),
        (false, true, false, true) => Some(ResizeEdge::BottomRight),
        (true, false, false, false) => Some(ResizeEdge::Top),
        (false, true, false, false) => Some(ResizeEdge::Bottom),
        (false, false, true, false) => Some(ResizeEdge::Left),
        (false, false, false, true) => Some(ResizeEdge::Right),
        _ => None,
    }
}

/// Render a premultiplied RGBA SSD titlebar of `width × TITLEBAR_H`.
#[must_use]
pub fn render_titlebar(width: i32, theme: &Theme, model: &WindowChromeModel<'_>) -> Pixels {
    let w = width.max(BUTTON_SIZE * 3 + 32);
    let mut c = Canvas::new(Size::new(w, TITLEBAR_H));
    let bg = if model.focused {
        theme.surface2
    } else {
        theme.surface1
    };
    let rect = Rect::new(0, 0, w, TITLEBAR_H);
    c.fill_rounded_rect(rect, 12.0, bg);
    // Square off bottom corners so it sits flush on top of the client surface.
    c.fill_rect(Rect::new(0, TITLEBAR_H - 12, w, 12), bg);

    let title_w = (w - BUTTON_SIZE * 3 - 24).max(16);
    let label = if model.title.is_empty() {
        "Application"
    } else {
        model.title
    };
    let ink = if model.focused { theme.text } else { theme.text2 };
    let title_style = LabelStyle {
        size: 14.0,
        line_height: 18.0,
        color: ink,
        ..LabelStyle::default()
    };
    chrome::draw(&mut c, label, &title_style, 14, 9, Some(title_w));

    let [min_r, max_r, close_r] = button_rects(rect);
    let draw_btn = |c: &mut Canvas, r: Rect, glyph: &str, hit: TitlebarHit, is_close: bool| {
        if model.hovered == Some(hit) {
            let hover_bg = if is_close {
                theme.danger
            } else {
                theme.surface3
            };
            c.fill_rounded_rect(
                Rect::new(r.x + 4, r.y + 4, r.w - 8, r.h - 8),
                8.0,
                hover_bg,
            );
        }
        let btn_ink = if is_close && model.hovered == Some(hit) {
            theme.text_on_accent
        } else {
            ink
        };
        let btn_style = LabelStyle {
            size: 14.0,
            line_height: 18.0,
            color: btn_ink,
            ..LabelStyle::default()
        };
        chrome::draw_ink_centered(c, glyph, &btn_style, r);
    };

    draw_btn(&mut c, min_r, "—", TitlebarHit::Minimize, false);
    draw_btn(
        &mut c,
        max_r,
        if model.maximized { "❐" } else { "□" },
        TitlebarHit::Maximize,
        false,
    );
    draw_btn(&mut c, close_r, "✕", TitlebarHit::Close, true);

    c.into_pixels()
}

#[cfg(test)]
mod tests {
    use super::*;
    use de_design::tokens::LIGHT;

    #[test]
    fn titlebar_hit_distinguishes_move_and_buttons() {
        let r = Rect::new(100, 80, 600, TITLEBAR_H);
        assert_eq!(hit_titlebar(r, Point::new(150, 96)), Some(TitlebarHit::Move));
        assert_eq!(
            hit_titlebar(r, Point::new(700 - 10, 96)),
            Some(TitlebarHit::Close)
        );
        assert_eq!(
            hit_titlebar(r, Point::new(700 - BUTTON_SIZE - 10, 96)),
            Some(TitlebarHit::Maximize)
        );
        assert_eq!(
            hit_titlebar(r, Point::new(700 - BUTTON_SIZE * 2 - 10, 96)),
            Some(TitlebarHit::Minimize)
        );
        assert_eq!(hit_titlebar(r, Point::new(50, 96)), None);
    }

    #[test]
    fn resize_edge_detects_borders_and_corners() {
        let win = Rect::new(100, 100, 400, 300);
        assert_eq!(
            hit_resize_edge(win, Point::new(96, 96), RESIZE_MARGIN),
            Some(ResizeEdge::TopLeft)
        );
        assert_eq!(
            hit_resize_edge(win, Point::new(504, 250), RESIZE_MARGIN),
            Some(ResizeEdge::Right)
        );
        assert_eq!(
            hit_resize_edge(win, Point::new(250, 250), RESIZE_MARGIN),
            None
        );
    }

    #[test]
    fn render_titlebar_outputs_expected_dimensions() {
        let px = render_titlebar(
            640,
            &LIGHT,
            &WindowChromeModel {
                title: "Rabbiz Notes",
                focused: true,
                maximized: false,
                hovered: Some(TitlebarHit::Close),
            },
        );
        assert_eq!(px.size, Size::new(640, TITLEBAR_H));
        assert!(px.data.iter().any(|&b| b != 0));
    }
}
