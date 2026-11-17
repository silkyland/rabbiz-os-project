# DE-SEAMS — `de-api` signature freeze (v1)

> Frozen for v1. Changes here need owner/integration-lane sign-off before M1
> work lands. Source of truth: `tools/rabbiz-de/crates/de-api/`.

## Dependency rules

`de-api` depends on **nothing** outside `std` + `serde` (derive only). It must
not know about smithay, tiny-skia, cosmic-text, zbus or D-Bus. Every other
crate may depend on `de-api`; `de-api` depends on no `de-*` crate.

Crate dependency direction (no cycles, matches `tools/rabbiz-de/Cargo.toml`):

```
de-api
 └── de-design
 └── de-config
 └── de-paint (de-api, de-design, tiny-skia, cosmic-text, resvg, png)
 └── de-ui (de-design, de-paint, de-config)
 └── de-apps (de-api, de-config, freedesktop-desktop-entry)
 └── de-system (de-api, de-config, zbus)          # Linux + stubs
 └── de-backend (de-api, smithay)                # Linux
 └── de-render (de-api, de-paint, smithay)       # Linux
 └── de-protocols (de-api, smithay)              # Linux
 └── de-core (all of the above)                  # Linux
 └── de-sim (de-api, de-design, de-paint, de-ui, de-config, tiny-skia, minifb)
 └── de-bin (everything; the only binary besides de-sim)
```

## Frozen signatures

Geometry (`geom.rs`): `Point{x:i32,y:i32}`, `Size{w:i32,h:i32}`,
`Rect{x:i32,y:i32,w:i32,h:i32}` — physical pixels only. Helpers: `new`,
`from_origin_size`, `size`, `origin`, `right`, `bottom`, `is_empty`,
`contains`, `translate`, `intersect -> Option<Rect>`, `union`, `inset`,
`Size::area -> i64`.

Scene (`scene.rs`): `LayerId(u64)`, `TextureId(u64)` opaque ids;
`Pixels{size:Size,data:Vec<u8>}` premultiplied RGBA8 (`4*w*h` bytes);
`Texture{id,pixels,damage:Vec<Rect>}` (empty damage = full upload);
`Layer{id,texture:Option<TextureId>,dst:Rect,opacity:f32,corner_radius:f32,color:[u8;4],damage:Vec<Rect>,visible:bool}` with `Layer::textured` / `Layer::solid` constructors;
`Scene{output:Size,clear:[u8;4],layers:Vec<Layer>,cursor:Option<Point>}` bottom-to-top.

Input (`input.rs`): `InputEvent::{Touch,Pointer,Key,Switch}`; every variant
carries `time_ms` (CLOCK_MONOTONIC ms) — required for spring velocity
(plan §7.17a). `TouchEvent{slot,phase:Down|Motion|Up|Cancel,pos,time_ms}`,
`PointerEvent::{Motion,Button,Axis}`, `KeyEvent{keycode,keysym:Option<u32>,state,time_ms}`,
`SwitchEvent::{Lid,TabletMode}`, `InputEvent::time_ms() -> u64`.

Services (`services.rs`): `ServicesSnapshot` (plain `Clone` data: `wifi`,
`bluetooth`, `battery`, `volume`, `brightness_pct`, `dnd`, `airplane`,
`notifications`, `media`, `media_playing`); `ServiceCommand` enum
(`SetBrightness(u8)`, `SetVolume(u8)`, `SetMuted`, `SetWifiEnabled`,
`ConnectWifi{ssid,psk}`, `SetBluetoothEnabled`, `SetDnd`, `SetAirplane`,
`DismissNotification(u32)`, `ClearNotifications`, `Media{PlayPause,Next,Prev}`,
`Session(String)`); `trait Services: Send { fn snapshot(&self)->ServicesSnapshot; fn commands(&self)->&Sender<ServiceCommand>; }`
— synchronous reads, mutation via `std::sync::mpsc`, **no async runtime**
across this seam.

Stats (`stats.rs`): `RendererKind::{Gl,Soft}` (serde `"gl"|"soft"`);
`OutputInfo{name:String,width:u32,height:u32,refresh_hz:f64,scale:f64}`,
`frame_interval_ms()`; `TimingSummary{p50,p95,max}`;
`FrameStats` = per-second JSONL schema (`t_ms,frames,cpu_ms,gpu_ms,damage_px,
layers,textures_mb,cma_free_kb,rss_kb,direct_scanout,primary_composite,
missed_frames,renderer`); `StatsSummary` = run summary (`scenario,duration_ms,
frames,cpu_ms,gpu_ms,missed_frames,rss_kb_peak,cma_free_min_kb,renderer`).
Field names are the wire contract for `de_probe.sh`/`de_bench.sh` — do not
rename.

## Ownership notes

- Lanes may add methods/impls inside `de-api` only through the integration
  lane (it owns the crate); type *fields* above are frozen.
- Host lanes (paint/ui/sim/system stubs) must compile `de-api` on macOS and
  Linux with no cfg gates.
- Timestamps: `time_ms` always means `CLOCK_MONOTONIC` milliseconds.
