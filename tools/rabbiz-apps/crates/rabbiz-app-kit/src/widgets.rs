//! Reusable Material Design 3 widgets built on `crate::ui` and `de_paint::Canvas`.

use de_api::{Pixels, Point, Rect, Size};
use de_design::tokens::{self, Theme};
use de_paint::{text, Canvas};

use crate::{icons, ui};

/// Rasterize and blit a Material icon with its ink bounds centered on `(cx, cy)`.
fn draw_icon_centered(
    c: &mut Canvas,
    codepoint: &str,
    size: f32,
    color: [u8; 4],
    cx: i32,
    cy: i32,
) {
    if codepoint.is_empty() {
        return;
    }
    if let Some(px) = text::icon(codepoint, size, color) {
        let (x0, y0, x1, y1) =
            text::ink_bounds(&px).unwrap_or((0, 0, px.width() as i32, px.height() as i32));
        c.blit(
            &Pixels::new(
                Size::new(px.width() as i32, px.height() as i32),
                px.data().to_vec(),
            ),
            Point::new(cx - (x0 + x1) / 2, cy - (y0 + y1) / 2),
        );
    }
}

/// Draw a single-line ellipsized label vertically centered inside `r`, left-aligned at `r.x`.
fn draw_single_line_vcenter(c: &mut Canvas, s: &str, st: &text::LabelStyle, r: Rect) {
    if s.is_empty() || r.w <= 0 || r.h <= 0 {
        return;
    }
    if let Some(px) = text::label_clamped(s, *st, r.w.max(8), 1) {
        let w = px.width() as i32;
        let h = px.height() as i32;
        c.blit(
            &Pixels::new(Size::new(w, h), px.data().to_vec()),
            Point::new(r.x, r.y + (r.h - h).max(0) / 2),
        );
    }
}

// ---------------------------------------------------------------------------
// TopAppBar
// ---------------------------------------------------------------------------

/// Hit target within a [`TopAppBar`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TopBarHit {
    Back,
    Action,
}

/// Material 3 top app bar with optional back navigation and trailing action button.
pub struct TopAppBar;

impl TopAppBar {
    pub const HEIGHT: i32 = 64;

    const BTN_SIZE: i32 = 48;
    const EDGE_PAD: i32 = 12;

    #[must_use]
    pub fn layout(size: Size, top_safe: i32) -> Rect {
        Rect::new(0, top_safe, size.w.max(0), Self::HEIGHT)
    }

    #[must_use]
    pub fn back_rect(bar: Rect) -> Rect {
        Rect::new(
            bar.x + Self::EDGE_PAD,
            bar.y + (bar.h - Self::BTN_SIZE) / 2,
            Self::BTN_SIZE,
            Self::BTN_SIZE,
        )
    }

    #[must_use]
    pub fn action_rect(bar: Rect) -> Rect {
        Rect::new(
            bar.right() - Self::EDGE_PAD - Self::BTN_SIZE,
            bar.y + (bar.h - Self::BTN_SIZE) / 2,
            Self::BTN_SIZE,
            Self::BTN_SIZE,
        )
    }

    pub fn render(
        c: &mut Canvas,
        bar: Rect,
        theme: &Theme,
        title: &str,
        has_back: bool,
        action_icon: Option<&str>,
    ) {
        if bar.is_empty() {
            return;
        }
        c.fill_rect(bar, theme.bg);
        c.fill_rect(Rect::new(bar.x, bar.bottom() - 1, bar.w, 1), theme.line);

        let back = Self::back_rect(bar);
        let action = Self::action_rect(bar);

        if has_back {
            draw_icon_centered(
                c,
                icons::BACK,
                28.0,
                theme.text,
                back.x + back.w / 2,
                back.y + back.h / 2,
            );
        }

        if let Some(icon) = action_icon {
            draw_icon_centered(
                c,
                icon,
                28.0,
                theme.text,
                action.x + action.w / 2,
                action.y + action.h / 2,
            );
        }

        let left_x = if has_back {
            back.right() + 8
        } else {
            bar.x + ui::MARGIN
        };
        let right_x = if action_icon.is_some() {
            action.x - 8
        } else {
            bar.right() - ui::MARGIN
        };
        let title_rect = Rect::new(left_x, bar.y, (right_x - left_x).max(0), bar.h);
        draw_single_line_vcenter(c, title, &ui::styled(26.0, 600, theme.text), title_rect);
    }

