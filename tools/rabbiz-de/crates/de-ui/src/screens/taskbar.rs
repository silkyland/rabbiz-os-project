//! Desktop Mode bottom taskbar: launcher button, running window pills, tray
//! indicator, quick-settings trigger, and clock.

use de_api::{Pixels, Point, Rect, Size};
use de_design::tokens::Theme;
use de_paint::{text::LabelStyle, Canvas};

use crate::chrome;

/// Height of the Desktop Mode bottom taskbar in logical px.
pub const TASKBAR_H: i32 = 48;

const PAD_X: i32 = 12;
const LAUNCHER_W: i32 = 44;
const PILL_W: i32 = 148;
const PILL_GAP: i32 = 8;
const TRAY_W: i32 = 52;
const QS_W: i32 = 112;

/// One running app pill shown on the taskbar.
#[derive(Debug, Clone, Copy)]
pub struct TaskbarApp<'a> {
    pub label: &'a str,
    pub focused: bool,
    pub minimized: bool,
}

/// What a tap/click on the taskbar hit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskbarHit {
    Launcher,
    Window(usize),
    Tray,
    QuickSettings,
}

/// Complete taskbar render model.
#[derive(Debug, Clone, Copy)]
pub struct TaskbarModel<'a> {
    pub apps: &'a [TaskbarApp<'a>],
    pub tray_count: usize,
    pub clock: &'a str,
    pub launcher_open: bool,
    pub hovered: Option<TaskbarHit>,
}

fn launcher_rect(size: Size) -> Rect {
    Rect::new(PAD_X, size.h - TASKBAR_H + 6, LAUNCHER_W, TASKBAR_H - 12)
}

fn qs_rect(size: Size) -> Rect {
    Rect::new(
        size.w - PAD_X - QS_W,
        size.h - TASKBAR_H + 6,
        QS_W,
        TASKBAR_H - 12,
    )
}

fn tray_rect(size: Size) -> Rect {
    let qs = qs_rect(size);
    Rect::new(qs.x - TRAY_W - 6, qs.y, TRAY_W, qs.h)
}

fn window_rects(size: Size, count: usize) -> Vec<Rect> {
    let start_x = PAD_X + LAUNCHER_W + 12;
    let right_limit = tray_rect(size).x - 12;
    let avail = (right_limit - start_x).max(0);
    if count == 0 || avail < 64 {
        return Vec::new();
    }
    let pill_w = ((avail - PILL_GAP * (count as i32 - 1)) / count as i32).clamp(64, PILL_W);
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let x = start_x + i as i32 * (pill_w + PILL_GAP);
        if x + pill_w > right_limit {
            break;
        }
        out.push(Rect::new(x, size.h - TASKBAR_H + 6, pill_w, TASKBAR_H - 12));
    }
    out
}

/// Hit-test a pointer/touch coordinate inside the taskbar strip.
#[must_use]
pub fn hit(size: Size, pos: Point, app_count: usize) -> Option<TaskbarHit> {
    if pos.y < size.h - TASKBAR_H || pos.y >= size.h {
        return None;
    }
    if launcher_rect(size).contains(pos) {
        return Some(TaskbarHit::Launcher);
    }
    if qs_rect(size).contains(pos) {
        return Some(TaskbarHit::QuickSettings);
    }
    if tray_rect(size).contains(pos) {
        return Some(TaskbarHit::Tray);
    }
    for (i, r) in window_rects(size, app_count).iter().enumerate() {
        if r.contains(pos) {
            return Some(TaskbarHit::Window(i));
        }
    }
    None
}

