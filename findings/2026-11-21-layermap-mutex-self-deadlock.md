---
status: verified
lane: de
device: any
kernel: n/a
author: Bundit Nuntates
ai_assisted: true
date: 2026-11-21
evidence:
  - cmd: "wayland-info && cargo test --manifest-path tools/rabbiz-de/Cargo.toml --workspace"
supersedes: []
---

# LayerMap mutex self-deadlock — ctl/emu/wayland เงียบหลัง standards wave

**Lane:** de · **Device:** host emulator (virtual backend) · **Evidence:** L0 host-verified

## Symptom

หลังเพิ่ม `wp_presentation` feedback ใน standards wave, `rabbiz-de` เปิดอยู่แต่ตายทั้งท่อน:
- TCP `--ctl-listen`/`--emu-listen` accept (kernel backlog) แต่ไม่ตอบ
- `rabbiz-de-ctl.sock` เชื่อมได้แต่เงียบ
- ไม่มี panic ใน log, process sleeping, thread หลัก `futex_wait`

## Root cause

`smithay::desktop::layer_map_for_output` คืน `MutexGuard<'_, LayerMap>`
(std `Mutex`, non-reentrant) ใน `render.rs` ทั้งสอง path (virtual fast path +
full render) เก็บ `let map = layer_map_for_output(...)` ไว้ถึงท้าย scope แล้วเรียก
`emit_presentation_feedback` ซึ่งเรียก `layer_map_for_output` บน output เดิมอีกครั้ง
→ main thread lock mutex ที่ตัวเองถือ → self-deadlock ใน dispatch แรกของ event loop

## Why it looked like "everything bound but dead"

- `wayland-0`, ctl unix, TCP listeners ทั้งหมด bind ก่อน event loop dispatch แรก
  → socket มี แต่ timer 500ms (ที่ขับ `poll_ctl_ipc`/`poll_emu_server`/`poll_panels`)
  ไม่เคย fire
- Connect สำเร็จเพราะ TCP handshake เสร็จใน kernel backlog โดยไม่ต้อง `accept()`
- zbus/`Controls` worker threads ยัง spawn subprocess ปกติ → vfork storm
  ที่ดูเหมือน process "ทำงานอยู่" ทำให้เดาผิดว่า loop รัน

## Fix

`drop(map);` ก่อนเรียก `emit_presentation_feedback` ทั้ง 2 call site
(`render.rs` ~line 155, ~line 438)

## Lesson

เวลาเพิ่ม call ที่แตะ `layer_map_for_output`/`Space`/smithay user_data ข้างใน
frame path ให้เช็ก guard ที่ถืออยู่ใน scope เดียวกันเสมอ — std `Mutex` deadlock
ไม่มี panic ไม่มี log มีแค่ `futex_wait` ใน wchan

## Debug path that worked (host, yama ptrace_scope=1)

- `gdb -p` ใช้ไม่ได้ → รัน inferior ใต้ `gdb --args` (gdb เป็น parent ได้) หรือ
- อ่าน `/proc/<pid>/task/*/wchan` — thread หลัก `futex_wait` + ไม่มี thread ใน
  `ep_poll`/`do_epoll_wait` ของ calloop = event loop ไม่ dispatch
- เทียบ "socket bound แต่ poll ไม่ทำงาน" กับโค้ด: bind อยู่ใน `De::new`/
  `ListeningSocketSource` (ก่อน dispatch) → block ต้องอยู่หลัง bind ก่อน tick แรก

## Follow-up: upstream smithay bug kills Chrome (NoBuffer)

หลังแก้ deadlock แล้ว Chrome `Connection reset by peer` ตอน first commit —
`WAYLAND_DEBUG=server` จับได้:

```
<- xdg_toplevel_icon_v1@43.destroy
<- wl_buffer@45.destroy
-> wl_display@1.error(icon_v1@43, 3, "buffer destroyed before toplevel icon")
```

**Root cause:** `ShmBufferUserData::remove_destruction_hook` ใน
`smithay-0.7.0/src/wayland/shm/mod.rs` ใช้ predicate `hook.id != hook_id`
กับ `position()` → ลบ hook ตัวแรกที่*ไม่ตรง* (มี hook เดียว = ไม่ลบอะไรเลย)
→ hook ของ `xdg_toplevel_icon_v1.add_buffer` ค้างหลัง `icon.destroy` →
`wl_buffer.destroy` ยิง `NoBuffer` protocol error ฆ่า client — Chrome/
Chromium ทำ `add_buffer → set_icon → icon.destroy → buffer.destroy`
พอดี (alacritty/GTK ไม่ใช้ toplevel_icon เลยรอด)

**Fix:** `vendor/smithay` (copy ของ 0.7.0) แก้เป็น `==` + `[patch.crates-io]`
ใน `tools/rabbiz-de/Cargo.toml` — upstream main แก้แล้วแต่ยังไม่ release;
เอา patch ออกเมื่อ bump pin เกิน 0.7.0

**Verified:** Chrome map + tap navigate (Web Store) บน emu; GTK (Clocks)
map ปกติ; `cargo test` ทั้งชุดผ่าน
