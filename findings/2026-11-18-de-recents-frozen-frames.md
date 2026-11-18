---
status: reproduced
lane: de
device: any
kernel: n/a
author: Devin
ai_assisted: true
date: 2026-11-18
evidence:
  - cmd: "cargo test --manifest-path tools/rabbiz-de/Cargo.toml -p de-core offscreen_capture_preserves_rgba_and_row_order"
  - cmd: "LIBGL_ALWAYS_SOFTWARE=1 cargo test --manifest-path tools/rabbiz-de/Cargo.toml -p de-core gles_offscreen_capture -- --ignored"
supersedes: []
---

# Recents offscreen readback is top-first on Pixman and GLES

## Observed

The same two-color scene reads back as premultiplied RGBA, with red on top and
blue below, on host Pixman and Mesa software GLES. Reversing rows based on the
GLES mapping's `flipped()` flag inverted that scene: the offscreen projection
already accounts for the renderer's coordinate convention.

## Interpretation

Recents can freeze a small copy of the window render tree instead of accessing
only a client SHM buffer. A thumbnail belongs to the live toplevel; navigating
to Home or Recents does not send a close request, and a background commit does
not replace the thumbnail. This path is separate from external screencopy.

## Reproduce

Run the two host tests above. On a handset (not yet tested), open two apps,
change their visible state, and alternate quick Home swipes with swipe-up-and-hold
into Recents. Verify that both cards keep their last content, and tapping a card
expands it and returns to the same window instead of starting a new app. Repeat
with a GLES/dmabuf client on Panfrost and verify orientation and frame pacing.