    #[must_use]
    pub fn hit(bar: Rect, p: Point, has_back: bool, has_action: bool) -> Option<TopBarHit> {
        if has_back && Self::back_rect(bar).contains(p) {
            return Some(TopBarHit::Back);
        }
        if has_action && Self::action_rect(bar).contains(p) {
            return Some(TopBarHit::Action);
        }
        None
    }
}

// ---------------------------------------------------------------------------
// BottomTabBar
// ---------------------------------------------------------------------------

/// Material 3 bottom navigation tab bar with active pill indicator.
pub struct BottomTabBar;

impl BottomTabBar {
    pub const HEIGHT: i32 = 76;

    #[must_use]
    pub fn layout(size: Size, bottom_safe: i32) -> Rect {
        Rect::new(
            0,
            (size.h - bottom_safe - Self::HEIGHT).max(0),
            size.w.max(0),
            Self::HEIGHT,
        )
    }

    pub fn render(c: &mut Canvas, bar: Rect, theme: &Theme, labels: &[&str], active: usize) {
        if bar.is_empty() {
            return;
        }
        c.fill_rect(bar, theme.surface1);
        c.fill_rect(Rect::new(bar.x, bar.y, bar.w, 1), theme.line);

        let count = labels.len();
        if count == 0 {
            return;
        }

        for (i, label) in labels.iter().enumerate() {
            let x0 = bar.x + ((bar.w as i64 * i as i64) / count as i64) as i32;
            let x1 = bar.x + ((bar.w as i64 * (i + 1) as i64) / count as i64) as i32;
            let tab = Rect::new(x0, bar.y, (x1 - x0).max(1), bar.h);
            let is_active = i == active;

            if is_active {
                let pill_w = (tab.w - 20).clamp(48, 140);
                let pill_h = 40.min(tab.h - 12).max(24);
                let pill = Rect::new(
                    tab.x + (tab.w - pill_w) / 2,
                    tab.y + (tab.h - pill_h) / 2,
                    pill_w,
                    pill_h,
                );
                c.fill_rounded_rect(pill, tokens::RADIUS_PILL, theme.accent);
            }

            let fg = if is_active {
                theme.text_on_accent
            } else {
                theme.text2
            };
            ui::draw_centered(c, label, &ui::label(fg), tab);
        }
    }

    #[must_use]
    pub fn hit(bar: Rect, p: Point, count: usize) -> Option<usize> {
        if count == 0 || bar.w <= 0 || !bar.contains(p) {
            return None;
        }
        let rel_x = (p.x - bar.x) as usize;
        let idx = (rel_x * count) / (bar.w as usize);
        Some(idx.min(count - 1))
    }
}

// ---------------------------------------------------------------------------
// SearchPill
// ---------------------------------------------------------------------------

/// Hit target within a [`SearchPill`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchHit {
    Field,
    Clear,
}

/// Material 3 pill-shaped search bar with leading icon and optional clear button.
pub struct SearchPill;

impl SearchPill {
    pub const HEIGHT: i32 = 52;

    #[must_use]
    pub fn clear_rect(rect: Rect) -> Rect {
        let side = rect.h.clamp(36, 48);
        Rect::new(
            rect.right() - side - 4,
            rect.y + (rect.h - side) / 2,
            side,
            side,
        )
    }

