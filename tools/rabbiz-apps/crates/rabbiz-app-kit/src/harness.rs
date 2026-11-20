//! Cross-platform headless app test harness and golden PNG comparator.
//!
//! Drives any [`AppState`] deterministically with synthetic touch, swipe,
//! keyboard, IME, and animation-tick events, then renders to premultiplied
//! RGBA8 [`Pixels`] for pixel assertions and golden regression checks.

use std::fmt;
use std::path::Path;
use std::time::Duration;

use de_api::{Pixels, Point, Size};
use de_design::tokens::{self, Theme};
use de_paint::{decode_png, encode_png, Canvas};
use wayland_protocols::wp::text_input::zv3::client::zwp_text_input_v3::ContentPurpose;

use crate::app::{AppEvent, AppFrame, AppState};
use crate::wayland::TextInputCtl;

const DEFAULT_SIZE: Size = Size::new(720, 1600);
const GOLDEN_BACKDROP: [u8; 3] = [0x16, 0x10, 0x1A];

/// Result of comparing a rendered frame against a golden PNG.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GoldenCompare {
    /// Fraction of pixels differing by `> 2/255` on any RGB channel (`0.0..=1.0`).
    pub diff_frac: f64,
    /// Mean absolute error per RGB channel (`0.0..=1.0`).
    pub mae: f64,
    pub pass: bool,
}

impl fmt::Display for GoldenCompare {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "diff={:.4}% mae={:.5} {}",
            self.diff_frac * 100.0,
            self.mae,
            if self.pass { "PASS" } else { "FAIL" }
        )
    }
}

/// Deterministic headless runner for any [`AppState`] implementation.
pub struct TestHarness<S: AppState> {
    state: S,
    size: Size,
    theme: Theme,
    ti: TextInputCtl,
    last_frame: AppFrame,
    quit_requested: bool,
}

impl<S: AppState> TestHarness<S> {
    /// Create a harness targeting `720x1600` Light Cream Glass with an inert [`TextInputCtl`].
    #[must_use]
    pub fn new(state: S) -> Self {
        Self {
            state,
            size: DEFAULT_SIZE,
            theme: tokens::LIGHT,
            ti: TextInputCtl::headless(),
            last_frame: AppFrame::default(),
            quit_requested: false,
        }
    }

    #[must_use]
    pub fn with_size(mut self, size: Size) -> Self {
        self.set_size(size);
        self
    }

    #[must_use]
    pub fn with_theme(mut self, theme: Theme) -> Self {
        self.theme = theme;
        self
    }

    pub fn set_size(&mut self, size: Size) {
        self.size = Size::new(size.w.max(1), size.h.max(1));
    }

    pub fn set_theme(&mut self, theme: Theme) {
        self.theme = theme;
    }

    #[must_use]
    pub fn size(&self) -> Size {
        self.size
    }

    #[must_use]
    pub fn theme(&self) -> &Theme {
        &self.theme
    }

    #[must_use]
    pub fn state(&self) -> &S {
        &self.state
    }

    pub fn state_mut(&mut self) -> &mut S {
        &mut self.state
    }

    #[must_use]
    pub fn ime_active(&self) -> bool {
        self.ti.is_active()
    }

    #[must_use]
    pub fn ime_purpose(&self) -> Option<ContentPurpose> {
        self.ti.purpose()
    }

    #[must_use]
    pub fn quit_requested(&self) -> bool {
        self.quit_requested
    }

    #[must_use]
    pub fn last_frame(&self) -> AppFrame {
        self.last_frame
    }

    /// Dispatch a single [`AppEvent`] and return the resulting [`AppFrame`].
    pub fn send(&mut self, event: AppEvent) -> AppFrame {
        let frame = self.state.handle_event(event, &self.ti);
        self.last_frame = frame;
        if frame.quit {
            self.quit_requested = true;
        }
        frame
    }

    /// Send a primary finger tap (`TouchDown` followed by `TouchUp` at `pos`).
    pub fn tap(&mut self, pos: Point) -> AppFrame {
        let down = self.send(AppEvent::TouchDown { id: 0, pos });
        let up = self.send(AppEvent::TouchUp { id: 0, pos });
        AppFrame {
            dirty: down.dirty || up.dirty,
            animating: up.animating,
            quit: down.quit || up.quit,
        }
    }

