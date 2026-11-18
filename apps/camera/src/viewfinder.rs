//! Handset camera controls in the RabbizOS Cream Glass palette.

use crate::preview::{HEIGHT, PORTRAIT_WIDTH};
use de_api::{Pixels, Point, Rect, Size};
use de_paint::{Canvas, LabelStyle};

pub const WIDTH: usize = PORTRAIT_WIDTH;
pub const HEIGHT_PX: usize = 800;
pub const VIEW_Y: usize = 80;
pub const FLASH: Rect = Rect::new(16, 20, 48, 48);
pub const GRID: Rect = Rect::new(296, 20, 48, 48);
pub const ZOOM_ONE: Rect = Rect::new(126, 556, 54, 48);
pub const ZOOM_TWO: Rect = Rect::new(180, 556, 54, 48);
const MODE_BEFORE: Rect = Rect::new(64, 616, 76, 48);
const MODE_CENTER: Rect = Rect::new(142, 616, 76, 48);
const MODE_AFTER: Rect = Rect::new(220, 616, 76, 48);
pub const SHUTTER: Rect = Rect::new(138, 670, 84, 84);
pub const RECENT: Rect = Rect::new(26, 686, 52, 52);
pub const FLIP: Rect = Rect::new(284, 688, 48, 48);

const NIGHT: [u8; 4] = [9, 7, 10, 255];
const PLUM: [u8; 4] = [48, 33, 53, 255];
const GLASS: [u8; 4] = [75, 59, 78, 223];
pub const CREAM: [u8; 4] = [244, 237, 225, 255];
const DIM: [u8; 4] = [191, 174, 180, 255];
const COPPER: [u8; 4] = [184, 104, 58, 255];
const COPPER_GLASS: [u8; 4] = [150, 76, 40, 255];
const RED: [u8; 4] = [224, 89, 82, 255];

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Photo,
    Video,
}

pub fn mode_tabs(active: Mode) -> [(Mode, Rect); 2] {
    match active {
        Mode::Photo => [(Mode::Video, MODE_BEFORE), (Mode::Photo, MODE_CENTER)],
        Mode::Video => [(Mode::Video, MODE_CENTER), (Mode::Photo, MODE_AFTER)],
    }
}

pub struct Controls {
    pub ready: bool,
    pub grid: bool,
    pub screen_flash: bool,
    pub can_flip: bool,
    pub mode: Mode,
    pub recording: bool,
}

fn ink_bounds(width: i32, rgba: &[u8]) -> Option<Rect> {
    let mut left = width;
    let mut top = i32::MAX;
    let mut right = 0;
    let mut bottom = 0;
    for (index, pixel) in rgba.as_chunks::<4>().0.iter().enumerate() {
        if pixel[3] >= 16 {
            let x = index as i32 % width;
            let y = index as i32 / width;
            left = left.min(x);
            top = top.min(y);
            right = right.max(x + 1);
            bottom = bottom.max(y + 1);
        }
    }
    (right > left).then(|| Rect::new(left, top, right - left, bottom - top))
}

fn center_label(canvas: &mut Canvas, text: &str, area: Rect, style: LabelStyle) -> Option<Rect> {
    let raster = de_paint::label(text, style, Some(area.w))?;
    let size = Size::new(raster.width() as i32, raster.height() as i32);
    let bounds = ink_bounds(size.w, raster.data())?;
    let x = area.x + (area.w - bounds.w) / 2;
    let y = area.y + (area.h - bounds.h) / 2;
    canvas.blit(
        &Pixels::new(size, raster.take()),
        Point::new(x - bounds.x, y - bounds.y),
    );
    Some(Rect::new(x, y, bounds.w, bounds.h))
}

fn label(canvas: &mut Canvas, text: &str, area: Rect, size: f32, color: [u8; 4], weight: u16) {
    center_label(
        canvas,
        text,
        area,
        LabelStyle {
            size,
            line_height: size + 4.0,
            color,
            weight,
            ..LabelStyle::default()
        },
    );
}

fn icon(canvas: &mut Canvas, glyph: &str, button: Rect, active: bool) {
    let (background, foreground) = if active { (CREAM, PLUM) } else { (PLUM, CREAM) };
    canvas.fill_rounded_rect(button, 24.0, background);
    center_label(canvas, glyph, button, LabelStyle::icon(24.0, foreground));
}

