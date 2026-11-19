//! Clock layout, control geometry, and canvas rendering.

use std::time::Duration;

use de_api::{Pixels, Size};
use de_design::tokens;
use rabbiz_app_kit::{ui, Canvas, Rect, Theme};

use crate::model::{
    format_date, format_stopwatch, format_timer, lap_delta, local_now, next_alarm_text, Alarm, Tab,
    TAB_COUNT, TAB_LABELS,
};
use crate::ClockApp;

const RING_STROKE: f32 = 16.0;

/// What a control does — the single source for both hit-testing and drawing.
#[derive(Clone)]
pub(crate) struct Control {
    pub(crate) r: Rect,
    pub(crate) id: u32,
    pub(crate) kind: Kind,
}

#[derive(Clone)]
pub(crate) enum Kind {
    Tab(usize),
    Pill { label: String, primary: bool },
    Switch { on: bool },
    Stepper { dh: i32, dm: i32 },
}

pub(crate) fn tab_bar(w: i32) -> Rect {
    Rect::new(ui::MARGIN, ui::TOP_SAFE + 8, w - ui::MARGIN * 2, 56)
}

pub(crate) fn content_top(w: i32) -> i32 {
    tab_bar(w).bottom() + 20
}

pub(crate) fn content_bottom(h: i32) -> i32 {
    h - ui::BOTTOM_SAFE - 16
}