    pub fn render(
        c: &mut Canvas,
        rect: Rect,
        theme: &Theme,
        query: &str,
        placeholder: &str,
        focused: bool,
    ) {
        if rect.is_empty() {
            return;
        }
        let radius = (rect.h as f32) / 2.0;
        if focused {
            c.fill_rounded_rect(rect, radius, theme.accent);
            let inner = Rect::new(
                rect.x + 2,
                rect.y + 2,
                (rect.w - 4).max(1),
                (rect.h - 4).max(1),
            );
            c.fill_rounded_rect(inner, (inner.h as f32) / 2.0, theme.surface2);
        } else {
            c.fill_rounded_rect(rect, radius, theme.surface1);
        }

        let icon_color = if focused { theme.accent } else { theme.text3 };
        draw_icon_centered(
            c,
            icons::SEARCH,
            22.0,
            icon_color,
            rect.x + 26,
            rect.y + rect.h / 2,
        );

        let clear = Self::clear_rect(rect);
        let text_x = rect.x + 48;
        let text_right = if query.is_empty() {
            rect.right() - 18
        } else {
            clear.x - 4
        };
        let text_rect = Rect::new(text_x, rect.y, (text_right - text_x).max(0), rect.h);

        if query.is_empty() {
            draw_single_line_vcenter(c, placeholder, &ui::body(theme.text3), text_rect);
        } else {
            draw_single_line_vcenter(c, query, &ui::body(theme.text), text_rect);
            draw_icon_centered(
                c,
                icons::CLOSE,
                20.0,
                theme.text2,
                clear.x + clear.w / 2,
                clear.y + clear.h / 2,
            );
        }
    }

    #[must_use]
    pub fn hit(rect: Rect, p: Point, has_query: bool) -> Option<SearchHit> {
        if !rect.contains(p) {
            return None;
        }
        if has_query && Self::clear_rect(rect).contains(p) {
            return Some(SearchHit::Clear);
        }
        Some(SearchHit::Field)
    }
}

// ---------------------------------------------------------------------------
// ToggleSwitch
// ---------------------------------------------------------------------------

/// Material 3 toggle switch wrapper around [`ui::draw_switch`].
pub struct ToggleSwitch;

impl ToggleSwitch {
    pub const WIDTH: i32 = 56;
    pub const HEIGHT: i32 = 32;

    pub fn render(c: &mut Canvas, rect: Rect, theme: &Theme, checked: bool) {
        let x = rect.x + (rect.w - Self::WIDTH) / 2;
        let y = rect.y + (rect.h - Self::HEIGHT) / 2;
        ui::draw_switch(c, theme, x, y, checked);
    }
}

// ---------------------------------------------------------------------------
// Toast
// ---------------------------------------------------------------------------

/// Transient bottom-centered Material 3 snackbar/toast pill.
pub struct Toast;

impl Toast {
    const HEIGHT: i32 = 48;
    const BOTTOM_GAP: i32 = 16;

    pub fn render(
        c: &mut Canvas,
        output_size: Size,
        bottom_safe: i32,
        theme: &Theme,
        message: &str,
    ) {
        if message.is_empty() || output_size.is_empty() {
            return;
        }
        let max_w = (output_size.w - ui::MARGIN * 2).max(120);
        let style = ui::label(theme.bg);
        let text_w = text::label_clamped(message, style, (max_w - 40).max(40), 1)
            .map_or(120, |px| px.width() as i32);
        let pill_w = (text_w + 40).clamp(120, max_w);
        let pill = Rect::new(
            (output_size.w - pill_w) / 2,
            (output_size.h - bottom_safe - Self::BOTTOM_GAP - Self::HEIGHT).max(0),
            pill_w,
            Self::HEIGHT,
        );
        c.fill_rounded_rect(pill, tokens::RADIUS_PILL, theme.text);
        ui::draw_centered(c, message, &style, pill);
    }
}

// ---------------------------------------------------------------------------
// EmptyState
// ---------------------------------------------------------------------------

/// Centered placeholder view with an icon badge, title, and optional subtitle.
pub struct EmptyState;

impl EmptyState {
    pub fn render(
        c: &mut Canvas,
        area: Rect,
        theme: &Theme,
        icon: &str,
        title: &str,
        subtitle: &str,
    ) {
        if area.is_empty() {
            return;
        }
        let has_sub = !subtitle.is_empty();
        let block_h = 72 + 16 + 34 + if has_sub { 8 + 28 } else { 0 };
        let top_y = area.y + (area.h - block_h) / 2;
        let cx = area.x + area.w / 2;

        let badge = Rect::new(cx - 36, top_y, 72, 72);
        c.fill_rounded_rect(badge, tokens::RADIUS_PILL, theme.surface2);
        draw_icon_centered(c, icon, 36.0, theme.text3, cx, badge.y + 36);

        let text_w = (area.w - ui::MARGIN * 2).max(40);
        let title_rect = Rect::new(area.x + ui::MARGIN, badge.bottom() + 16, text_w, 34);
        ui::draw_centered(c, title, &ui::styled(26.0, 600, theme.text), title_rect);

        if has_sub {
            let sub_rect = Rect::new(area.x + ui::MARGIN, title_rect.bottom() + 8, text_w, 28);
            ui::draw_centered(c, subtitle, &ui::body(theme.text2), sub_rect);
        }
    }
}

