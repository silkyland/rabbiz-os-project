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
        let data = px.data();
        let mut min_y = h;
        let mut max_y = -1;
        for row in 0..h {
            let start = (row * w * 4) as usize;
            let end = start + (w * 4) as usize;
            if data[start..end].chunks_exact(4).any(|p| p[3] > 0) {
                min_y = min_y.min(row);
                max_y = max_y.max(row);
            }
        }
        let y = if max_y >= min_y {
            let ink_h = max_y - min_y + 1;
            r.y + (r.h - ink_h) / 2 - min_y
        } else {
            r.y + (r.h - h).max(0) / 2
        };
        c.blit(
            &Pixels::new(Size::new(w, h), data.to_vec()),
            Point::new(r.x, y),
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

/// Round column edges up to match the pixel boundaries used by the hit tests.
fn equal_column(bar: Rect, index: usize, count: usize) -> Rect {
    let edge = |index: usize| bar.x + (bar.w as usize * index).div_ceil(count) as i32;
    let x = edge(index);
    Rect::new(x, bar.y, edge(index + 1) - x, bar.h)
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
            let tab = equal_column(bar, i, count);
            if tab.is_empty() {
                continue;
            }
            let is_active = i == active;

            if is_active {
                let pill_w = (tab.w - 20).clamp(48, 140).min(tab.w);
                let pill_h = (tab.h - 12).clamp(24, 40).min(tab.h);
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

impl BottomTabBar {
    /// Icon+label variant (M3 navigation bar): `tabs` is `(icon, label)`
    /// pairs; the active indicator is an accent-tinted pill behind the icon.
    pub fn render_with_icons(
        c: &mut Canvas,
        bar: Rect,
        theme: &Theme,
        tabs: &[(&str, &str)],
        active: usize,
    ) {
        if bar.is_empty() {
            return;
        }
        c.fill_rect(bar, theme.surface1);
        c.fill_rect(Rect::new(bar.x, bar.y, bar.w, 1), theme.line);

        let count = tabs.len();
        if count == 0 {
            return;
        }

        for (i, (icon, label)) in tabs.iter().enumerate() {
            let tab = equal_column(bar, i, count);
            if tab.is_empty() {
                continue;
            }
            let is_active = i == active;

            let icon_cy = tab.y + 24;
            if is_active {
                let width = 64.min(tab.w);
                let pill = Rect::new(tab.x + (tab.w - width) / 2, icon_cy - 17, width, 34);
                c.fill_rounded_rect(
                    pill,
                    tokens::RADIUS_PILL,
                    ui::with_alpha(theme.accent, 0x33),
                );
            }
            let icon_col = if is_active { theme.text } else { theme.text2 };
            draw_icon_centered(c, icon, 24.0, icon_col, tab.x + tab.w / 2, icon_cy);

            let label_rect = Rect::new(tab.x, icon_cy + 18, tab.w, 28);
            let label_col = if is_active { theme.text } else { theme.text2 };
            ui::draw_centered(c, label, &ui::caption(label_col), label_rect);
        }
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
            draw_single_line_vcenter(c, placeholder, &ui::body(theme.text2), text_rect);
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
        const BADGE: i32 = 64;
        const TITLE: i32 = 34;
        const SUBTITLE: i32 = 28;
        let has_sub = !subtitle.is_empty();
        let text_h = TITLE + if has_sub { 4 + SUBTITLE } else { 0 };
        let show_icon = !icon.is_empty() && area.h >= BADGE + 16 + text_h;
        let block_h = text_h + if show_icon { BADGE + 16 } else { 0 };
        let mut y = area.y + ((area.h - block_h) / 2).max(0);
        let cx = area.x + area.w / 2;

        if show_icon {
            let badge = Rect::new(cx - BADGE / 2, y, BADGE, BADGE);
            let tint = ui::mix(theme.bg, theme.accent, 0.10);
            c.fill_rounded_rect(badge, tokens::RADIUS_PILL, tint);
            draw_icon_centered(c, icon, 32.0, theme.text2, cx, y + BADGE / 2);
            y = badge.bottom() + 16;
        }

        let margin = ui::MARGIN.min(area.w / 4);
        let title_rect = Rect::new(area.x + margin, y, area.w - margin * 2, TITLE);
        ui::draw_centered(c, title, &ui::styled(26.0, 600, theme.text), title_rect);

        if has_sub {
            let sub_rect = Rect::new(
                title_rect.x,
                title_rect.bottom() + 4,
                title_rect.w,
                SUBTITLE,
            );
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

// ---------------------------------------------------------------------------
// ListRow
// ---------------------------------------------------------------------------

/// One-line M3 list row height (px).
pub const ROW_H_ONE: i32 = 72;
/// Two-line (title + subtitle) M3 list row height (px).
pub const ROW_H_TWO: i32 = 96;

/// Leading slot of a [`list_row`].
#[derive(Clone, Copy, Default)]
pub enum RowLeading<'a> {
    /// No leading content — the text column starts at the page margin.
    #[default]
    None,
    /// 24 px Material glyph in `text2`.
    Icon(&'a str),
    /// 48 px tinted circle with the first character of `name` (uppercased)
    /// in `tint`; falls back to a person glyph for empty names. Pair with
    /// [`ui::avatar_tint`] for a stable per-peer color.
    Avatar { name: &'a str, tint: [u8; 4] },
    /// 48 px rounded image tile, blitted scaled into the slot.
    Image(&'a Pixels),
}

/// Trailing slot of a [`list_row`].
#[derive(Clone, Copy, Default)]
pub enum RowTrailing<'a> {
    /// Nothing on the right.
    #[default]
    None,
    /// 24 px Material glyph in `text3` inside a 48 px touch box.
    Icon(&'a str),
    /// M3 toggle switch showing `on`.
    Switch(bool),
    /// Right-aligned caption text in `text2` (timestamps, counts).
    Text(&'a str),
}

/// Declarative content of one M3 list row — see [`list_row`].
#[derive(Clone, Copy, Default)]
pub struct ListRowSpec<'a> {
    pub leading: RowLeading<'a>,
    /// Primary text (`ui::body`, `t.text`), single-line ellipsized.
    pub title: &'a str,
    /// Optional second line (`ui::caption`, `t.text2`). Empty = one-line row.
    pub subtitle: &'a str,
    pub trailing: RowTrailing<'a>,
    /// Draw a `state_layer` press overlay over the whole row.
    pub pressed: bool,
    /// Hairline at the row bottom, inset to the text column when a leading
    /// slot is present (M3 inset divider).
    pub divider: bool,
}

impl<'a> ListRowSpec<'a> {
    #[must_use]
    pub fn new(title: &'a str) -> Self {
        Self {
            title,
            ..Self::default()
        }
    }
}

/// M3 list item: optional leading slot, title (+ subtitle), optional
/// trailing slot, optional press overlay and inset divider. `row` is the
/// full row band ([`ROW_H_ONE`]/[`ROW_H_TWO`] tall); the row is the hit
/// target — no sub-hit fn is provided.
pub fn list_row(c: &mut Canvas, t: &Theme, row: Rect, spec: &ListRowSpec) {
    if row.is_empty() {
        return;
    }
    let cy = row.y + row.h / 2;

    // --- leading slot -----------------------------------------------------
    let has_leading = !matches!(spec.leading, RowLeading::None);
    match spec.leading {
        RowLeading::None => {}
        RowLeading::Icon(icon) => {
            draw_icon_centered(c, icon, 24.0, t.text2, row.x + 16 + 24, cy);
        }
        RowLeading::Avatar { name, tint } => {
            let av = Rect::new(row.x + 16, cy - 24, 48, 48);
            avatar(c, name, av, tint, tint);
        }
        RowLeading::Image(px) => {
            let tile = Rect::new(row.x + 16, cy - 24, 48, 48);
            c.blit_scaled(px, tile, 1.0);
        }
    }

    // --- trailing slot ----------------------------------------------------
    let mut trail_reserve = ui::MARGIN;
    match spec.trailing {
        RowTrailing::None => {}
        RowTrailing::Icon(icon) => {
            let bx = row.right() - 16 - 48;
            draw_icon_centered(c, icon, 24.0, t.text3, bx + 24, cy);
            trail_reserve = 16 + 48;
        }
        RowTrailing::Switch(on) => {
            let sw = Rect::new(
                row.right() - ui::MARGIN - ToggleSwitch::WIDTH,
                cy - ToggleSwitch::HEIGHT / 2,
                ToggleSwitch::WIDTH,
                ToggleSwitch::HEIGHT,
            );
            ToggleSwitch::render(c, sw, t, on);
            trail_reserve = ui::MARGIN + ToggleSwitch::WIDTH;
        }
        RowTrailing::Text(s) => {
            let w = text::label_clamped(s, ui::caption(t.text2), 160, 1)
                .map_or(48, |px| px.width() as i32);
            let r = Rect::new(row.right() - ui::MARGIN - w, row.y, w, row.h);
            draw_single_line_vcenter(c, s, &ui::caption(t.text2), r);
            trail_reserve = ui::MARGIN + w + 8;
        }
    }

    // --- text column ------------------------------------------------------
    let text_x = row.x + if has_leading { 80 } else { ui::MARGIN };
    let text_right = row.right() - trail_reserve;
    if text_right > text_x {
        let text_rect = Rect::new(text_x, row.y, text_right - text_x, row.h);
        if spec.subtitle.is_empty() {
            draw_single_line_vcenter(c, spec.title, &ui::body(t.text), text_rect);
        } else {
            let title_r = Rect::new(text_x, cy - 30, text_rect.w, 30);
            let sub_r = Rect::new(text_x, cy + 2, text_rect.w, 26);
            draw_single_line_vcenter(c, spec.title, &ui::body(t.text), title_r);
            draw_single_line_vcenter(c, spec.subtitle, &ui::caption(t.text2), sub_r);
        }
    }

    // --- divider + press --------------------------------------------------
    if spec.divider {
        let x0 = if has_leading {
            text_x
        } else {
            row.x + ui::MARGIN
        };
        ui::divider_inset(c, t, x0, row.right(), row.bottom() - 1);
    }
    if spec.pressed {
        c.fill_rect(row, ui::state_layer(t, 0.10));
    }
}

// ---------------------------------------------------------------------------
// Avatar
// ---------------------------------------------------------------------------

/// Avatar circle: `tint` at low alpha fill with the uppercased first char of
/// `name` in `text_color` (person glyph when `name` is empty). Pair with
/// [`ui::avatar_tint`] so a peer keeps the same color in every app.
pub fn avatar(c: &mut Canvas, name: &str, r: Rect, tint: [u8; 4], text_color: [u8; 4]) {
    if r.is_empty() {
        return;
    }
    c.fill_rounded_rect(r, (r.h as f32) / 2.0, ui::with_alpha(tint, 0x2E));
    if let Some(ch) = name.chars().next() {
        let s: String = ch.to_uppercase().collect();
        let st = ui::styled(r.h as f32 * 0.44, 600, text_color);
        ui::draw_centered(c, &s, &st, r);
    } else {
        draw_icon_centered(
            c,
            icons::PERSON,
            r.h as f32 * 0.52,
            text_color,
            r.x + r.w / 2,
            r.y + r.h / 2,
        );
    }
}

// ---------------------------------------------------------------------------
// FAB
// ---------------------------------------------------------------------------

/// FAB size classes — `px()` is the button edge length.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FabSize {
    /// 84 px (M3 56 dp at our 1.5× panel scale).
    #[default]
    Standard,
    /// 60 px mini FAB (M3 40 dp).
    Small,
}

impl FabSize {
    #[must_use]
    pub const fn px(self) -> i32 {
        match self {
            Self::Standard => 84,
            Self::Small => 60,
        }
    }
}

/// M3 floating action button: `accent` rounded square with a soft shadow
/// and a `text_on_accent` icon. `r` is the button rect — make it
/// [`FabSize::px`] square for the standard sizes.
pub fn fab(c: &mut Canvas, t: &Theme, r: Rect, icon: &str, pressed: bool) {
    if r.is_empty() {
        return;
    }
    let radius = (r.h as f32) / 3.0;
    // Cheap elevation: shadow tinted rounded rect dropped 4 px.
    c.fill_rounded_rect(r.translate(0, 4), radius, t.shadow);
    c.fill_rounded_rect(r, radius, t.accent);
    if pressed {
        c.fill_rounded_rect(r, radius, ui::with_alpha(t.text_on_accent, 0x26));
    }
    draw_icon_centered(
        c,
        icon,
        r.h as f32 * 0.36,
        t.text_on_accent,
        r.x + r.w / 2,
        r.y + r.h / 2,
    );
}

// ---------------------------------------------------------------------------
// IconBtn
// ---------------------------------------------------------------------------

/// Fill style of an [`icon_btn`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IconBtnStyle {
    /// Transparent — glyph only (app-bar actions).
    #[default]
    Plain,
    /// `surface2` tonal circle.
    Tonal,
    /// `accent` filled circle with `text_on_accent` glyph.
    Filled,
}

/// 48 px-class circular icon button. `r` is the touch rect; the glyph is
/// centered inside it. `selected` tints the glyph `accent` (Plain/Tonal).
pub fn icon_btn(
    c: &mut Canvas,
    t: &Theme,
    r: Rect,
    icon: &str,
    style: IconBtnStyle,
    selected: bool,
    pressed: bool,
) {
    if r.is_empty() {
        return;
    }
    let radius = (r.h as f32) / 2.0;
    let glyph = match style {
        IconBtnStyle::Plain => {
            if pressed {
                ui::press_overlay(c, t, r, radius, 0.10);
            }
            if selected {
                t.accent
            } else {
                t.text2
            }
        }
        IconBtnStyle::Tonal => {
            c.fill_rounded_rect(r, radius, t.surface2);
            if pressed {
                ui::press_overlay(c, t, r, radius, 0.10);
            }
            if selected {
                t.accent
            } else {
                t.text
            }
        }
        IconBtnStyle::Filled => {
            c.fill_rounded_rect(r, radius, t.accent);
            if pressed {
                c.fill_rounded_rect(r, radius, ui::with_alpha(t.text_on_accent, 0x26));
            }
            t.text_on_accent
        }
    };
    draw_icon_centered(
        c,
        icon,
        24.0_f32.min(r.h as f32 * 0.5),
        glyph,
        r.x + r.w / 2,
        r.y + r.h / 2,
    );
}

// ---------------------------------------------------------------------------
// Chip
// ---------------------------------------------------------------------------

/// Filter/assist chip height (px).
pub const CHIP_H: i32 = 48;

/// One M3 chip: `selected` paints an accent-tinted fill + `text` label;
/// unselected paints a `line` outline (on a `surface1` interior — reads as
/// transparent where the row/page background is `surface1`) + `text2` label.
pub fn chip(c: &mut Canvas, t: &Theme, r: Rect, label: &str, selected: bool, pressed: bool) {
    if r.is_empty() {
        return;
    }
    let radius = tokens::RADIUS_CHIP.max((r.h as f32) / 2.0 - 6.0);
    let (fg, overlay) = if selected {
        c.fill_rounded_rect(r, radius, ui::with_alpha(t.accent, 0x30));
        (t.text, 0.10)
    } else {
        ui::stroke_rounded(c, t, r, radius, t.line, 1);
        (t.text2, 0.08)
    };
    if pressed {
        ui::press_overlay(c, t, r, radius, overlay);
    }
    ui::draw_centered(c, label, &ui::label(fg), r);
}

/// Chip width = measured label + horizontal padding (for [`chips_layout`]).
#[must_use]
pub fn chip_width(t: &Theme, label: &str, selected: bool) -> i32 {
    let st = ui::label(if selected { t.text } else { t.text2 });
    text::label(label, st, None).map_or(48, |px| px.width() as i32) + 32
}

/// Horizontal chip strip layout: `labels`-order rects, `CHIP_H` tall, `gap`
/// apart, starting at `(x, y)`.
#[must_use]
pub fn chips_layout(t: &Theme, x: i32, y: i32, items: &[(&str, bool)], gap: i32) -> Vec<Rect> {
    let mut rects = Vec::with_capacity(items.len());
    let mut cx = x;
    for (label, selected) in items {
        let w = chip_width(t, label, *selected);
        rects.push(Rect::new(cx, y, w, CHIP_H));
        cx += w + gap;
    }
    rects
}

/// Render a horizontal chip group; returns the chip rects in `items` order
/// for hit-testing.
pub fn chips_row(
    c: &mut Canvas,
    t: &Theme,
    x: i32,
    y: i32,
    items: &[(&str, bool)],
    gap: i32,
    pressed: Option<usize>,
) -> Vec<Rect> {
    let rects = chips_layout(t, x, y, items, gap);
    for (i, ((label, selected), r)) in items.iter().zip(rects.iter()).enumerate() {
        chip(c, t, *r, label, *selected, pressed == Some(i));
    }
    rects
}

// ---------------------------------------------------------------------------
// Slider
// ---------------------------------------------------------------------------

/// Groove height of a [`slider`] (px).
pub const SLIDER_TRACK_H: i32 = 8;
/// Thumb diameter of a [`slider`] (px).
pub const SLIDER_THUMB: i32 = 28;

/// M3-ish slider: `surface3` groove with an `accent` fill up to `frac` and
/// an `accent` thumb. `track` is the horizontal lane — the groove is
/// centered vertically inside it; `active` (touching) adds a focus ring.
pub fn slider(c: &mut Canvas, t: &Theme, track: Rect, frac: f32, active: bool) {
    if track.is_empty() {
        return;
    }
    let f = frac.clamp(0.0, 1.0);
    let cy = track.y + track.h / 2;
    let groove = Rect::new(track.x, cy - SLIDER_TRACK_H / 2, track.w, SLIDER_TRACK_H);
    c.fill_rounded_rect(groove, (SLIDER_TRACK_H as f32) / 2.0, t.surface3);
    let thumb_x = track.x + (track.w as f32 * f) as i32;
    let fill_w = (thumb_x - track.x).max(0);
    if fill_w > 0 {
        let fill = Rect::new(track.x, groove.y, fill_w, SLIDER_TRACK_H);
        c.fill_rounded_rect(fill, (SLIDER_TRACK_H as f32) / 2.0, t.accent);
    }
    if active {
        let ring = Rect::new(thumb_x - 20, cy - 20, 40, 40);
        c.fill_rounded_rect(ring, 20.0, ui::with_alpha(t.accent, 0x1F));
    }
    let thumb = Rect::new(
        thumb_x - SLIDER_THUMB / 2,
        cy - SLIDER_THUMB / 2,
        SLIDER_THUMB,
        SLIDER_THUMB,
    );
    c.fill_rounded_rect(thumb, (SLIDER_THUMB as f32) / 2.0, t.accent);
}

/// Fraction (0.0–1.0) of `track` at pointer x — the inverse of [`slider`]'s
/// thumb mapping, for drag handling.
#[must_use]
pub fn slider_frac_at(x: i32, track: Rect) -> f32 {
    if track.w <= 0 {
        return 0.0;
    }
    ((x - track.x) as f32 / track.w as f32).clamp(0.0, 1.0)
}

// ---------------------------------------------------------------------------
// SegmentedBar
// ---------------------------------------------------------------------------

/// M3 segmented control with a tonal indicator. The segments split the
/// track evenly, including when its width is not divisible by their count.
pub fn segmented(c: &mut Canvas, t: &Theme, bar: Rect, labels: &[&str], active: usize) {
    if bar.is_empty() || labels.is_empty() {
        return;
    }
    c.fill_rounded_rect(bar, (bar.h as f32) / 2.0, t.surface2);
    let count = labels.len();
    for (i, label) in labels.iter().enumerate() {
        let seg = equal_column(bar, i, count);
        if seg.is_empty() {
            continue;
        }
        let is_active = i == active;
        let col = if is_active { t.text } else { t.text2 };
        if is_active {
            let inner = seg.inset(4);
            let tint = ui::mix(t.surface2, t.accent, 0.14);
            c.fill_rounded_rect(inner, (inner.h as f32) / 2.0, tint);
        }
        ui::draw_centered(
            c,
            label,
            &ui::styled(20.0, if is_active { 600 } else { 400 }, col),
            seg,
        );
    }
}

/// Segment index under `p`, or `None` outside `bar` / for an empty control.
#[must_use]
pub fn segmented_hit(bar: Rect, p: Point, count: usize) -> Option<usize> {
    if count == 0 || bar.w <= 0 || !bar.contains(p) {
        return None;
    }
    let rel_x = (p.x - bar.x) as usize;
    Some(((rel_x * count) / bar.w as usize).min(count - 1))
}

// ---------------------------------------------------------------------------
// Dialog
// ---------------------------------------------------------------------------

/// Geometry produced by [`dialog_layout`] (and returned by [`dialog`]):
/// card + right-aligned action rects in label order for hit-testing.
pub struct DialogLayout {
    /// The centered surface1 card.
    pub card: Rect,
    /// One 48 px-tall button rect per action label, in the same order.
    pub actions: Vec<Rect>,
}

/// Layout math shared by [`dialog`] and hit-testing: card centered in
/// `size` (≤ 560 px wide), action buttons measured per label.
#[must_use]
pub fn dialog_layout(size: Size, body: &[&str], actions: &[&str]) -> DialogLayout {
    const CARD_W_MAX: i32 = 560;
    const PAD: i32 = 24;
    const TITLE_H: i32 = 36;
    const BODY_PITCH: i32 = 30;
    const BTN_H: i32 = 48;

    let card_w = (size.w - ui::MARGIN * 2).clamp(0, CARD_W_MAX);
    let body_h = body.len() as i32 * BODY_PITCH;
    let body_gap = if body.is_empty() { 0 } else { 12 };
    let card_h = PAD + TITLE_H + body_gap + body_h + 20 + BTN_H + 16;
    let card = Rect::new(
        ((size.w - card_w) / 2).max(0),
        ((size.h - card_h) / 2).max(0),
        card_w,
        card_h,
    );

    // Action row: right-aligned, buttons laid out right-to-left (order in
    // `actions` is left-to-right, so iterate reversed and unshift).
    let btn_y = card.bottom() - 16 - BTN_H;
    let mut rects: Vec<Rect> = Vec::with_capacity(actions.len());
    let mut right = card.right() - 12;
    for label in actions.iter().rev() {
        let w = dialog_action_width(label) + 24;
        let r = Rect::new(right - w, btn_y, w, BTN_H);
        rects.insert(0, r);
        right = r.x - 8;
    }
    DialogLayout {
        card,
        actions: rects,
    }
}

/// Label width of a dialog text button (20 px/600, color-independent).
fn dialog_action_width(label: &str) -> i32 {
    text::label(label, ui::label([0, 0, 0, 255]), None).map_or(40, |px| px.width() as i32)
}

/// M3 alert dialog: `scrim` + centered `surface1` card with a TITLE-style
/// headline, `body` lines in `text2`, and right-aligned `accent` text
/// buttons. `pressed_action` highlights one button. Returns the
/// [`DialogLayout`] used — keep it for hit-testing.
pub fn dialog(
    c: &mut Canvas,
    t: &Theme,
    size: Size,
    title: &str,
    body: &[&str],
    actions: &[&str],
    pressed_action: Option<usize>,
) -> DialogLayout {
    let layout = dialog_layout(size, body, actions);
    if size.is_empty() {
        return layout;
    }
    c.fill_rect(Rect::from_origin_size(Point::ZERO, size), t.scrim);
    let card = layout.card;
    if card.is_empty() {
        return layout;
    }
    c.fill_rounded_rect(card, tokens::RADIUS_CARD, t.surface1);

    let title_style = text::LabelStyle::from_type(de_design::type_scale::TITLE, t.text);
    ui::draw_text(
        c,
        title,
        &title_style,
        card.x + 24,
        card.y + 24,
        Some(card.w - 48),
    );
    let mut y = card.y + 24 + 36 + if body.is_empty() { 0 } else { 12 };
    for line in body {
        ui::draw_text(
            c,
            line,
            &ui::body(t.text2),
            card.x + 24,
            y,
            Some(card.w - 48),
        );
        y += 30;
    }
    for (i, (label, r)) in actions.iter().zip(layout.actions.iter()).enumerate() {
        if pressed_action == Some(i) {
            ui::press_overlay(c, t, *r, r.h as f32 / 2.0, 0.10);
        }
        ui::draw_centered(c, label, &ui::label(t.accent), *r);
    }
    layout
}

/// Action index under `p`, or `None` when the point hits no button.
#[must_use]
pub fn dialog_action_at(layout: &DialogLayout, p: Point) -> Option<usize> {
    layout.actions.iter().position(|r| r.contains(p))
}

/// `true` when `p` is inside `size` but outside the dialog card — the
/// dismiss gesture.
#[must_use]
pub fn dialog_on_scrim(layout: &DialogLayout, size: Size, p: Point) -> bool {
    Rect::from_origin_size(Point::ZERO, size).contains(p) && !layout.card.contains(p)
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
    fn bottom_tabs_draw_and_hit_the_same_odd_width_columns() {
        let bar = Rect::new(7, 12, 401, BottomTabBar::HEIGHT);
        for x in bar.x..bar.right() {
            let p = Point::new(x, bar.y);
            let index = BottomTabBar::hit(bar, p, 3).unwrap();
            assert!(equal_column(bar, index, 3).contains(p));
        }
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

    #[test]
    fn list_row_renders_all_slot_combinations() {
        let t = &tokens::LIGHT;
        let mut c = Canvas::new(Size::new(720, 400));
        for (i, spec) in [
            ListRowSpec::new("Wi-Fi"),
            ListRowSpec {
                leading: RowLeading::Icon(icons::WIFI),
                title: "Wi-Fi",
                subtitle: "Connected",
                trailing: RowTrailing::Switch(true),
                pressed: false,
                divider: true,
            },
            ListRowSpec {
                leading: RowLeading::Avatar {
                    name: "Ada",
                    tint: ui::avatar_tint("Ada"),
                },
                title: "Ada",
                subtitle: "Mobile",
                trailing: RowTrailing::Icon(icons::CHEVRON_RIGHT),
                pressed: true,
                divider: false,
            },
            ListRowSpec {
                title: "Recents",
                trailing: RowTrailing::Text("12:34"),
                ..ListRowSpec::new("Recents")
            },
        ]
        .iter()
        .enumerate()
        {
            list_row(&mut c, t, Rect::new(0, 96 * i as i32, 720, ROW_H_TWO), spec);
        }
        assert!(has_nonzero_pixels(&c));
    }

    #[test]
    fn fab_icon_btn_and_avatar_smoke_render() {
        let t = &tokens::DARK;
        let mut c = Canvas::new(Size::new(300, 200));
        fab(
            &mut c,
            t,
            Rect::new(10, 10, FabSize::Standard.px(), 84),
            icons::ADD,
            false,
        );
        icon_btn(
            &mut c,
            t,
            Rect::new(110, 10, 48, 48),
            icons::SEARCH,
            IconBtnStyle::Filled,
            false,
            true,
        );
        icon_btn(
            &mut c,
            t,
            Rect::new(170, 10, 48, 48),
            icons::STAR,
            IconBtnStyle::Tonal,
            true,
            false,
        );
        avatar(
            &mut c,
            "bob",
            Rect::new(10, 110, 64, 64),
            ui::avatar_tint("bob"),
            t.accent,
        );
        avatar(
            &mut c,
            "",
            Rect::new(90, 110, 64, 64),
            ui::avatar_tint(""),
            t.accent,
        );
        assert!(has_nonzero_pixels(&c));
    }

    #[test]
    fn chips_layout_and_render() {
        let t = &tokens::LIGHT;
        let items: &[(&str, bool)] = &[("All", true), ("Missed", false), ("Voicemail", false)];
        let rects = chips_layout(t, 24, 100, items, 8);
        assert_eq!(rects.len(), 3);
        assert_eq!(rects[0].h, CHIP_H);
        for (a, b) in rects.iter().zip(rects.iter().skip(1)) {
            assert_eq!(a.right() + 8, b.x, "chips must be gap-separated");
        }

        let mut c = Canvas::new(Size::new(720, 200));
        let hit_rects = chips_row(&mut c, t, 24, 100, items, 8, Some(1));
        assert_eq!(hit_rects, rects);
        assert!(has_nonzero_pixels(&c));
    }

    #[test]
    fn slider_frac_at_is_inverse_of_thumb_position() {
        let track = Rect::new(24, 100, 400, 48);
        assert_eq!(slider_frac_at(24, track), 0.0);
        assert_eq!(slider_frac_at(424, track), 1.0);
        assert!((slider_frac_at(224, track) - 0.5).abs() < 0.01);
        // Clamps outside the lane and survives degenerate rects.
        assert_eq!(slider_frac_at(0, track), 0.0);
        assert_eq!(slider_frac_at(10_000, track), 1.0);
        assert_eq!(slider_frac_at(10, Rect::new(0, 0, 0, 0)), 0.0);

        let mut c = Canvas::new(Size::new(460, 160));
        slider(&mut c, &tokens::DARK, track, 0.5, true);
        assert!(has_nonzero_pixels(&c));
    }

    #[test]
    fn segmented_hit_and_render() {
        let bar = Rect::new(24, 60, 480, 48);
        assert_eq!(segmented_hit(bar, Point::new(30, 70), 3), Some(0));
        assert_eq!(segmented_hit(bar, Point::new(250, 70), 3), Some(1));
        assert_eq!(segmented_hit(bar, Point::new(503, 70), 3), Some(2));
        assert_eq!(segmented_hit(bar, Point::new(10, 70), 3), None);
        assert_eq!(segmented_hit(bar, Point::new(250, 10), 3), None);
        assert_eq!(segmented_hit(bar, Point::new(250, 70), 0), None);

        let mut c = Canvas::new(Size::new(560, 160));
        segmented(&mut c, &tokens::LIGHT, bar, &["Day", "Week", "Month"], 1);
        assert!(has_nonzero_pixels(&c));
    }

    #[test]
    fn dialog_layout_actions_and_render() {
        let size = Size::new(720, 1600);
        let layout = dialog_layout(size, &["Line one", "Line two"], &["Cancel", "OK"]);
        assert_eq!(layout.actions.len(), 2);
        // Actions are right-aligned, in label order, 48px tall.
        assert!(layout.actions[0].x < layout.actions[1].x);
        assert!(layout.actions[1].right() <= layout.card.right());
        for a in &layout.actions {
            assert_eq!(a.h, 48);
            assert_eq!(a.bottom(), layout.card.bottom() - 16);
        }
        assert_eq!(
            dialog_action_at(
                &layout,
                Point::new(layout.actions[0].x + 4, layout.actions[0].y + 4)
            ),
            Some(0)
        );
        assert!(dialog_on_scrim(&layout, size, Point::new(10, 10)));
        assert!(!dialog_on_scrim(&layout, size, layout.card.center()));

        let mut c = Canvas::new(size);
        let l2 = dialog(
            &mut c,
            &tokens::DARK,
            size,
            "Delete contact?",
            &["This cannot be undone."],
            &["Cancel", "Delete"],
            Some(1),
        );
        assert_eq!(l2.actions.len(), 2);
        assert!(has_nonzero_pixels(&c));
    }

    #[test]
    fn bottom_tab_bar_icons_variant_renders() {
        let mut c = Canvas::new(Size::new(720, BottomTabBar::HEIGHT));
        BottomTabBar::render_with_icons(
            &mut c,
            Rect::new(0, 0, 720, BottomTabBar::HEIGHT),
            &tokens::LIGHT,
            &[
                (icons::DIALPAD, "Keypad"),
                (icons::HISTORY, "Recents"),
                (icons::PERSON, "Contacts"),
            ],
            1,
        );
        assert!(has_nonzero_pixels(&c));
    }
}
