//! Context menu component (docs/DE-DESKTOP-MODE.md, Phase A).
//!
//! The shell opens one of these on a right-click or a long-press: on the empty home screen, on an app icon,
//! on a notification. It is data in, pixels out like every other `de-ui` screen: a [`Menu`] describes the
//! items, [`layout`] places it on the output (flipping and clamping so it never leaves the screen),
//! [`Selection`] handles hover and keyboard or controller navigation, [`hit_test`] answers pointer and touch,
//! and [`render`] draws it. Nothing here reads the clock or the device.
//!
//! Not built yet: submenus, icons, and a drop shadow.

use de_api::{Pixels, Point, Rect, Size};
use de_design::tokens::{self, Theme};
use de_paint::Canvas;

use crate::chrome::style;

/// One row of a menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    /// Something the user can pick.
    Action(Action),
    /// A thin divider between groups.
    Separator,
}

/// A selectable row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Action {
    /// Stable id returned when the row is activated (`"desktop.wallpaper"`).
    pub id: String,
    /// Text shown to the user (already translated).
    pub label: String,
    /// Optional hint on the right (`"Ctrl+L"`).
    pub shortcut: Option<String>,
    /// `Some(true/false)` draws a check mark state; `None` is a plain action.
    pub checked: Option<bool>,
    /// Greyed out and not selectable.
    pub enabled: bool,
    /// Drawn in the danger colour (delete, power off).
    pub danger: bool,
}

impl Action {
    /// A plain, enabled action.
    #[must_use]
    pub fn new(id: &str, label: &str) -> Self {
        Self {
            id: id.to_string(),
            label: label.to_string(),
            shortcut: None,
            checked: None,
            enabled: true,
            danger: false,
        }
    }
}

/// The items of a menu, top to bottom.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Menu {
    /// Rows in display order.
    pub items: Vec<Item>,
}

/// Sizes in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Metrics {
    /// Height of an action row (touch targets stay comfortable).
    pub row_h: i32,
    /// Height of a separator row.
    pub separator_h: i32,
    /// Inner padding on the left and right of a row.
    pub pad_x: i32,
    /// Padding above the first and below the last row.
    pub pad_y: i32,
    /// Narrowest and widest the menu may be.
    pub min_w: i32,
    /// Widest the menu may be.
    pub max_w: i32,
    /// Distance kept from the screen edges.
    pub edge: i32,
}

impl Metrics {
    /// For a touch-first handset at 720 px width.
    #[must_use]
    pub const fn touch() -> Self {
        Self {
            row_h: 88,
            separator_h: 17,
            pad_x: 32,
            pad_y: 12,
            min_w: 360,
            max_w: 560,
            edge: 16,
        }
    }
}

/// Where a menu ended up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    /// The whole menu.
    pub rect: Rect,
    /// `(item index, row rectangle)` for every item, in order.
    pub rows: Vec<(usize, Rect)>,
}

/// Place `menu` with its top-left corner at `anchor` (the point that was clicked). If it would run off the
/// right or bottom edge it opens to the left of or above the anchor instead, and is finally clamped so it is
/// always fully on the output. `measure` returns the width in pixels of a label.
#[must_use]
pub fn layout(
    menu: &Menu,
    anchor: Point,
    output: Size,
    m: &Metrics,
    measure: &dyn Fn(&str) -> i32,
) -> Layout {
    let text_w = menu
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Action(a) => {
                let shortcut = a.shortcut.as_deref().map_or(0, |s| measure(s) + m.pad_x);
                Some(measure(&a.label) + shortcut)
            }
            Item::Separator => None,
        })
        .max()
        .unwrap_or(0);
    let w = (text_w + m.pad_x * 2).clamp(m.min_w, m.max_w).min(output.w - 2 * m.edge);
    let h = m.pad_y * 2
        + menu
            .items
            .iter()
            .map(|i| match i {
                Item::Action(_) => m.row_h,
                Item::Separator => m.separator_h,
            })
            .sum::<i32>();

    let mut x = anchor.x;
    let mut y = anchor.y;
    if x + w > output.w - m.edge {
        x = anchor.x - w;
    }
    if y + h > output.h - m.edge {
        y = anchor.y - h;
    }
    x = x.clamp(m.edge, (output.w - m.edge - w).max(m.edge));
    y = y.clamp(m.edge, (output.h - m.edge - h).max(m.edge));
    let rect = Rect::new(x, y, w, h);

    let mut rows = Vec::with_capacity(menu.items.len());
    let mut ry = y + m.pad_y;
    for (i, item) in menu.items.iter().enumerate() {
        let rh = match item {
            Item::Action(_) => m.row_h,
            Item::Separator => m.separator_h,
        };
        rows.push((i, Rect::new(x, ry, w, rh)));
        ry += rh;
    }
    Layout { rect, rows }
}