/// Every tappable control for the current state at size (w,h).
/// `render` draws from this same list, so hit-testing can never drift
/// from what's on screen (no hardcoded 720x1600 anywhere).
pub(crate) fn controls(app: &ClockApp, w: i32, h: i32) -> Vec<Control> {
    let mut out = Vec::new();
    let bar = tab_bar(w);
    let seg = bar.w / TAB_COUNT as i32;
    for i in 0..TAB_COUNT {
        out.push(Control {
            r: Rect::new(bar.x + seg * i as i32, bar.y, seg, bar.h),
            id: i as u32,
            kind: Kind::Tab(i),
        });
    }
    // The alert banner pushes tab content down; controls must match.
    let top = content_top(w) + if app.alert.is_some() { 92 } else { 0 };
    let bottom = content_bottom(h);
    match app.tab {
        Tab::Clock => {}
        Tab::Alarm => {
            // "New alarm" / "Close" pill at the section's top-right.
            let bw = 180;
            out.push(Control {
                r: Rect::new(w - ui::MARGIN - bw, top, bw, 52),
                id: 199,
                kind: Kind::Pill {
                    label: if app.adding {
                        "Close".into()
                    } else {
                        "New alarm".into()
                    },
                    primary: !app.adding,
                },
            });
            let mut y = top + 68;
            if app.adding {
                // Add panel card: two stepper columns + save/cancel.
                let panel = Rect::new(ui::MARGIN, y, w - ui::MARGIN * 2, 360);
                let col_w = (panel.w - 72) / 2;
                for col in 0..2 {
                    let cx = panel.x + 24 + col * (col_w + 24);
                    let is_hour = col == 0;
                    // − on the left, + on the right of each column.
                    out.push(Control {
                        r: Rect::new(cx, panel.y + 170, 88, 56),
                        id: 200 + col as u32 * 2,
                        kind: Kind::Stepper {
                            dh: if is_hour { -1 } else { 0 },
                            dm: if is_hour { 0 } else { -1 },
                        },
                    });
                    out.push(Control {
                        r: Rect::new(cx + col_w - 88, panel.y + 170, 88, 56),
                        id: 201 + col as u32 * 2,
                        kind: Kind::Stepper {
                            dh: if is_hour { 1 } else { 0 },
                            dm: if is_hour { 0 } else { 1 },
                        },
                    });
                }
                let bw2 = (panel.w - 48 - 16) / 2;
                out.push(Control {
                    r: Rect::new(panel.x + 24, panel.y + panel.h - 72, bw2, 56),
                    id: 210,
                    kind: Kind::Pill {
                        label: "Save".into(),
                        primary: true,
                    },
                });
                out.push(Control {
                    r: Rect::new(panel.x + 24 + bw2 + 16, panel.y + panel.h - 72, bw2, 56),
                    id: 211,
                    kind: Kind::Pill {
                        label: "Cancel".into(),
                        primary: false,
                    },
                });
                y += panel.h + 16;
            }
            // Alarm rows: switch toggles, × deletes.
            let sorted = app.sorted_alarms();
            for (i, a) in sorted.iter().enumerate() {
                if y + 76 > bottom {
                    break;
                }
                let r = Rect::new(ui::MARGIN, y, w - ui::MARGIN * 2, 76);
                out.push(Control {
                    r: Rect::new(r.right() - ui::MARGIN - 52, r.y + 12, 52, 52),
                    id: 300 + i as u32 * 2 + 1,
                    kind: Kind::Pill {
                        label: "×".into(),
                        primary: false,
                    },
                });
                out.push(Control {
                    r: Rect::new(r.right() - ui::MARGIN - 52 - 84, r.y + 22, 56, 32),
                    id: 300 + i as u32 * 2,
                    kind: Kind::Switch { on: a.enabled },
                });
                y += 88;
            }
        }
        Tab::Timer => {
            let bw = (w - ui::MARGIN * 2 - 48) / 4;
            let cy = bottom - 140;
            for (i, label) in ["+1m", "+5m", "+10m", "Reset"].iter().enumerate() {
                out.push(Control {
                    r: Rect::new(ui::MARGIN + i as i32 * (bw + 16), cy, bw, 52),
                    id: 20 + i as u32,
                    kind: Kind::Pill {
                        label: label.to_string(),
                        primary: false,
                    },
                });
            }
            out.push(Control {
                r: Rect::new(ui::MARGIN, bottom - 72, w - ui::MARGIN * 2, 64),
                id: 24,
                kind: Kind::Pill {
                    label: if app.tm_finished() {
                        "Reset".into()
                    } else if app.tm_running {
                        "Pause".into()
                    } else {
                        "Start".into()
                    },
                    primary: true,
                },
            });
        }
        Tab::Stopwatch => {
            let bw = (w - ui::MARGIN * 2 - 16) / 2;
            out.push(Control {
                r: Rect::new(ui::MARGIN, bottom - 72, bw, 64),
                id: 10,
                kind: Kind::Pill {
                    label: if app.sw_running {
                        "Pause".into()
                    } else {
                        "Start".into()
                    },
                    primary: true,
                },
            });
            out.push(Control {
                r: Rect::new(ui::MARGIN + bw + 16, bottom - 72, bw, 64),
                id: 11,
                kind: Kind::Pill {
                    label: if app.sw_running {
                        "Lap".into()
                    } else {
                        "Reset".into()
                    },
                    primary: false,
                },
            });
        }
    }
    out
}

