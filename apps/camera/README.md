# Rabbiz Camera

A Rust camera app with a live viewfinder, still capture and video recording. It uses the
RabbizOS painting stack and runs independently of the desktop shell.

Install Rust and make FFmpeg available on `PATH`. To try the viewfinder
with an animated test pattern from the repository root:

```sh
cargo run --manifest-path apps/camera/Cargo.toml
```

Tap the round shutter or press **Space** to save the framed image.
Select **Video** and press the shutter to start or stop a 360×480,
24 fps H.264 MP4 recording. Video is silent. FFmpeg must include
`libx264`. In **Photo**, the **Flash** toggle briefly lights the screen
before capturing a frame. This is intended for a front-facing camera.

Click **Grid** or press **G** to toggle the composition grid. Use **1×**
and **2×** in the viewfinder for digital zoom. Click the latest-photo
thumbnail or press **R** to review it and return to the viewfinder.
Photos go to the working directory by default. Use `--output-dir` to
select an existing destination. Press **Esc** to close the app.

Use `--input sample.mp4` to loop a video fixture. On Linux, pass the
V4L2 path for a USB camera, for example `--device /dev/video0`. On
macOS, list AVFoundation devices with
`ffmpeg -f avfoundation -list_devices true -i ""`, then pass the video
device index, for example `--device 0`. macOS may ask for camera access.
Supply `--alternate-device <path-or-index>` for a second camera, or
`--alternate-input sample-2.mp4` for a second sample. The **Flip** button
appears when an alternate source is configured and switches inputs
between recordings.

The portrait viewfinder center-crops the source without stretching and
saves the same 360×480 area as a PNG. The composition grid is
a viewfinder overlay. Existing files are protected against overwrites.
For automated testing, use `--capture output.png` to save the full
640×480 source frame without opening a window.

The built-in A21s camera requires the separate sensor and CSI-2
bring-up before it can supply frames.