/// Result of [`hit_test`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hit {
    /// On the selectable row with this item index.
    Item(usize),
    /// On the menu but not on a selectable row (padding, separator, disabled row).
    Inside,
    /// Outside the menu: a click or tap here dismisses it.
    Outside,
}

/// What is under `p`?
#[must_use]
pub fn hit_test(menu: &Menu, lay: &Layout, p: Point) -> Hit {
    if !lay.rect.contains(p) {
        return Hit::Outside;
    }
    for (i, r) in &lay.rows {
        if r.contains(p) {
            return match &menu.items[*i] {
                Item::Action(a) if a.enabled => Hit::Item(*i),
                _ => Hit::Inside,
            };
        }
    }
    Hit::Inside
}

/// Hover and keyboard or controller focus, shared by mouse, keys and D-pad.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Selection {
    /// Item index of the highlighted row.
    pub current: Option<usize>,
}

impl Selection {
    fn selectable(menu: &Menu, i: usize) -> bool {
        matches!(&menu.items[i], Item::Action(a) if a.enabled)
    }

    /// Move to the next selectable row, wrapping around (Down arrow, D-pad down).
    pub fn next(&mut self, menu: &Menu) {
        let n = menu.items.len();
        if n == 0 {
            return;
        }
        let start = self.current.unwrap_or(n - 1);
        self.current = (1..=n)
            .map(|k| (start + k) % n)
            .find(|i| Self::selectable(menu, *i))
            .or(self.current);
    }

    /// Move to the previous selectable row, wrapping around (Up arrow, D-pad up).
    pub fn prev(&mut self, menu: &Menu) {
        let n = menu.items.len();
        if n == 0 {
            return;
        }
        let start = self.current.unwrap_or(0);
        self.current = (1..=n)
            .map(|k| (start + n - k) % n)
            .find(|i| Self::selectable(menu, *i))
            .or(self.current);
    }

    /// Follow the pointer: highlight the row under it, or nothing.
    pub fn hover(&mut self, hit: Hit) {
        self.current = match hit {
            Hit::Item(i) => Some(i),
            Hit::Inside | Hit::Outside => None,
        };
    }

    /// The id to report if the highlighted row is activated (Enter, A button).
    #[must_use]
    pub fn activate<'a>(&self, menu: &'a Menu) -> Option<&'a str> {
        let i = self.current?;
        match &menu.items[i] {
            Item::Action(a) if a.enabled => Some(a.id.as_str()),
            _ => None,
        }
    }
}