pub(crate) fn render(app: &mut ClockApp, canvas: &mut Canvas, theme: &Theme) {
    let size = canvas.size();
    let (w, h) = (size.w, size.h);
    app.view = (w, h);
    let top = content_top(w);
    let bottom = content_bottom(h);
    let ctrls = controls(app, w, h);

    // ---- segmented tab bar with sliding accent indicator ----
    let bar = tab_bar(w);
    canvas.fill_rounded_rect(bar, 28.0, theme.surface1);
    let seg = bar.w / TAB_COUNT as i32;
    let frac = app.tab_frac();
    if !app.tab_anim_active() {
        app.tab_anim = None;
    }
    let ix = bar.x + (frac * seg as f32) as i32;
    canvas.fill_rounded_rect(
        Rect::new(ix + 6, bar.y + 6, seg - 12, bar.h - 12),
        22.0,
        theme.accent,
    );
    for c in &ctrls {
        if let Kind::Tab(i) = c.kind {
            let active = i == app.tab.index();
            let fg = if active {
                theme.text_on_accent
            } else {
                theme.text2
            };
            let r = if app.pressed == Some(c.id) {
                Rect::new(c.r.x + 3, c.r.y + 3, c.r.w - 6, c.r.h - 6)
            } else {
                c.r
            };
            ui::draw_centered(
                canvas,
                TAB_LABELS[i],
                &ui::styled(20.0, if active { 600 } else { 500 }, fg),
                r,
            );
        }
    }

    // ---- in-app alert banner ----
    let mut body_top = top;
    if let Some(alert) = &app.alert {
        let br = Rect::new(ui::MARGIN, top, w - ui::MARGIN * 2, 76);
        canvas.fill_rounded_rect(br, tokens::RADIUS_ROW, theme.surface2);
        canvas.fill_rounded_rect(Rect::new(br.x, br.y, 6, br.h), 3.0, theme.accent);
        ui::draw_text(
            canvas,
            &alert.title,
            &ui::label(theme.text),
            br.x + 24,
            br.y + 10,
            Some(br.w - 48),
        );
        ui::draw_text(
            canvas,
            &alert.body,
            &ui::caption(theme.text2),
            br.x + 24,
            br.y + 42,
            Some(br.w - 48),
        );
        body_top = br.bottom() + 16;
    }

    match app.tab {
        Tab::Clock => render_clock(app, canvas, theme, w, body_top, bottom),
        Tab::Alarm => render_alarm(app, canvas, theme, w, body_top, bottom, &ctrls),
        Tab::Timer => render_timer(app, canvas, theme, w, body_top, bottom, &ctrls),
        Tab::Stopwatch => render_stopwatch(app, canvas, theme, w, body_top, bottom, &ctrls),
    }
}

fn draw_control(app: &ClockApp, canvas: &mut Canvas, theme: &Theme, c: &Control) {
    let pressed = app.pressed == Some(c.id);
    match &c.kind {
        Kind::Pill { label, primary } => {
            ui::draw_pill_button(canvas, theme, c.r, label, *primary, pressed);
        }
        Kind::Switch { on } => {
            let r = if pressed {
                Rect::new(c.r.x - 4, c.r.y - 4, c.r.w + 8, c.r.h + 8)
            } else {
                c.r
            };
            canvas.fill_rounded_rect(r, 16.0, if *on { theme.accent } else { theme.surface2 });
            let kx = if *on { r.x + r.w - 4 - 24 } else { r.x + 4 };
            canvas.fill_rounded_rect(
                Rect::new(kx, r.y + 4, 24, 24),
                12.0,
                if *on {
                    theme.text_on_accent
                } else {
                    theme.text3
                },
            );
        }
        Kind::Stepper { dh, dm } => {
            let label = if *dh != 0 {
                if *dh > 0 {
                    "+1h"
                } else {
                    "−1h"
                }
            } else if *dm > 0 {
                "+1m"
            } else {
                "−1m"
            };
            ui::draw_pill_button(canvas, theme, c.r, label, false, pressed);
        }
        Kind::Tab(_) => {}
    }
}

fn render_clock(app: &ClockApp, canvas: &mut Canvas, theme: &Theme, w: i32, top: i32, bottom: i32) {
    let now = local_now();
    // Big time, in the upper part of the content area.
    let time_rect = Rect::new(
        ui::MARGIN,
        top + (bottom - top) / 5,
        w - ui::MARGIN * 2,
        150,
    );
    ui::draw_centered(
        canvas,
        &format!("{:02}:{:02}", now.hour, now.minute),
        &ui::styled(104.0, 600, theme.text),
        time_rect,
    );
    let date_rect = Rect::new(ui::MARGIN, time_rect.bottom() + 8, w - ui::MARGIN * 2, 44);
    ui::draw_centered(
        canvas,
        &format_date(&now),
        &ui::body(theme.text2),
        date_rect,
    );

    // Next-alarm card pinned above the bottom edge.
    let sorted: Vec<&Alarm> = {
        let mut v: Vec<&Alarm> = app.alarms.iter().collect();
        v.sort_by_key(|a| (a.hour, a.minute));
        v
    };
    let card_h = 88;
    let card = Rect::new(ui::MARGIN, bottom - card_h, w - ui::MARGIN * 2, card_h);
    canvas.fill_rounded_rect(card, tokens::RADIUS_CARD, theme.surface1);
    if let Some(text) = next_alarm_text(&sorted, &now) {
        ui::draw_text(
            canvas,
            &text,
            &ui::label(theme.text),
            card.x + 24,
            card.y + 18,
            Some(card.w - 48),
        );
        ui::draw_text(
            canvas,
            "Alarms ring while Clock is open",
            &ui::caption(theme.text3),
            card.x + 24,
            card.y + 50,
            Some(card.w - 48),
        );
    } else {
        ui::draw_centered(canvas, "No alarms set", &ui::body(theme.text3), card);
    }
}