// ---------------------------------------------------------------------------
// BottomSheet
// ---------------------------------------------------------------------------

/// Material 3 modal bottom sheet with dimmed scrim, rounded top corners, and drag handle.
pub struct BottomSheet;

impl BottomSheet {
    #[must_use]
    pub fn sheet_rect(output_size: Size, height: i32) -> Rect {
        let h = height.clamp(0, output_size.h.max(0));
        Rect::new(0, (output_size.h - h).max(0), output_size.w.max(0), h)
    }

    pub fn render_backdrop_and_sheet(
        c: &mut Canvas,
        output_size: Size,
        sheet: Rect,
        theme: &Theme,
        title: &str,
    ) {
        if output_size.is_empty() {
            return;
        }
        let full = Rect::from_origin_size(Point::ZERO, output_size);
        c.fill_rect(full, theme.scrim);

        if sheet.is_empty() {
            return;
        }
        c.fill_rounded_rect(sheet, tokens::RADIUS_SHEET, theme.surface1);
        // Square off the bottom corners so the sheet sits flush with the display edge.
        let r_int = (tokens::RADIUS_SHEET as i32).min(sheet.h / 2);
        if sheet.h > r_int {
            c.fill_rect(
                Rect::new(sheet.x, sheet.y + r_int, sheet.w, sheet.h - r_int),
                theme.surface1,
            );
        }

        let handle = Rect::new(sheet.x + (sheet.w - 44) / 2, sheet.y + 12, 44, 5);
        c.fill_rounded_rect(handle, 2.5, ui::state_layer(theme, 0.35));

        if !title.is_empty() {
            let title_rect = Rect::new(
                sheet.x + ui::MARGIN,
                sheet.y + 28,
                (sheet.w - ui::MARGIN * 2).max(0),
                38,
            );
            draw_single_line_vcenter(c, title, &ui::styled(28.0, 600, theme.text), title_rect);
        }
    }