/// Render the full-width bottom taskbar strip (`size.w × TASKBAR_H`).
#[must_use]
pub fn render(size: Size, theme: &Theme, model: &TaskbarModel<'_>) -> Pixels {
    let mut c = Canvas::new(Size::new(size.w, TASKBAR_H));
    let bar = Rect::new(0, 0, size.w, TASKBAR_H);
    c.fill_rect(bar, theme.surface1);
    c.fill_rect(Rect::new(0, 0, size.w, 1), theme.line);

    let to_local = |r: Rect| Rect::new(r.x, r.y - (size.h - TASKBAR_H), r.w, r.h);

    // Launcher button
    let lr = to_local(launcher_rect(size));
    let l_bg = if model.launcher_open || model.hovered == Some(TaskbarHit::Launcher) {
        theme.accent
    } else {
        theme.surface2
    };
    let l_ink = if model.launcher_open || model.hovered == Some(TaskbarHit::Launcher) {
        theme.text_on_accent
    } else {
        theme.text
    };
    c.fill_rounded_rect(lr, 10.0, l_bg);
    chrome::draw_ink_centered(
        &mut c,
        "⊞",
        &LabelStyle {
            size: 18.0,
            line_height: 22.0,
            color: l_ink,
            ..LabelStyle::default()
        },
        lr,
    );

    // Running window pills
    for (i, r) in window_rects(size, model.apps.len()).into_iter().enumerate() {
        let local = to_local(r);
        let app = model.apps[i];
        let bg = if app.focused {
            theme.surface3
        } else if model.hovered == Some(TaskbarHit::Window(i)) {
            theme.surface2
        } else {
            theme.bg
        };
        c.fill_rounded_rect(local, 10.0, bg);
        if app.focused {
            let pill = Rect::new(local.x + 12, local.y + local.h - 3, local.w - 24, 2);
            c.fill_rect(pill, theme.accent);
        }
        let ink = if app.minimized { theme.text2 } else { theme.text };
        chrome::draw(
            &mut c,
            app.label,
            &LabelStyle {
                size: 13.0,
                line_height: 17.0,
                color: ink,
                ..LabelStyle::default()
            },
            local.x + 10,
            local.y + 9,
            Some(local.w - 20),
        );
    }

    // Tray badge
    let tr = to_local(tray_rect(size));
    let t_bg = if model.hovered == Some(TaskbarHit::Tray) {
        theme.surface3
    } else {
        theme.surface2
    };
    c.fill_rounded_rect(tr, 10.0, t_bg);
    let tray_label = if model.tray_count == 0 {
        "•".to_string()
    } else {
        format!("• {}", model.tray_count)
    };
    chrome::draw_ink_centered(
        &mut c,
        &tray_label,
        &LabelStyle {
            size: 13.0,
            line_height: 17.0,
            color: theme.text2,
            ..LabelStyle::default()
        },
        tr,
    );

    // Quick settings + Clock
    let qr = to_local(qs_rect(size));
    let q_bg = if model.hovered == Some(TaskbarHit::QuickSettings) {
        theme.surface3
    } else {
        theme.surface2
    };
    c.fill_rounded_rect(qr, 10.0, q_bg);
    chrome::draw_ink_centered(
        &mut c,
        model.clock,
        &LabelStyle {
            size: 13.0,
            line_height: 17.0,
            color: theme.text,
            ..LabelStyle::default()
        },
        qr,
    );

    c.into_pixels()
}

#[cfg(test)]
mod tests {
    use super::*;
    use de_design::tokens::DARK;

    #[test]
    fn hit_tests_launcher_windows_tray_and_qs() {
        let size = Size::new(1600, 720);
        assert_eq!(
            hit(size, Point::new(24, 700), 2),
            Some(TaskbarHit::Launcher)
        );
        assert_eq!(
            hit(size, Point::new(1550, 700), 2),
            Some(TaskbarHit::QuickSettings)
        );
        assert_eq!(hit(size, Point::new(1450, 700), 2), Some(TaskbarHit::Tray));
        assert_eq!(
            hit(size, Point::new(100, 700), 2),
            Some(TaskbarHit::Window(0))
        );
        assert_eq!(hit(size, Point::new(100, 400), 2), None);
    }

    #[test]
    fn render_produces_exact_taskbar_buffer() {
        let size = Size::new(1600, 720);
        let apps = [
            TaskbarApp {
                label: "Notes",
                focused: true,
                minimized: false,
            },
            TaskbarApp {
                label: "Files",
                focused: false,
                minimized: false,
            },
        ];
        let px = render(
            size,
            &DARK,
            &TaskbarModel {
                apps: &apps,
                tray_count: 1,
                clock: "16:30",
                launcher_open: false,
                hovered: Some(TaskbarHit::Launcher),
            },
        );
        assert_eq!(px.size, Size::new(1600, TASKBAR_H));
        assert!(px.data.iter().any(|&b| b != 0));
    }
}
