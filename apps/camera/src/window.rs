use crate::photo;
use crate::preview::{Framing, Preview, Source, HEIGHT};
use crate::video::Recording;
use crate::viewfinder::{
    draw_chrome, draw_frame, draw_recent, draw_zoom, mode_tabs, Controls, Mode, CREAM, FLASH, FLIP,
    GRID, HEIGHT_PX as WINDOW_HEIGHT, RECENT, SHUTTER, WIDTH as WINDOW_WIDTH, ZOOM_ONE, ZOOM_TWO,
};
use anyhow::Result;
use de_api::{Rect, Size};
use de_paint::Canvas;
use minifb::{Key, KeyRepeat, MouseButton, MouseMode, Window, WindowOptions};
use std::path::Path;
use std::sync::mpsc::TryRecvError;
use std::time::{Duration, Instant};

fn zoom_center(frame: &[u8], output: &mut [u8]) {
    let source = frame.as_chunks::<3>().0;
    for (index, pixel) in output.as_chunks_mut::<3>().0.iter_mut().enumerate() {
        let x = WINDOW_WIDTH / 4 + (index % WINDOW_WIDTH) / 2;
        let y = HEIGHT / 4 + (index / WINDOW_WIDTH) / 2;
        *pixel = source[y * WINDOW_WIDTH + x];
    }
}

fn under_pointer(window: &Window, rect: Rect) -> bool {
    window
        .get_mouse_pos(MouseMode::Discard)
        .is_some_and(|(x, y)| {
            x >= rect.x as f32
                && x < (rect.x + rect.w) as f32
                && y >= rect.y as f32
                && y < (rect.y + rect.h) as f32
        })
}

fn report_saved(result: Result<std::path::PathBuf>) -> String {
    match result {
        Ok(path) => {
            println!("Saved {}", path.display());
            format!(
                "Saved {}",
                path.file_name().unwrap_or_default().to_string_lossy()
            )
        }
        Err(error) => {
            eprintln!("Capture failed: {error:#}");
            String::from("Could not save capture. See terminal for details.")
        }
    }
}