    #[must_use]
    pub fn hit_outside(output_size: Size, sheet: Rect, p: Point) -> bool {
        Rect::from_origin_size(Point::ZERO, output_size).contains(p) && !sheet.contains(p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn has_nonzero_pixels(c: &Canvas) -> bool {
        c.pixmap().data().iter().any(|&b| b != 0)
    }

    #[test]
    fn top_app_bar_layout_hit_and_render() {
        let size = Size::new(720, 1600);
        let bar = TopAppBar::layout(size, 44);
        assert_eq!(bar, Rect::new(0, 44, 720, TopAppBar::HEIGHT));

        let back = TopAppBar::back_rect(bar);
        let action = TopAppBar::action_rect(bar);
        assert!(bar.contains(Point::new(back.x, back.y)));
        assert!(bar.contains(Point::new(action.x, action.y)));

        let back_pt = Point::new(back.x + back.w / 2, back.y + back.h / 2);
        let action_pt = Point::new(action.x + action.w / 2, action.y + action.h / 2);
        let center_pt = Point::new(bar.w / 2, bar.y + bar.h / 2);

        assert_eq!(
            TopAppBar::hit(bar, back_pt, true, true),
            Some(TopBarHit::Back)
        );
        assert_eq!(TopAppBar::hit(bar, back_pt, false, true), None);
        assert_eq!(
            TopAppBar::hit(bar, action_pt, true, true),
            Some(TopBarHit::Action)
        );
        assert_eq!(TopAppBar::hit(bar, action_pt, true, false), None);
        assert_eq!(TopAppBar::hit(bar, center_pt, true, true), None);

        let mut c = Canvas::new(Size::new(720, 160));
        TopAppBar::render(
            &mut c,
            bar,
            &tokens::LIGHT,
            "Settings",
            true,
            Some(icons::MORE_VERT),
        );
        assert!(has_nonzero_pixels(&c));
    }

    #[test]
    fn bottom_tab_bar_layout_hit_and_render() {
        let size = Size::new(720, 1600);
        let bar = BottomTabBar::layout(size, 44);
        assert_eq!(bar, Rect::new(0, 1600 - 44 - 76, 720, BottomTabBar::HEIGHT));

        assert_eq!(
            BottomTabBar::hit(bar, Point::new(10, bar.y + 20), 3),
            Some(0)
        );
        assert_eq!(
            BottomTabBar::hit(bar, Point::new(360, bar.y + 20), 3),
            Some(1)
        );
        assert_eq!(
            BottomTabBar::hit(bar, Point::new(719, bar.y + 20), 3),
            Some(2)
        );
        assert_eq!(BottomTabBar::hit(bar, Point::new(360, bar.y - 1), 3), None);
        assert_eq!(BottomTabBar::hit(bar, Point::new(360, bar.y + 20), 0), None);

        let local_bar = Rect::new(0, 0, 720, BottomTabBar::HEIGHT);
        let mut c = Canvas::new(Size::new(720, BottomTabBar::HEIGHT));
        BottomTabBar::render(
            &mut c,
            local_bar,
            &tokens::DARK,
            &["Keypad", "Recents", "Contacts"],
            1,
        );
        assert!(has_nonzero_pixels(&c));
    }

    #[test]
    fn search_pill_hit_and_render() {
        let rect = Rect::new(24, 16, 672, SearchPill::HEIGHT);
        let clear = SearchPill::clear_rect(rect);
        let clear_pt = Point::new(clear.x + clear.w / 2, clear.y + clear.h / 2);
        let field_pt = Point::new(rect.x + 120, rect.y + rect.h / 2);

        assert_eq!(
            SearchPill::hit(rect, field_pt, false),
            Some(SearchHit::Field)
        );
        assert_eq!(
            SearchPill::hit(rect, clear_pt, false),
            Some(SearchHit::Field)
        );
        assert_eq!(
            SearchPill::hit(rect, clear_pt, true),
            Some(SearchHit::Clear)
        );
        assert_eq!(SearchPill::hit(rect, Point::new(0, 0), true), None);

        let mut c = Canvas::new(Size::new(720, 96));
        SearchPill::render(&mut c, rect, &tokens::LIGHT, "hello", "Search", true);
        assert!(has_nonzero_pixels(&c));
    }

    #[test]
    fn bottom_sheet_geometry_hit_and_render() {
        let size = Size::new(720, 1600);
        let sheet = BottomSheet::sheet_rect(size, 420);
        assert_eq!(sheet, Rect::new(0, 1180, 720, 420));

        assert!(BottomSheet::hit_outside(size, sheet, Point::new(360, 400)));
        assert!(!BottomSheet::hit_outside(
            size,
            sheet,
            Point::new(360, 1200)
        ));
        assert!(!BottomSheet::hit_outside(size, sheet, Point::new(-1, 400)));

        let small = Size::new(360, 400);
        let small_sheet = BottomSheet::sheet_rect(small, 180);
        let mut c = Canvas::new(small);
        BottomSheet::render_backdrop_and_sheet(
            &mut c,
            small,
            small_sheet,
            &tokens::DARK,
            "Options",
        );
        assert!(has_nonzero_pixels(&c));
    }

    #[test]
    fn toggle_toast_and_empty_state_smoke_render() {
        let mut c = Canvas::new(Size::new(400, 400));
        ToggleSwitch::render(
            &mut c,
            Rect::new(20, 20, ToggleSwitch::WIDTH, ToggleSwitch::HEIGHT),
            &tokens::LIGHT,
            true,
        );
        assert!(has_nonzero_pixels(&c));

        let mut c2 = Canvas::new(Size::new(400, 200));
        Toast::render(&mut c2, Size::new(400, 200), 24, &tokens::LIGHT, "Saved");
        assert!(has_nonzero_pixels(&c2));

        let mut c3 = Canvas::new(Size::new(400, 400));
        EmptyState::render(
            &mut c3,
            Rect::new(0, 0, 400, 400),
            &tokens::DARK,
            icons::FOLDER,
            "No files",
            "Folder is empty",
        );
        assert!(has_nonzero_pixels(&c3));
    }
}
