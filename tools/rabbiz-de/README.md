# `rabbiz-de` — scaffold

This is the first runnable slice of the desktop environment described in
[`docs/DE-PLAN.md`](../../docs/DE-PLAN.md). It crosses every seam the host can
exercise; the DRM/compositor lanes arrive in M0-T3 on the device.

## Run

```sh
scripts/de_test_host.sh
# or, by hand:
cargo run --manifest-path tools/rabbiz-de/Cargo.toml -p de-sim -- \
  --scene brand --out build/de/brand.png
cargo run --manifest-path tools/rabbiz-de/Cargo.toml -p de-sim -- \
  --scene lock --out build/de/lock.png
```

Output is a 720×1600 PNG: the P0 boot frame (`--scene brand`) or the P1 lock
screen skeleton (`--scene lock`). Without `--out`, the file is named after the
scene (`build/de/<scene>.png`).

## Shape

```
de-api      platform-neutral Point/Rect/Size, Color, DrawOp, Scene
de-design   the only place tokens live (plum/copper/cream, type, spacing)
de-paint    scene -> tiny-skia pixmap -> PNG; cosmic-text for text
de-ui       screens; returns a Scene, never draws or reads the OS
de-sim      wiring: parse CLI, build a screen, paint it, write the file
```

Dependency direction: `de-api ← de-design ← de-ui ← de-sim`, and
`de-api ← de-paint ← de-sim`. `de-paint` must not know about screens;
`de-ui` must not know about tiny-skia, cosmic-text, files or Wayland.
`scripts/check_seams.py` fails the build if that changes.

A second screen (for example `home`) is a new `crates/de-ui/src/<screen>.rs`
plus a `--scene` arm in `de-sim`; it does not add a crate.