pub fn run(
    mut preview: Preview,
    source: Source,
    alternate: Option<Source>,
    output: &Path,
) -> Result<()> {
    let sources = [Some(source), alternate];
    let mut active = 0;
    let mut mode = Mode::Photo;
    let mut recording: Option<Recording> = None;
    let mut screen_flash = false;
    let mut flash_until = None;
    let mut window = Window::new(
        "Rabbiz Camera",
        WINDOW_WIDTH,
        WINDOW_HEIGHT,
        WindowOptions::default(),
    )?;
    window.set_target_fps(30);
    let mut canvas = Canvas::new(Size::new(WINDOW_WIDTH as i32, WINDOW_HEIGHT as i32));
    let mut buffer = vec![0; WINDOW_WIDTH * WINDOW_HEIGHT];
    let mut current = None;
    let mut status = String::from("Opening preview…");
    let mut started = Instant::now();
    let mut mouse_down = false;
    let mut ended = false;
    let mut grid = true;
    let mut doubled = false;
    let mut zoomed = vec![0; WINDOW_WIDTH * HEIGHT * 3];
    let mut recent: Option<Vec<u8>> = None;
    let mut reviewing = false;
    let mut refresh_chrome = true;

    while window.is_open() && !window.is_key_down(Key::Escape) {
        let down = window.get_mouse_down(MouseButton::Left);
        let clicked = down && !mouse_down;
        let pressed = (clicked && under_pointer(&window, SHUTTER))
            || window.is_key_pressed(Key::Space, KeyRepeat::No);
        if (clicked && under_pointer(&window, GRID)) || window.is_key_pressed(Key::G, KeyRepeat::No)
        {
            grid = !grid;
            refresh_chrome = true;
        }
        if recording.is_none()
            && flash_until.is_none()
            && recent.is_some()
            && ((clicked && under_pointer(&window, RECENT))
                || window.is_key_pressed(Key::R, KeyRepeat::No))
        {
            reviewing = !reviewing;
            refresh_chrome = true;
        }
        mouse_down = down;
        if clicked && recording.is_none() && flash_until.is_none() {
            if let Some((selected, _)) = mode_tabs(mode)
                .into_iter()
                .find(|&(_, rect)| under_pointer(&window, rect))
            {
                mode = selected;
                reviewing = false;
                status.clear();
                refresh_chrome = true;
            } else if mode == Mode::Photo && under_pointer(&window, FLASH) {
                screen_flash = !screen_flash;
                status = String::from(if screen_flash {
                    "Screen flash on"
                } else {
                    "Screen flash off"
                });
                refresh_chrome = true;
            } else if under_pointer(&window, FLIP) {
                if let Some(next) = &sources[1 - active] {
                    match Preview::start(next, Framing::Portrait) {
                        Ok(next_preview) => {
                            preview = next_preview;
                            active = 1 - active;
                            current = None;
                            ended = false;
                            started = Instant::now();
                            reviewing = false;
                            status = String::from("Opening alternate input…");
                        }
                        Err(error) => {
                            eprintln!("Switch input failed: {error:#}");
                            status = String::from("Could not open alternate input");
                        }
                    }
                    refresh_chrome = true;
                }
            }
        }

        let flash_complete = flash_until.is_some_and(|until| Instant::now() >= until);
        if flash_complete {
            flash_until = None;
            refresh_chrome = true;
        }
        if (pressed || flash_complete) && !ended && !reviewing && flash_until.is_none() {
            if let Some(frame) = current.as_deref() {
                let frame = if doubled { zoomed.as_slice() } else { frame };
                if mode == Mode::Video {
                    if let Some(video) = recording.take() {
                        status = report_saved(video.finish());
                    } else {
                        match Recording::start(output, frame) {
                            Ok(video) => {
                                recording = Some(video);
                                status = String::from("Recording · tap shutter to stop");
                            }
                            Err(error) => status = report_saved(Err(error)),
                        }
                    }
                } else if screen_flash && !flash_complete {
                    flash_until = Some(Instant::now() + Duration::from_millis(250));
                } else {
                    let result = photo::next_path(output)
                        .and_then(|path| photo::save(&path, frame, WINDOW_WIDTH).map(|()| path));
                    if result.is_ok() {
                        recent = Some(frame.to_vec());
                    }
                    status = report_saved(result);
                }
                refresh_chrome = true;
            }
        }

        if clicked && !reviewing && flash_until.is_none() {
            if under_pointer(&window, ZOOM_ONE) {
                doubled = false;
            } else if under_pointer(&window, ZOOM_TWO) {
                doubled = true;
            }
        }

        if !ended {
            match preview.frames.try_recv() {
                Ok(frame) => {
                    if current.is_none() {
                        status.clear();
                        refresh_chrome = true;
                    }
                    if let Some(video) = &recording {
                        let recorded = if doubled {
                            zoom_center(&frame, &mut zoomed);
                            zoomed.as_slice()
                        } else {
                            &frame
                        };
                        if let Err(error) = video.add(recorded) {
                            eprintln!("Recording interrupted: {error:#}");
                            if let Some(video) = recording.take() {
                                status = report_saved(video.finish());
                            }
                            refresh_chrome = true;
                        }
                    }
                    current = Some(frame);
                }
                Err(TryRecvError::Disconnected) => {
                    ended = true;
                    status = if let Some(video) = recording.take() {
                        report_saved(video.finish())
                    } else {
                        String::from("Preview ended. Check the input.")
                    };
                    refresh_chrome = true;
                }
                Err(TryRecvError::Empty) => {
                    if current.is_none() && started.elapsed() > Duration::from_secs(10) {
                        let message = "Waiting for input. Check camera access.";
                        refresh_chrome |= status != message;
                        status = String::from(message);
                    }
                }
            }
        }
        if refresh_chrome {
            draw_chrome(
                &mut canvas,
                if sources[1].is_some() {
                    if active == 0 {
                        "Input 1"
                    } else {
                        "Input 2"
                    }
                } else {
                    sources[0].as_ref().map_or("", Source::label)
                },
                if reviewing {
                    "Latest photo · R to return"
                } else {
                    &status
                },
                Controls {
                    ready: current.is_some() && !ended && !reviewing,
                    grid,
                    screen_flash,
                    can_flip: sources[1].is_some() && recording.is_none(),
                    mode,
                    recording: recording.is_some(),
                },
            );
            refresh_chrome = false;
        }
        if reviewing {
            if let Some(frame) = recent.as_deref() {
                draw_frame(&mut canvas, frame, false);
            }
        } else if let Some(frame) = current.as_deref() {
            let shown = if doubled {
                zoom_center(frame, &mut zoomed);
                zoomed.as_slice()
            } else {
                frame
            };
            draw_frame(&mut canvas, shown, grid);
            draw_zoom(&mut canvas, doubled);
        }
        if let Some(frame) = recent.as_deref() {
            draw_recent(&mut canvas, frame, reviewing);
        }
        if flash_until.is_some() {
            canvas.clear(CREAM);
        }
        for (out, rgba) in buffer
            .iter_mut()
            .zip(canvas.pixmap().data().as_chunks::<4>().0)
        {
            *out = u32::from_be_bytes([0, rgba[0], rgba[1], rgba[2]]);
        }
        window.update_with_buffer(&buffer, WINDOW_WIDTH, WINDOW_HEIGHT)?;
    }
    if let Some(video) = recording {
        println!("Saved {}", video.finish()?.display());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::viewfinder::VIEW_Y;

    #[test]
    fn viewfinder_and_still_use_the_same_portrait_frame() {
        let mut frame = vec![0; WINDOW_WIDTH * HEIGHT * 3];
        frame[..3].copy_from_slice(&[12, 34, 56]);
        let mut canvas = Canvas::new(Size::new(WINDOW_WIDTH as i32, WINDOW_HEIGHT as i32));
        draw_frame(&mut canvas, &frame, true);
        let pixels = canvas.pixmap().data().as_chunks::<4>().0;
        assert_eq!(pixels[VIEW_Y * WINDOW_WIDTH], [12, 34, 56, 255]);
        assert_ne!(
            pixels[VIEW_Y * WINDOW_WIDTH + WINDOW_WIDTH / 3],
            [0, 0, 0, 255]
        );

        let directory = tempfile::tempdir().expect("temporary photo folder");
        let path = directory.path().join("still.png");
        photo::save(&path, &frame, WINDOW_WIDTH).expect("save portrait");
        let file = std::fs::File::open(path).expect("open portrait");
        let mut reader = png::Decoder::new(std::io::BufReader::new(file))
            .read_info()
            .expect("read PNG metadata");
        let mut decoded = vec![0; frame.len()];
        let info = reader.next_frame(&mut decoded).expect("read photo pixels");
        assert_eq!((info.width, info.height), (360, 480));
        assert_eq!(decoded, frame);
    }
}