/// Draw the menu into a pixmap exactly the size of `lay.rect`; blit it at `lay.rect`'s origin.
#[must_use]
pub fn render(menu: &Menu, lay: &Layout, sel: Selection, t: &Theme) -> Pixels {
    let origin = Point::new(lay.rect.x, lay.rect.y);
    let mut c = Canvas::new(Size::new(lay.rect.w, lay.rect.h));
    c.fill_rounded_rect(Rect::new(0, 0, lay.rect.w, lay.rect.h), tokens::RADIUS_CARD, t.line);
    c.fill_rounded_rect(
        Rect::new(1, 1, lay.rect.w - 2, lay.rect.h - 2),
        tokens::RADIUS_CARD - 1.0,
        t.surface2,
    );
    for (i, r) in &lay.rows {
        let local = Rect::new(r.x - origin.x, r.y - origin.y, r.w, r.h);
        match &menu.items[*i] {
            Item::Separator => {
                c.fill_rect(Rect::new(local.x + 24, local.y + local.h / 2, local.w - 48, 1), t.line);
            }
            Item::Action(a) => {
                if sel.current == Some(*i) && a.enabled {
                    c.fill_rounded_rect(
                        Rect::new(local.x + 8, local.y, local.w - 16, local.h),
                        tokens::RADIUS_ROW,
                        t.accent_dim,
                    );
                }
                let color = if !a.enabled {
                    t.text3
                } else if a.danger {
                    t.danger
                } else {
                    t.text
                };
                let label = match a.checked {
                    Some(true) => format!("\u{2713}  {}", a.label),
                    Some(false) => format!("    {}", a.label),
                    None => a.label.clone(),
                };
                crate::chrome::draw(
                    &mut c,
                    &label,
                    &style::body(color),
                    local.x + 32,
                    local.y + (local.h - 36) / 2,
                    Some(local.w - 64),
                );
                if let Some(s) = &a.shortcut {
                    crate::chrome::draw(
                        &mut c,
                        s,
                        &style::caption(t.text3),
                        local.x + local.w - 32 - 100,
                        local.y + (local.h - 30) / 2,
                        Some(100),
                    );
                }
            }
        }
    }
    c.into_pixels()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Menu {
        Menu {
            items: vec![
                Item::Action(Action::new("desktop.wallpaper", "Change wallpaper")),
                Item::Action(Action::new("desktop.display", "Display settings")),
                Item::Separator,
                Item::Action(Action {
                    enabled: false,
                    ..Action::new("desktop.widgets", "Add widget")
                }),
                Item::Action(Action {
                    danger: true,
                    ..Action::new("session.lock", "Lock")
                }),
            ],
        }
    }

    // 14 px per character keeps the tests independent of fonts.
    fn fake(s: &str) -> i32 {
        14 * i32::try_from(s.chars().count()).unwrap()
    }

    const OUT: Size = Size { w: 720, h: 1600 };

    #[test]
    fn it_opens_at_the_anchor_when_there_is_room() {
        let lay = layout(&sample(), Point::new(100, 200), OUT, &Metrics::touch(), &fake);
        assert_eq!((lay.rect.x, lay.rect.y), (100, 200));
        assert_eq!(lay.rows.len(), 5);
    }

    #[test]
    fn it_flips_left_and_up_near_the_bottom_right_corner() {
        let anchor = Point::new(700, 1580);
        let lay = layout(&sample(), anchor, OUT, &Metrics::touch(), &fake);
        assert!(lay.rect.x + lay.rect.w <= anchor.x, "opens to the left of the anchor");
        assert!(lay.rect.y + lay.rect.h <= anchor.y, "opens above the anchor");
    }

    #[test]
    fn it_never_leaves_the_output_wherever_it_is_opened() {
        let m = Metrics::touch();
        for (x, y) in [(0, 0), (719, 0), (0, 1599), (719, 1599), (360, 800), (-50, -50), (900, 2000)] {
            let lay = layout(&sample(), Point::new(x, y), OUT, &m, &fake);
            assert!(lay.rect.x >= m.edge && lay.rect.y >= m.edge, "{x},{y}");
            assert!(lay.rect.x + lay.rect.w <= OUT.w - m.edge, "{x},{y}");
            assert!(lay.rect.y + lay.rect.h <= OUT.h - m.edge, "{x},{y}");
        }
    }

    #[test]
    fn rows_are_stacked_without_gaps_inside_the_menu() {
        let lay = layout(&sample(), Point::new(100, 200), OUT, &Metrics::touch(), &fake);
        let mut y = lay.rect.y + Metrics::touch().pad_y;
        for (_, r) in &lay.rows {
            assert_eq!(r.y, y);
            y += r.h;
        }
        assert_eq!(y + Metrics::touch().pad_y, lay.rect.y + lay.rect.h);
    }

    #[test]
    fn the_width_follows_the_longest_label_within_limits() {
        let m = Metrics::touch();
        let tiny = Menu { items: vec![Item::Action(Action::new("a", "OK"))] };
        assert_eq!(layout(&tiny, Point::new(0, 0), OUT, &m, &fake).rect.w, m.min_w);
        let long = Menu {
            items: vec![Item::Action(Action::new("a", &"x".repeat(80)))],
        };
        assert_eq!(layout(&long, Point::new(0, 0), OUT, &m, &fake).rect.w, m.max_w);
    }

    #[test]
    fn hit_testing_tells_rows_padding_disabled_rows_and_outside_apart() {
        let menu = sample();
        let lay = layout(&menu, Point::new(100, 200), OUT, &Metrics::touch(), &fake);
        let mid = |i: usize| {
            let r = lay.rows[i].1;
            Point::new(r.x + r.w / 2, r.y + r.h / 2)
        };
        assert_eq!(hit_test(&menu, &lay, mid(0)), Hit::Item(0));
        assert_eq!(hit_test(&menu, &lay, mid(2)), Hit::Inside, "separator");
        assert_eq!(hit_test(&menu, &lay, mid(3)), Hit::Inside, "disabled row");
        assert_eq!(hit_test(&menu, &lay, Point::new(lay.rect.x + 5, lay.rect.y + 2)), Hit::Inside, "padding");
        assert_eq!(hit_test(&menu, &lay, Point::new(5, 5)), Hit::Outside);
    }

    #[test]
    fn keys_skip_separators_and_disabled_rows_and_wrap() {
        let menu = sample(); // selectable: 0, 1, 4
        let mut s = Selection::default();
        s.next(&menu);
        assert_eq!(s.current, Some(0));
        s.next(&menu);
        assert_eq!(s.current, Some(1));
        s.next(&menu);
        assert_eq!(s.current, Some(4), "skips the separator and the disabled row");
        s.next(&menu);
        assert_eq!(s.current, Some(0), "wraps");
        s.prev(&menu);
        assert_eq!(s.current, Some(4), "wraps backwards");
        s.prev(&menu);
        assert_eq!(s.current, Some(1));
    }

    #[test]
    fn activating_reports_the_id_only_for_an_enabled_highlighted_row() {
        let menu = sample();
        let mut s = Selection::default();
        assert_eq!(s.activate(&menu), None, "nothing highlighted");
        s.current = Some(1);
        assert_eq!(s.activate(&menu), Some("desktop.display"));
        s.current = Some(3);
        assert_eq!(s.activate(&menu), None, "disabled");
    }

    #[test]
    fn hover_follows_the_pointer_and_clears_off_the_rows() {
        let mut s = Selection::default();
        s.hover(Hit::Item(2));
        assert_eq!(s.current, Some(2));
        s.hover(Hit::Inside);
        assert_eq!(s.current, None);
    }

    #[test]
    fn a_menu_with_nothing_selectable_does_not_panic_or_select() {
        let menu = Menu { items: vec![Item::Separator] };
        let mut s = Selection::default();
        s.next(&menu);
        s.prev(&menu);
        assert_eq!(s.current, None);
        let empty = Menu::default();
        let mut s = Selection::default();
        s.next(&empty);
        s.prev(&empty);
        assert_eq!(s.current, None, "an empty menu has nothing to select and must not panic");
        let lay = layout(&empty, Point::new(10, 10), OUT, &Metrics::touch(), &fake);
        assert_eq!(lay.rows.len(), 0);
    }

    #[test]
    fn render_produces_a_pixmap_the_size_of_the_menu() {
        let menu = sample();
        let lay = layout(&menu, Point::new(100, 200), OUT, &Metrics::touch(), &fake);
        let px = render(&menu, &lay, Selection { current: Some(0) }, &tokens::DARK);
        assert_eq!((px.size.w, px.size.h), (lay.rect.w, lay.rect.h));
        assert!(px.data.iter().any(|b| *b != 0), "something was drawn");
    }
}
