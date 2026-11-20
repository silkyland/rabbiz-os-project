//! Versioned C99 ABI (`librabbiz_sdk_c`) for C, C++, Qt, GTK, and Dart FFI
//! applications targeting RabbizOS.

#![allow(clippy::missing_safety_doc)]

use std::ffi::{c_char, c_void, CStr, CString};
use std::ptr;

use de_api::{Rect, Size};
use de_design::tokens::Theme;
use de_paint::Canvas;
use wayland_protocols::wp::text_input::zv3::client::zwp_text_input_v3::ContentPurpose;
use rabbiz_app_kit::{AppEvent, AppFrame, AppState, TextInputCtl};

/// Frozen C ABI version constant (`1`).
pub const RABBIZ_SDK_ABI_VERSION: u32 = 1;

#[repr(C)]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RabbizColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl RabbizColor {
    #[must_use]
    pub const fn from_rgba(c: [u8; 4]) -> Self {
        Self {
            r: c[0],
            g: c[1],
            b: c[2],
            a: c[3],
        }
    }

    #[must_use]
    pub const fn to_rgba(self) -> [u8; 4] {
        [self.r, self.g, self.b, self.a]
    }
}

#[repr(C)]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RabbizRect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl From<Rect> for RabbizRect {
    fn from(r: Rect) -> Self {
        Self {
            x: r.x,
            y: r.y,
            w: r.w,
            h: r.h,
        }
    }
}

impl From<RabbizRect> for Rect {
    fn from(r: RabbizRect) -> Self {
        Self::new(r.x, r.y, r.w, r.h)
    }
}

#[repr(C)]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RabbizSize {
    pub w: i32,
    pub h: i32,
}

#[repr(C)]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RabbizPoint {
    pub x: i32,
    pub y: i32,
}

#[repr(C)]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RabbizThemeC {
    pub bg: RabbizColor,
    pub canvas: RabbizColor,
    pub surface1: RabbizColor,
    pub surface2: RabbizColor,
    pub surface3: RabbizColor,
    pub text: RabbizColor,
    pub text2: RabbizColor,
    pub text3: RabbizColor,
    pub text_on_accent: RabbizColor,
    pub accent: RabbizColor,
    pub danger: RabbizColor,
    pub success: RabbizColor,
    pub warning: RabbizColor,
    pub line: RabbizColor,
}

impl From<&Theme> for RabbizThemeC {
    fn from(t: &Theme) -> Self {
        Self {
            bg: RabbizColor::from_rgba(t.bg),
            canvas: RabbizColor::from_rgba(t.canvas),
            surface1: RabbizColor::from_rgba(t.surface1),
            surface2: RabbizColor::from_rgba(t.surface2),
            surface3: RabbizColor::from_rgba(t.surface3),
            text: RabbizColor::from_rgba(t.text),
            text2: RabbizColor::from_rgba(t.text2),
            text3: RabbizColor::from_rgba(t.text3),
            text_on_accent: RabbizColor::from_rgba(t.text_on_accent),
            accent: RabbizColor::from_rgba(t.accent),
            danger: RabbizColor::from_rgba(t.danger),
            success: RabbizColor::from_rgba(t.success),
            warning: RabbizColor::from_rgba(t.warning),
            line: RabbizColor::from_rgba(t.line),
        }
    }
}

pub const RABBIZ_EV_TOUCH_DOWN: u32 = 1;
pub const RABBIZ_EV_TOUCH_MOVE: u32 = 2;
pub const RABBIZ_EV_TOUCH_UP: u32 = 3;
pub const RABBIZ_EV_KEY: u32 = 4;
pub const RABBIZ_EV_TEXT_COMMIT: u32 = 5;
pub const RABBIZ_EV_TEXT_BACKSPACE: u32 = 6;
pub const RABBIZ_EV_TICK: u32 = 7;
pub const RABBIZ_EV_SCROLL: u32 = 8;