fn render_alarm(
    app: &ClockApp,
    canvas: &mut Canvas,
    theme: &Theme,
    w: i32,
    top: i32,
    bottom: i32,
    controls: &[Control],
) {
    // Section label.
    ui::draw_text(
        canvas,
        "ALARMS",
        &ui::styled(18.0, 600, theme.text3),
        ui::MARGIN,
        top,
        Some(w - ui::MARGIN * 2),
    );
    let mut y = top + 68;

    // Add panel.
    if app.adding {
        let panel = Rect::new(ui::MARGIN, y, w - ui::MARGIN * 2, 360);
        canvas.fill_rounded_rect(panel, tokens::RADIUS_CARD, theme.surface1);
        ui::draw_centered(
            canvas,
            "NEW ALARM",
            &ui::styled(18.0, 600, theme.text3),
            Rect::new(panel.x, panel.y + 20, panel.w, 28),
        );
        let col_w = (panel.w - 72) / 2;
        for (col, (label, val)) in [("HOUR", app.draft_hour), ("MINUTE", app.draft_minute)]
            .iter()
            .enumerate()
        {
            let cx = panel.x + 24 + col as i32 * (col_w + 24);
            ui::draw_centered(
                canvas,
                label,
                &ui::styled(18.0, 600, theme.text3),
                Rect::new(cx, panel.y + 60, col_w, 26),
            );
            ui::draw_centered(
                canvas,
                &format!("{:02}", val),
                &ui::styled(56.0, 600, theme.text),
                Rect::new(cx, panel.y + 92, col_w, 72),
            );
        }
        y += panel.h + 16;
    }

    // Rows.
    let sorted = app.sorted_alarms();
    if sorted.is_empty() && !app.adding {
        let empty = Rect::new(ui::MARGIN, y + 40, w - ui::MARGIN * 2, 220);
        canvas.fill_rounded_rect(empty, tokens::RADIUS_CARD, theme.surface1);
        ui::draw_centered(
            canvas,
            "No alarms yet",
            &ui::title(theme.text),
            Rect::new(empty.x, empty.y + 56, empty.w, 48),
        );
        ui::draw_centered(
            canvas,
            "Tap New alarm and set a time with the steppers.",
            &ui::body(theme.text3),
            Rect::new(empty.x + 32, empty.y + 116, empty.w - 64, 80),
        );
    }
    for a in &sorted {
        if y + 76 > bottom {
            break;
        }
        let r = Rect::new(ui::MARGIN, y, w - ui::MARGIN * 2, 76);
        canvas.fill_rounded_rect(r, tokens::RADIUS_ROW, theme.surface1);
        let dim = if a.enabled { theme.text } else { theme.text3 };
        ui::draw_text(
            canvas,
            &format!("{:02}:{:02}", a.hour, a.minute),
            &ui::styled(36.0, 600, dim),
            r.x + 24,
            r.y + 16,
            Some(r.w - 220),
        );
        ui::draw_text(
            canvas,
            if a.enabled { "Every day" } else { "Off" },
            &ui::caption(theme.text3),
            r.x + 26,
            r.y + 56,
            Some(r.w - 220),
        );
        y += 88;
    }
    // Controls (switches, ×, panel buttons, New alarm) drawn on top.
    for c in controls {
        draw_control(app, canvas, theme, c);
    }
}