    /// Send a linear touch drag across `steps` intermediate `TouchMove` events.
    pub fn swipe(&mut self, from: Point, to: Point, steps: usize, step_dt: Duration) -> AppFrame {
        let steps = steps.max(1);
        let mut any_dirty = self.send(AppEvent::TouchDown { id: 0, pos: from }).dirty;
        for i in 1..=steps {
            let t = i as f32 / steps as f32;
            let x = (from.x as f32 + (to.x - from.x) as f32 * t).round() as i32;
            let y = (from.y as f32 + (to.y - from.y) as f32 * t).round() as i32;
            let f = self.send(AppEvent::TouchMove {
                id: 0,
                pos: Point::new(x, y),
            });
            any_dirty |= f.dirty;
            if !step_dt.is_zero() && f.animating {
                any_dirty |= self.tick(step_dt).dirty;
            }
        }
        let up = self.send(AppEvent::TouchUp { id: 0, pos: to });
        AppFrame {
            dirty: any_dirty || up.dirty,
            animating: up.animating,
            quit: up.quit || self.quit_requested,
        }
    }

    /// Dispatch a key event (`keysym` + optional UTF-8 text).
    pub fn key(&mut self, keysym: u32, utf8: Option<&str>) -> AppFrame {
        self.send(AppEvent::Key {
            keysym,
            utf8: utf8.map(str::to_string),
        })
    }

    /// Dispatch an IME commit string (`AppEvent::TextCommit`).
    pub fn type_text(&mut self, text: &str) -> AppFrame {
        if text.is_empty() {
            return self.last_frame;
        }
        self.send(AppEvent::TextCommit(text.to_string()))
    }

    /// Dispatch an IME surrounding-text backspace (`AppEvent::TextBackspace`).
    pub fn backspace(&mut self, count: usize) -> AppFrame {
        self.send(AppEvent::TextBackspace(count))
    }

    /// Advance animations by `dt`.
    pub fn tick(&mut self, dt: Duration) -> AppFrame {
        self.send(AppEvent::Tick(dt))
    }

    /// Advance animation ticks until `animating` clears or `max_steps` is reached.
    pub fn settle(&mut self, max_steps: usize, dt: Duration) -> usize {
        let mut ran = 0;
        while self.last_frame.animating && ran < max_steps {
            self.tick(dt);
            ran += 1;
        }
        ran
    }

    /// Render the current app state into premultiplied RGBA8 [`Pixels`].
    #[must_use]
    pub fn render(&mut self) -> Pixels {
        let mut canvas = Canvas::filled(self.size, self.theme.bg);
        self.state.render(&mut canvas, &self.theme);
        canvas.into_pixels()
    }

    /// Render the current state and write it as a PNG file (creating parent dirs).
    pub fn save_png(&mut self, path: &Path) -> Result<(), String> {
        let px = self.render();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("mkdir {}: {e}", parent.display()))?;
        }
        let bytes = encode_png(&px).map_err(|e| format!("png encode: {e}"))?;
        std::fs::write(path, bytes).map_err(|e| format!("write {}: {e}", path.display()))
    }

    /// Render the current state and compare it against a golden PNG on disk.
    pub fn compare_golden(&mut self, golden_path: &Path) -> Result<GoldenCompare, String> {
        let actual = self.render();
        compare_pixels_to_golden(&actual, golden_path)
    }
}

fn composite_straight(px: &Pixels) -> Vec<u8> {
    let mut out = Vec::with_capacity(px.data.len());
    for c in px.data.as_chunks::<4>().0 {
        let a = u32::from(c[3]);
        let inv = 255 - a;
        for k in 0..3 {
            out.push(
                ((u32::from(c[k]) * 255 + u32::from(GOLDEN_BACKDROP[k]) * inv) / 255).min(255)
                    as u8,
            );
        }
        out.push(255);
    }
    out
}