#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RabbizEventKind {
    TouchDown = RABBIZ_EV_TOUCH_DOWN,
    TouchMove = RABBIZ_EV_TOUCH_MOVE,
    TouchUp = RABBIZ_EV_TOUCH_UP,
    Key = RABBIZ_EV_KEY,
    TextCommit = RABBIZ_EV_TEXT_COMMIT,
    TextBackspace = RABBIZ_EV_TEXT_BACKSPACE,
    Tick = RABBIZ_EV_TICK,
    Scroll = RABBIZ_EV_SCROLL,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct RabbizEventC {
    pub kind: u32,
    pub id: i32,
    pub x: i32,
    pub y: i32,
    pub keysym: u32,
    pub count: u32,
    pub dt_ms: u32,
    pub utf8: *const c_char,
}

#[repr(C)]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RabbizFrameResultC {
    pub dirty: bool,
    pub animating: bool,
    pub quit: bool,
}

/// Opaque C handle wrapping a `de_paint::Canvas`.
pub struct RabbizCanvasC {
    pub(crate) inner: Canvas,
}

/// Opaque C handle wrapping a `TextInputCtl`.
pub struct RabbizTextInputC<'a> {
    pub(crate) inner: &'a TextInputCtl,
}

pub type RabbizOnEventFn =
    extern "C" fn(*mut c_void, *const RabbizEventC, *mut RabbizTextInputC) -> RabbizFrameResultC;
pub type RabbizOnRenderFn = extern "C" fn(*mut c_void, *mut RabbizCanvasC, *const RabbizThemeC);

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RabbizAppCallbacksC {
    pub app_id: *const c_char,
    pub title: *const c_char,
    pub userdata: *mut c_void,
    pub on_event: Option<RabbizOnEventFn>,
    pub on_render: Option<RabbizOnRenderFn>,
}

unsafe fn cstr_to_str<'a>(ptr: *const c_char) -> Option<&'a str> {
    if ptr.is_null() {
        return None;
    }
    // SAFETY: Caller guarantees `ptr` points to a NUL-terminated C string.
    unsafe { CStr::from_ptr(ptr).to_str().ok() }
}

/// Return the frozen C ABI version (`RABBIZ_SDK_ABI_VERSION`).
#[no_mangle]
pub extern "C" fn rabbiz_sdk_abi_version() -> u32 {
    RABBIZ_SDK_ABI_VERSION
}

/// Populate `out` with the active system Light/Dark [`RabbizThemeC`].
#[no_mangle]
pub unsafe extern "C" fn rabbiz_theme_load(out: *mut RabbizThemeC) {
    if out.is_null() {
        return;
    }
    let theme = rabbiz_app_kit::load_theme();
    // SAFETY: `out` was checked for non-null above.
    unsafe {
        *out = RabbizThemeC::from(&theme);
    }
}

/// Query pixel dimensions of `c` (returns `0x0` if `c` is null).
#[no_mangle]
pub unsafe extern "C" fn rabbiz_canvas_size(c: *const RabbizCanvasC) -> RabbizSize {
    if c.is_null() {
        return RabbizSize { w: 0, h: 0 };
    }
    // SAFETY: `c` is non-null.
    let size = unsafe { (*c).inner.size() };
    RabbizSize {
        w: size.w,
        h: size.h,
    }
}

/// Fill the entire canvas with `color`.
#[no_mangle]
pub unsafe extern "C" fn rabbiz_canvas_clear(c: *mut RabbizCanvasC, color: RabbizColor) {
    if c.is_null() {
        return;
    }
    // SAFETY: `c` is non-null.
    unsafe {
        (*c).inner.clear(color.to_rgba());
    }
}

/// Fill an axis-aligned rectangle on `c`.
#[no_mangle]
pub unsafe extern "C" fn rabbiz_canvas_fill_rect(
    c: *mut RabbizCanvasC,
    rect: RabbizRect,
    color: RabbizColor,
) {
    if c.is_null() {
        return;
    }
    // SAFETY: `c` is non-null.
    unsafe {
        (*c).inner.fill_rect(Rect::from(rect), color.to_rgba());
    }
}