fn render_timer(
    app: &ClockApp,
    canvas: &mut Canvas,
    theme: &Theme,
    w: i32,
    top: i32,
    bottom: i32,
    controls: &[Control],
) {
    let remaining = app.current_tm();
    let finished = app.tm_finished() || (!app.tm_running && app.tm_bank.is_zero());
    let total = app.tm_total.max(Duration::from_secs(1));
    let frac = if finished {
        0.0
    } else {
        (remaining.as_secs_f64() / total.as_secs_f64()).clamp(0.0, 1.0)
    };

    // Ring centered in the space above the preset row.
    let ring_r = ((w - ui::MARGIN * 2) / 2 - 40).min(((bottom - top - 220) / 2).max(80));
    let cx = w / 2;
    let cy = top + (bottom - top - 220) / 2 + ring_r + 10;
    let ring_mid = ring_r as f32 + RING_STROKE / 2.0;

    // Track + progress arc, generated as premultiplied pixels.
    let track = ring_tile(ring_mid, RING_STROKE, None, theme.surface3);
    let ox = cx - track.size.w / 2;
    let oy = cy - track.size.h / 2;
    canvas.blit(&track, de_api::Point::new(ox, oy));
    let prog_color = if finished { theme.danger } else { theme.accent };
    let prog = ring_tile(ring_mid, RING_STROKE, Some(frac as f32), prog_color);
    canvas.blit(&prog, de_api::Point::new(ox, oy));

    // Time inside the ring.
    let inner = Rect::new(cx - ring_r, cy - ring_r, ring_r * 2, ring_r * 2);
    ui::draw_centered(
        canvas,
        &format_timer(remaining),
        &ui::styled(72.0, 600, if finished { theme.danger } else { theme.text }),
        Rect::new(inner.x, inner.y + inner.h / 2 - 60, inner.w, 120),
    );
    ui::draw_centered(
        canvas,
        &format!("of {}", format_timer(total)),
        &ui::caption(theme.text3),
        Rect::new(inner.x, inner.y + inner.h / 2 + 40, inner.w, 40),
    );
    if finished {
        ui::draw_centered(
            canvas,
            "Time's up",
            &ui::label(theme.danger),
            Rect::new(inner.x, inner.y + inner.h / 2 - 130, inner.w, 40),
        );
    }

    for c in controls {
        draw_control(app, canvas, theme, c);
    }
}

fn render_stopwatch(
    app: &ClockApp,
    canvas: &mut Canvas,
    theme: &Theme,
    w: i32,
    top: i32,
    bottom: i32,
    controls: &[Control],
) {
    let elapsed = app.current_sw();
    let time_rect = Rect::new(ui::MARGIN, top + 24, w - ui::MARGIN * 2, 140);
    ui::draw_centered(
        canvas,
        &format_stopwatch(elapsed),
        &ui::styled(88.0, 600, theme.text),
        time_rect,
    );

    // Lap rows, newest first, directly under the time.
    let mut y = time_rect.bottom() + 24;
    let row_h = 72;
    let max_rows = ((bottom - 72 - y) / (row_h + 16)).max(0) as usize;
    for row in 0..app.laps.len().min(max_rows) {
        let r = Rect::new(ui::MARGIN, y, w - ui::MARGIN * 2, row_h);
        canvas.fill_rounded_rect(r, tokens::RADIUS_ROW, theme.surface1);
        let lap_no = app.laps.len() - row;
        ui::draw_text(
            canvas,
            &format!("Lap {lap_no}"),
            &ui::label(theme.text2),
            r.x + 24,
            r.y + 14,
            Some(160),
        );
        let delta = lap_delta(&app.laps, row);
        ui::draw_text(
            canvas,
            &format!("+{}", format_stopwatch(delta)),
            &ui::caption(theme.text3),
            r.x + 24,
            r.y + 44,
            Some(220),
        );
        // Total, right-aligned via measured width approximation.
        let total_s = format_stopwatch(app.laps[app.laps.len() - 1 - row]);
        ui::draw_text(
            canvas,
            &total_s,
            &ui::styled(30.0, 600, theme.text),
            r.right() - ui::MARGIN - 260,
            r.y + 20,
            Some(240),
        );
        y += row_h + 16;
    }

    for c in controls {
        draw_control(app, canvas, theme, c);
    }
}