fn compare_pixels_to_golden(
    actual: &Pixels,
    golden_path: &Path,
) -> Result<GoldenCompare, String> {
    let bytes =
        std::fs::read(golden_path).map_err(|e| format!("read {}: {e}", golden_path.display()))?;
    let golden = decode_png(&bytes).map_err(|e| format!("decode golden: {e}"))?;
    if golden.size != actual.size {
        return Err(format!(
            "size mismatch: actual {}x{} vs golden {}x{}",
            actual.size.w, actual.size.h, golden.size.w, golden.size.h
        ));
    }
    let a = composite_straight(actual);
    let g = composite_straight(&golden);
    let px_count = actual.size.area().max(1) as u64;
    let (mut diff_px, mut err_sum) = (0u64, 0u64);
    for (ca, cg) in a.as_chunks::<4>().0.iter().zip(g.as_chunks::<4>().0.iter()) {
        let mut maxd = 0u64;
        for k in 0..3 {
            let d = u64::from(ca[k].abs_diff(cg[k]));
            maxd = maxd.max(d);
            err_sum += d;
        }
        if maxd > 2 {
            diff_px += 1;
        }
    }
    let diff_frac = diff_px as f64 / px_count as f64;
    let mae = err_sum as f64 / (px_count as f64 * 3.0 * 255.0);
    Ok(GoldenCompare {
        diff_frac,
        mae,
        pass: diff_frac <= 0.01 && mae <= 1.0 / 255.0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text_input::TextBuffer;
    use de_api::Rect;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_SEQ: AtomicU64 = AtomicU64::new(1);

    struct DummyApp {
        buf: TextBuffer,
        taps: usize,
        swiped: bool,
        anim_left: usize,
        box_color: [u8; 4],
    }

    impl DummyApp {
        fn new() -> Self {
            Self {
                buf: TextBuffer::new(),
                taps: 0,
                swiped: false,
                anim_left: 0,
                box_color: [0xA6, 0x5A, 0x2E, 0xFF],
            }
        }
    }

    impl AppState for DummyApp {
        fn app_id(&self) -> &'static str {
            "dummy-app"
        }

        fn title(&self) -> &'static str {
            "Dummy"
        }

        fn handle_event(&mut self, event: AppEvent, ti: &TextInputCtl) -> AppFrame {
            match event {
                AppEvent::TouchDown { pos, .. } => {
                    if pos.y < 100 {
                        ti.activate(ContentPurpose::Normal);
                    } else {
                        ti.deactivate();
                    }
                    self.taps += 1;
                    AppFrame {
                        dirty: true,
                        animating: false,
                        quit: false,
                    }
                }
                AppEvent::TouchMove { .. } => {
                    self.swiped = true;
                    AppFrame {
                        dirty: true,
                        animating: false,
                        quit: false,
                    }
                }
                AppEvent::TextCommit(s) => {
                    self.buf.insert(&s);
                    self.anim_left = 3;
                    AppFrame {
                        dirty: true,
                        animating: true,
                        quit: false,
                    }
                }
                AppEvent::TextBackspace(n) => {
                    for _ in 0..n {
                        self.buf.backspace();
                    }
                    AppFrame {
                        dirty: true,
                        animating: false,
                        quit: false,
                    }
                }
                AppEvent::Key { keysym: 0xFF1B, .. } => AppFrame {
                    dirty: false,
                    animating: false,
                    quit: true,
                },
                AppEvent::Tick(_) => {
                    self.anim_left = self.anim_left.saturating_sub(1);
                    AppFrame {
                        dirty: true,
                        animating: self.anim_left > 0,
                        quit: false,
                    }
                }
                _ => AppFrame::default(),
            }
        }

        fn render(&mut self, canvas: &mut Canvas, _theme: &Theme) {
            canvas.fill_rect(Rect::new(10, 10, 120, 120), self.box_color);
        }
    }

    fn temp_png_path(label: &str) -> PathBuf {
        let seq = TEST_SEQ.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir()
            .join(format!("rabbiz-harness-{label}-{}-{seq}", std::process::id()))
            .join("frame.png")
    }

    #[test]
    fn harness_drives_touch_swipe_thai_text_ime_and_settle() {
        let mut h = TestHarness::new(DummyApp::new()).with_size(Size::new(240, 320));
        assert_eq!(h.size(), Size::new(240, 320));
        assert!(!h.ime_active());

        h.tap(Point::new(20, 40));
        assert!(h.ime_active());
        assert_eq!(h.ime_purpose(), Some(ContentPurpose::Normal));
        assert_eq!(h.state().taps, 1);

        h.type_text("สวัสดี");
        assert_eq!(h.state().buf.as_str(), "สวัสดี");
        assert_eq!(h.settle(10, Duration::from_millis(16)), 3);
        assert!(!h.last_frame().animating);

        h.backspace(1);
        assert_eq!(h.state().buf.as_str(), "สวัส");

        h.swipe(
            Point::new(120, 260),
            Point::new(120, 140),
            4,
            Duration::from_millis(16),
        );
        assert!(h.state().swiped);
        assert!(!h.ime_active());

        h.key(0xFF1B, None);
        assert!(h.quit_requested());
    }

    #[test]
    fn harness_save_png_and_golden_compare_detect_match_and_diff() {
        let path = temp_png_path("golden");
        let mut h = TestHarness::new(DummyApp::new())
            .with_size(Size::new(160, 160))
            .with_theme(tokens::DARK);
        h.save_png(&path).expect("save golden PNG");

        let exact = h.compare_golden(&path).expect("compare matching golden");
        assert!(exact.pass, "expected PASS, got {exact}");
        assert_eq!(exact.mae, 0.0);

        h.state_mut().box_color = [0x3F, 0x9E, 0x8A, 0xFF];
        let changed = h.compare_golden(&path).expect("compare changed golden");
        assert!(!changed.pass, "expected FAIL after color change, got {changed}");

        if let Some(dir) = path.parent() {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}