/// Fill a rounded rectangle on `c`.
#[no_mangle]
pub unsafe extern "C" fn rabbiz_canvas_fill_rounded_rect(
    c: *mut RabbizCanvasC,
    rect: RabbizRect,
    radius: f32,
    color: RabbizColor,
) {
    if c.is_null() {
        return;
    }
    // SAFETY: `c` is non-null.
    unsafe {
        (*c).inner
            .fill_rounded_rect(Rect::from(rect), radius, color.to_rgba());
    }
}

/// Draw a UTF-8 text label at `(x, y)` with `size_px` and `color`, returning the
/// painted bounding [`RabbizRect`].
#[no_mangle]
pub unsafe extern "C" fn rabbiz_canvas_draw_text(
    c: *mut RabbizCanvasC,
    utf8: *const c_char,
    x: i32,
    y: i32,
    size_px: f32,
    color: RabbizColor,
) -> RabbizRect {
    if c.is_null() || utf8.is_null() {
        return RabbizRect { x, y, w: 0, h: 0 };
    }
    // SAFETY: `c` and `utf8` are non-null.
    let Some(text_str) = (unsafe { cstr_to_str(utf8) }) else {
        return RabbizRect { x, y, w: 0, h: 0 };
    };
    let style = rabbiz_app_kit::ui::styled(size_px.max(8.0), 500, color.to_rgba());
    // SAFETY: `c` is non-null.
    let rect = unsafe {
        rabbiz_app_kit::ui::draw_text(&mut (*c).inner, text_str, &style, x, y, None)
    };
    RabbizRect::from(rect)
}

/// Request on-screen keyboard / IME activation with `purpose` (`0 = Normal`, `1 = Number`, `2 = Phone`, `3 = Password`, `4 = Search`, `5 = Email`).
#[no_mangle]
pub unsafe extern "C" fn rabbiz_text_input_activate(ti: *mut RabbizTextInputC, purpose: u32) {
    if ti.is_null() {
        return;
    }
    let p = match purpose {
        1 => ContentPurpose::Digits,
        2 => ContentPurpose::Phone,
        3 => ContentPurpose::Password,
        5 => ContentPurpose::Email,
        _ => ContentPurpose::Normal,
    };
    // SAFETY: `ti` is non-null.
    unsafe {
        (*ti).inner.activate(p);
    }
}

/// Deactivate on-screen keyboard / IME.
#[no_mangle]
pub unsafe extern "C" fn rabbiz_text_input_deactivate(ti: *mut RabbizTextInputC) {
    if ti.is_null() {
        return;
    }
    // SAFETY: `ti` is non-null.
    unsafe {
        (*ti).inner.deactivate();
    }
}

/// Write the NUL-terminated app data directory path for `app_id` into `buf` (up to `cap` bytes).
///
/// Returns the byte length of the path (excluding the NUL terminator), or `0` on invalid input.
#[no_mangle]
pub unsafe extern "C" fn rabbiz_storage_data_dir(
    app_id: *const c_char,
    buf: *mut c_char,
    cap: usize,
) -> usize {
    // SAFETY: `cstr_to_str` checks for null.
    let Some(id) = (unsafe { cstr_to_str(app_id) }) else {
        return 0;
    };
    let path = rabbiz_sdk::storage::data_dir(id);
    let path_str = path.to_string_lossy();
    let bytes = path_str.as_bytes();
    if !buf.is_null() && cap > 0 {
        let copy_len = bytes.len().min(cap - 1);
        // SAFETY: `buf` is valid for `cap` bytes.
        unsafe {
            ptr::copy_nonoverlapping(bytes.as_ptr().cast::<c_char>(), buf, copy_len);
            *buf.add(copy_len) = 0;
        }
    }
    bytes.len()
}

struct CCallbackApp {
    app_id: &'static str,
    title: &'static str,
    userdata: *mut c_void,
    on_event: Option<RabbizOnEventFn>,
    on_render: Option<RabbizOnRenderFn>,
}

