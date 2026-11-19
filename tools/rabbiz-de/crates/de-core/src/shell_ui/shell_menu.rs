//! The shell's own context menu (docs/DE-DESKTOP-MODE.md, Phase A3): opened by a right-click or a long-press
//! on the empty home screen. The component lives in `de_ui::menu`; this module owns the open state and the
//! actions, and routes pointer, touch and keys while a menu is up. A menu is only ever opened over the shell
//! (never over an app: apps draw their own menus as xdg popups).

use de_api::{Point as ApiPoint, Size};
use de_ui::menu::{self, Action, Hit, Item, Layout, Menu, Metrics, Selection};
use de_ui::nav::Screen;

use crate::state::{De, Foreground};

/// evdev button code of the right mouse button.
const BTN_RIGHT: u32 = 0x111;
/// xkb keycodes are the evdev code plus 8.
const KEY_ESC: u32 = 1 + 8;
const KEY_ENTER: u32 = 28 + 8;
const KEY_KPENTER: u32 = 96 + 8;
const KEY_UP: u32 = 103 + 8;
const KEY_DOWN: u32 = 108 + 8;

/// An open menu.
pub(crate) struct ShellMenu {
    menu: Menu,
    layout: Layout,
    sel: Selection,
}

/// Width of a label in pixels, as the renderer will draw it.
fn measure(s: &str) -> i32 {
    de_paint::text::label(s, de_ui::chrome::style::body([0, 0, 0, 255]), None)
        .map_or(0, |p| i32::try_from(p.width()).unwrap_or(i32::MAX))
}

fn to_api(p: smithay::utils::Point<f64, smithay::utils::Logical>) -> ApiPoint {
    ApiPoint::new(p.x as i32, p.y as i32)
}

impl De {
    fn output_size(&self) -> Size {
        Size::new(self.output.width as i32, self.output.height as i32)
    }

    /// Whether a shell menu is open.
    pub(crate) fn menu_is_open(&self) -> bool {
        self.shell_menu.is_some()
    }

    /// The items of the empty-home menu.
    fn home_menu(&self) -> Menu {
        let s = &self.strings;
        Menu {
            items: vec![
                Item::Action(Action::new("home.wallpaper", s.get("menu.wallpaper"))),
                Item::Action(Action::new("home.settings", s.get("menu.settings"))),
                Item::Separator,
                Item::Action(Action::new("home.lock", s.get("menu.lock"))),
            ],
        }
    }

    /// Open the empty-home menu at `pos` if the shell is showing the home screen and nothing (an icon, the
    /// search pill, a dock slot) is under it. Returns whether a menu was opened.
    pub(crate) fn open_home_menu(&mut self, pos: ApiPoint) -> bool {
        if self.foreground != Foreground::Shell
            || self.nav.current != Screen::Home
            || self.morph.is_some()
            || self.menu_is_open()
        {
            return false;
        }
        let size = self.output_size();
        let prefs = self.app_prefs.prefs().clone();
        let total = self.catalog.home_order(&prefs).len();
        let on_page = total
            .saturating_sub(self.home_page * de_ui::home::PAGE_SIZE)
            .min(de_ui::home::PAGE_SIZE);
        let dock = self.catalog.dock(&prefs).len();
        if de_ui::home::hit_test(size, pos, on_page, dock).is_some() {
            return false;
        }
        let menu = self.home_menu();
        let layout = menu::layout(&menu, pos, size, &Metrics::touch(), &measure);
        self.shell_menu = Some(ShellMenu {
            menu,
            layout,
            sel: Selection::default(),
        });
        self.redraw_shell();
        true
    }

    fn redraw_shell(&mut self) {
        self.shell_dirty = true;
        self.needs_present = true;
    }

    /// Close the menu without choosing anything.
    pub(crate) fn close_menu(&mut self) {
        if self.shell_menu.take().is_some() {
            self.redraw_shell();
        }
    }