pub fn draw_chrome(canvas: &mut Canvas, source: &str, status: &str, controls: Controls) {
    canvas.clear(NIGHT);
    canvas.fill_rect(
        Rect::new(0, VIEW_Y as i32, WIDTH as i32, HEIGHT as i32),
        [17, 12, 18, 255],
    );

    if controls.mode == Mode::Photo {
        let glyph = if controls.screen_flash {
            "\u{e3e7}"
        } else {
            "\u{e3e6}"
        };
        icon(canvas, glyph, FLASH, controls.screen_flash);
    }
    icon(canvas, "\u{e3ec}", GRID, controls.grid);
    canvas.fill_rounded_rect(Rect::new(130, 24, 100, 38), 19.0, PLUM);
    label(canvas, "4:3", Rect::new(130, 28, 100, 17), 15.0, CREAM, 600);
    label(canvas, source, Rect::new(130, 45, 100, 12), 10.0, DIM, 400);

    for (mode, rect) in mode_tabs(controls.mode) {
        let name = if mode == Mode::Photo {
            "PHOTO"
        } else {
            "VIDEO"
        };
        let selected = mode == controls.mode;
        let color = if selected { CREAM } else { DIM };
        label(
            canvas,
            name,
            Rect::new(rect.x, rect.y, rect.w, 26),
            14.0,
            color,
            600,
        );
        if selected {
            canvas.fill_rounded_rect(
                Rect::new(rect.x + rect.w / 2 - 3, rect.y + 29, 6, 6),
                3.0,
                COPPER,
            );
        }
    }

    if controls.can_flip {
        icon(canvas, "\u{efeb}", FLIP, false);
    }

    canvas.fill_rounded_rect(SHUTTER, 42.0, GLASS);
    canvas.fill_rounded_rect(Rect::new(142, 674, 76, 76), 38.0, COPPER);
    canvas.fill_rounded_rect(Rect::new(144, 676, 72, 72), 36.0, NIGHT);
    if controls.recording {
        canvas.fill_rounded_rect(Rect::new(160, 692, 40, 40), 8.0, RED);
    } else {
        let fill = match (controls.ready, controls.mode) {
            (false, _) => DIM,
            (true, Mode::Photo) => CREAM,
            (true, Mode::Video) => RED,
        };
        canvas.fill_rounded_rect(Rect::new(147, 679, 66, 66), 33.0, fill);
        if controls.mode == Mode::Photo && controls.ready {
            canvas.fill_rounded_rect(Rect::new(177, 709, 6, 6), 3.0, [222, 191, 172, 255]);
        }
    }

    let feedback = LabelStyle {
        size: 12.0,
        line_height: 15.0,
        color: DIM,
        ..LabelStyle::default()
    };
    // Offset the text by half the space occupied by the recording indicator.
    let center_offset = if controls.recording { 7 } else { 0 };
    let status_area = Rect::new(20 + center_offset, 766, 320, 25);
    if let Some(ink) = center_label(canvas, status, status_area, feedback) {
        if controls.recording {
            canvas.fill_rounded_rect(
                Rect::new(ink.x - 14, ink.y + (ink.h - 6) / 2, 6, 6),
                3.0,
                RED,
            );
        }
    }
}

pub fn draw_frame(canvas: &mut Canvas, frame: &[u8], grid: bool) {
    let pixels = canvas.pixmap_mut().data_mut().as_chunks_mut::<4>().0;
    let area = &mut pixels[VIEW_Y * WIDTH..(VIEW_Y + HEIGHT) * WIDTH];
    for (out, &[r, g, b]) in area.iter_mut().zip(frame.as_chunks::<3>().0) {
        *out = [r, g, b, 255];
    }
    if grid {
        let line = [255, 245, 236, 83];
        for fraction in 1..=2 {
            let x = (WIDTH * fraction / 3) as i32;
            let y = (VIEW_Y + HEIGHT * fraction / 3) as i32;
            canvas.fill_rect(Rect::new(x, VIEW_Y as i32, 1, HEIGHT as i32), line);
            canvas.fill_rect(Rect::new(0, y, WIDTH as i32, 1), line);
        }
    }
}

pub fn draw_zoom(canvas: &mut Canvas, doubled: bool) {
    let capsule = Rect::new(ZOOM_ONE.x, ZOOM_ONE.y, ZOOM_ONE.w + ZOOM_TWO.w, ZOOM_ONE.h);
    let frame_end = (VIEW_Y + HEIGHT) as i32;
    canvas.fill_rect(
        Rect::new(
            capsule.x,
            frame_end,
            capsule.w,
            capsule.y + capsule.h - frame_end,
        ),
        NIGHT,
    );
    canvas.fill_rounded_rect(capsule, 24.0, GLASS);
    let selection = if doubled { ZOOM_TWO } else { ZOOM_ONE };
    let highlight = Rect::new(selection.x + 7, selection.y + 5, 40, 38);
    canvas.fill_rounded_rect(highlight, 19.0, COPPER_GLASS);
    for (text, target) in [("1×", ZOOM_ONE), ("2×", ZOOM_TWO)] {
        label(canvas, text, target, 14.0, CREAM, 600);
    }
}

pub fn draw_recent(canvas: &mut Canvas, photo: &[u8], reviewing: bool) {
    canvas.fill_rect(RECENT, NIGHT);
    canvas.fill_rounded_rect(RECENT, 13.0, GLASS);
    if reviewing {
        label(canvas, "Back", RECENT, 14.0, CREAM, 600);
        return;
    }

    let photo = photo.as_chunks::<3>().0;
    let pixels = canvas.pixmap_mut().data_mut().as_chunks_mut::<4>().0;
    let size = 48_usize;
    for y in 0..size {
        for x in 0..size {
            // Clip the scaled photo to the same rounded square as its frame.
            let corner_x = x.abs_diff(x.clamp(10, size - 11));
            let corner_y = y.abs_diff(y.clamp(10, size - 11));
            if corner_x * corner_x + corner_y * corner_y <= 100 {
                let [r, g, b] = photo[y * HEIGHT / size * WIDTH + x * WIDTH / size];
                let offset = (RECENT.y as usize + y + 2) * WIDTH + RECENT.x as usize + x + 2;
                pixels[offset] = [r, g, b, 255];
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn google_icon_ink_is_centered_within_each_button() {
        let button = Rect::new(0, 0, 48, 48);
        for glyph in ["\u{e3e6}", "\u{e3e7}", "\u{e3ec}", "\u{efeb}"] {
            let mut canvas = Canvas::new(Size::new(button.w, button.h));
            center_label(&mut canvas, glyph, button, LabelStyle::icon(24.0, CREAM))
                .expect("bundled Google Material icon");
            let ink = ink_bounds(button.w, canvas.pixmap().data()).expect("visible icon ink");
            assert!((2 * ink.x + ink.w - button.w).abs() <= 1);
            assert!((2 * ink.y + ink.h - button.h).abs() <= 1);
        }
    }
}