impl AppState for CCallbackApp {
    fn app_id(&self) -> &'static str {
        self.app_id
    }

    fn title(&self) -> &'static str {
        self.title
    }

    fn handle_event(&mut self, event: AppEvent, ti: &TextInputCtl) -> AppFrame {
        let Some(cb) = self.on_event else {
            return AppFrame::default();
        };
        let mut c_str_holder: Option<CString> = None;
        let ev_c = match event {
            AppEvent::TouchDown { id, pos } => RabbizEventC {
                kind: RABBIZ_EV_TOUCH_DOWN,
                id,
                x: pos.x,
                y: pos.y,
                keysym: 0,
                count: 0,
                dt_ms: 0,
                utf8: ptr::null(),
            },
            AppEvent::TouchMove { id, pos } => RabbizEventC {
                kind: RABBIZ_EV_TOUCH_MOVE,
                id,
                x: pos.x,
                y: pos.y,
                keysym: 0,
                count: 0,
                dt_ms: 0,
                utf8: ptr::null(),
            },
            AppEvent::TouchUp { id, pos } => RabbizEventC {
                kind: RABBIZ_EV_TOUCH_UP,
                id,
                x: pos.x,
                y: pos.y,
                keysym: 0,
                count: 0,
                dt_ms: 0,
                utf8: ptr::null(),
            },
            AppEvent::Key { keysym, utf8 } => {
                if let Some(s) = utf8 {
                    c_str_holder = CString::new(s).ok();
                }
                RabbizEventC {
                    kind: RABBIZ_EV_KEY,
                    id: 0,
                    x: 0,
                    y: 0,
                    keysym,
                    count: 0,
                    dt_ms: 0,
                    utf8: c_str_holder.as_ref().map_or(ptr::null(), |c| c.as_ptr()),
                }
            }
            AppEvent::TextCommit(s) => {
                c_str_holder = CString::new(s).ok();
                RabbizEventC {
                    kind: RABBIZ_EV_TEXT_COMMIT,
                    id: 0,
                    x: 0,
                    y: 0,
                    keysym: 0,
                    count: 0,
                    dt_ms: 0,
                    utf8: c_str_holder.as_ref().map_or(ptr::null(), |c| c.as_ptr()),
                }
            }
            AppEvent::TextBackspace(n) => RabbizEventC {
                kind: RABBIZ_EV_TEXT_BACKSPACE,
                id: 0,
                x: 0,
                y: 0,
                keysym: 0,
                count: n as u32,
                dt_ms: 0,
                utf8: ptr::null(),
            },
            AppEvent::Tick(dt) => RabbizEventC {
                kind: RABBIZ_EV_TICK,
                id: 0,
                x: 0,
                y: 0,
                keysym: 0,
                count: 0,
                dt_ms: dt.as_millis() as u32,
                utf8: ptr::null(),
            },
            AppEvent::Scroll { dx, dy } => RabbizEventC {
                kind: RABBIZ_EV_SCROLL,
                id: 0,
                x: dx as i32,
                y: dy as i32,
                keysym: 0,
                count: 0,
                dt_ms: 0,
                utf8: ptr::null(),
            },
            _ => return AppFrame::default(),
        };

        let mut ti_c = RabbizTextInputC { inner: ti };
        let res = cb(self.userdata, &raw const ev_c, &raw mut ti_c);
        AppFrame {
            dirty: res.dirty,
            animating: res.animating,
            quit: res.quit,
        }
    }

    fn render(&mut self, canvas: &mut Canvas, theme: &Theme) {
        let Some(cb) = self.on_render else {
            return;
        };
        let size = canvas.size();
        let taken = std::mem::replace(canvas, Canvas::new(Size::new(1, 1)));
        let mut c_canvas = RabbizCanvasC { inner: taken };
        let c_theme = RabbizThemeC::from(theme);
        cb(self.userdata, &raw mut c_canvas, &raw const c_theme);
        let _ = std::mem::replace(canvas, c_canvas.inner);
        debug_assert_eq!(canvas.size(), size);
    }
}