/// Build a square premultiplied-RGBA tile of a ring (track or arc).
/// `frac` = fraction of the circle covered, measured clockwise from 12 o'clock;
/// `None` = full track.
fn ring_tile(radius: f32, stroke: f32, frac: Option<f32>, color: [u8; 4]) -> Pixels {
    let half = (radius + stroke) as i32 + 2;
    let n = half * 2;
    let mut data = vec![0u8; (n * n * 4) as usize];
    let c = half as f32 - 0.5;
    let inner = radius - stroke / 2.0;
    let outer = radius + stroke / 2.0;
    let arc_end = frac.map(|f| f.clamp(0.0, 1.0) * std::f32::consts::TAU);
    for py in 0..n {
        for px in 0..n {
            let dx = px as f32 - c;
            let dy = py as f32 - c;
            let dist = (dx * dx + dy * dy).sqrt();
            // Coverage fades over ~1px at both edges for cheap AA.
            let cov = (outer + 0.6 - dist)
                .min(dist - (inner - 0.6))
                .clamp(0.0, 1.0);
            if cov <= 0.0 {
                continue;
            }
            if let Some(end) = arc_end {
                // Angle clockwise from top: 0 at 12 o'clock, TAU/4 at 3 o'clock.
                let ang = dy.atan2(dx) + std::f32::consts::FRAC_PI_2;
                let pos = ang.rem_euclid(std::f32::consts::TAU);
                if pos > end + 0.01 {
                    continue;
                }
            }
            // Premultiplied: rgb scaled by coverage × source alpha.
            let a = cov * f32::from(color[3]) / 255.0;
            let i = ((py * n + px) * 4) as usize;
            data[i] = (f32::from(color[0]) * a) as u8;
            data[i + 1] = (f32::from(color[1]) * a) as u8;
            data[i + 2] = (f32::from(color[2]) * a) as u8;
            data[i + 3] = (a * 255.0) as u8;
        }
    }
    Pixels::new(Size::new(n, n), data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controls_use_canvas_size_not_hardcoded() {
        let app = ClockApp::new();
        for (w, h) in [(720, 1600), (400, 800), (1080, 2400)] {
            for c in controls(&app, w, h) {
                assert!(c.r.w > 0 && c.r.h > 0, "degenerate rect at {w}x{h}");
                assert!(c.r.x >= 0 && c.r.y >= 0);
                assert!(c.r.right() <= w + ui::MARGIN, "control escapes width");
                assert!(c.r.bottom() <= h, "control escapes height at {w}x{h}");
            }
        }
    }

    #[test]
    fn ring_tile_coverage() {
        // Full track: center of a cardinal point is covered, far corner is not.
        let px = ring_tile(50.0, 10.0, None, [255, 0, 0, 255]);
        let at = |x: i32, y: i32| {
            let i = ((y * px.size.w + x) * 4) as usize;
            px.data[i + 3]
        };
        // Tile is 124x124, ring center ≈ (61.5, 61.5), band radius 45..55.
        assert!(at(62, 12) > 200, "top of ring should be covered");
        assert!(at(111, 62) > 200, "right of ring should be covered");
        assert_eq!(at(0, 0), 0, "corner is outside the ring");
        // Quarter arc covers only the first quadrant (top → right, clockwise).
        let q = ring_tile(50.0, 10.0, Some(0.25), [0, 255, 0, 255]);
        let atq = |x: i32, y: i32| {
            let i = ((y * q.size.w + x) * 4) as usize;
            q.data[i + 3]
        };
        assert!(atq(62, 12) > 200, "arc start (top) covered");
        assert!(atq(110, 60) > 200, "arc end (right) covered");
        assert_eq!(atq(12, 62), 0, "left side outside 25% arc");
        assert_eq!(atq(62, 111), 0, "bottom outside 25% arc");
    }
}