    /// Pointer moved: follow it with the highlight.
    pub(crate) fn menu_pointer_moved(
        &mut self,
        pos: smithay::utils::Point<f64, smithay::utils::Logical>,
    ) {
        let Some(m) = self.shell_menu.as_mut() else {
            return;
        };
        let before = m.sel;
        m.sel.hover(menu::hit_test(&m.menu, &m.layout, to_api(pos)));
        if m.sel != before {
            self.redraw_shell();
        }
    }

    /// A pointer button changed. Returns `true` when the menu consumed it.
    pub(crate) fn menu_pointer_button(
        &mut self,
        button: u32,
        pressed: bool,
        pos: smithay::utils::Point<f64, smithay::utils::Logical>,
    ) -> bool {
        if !pressed {
            return false;
        }
        if self.menu_is_open() {
            self.menu_press_at(to_api(pos));
            return true;
        }
        button == BTN_RIGHT && self.open_home_menu(to_api(pos))
    }

    /// A touch landed while a menu may be open. Returns `true` when the menu consumed it.
    pub(crate) fn menu_touch_down(
        &mut self,
        pos: smithay::utils::Point<f64, smithay::utils::Logical>,
    ) -> bool {
        if !self.menu_is_open() {
            return false;
        }
        self.menu_press_at(to_api(pos));
        true
    }

    /// Press or tap at `pos` with a menu open: activate the row, ignore padding, dismiss on the outside.
    fn menu_press_at(&mut self, pos: ApiPoint) {
        let Some(m) = self.shell_menu.as_ref() else {
            return;
        };
        match menu::hit_test(&m.menu, &m.layout, pos) {
            Hit::Item(i) => {
                let id = match &m.menu.items[i] {
                    Item::Action(a) => a.id.clone(),
                    Item::Separator => return,
                };
                self.close_menu();
                self.run_menu_action(&id);
            }
            Hit::Inside => {}
            Hit::Outside => self.close_menu(),
        }
    }

    /// A key went down with a menu open. Returns `true` when the menu consumed it.
    pub(crate) fn menu_key(&mut self, keycode: u32) -> bool {
        let Some(m) = self.shell_menu.as_mut() else {
            return false;
        };
        match keycode {
            KEY_ESC => self.close_menu(),
            KEY_DOWN => {
                m.sel.next(&m.menu);
                self.redraw_shell();
            }
            KEY_UP => {
                m.sel.prev(&m.menu);
                self.redraw_shell();
            }
            KEY_ENTER | KEY_KPENTER => {
                if let Some(id) = m.sel.activate(&m.menu).map(str::to_string) {
                    self.close_menu();
                    self.run_menu_action(&id);
                }
            }
            _ => return false,
        }
        true
    }

    fn run_menu_action(&mut self, id: &str) {
        match id {
            "home.wallpaper" => {
                let next = de_paint::wallpaper::next_id(&self.app_prefs.prefs().wallpaper_id);
                if self.app_prefs.set_wallpaper(next) {
                    self.wallpaper = None;
                }
            }
            "home.settings" => self.nav.go(Screen::Settings),
            "home.lock" => {
                self.power_lock();
            }
            other => eprintln!("rabbiz-de: unknown menu action {other}"),
        }
        self.redraw_shell();
    }

    /// Draw the open menu (if any) over a freshly rasterised shell frame.
    pub(crate) fn overlay_shell_menu(&self, base: de_api::Pixels) -> de_api::Pixels {
        let Some(m) = self.shell_menu.as_ref() else {
            return base;
        };
        let top = menu::render(&m.menu, &m.layout, m.sel, &self.theme);
        let mut canvas = de_paint::Canvas::new(base.size);
        canvas.blit(&base, ApiPoint::ZERO);
        canvas.blit(&top, ApiPoint::new(m.layout.rect.x, m.layout.rect.y));
        canvas.into_pixels()
    }
}