/// Run a C callback-based RabbizOS application (Wayland on Linux, host simulator on macOS).
#[no_mangle]
pub unsafe extern "C" fn rabbiz_app_run(cb: *const RabbizAppCallbacksC) -> i32 {
    if cb.is_null() {
        return -1;
    }
    // SAFETY: `cb` was checked for non-null.
    let cfg = unsafe { *cb };
    // SAFETY: `cstr_to_str` handles null pointers safely.
    let app_id_str = unsafe { cstr_to_str(cfg.app_id) }.unwrap_or("rabbiz-c-app");
    // SAFETY: `cstr_to_str` handles null pointers safely.
    let title_str = unsafe { cstr_to_str(cfg.title) }.unwrap_or("Rabbiz C App");

    let leaked_id: &'static str = Box::leak(app_id_str.to_string().into_boxed_str());
    let leaked_title: &'static str = Box::leak(title_str.to_string().into_boxed_str());

    let app = CCallbackApp {
        app_id: leaked_id,
        title: leaked_title,
        userdata: cfg.userdata,
        on_event: cfg.on_event,
        on_render: cfg.on_render,
    };
    rabbiz_app_kit::run_app(app);
    0
}

/// Render a single frame of a C application headlessly at `width x height` into `out_rgba`.
///
/// Returns the required byte size (`width * height * 4`), or `0` on invalid arguments.
#[no_mangle]
pub unsafe extern "C" fn rabbiz_app_render_headless(
    cb: *const RabbizAppCallbacksC,
    width: i32,
    height: i32,
    out_rgba: *mut u8,
    cap: usize,
) -> usize {
    if cb.is_null() || width <= 0 || height <= 0 {
        return 0;
    }
    let needed = (width as usize) * (height as usize) * 4;
    // SAFETY: `cb` is non-null.
    let cfg = unsafe { *cb };
    let theme = rabbiz_app_kit::load_theme();
    let mut c_canvas = RabbizCanvasC {
        inner: Canvas::filled(Size::new(width, height), theme.bg),
    };
    let c_theme = RabbizThemeC::from(&theme);
    if let Some(on_render) = cfg.on_render {
        on_render(cfg.userdata, &raw mut c_canvas, &raw const c_theme);
    }
    let px = c_canvas.inner.into_pixels();
    if !out_rgba.is_null() && cap >= needed {
        // SAFETY: `out_rgba` has capacity at least `needed` bytes.
        unsafe {
            ptr::copy_nonoverlapping(px.data.as_ptr(), out_rgba, needed);
        }
    }
    needed
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    extern "C" fn test_on_render(
        userdata: *mut c_void,
        canvas: *mut RabbizCanvasC,
        theme: *const RabbizThemeC,
    ) {
        unsafe {
            let counter = if userdata.is_null() {
                0
            } else {
                *(userdata.cast::<i32>())
            };
            let t = *theme;
            rabbiz_canvas_clear(canvas, t.bg);
            rabbiz_canvas_fill_rounded_rect(
                canvas,
                RabbizRect {
                    x: 16,
                    y: 16,
                    w: 120,
                    h: 48,
                },
                12.0,
                t.accent,
            );
            let msg = CString::new(format!("Clicks: {counter}")).unwrap();
            let _ = rabbiz_canvas_draw_text(canvas, msg.as_ptr(), 24, 24, 20.0, t.text_on_accent);
        }
    }

    #[test]
    fn c_abi_null_safety_and_headless_callback_rendering() {
        assert_eq!(rabbiz_sdk_abi_version(), 1);

        unsafe {
            rabbiz_theme_load(ptr::null_mut());
            let mut theme = RabbizThemeC::default();
            rabbiz_theme_load(&raw mut theme);
            assert_eq!(theme.bg.a, 255);

            assert_eq!(
                rabbiz_canvas_size(ptr::null()),
                RabbizSize { w: 0, h: 0 }
            );
            rabbiz_canvas_clear(ptr::null_mut(), theme.bg);
            rabbiz_canvas_fill_rect(
                ptr::null_mut(),
                RabbizRect {
                    x: 0,
                    y: 0,
                    w: 10,
                    h: 10,
                },
                theme.accent,
            );

            // Storage path query via C buffer
            let app_id = CString::new("com.rabbiz.ctest").unwrap();
            let mut path_buf = [0 as c_char; 256];
            let len =
                rabbiz_storage_data_dir(app_id.as_ptr(), path_buf.as_mut_ptr(), path_buf.len());
            assert!(len > 0);
            let c_path = CStr::from_ptr(path_buf.as_ptr()).to_str().unwrap();
            assert!(c_path.contains("com.rabbiz.ctest"));

            // Headless C app rendering
            let mut clicks: i32 = 7;
            let cb = RabbizAppCallbacksC {
                app_id: app_id.as_ptr(),
                title: app_id.as_ptr(),
                userdata: (&raw mut clicks).cast::<c_void>(),
                on_event: None,
                on_render: Some(test_on_render),
            };
            let mut rgba = vec![0u8; 200 * 100 * 4];
            let written =
                rabbiz_app_render_headless(&raw const cb, 200, 100, rgba.as_mut_ptr(), rgba.len());
            assert_eq!(written, 200 * 100 * 4);
            // Verify accent rect at (30, 30) differs from background at (2, 2).
            let bg_px = &rgba[((2 * 200 + 2) * 4)..((2 * 200 + 2) * 4 + 4)];
            let accent_px = &rgba[((30 * 200 + 30) * 4)..((30 * 200 + 30) * 4 + 4)];
            assert_ne!(bg_px, accent_px);
        }
    }

    #[test]
    fn c99_header_compiles_cleanly_with_cc_wall_wextra_werror() {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let include_dir = manifest_dir.parent().unwrap().parent().unwrap().join("include");
        let header_path = include_dir.join("rabbiz_sdk.h");
        assert!(header_path.exists(), "missing {}", header_path.display());

        let tmp_dir = std::env::temp_dir().join(format!(
            "rabbiz-sdk-c99-check-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&tmp_dir).unwrap();
        let c_file = tmp_dir.join("smoke.c");
        let o_file = tmp_dir.join("smoke.o");

        let c_source = r#"
#include "rabbiz_sdk.h"

static RabbizFrameResultC my_on_event(
    void *userdata,
    const RabbizEventC *ev,
    RabbizTextInputC *ti
) {
    (void)userdata;
    (void)ti;
    RabbizFrameResultC res = { false, false, false };
    if (ev->kind == RABBIZ_EV_TOUCH_DOWN) {
        res.dirty = true;
    }
    return res;
}

static void my_on_render(
    void *userdata,
    RabbizCanvasC *canvas,
    const RabbizThemeC *theme
) {
    (void)userdata;
    rabbiz_canvas_clear(canvas, theme->bg);
    RabbizRect r = { 10, 10, 100, 40 };
    rabbiz_canvas_fill_rounded_rect(canvas, r, 12.0f, theme->accent);
}

int check_abi(void) {
    RabbizAppCallbacksC cb = {
        "com.example.c99",
        "C99 Smoke",
        NULL,
        my_on_event,
        my_on_render
    };
    (void)cb;
    return (int)RABBIZ_SDK_ABI_VERSION;
}
"#;
        std::fs::write(&c_file, c_source).unwrap();

        let status = std::process::Command::new("cc")
            .arg("-std=c99")
            .arg("-Wall")
            .arg("-Wextra")
            .arg("-Werror")
            .arg("-I")
            .arg(&include_dir)
            .arg("-c")
            .arg(&c_file)
            .arg("-o")
            .arg(&o_file)
            .status()
            .expect("invoke cc");
        assert!(status.success(), "cc -std=c99 -Wall -Wextra -Werror failed");
        assert!(o_file.exists());

        let _ = std::fs::remove_dir_all(&tmp_dir);
    }
}
