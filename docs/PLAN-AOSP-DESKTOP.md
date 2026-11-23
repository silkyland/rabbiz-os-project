# RabbizOS Plan: AOSP-Like Parity & Desktop Mode Support

- **Date:** 2026-11-23 · **Lane:** `de` · **Status:** Proposed Architecture & Roadmap
- **Related:** [DE-DESKTOP-MODE.md](DE-DESKTOP-MODE.md), [AUDIT-DE-STANDARDS-2026-11-21.md](AUDIT-DE-STANDARDS-2026-11-21.md), [DE-KEYGUARD-AUTH.md](DE-KEYGUARD-AUTH.md), [DE-NOTIFICATIONS-SERVICES.md](DE-NOTIFICATIONS-SERVICES.md), [ROADMAP.md](../ROADMAP.md)

---

## 1. Design Question: Fullscreen Toggle vs. Orientation (ควรเป็นแนวนอนไหม?)

แยกออกเป็น **2 กรณี** ตามพฤติกรรมของ AOSP และ Desktop OS:

1. **Immersive Fullscreen Toggle (ซ่อน Status Bar + Gesture Bar):**
   - **ไม่ควรบังคับหมุนเป็นแนวนอน** — ให้คง orientation ปัจจุบันไว้ (เช่น อ่าน PDF หรือแอปแนวตั้งจะได้พื้นที่เต็มจอในแนวตั้ง)
   - การหมุนแนวนอนสำหรับวิดีโอ/เกมใช้:
     - `X-Rabbiz-Orientation=landscape` / `sensor_landscape` ใน `.desktop` ([orientation.rs](../tools/rabbiz-de/crates/de-api/src/orientation.rs))
     - **AOSP-style Rotate Suggestion Pill**: เมื่อ Rotation Lock เปิดอยู่ แล้วผู้ใช้เอียงเครื่องเป็นแนวนอน ให้แสดงปุ่มหมุนชั่วคราวที่มุมล่างขวา 3 วินาที
2. **Desktop Mode Toggle (สลับเข้าโหมด Desktop):**
   - **ควรสลับเป็นแนวนอนอัตโนมัติ** (`LandscapeCcw` → `1600×720` logical บน A21s; หรือ native landscape บนแท็บเล็ต/จอนอก)
   - เหตุผล: ความกว้าง 720 px ในแนวตั้งแคบเกินกว่าจะวาง 2 หน้าต่าง, Taskbar, Titlebar (SSD) และ Context menu ได้จริง ในขณะที่ 1600×720 รองรับ Side-by-side snapping และ Floating windows ได้
   - เมื่อปิด Desktop Mode กลับสู่ Handset Mode ให้คืนค่า orientation เดิมและ re-maximize หน้าต่างทั้งหมดโดยไม่ปิดแอป

---

## 2. Part A — AOSP-Like Parity Roadmap (เทียบเท่า AOSP)

### A1. SystemUI, Navigation & Launcher (`rabbiz-de`)
- [x] Home / Dock / Drawer / Global Search / Recents (finger-tracking) / Shade / HUN / Media Pill & Card
- [ ] **Immersive Mode & Transient Bars:** แอปขอซ่อน system bars ได้ (swipe จากขอบบน/ล่างเพื่อเรียกคืนชั่วคราว 2 วินาที แบบ AOSP `WindowInsetsController`)
- [ ] **AOSP Rotate Suggestion Pill:** ใช้ `OrientationDetector` ที่มีอยู่แล้วใน `de-api` เตือนปุ่มหมุนมุมจอเมื่อ `RotationPolicy.auto == false`
- [ ] **Quick Settings Parity:** เพิ่ม tiles ที่ขาด — **Do Not Disturb (DND)** (เชื่อมกับ HUN filter), **Auto-Rotate**, **Dark Theme**, **Desktop Mode**, **Screen Record** (shell-owned), และ **Media Output Switcher** ใน Shade/Volume OSD
- [ ] **Recents Actions:** ปุ่ม **Clear All**, **App Info** (เปิด Settings หน้าแอป), และ **Split-Screen** (ใน Tablet/Desktop)
- [ ] **Notification Parity:** Inline Reply (ผ่าน `org.freedesktop.Notifications` action/hint), Per-app mute toggle, และ Lockscreen redaction (`HideSensitive` / `HideAll` ตาม `DE-NOTIFICATIONS-SERVICES.md`)

### A2. Security, Auth & Sandboxing (เทียบ AOSP Keyguard / SELinux)
- [x] Argon2id PIN + `LockoutPolicy` + `bfu_unlocked` + `ext-session-lock-v1` + Option B privileged Wayland allow-list
- [ ] **Root-Owned `rabbiz-auth-helper` + PAM (`Passphrase`):** ย้าย `keyguard.json`/`keyguard.state` ไป `/var/lib/rabbiz-auth/<user>/` (ออกแบบ protocol ไว้แล้วใน `de-system/src/auth.rs`) และต่อ `pam_unix` สำหรับโหมด `Passphrase`
- [ ] **Per-App Sandbox (M5):** `wp_security_context_v1` + `bubblewrap` / Flatpak เพื่อให้แต่ละ third-party app มี restricted Wayland socket และไม่เห็นไฟล์/หน้าจอของแอปอื่น
- [ ] **XDG Portals & Secret Service:** ติดตั้ง `xdg-desktop-portal` + `xdg-desktop-portal-gtk` + `gnome-keyring` (autostart unit พร้อมแล้วใน `rootfs-overlay/etc/xdg/autostart/rabbiz-keyring.desktop`) เพื่อรองรับ FileChooser, OpenURI, Settings (dark-mode), และ `org.freedesktop.secrets`

### A3. Hardware, Sensors & Core Apps
- [ ] **Power (M5):** Suspend-to-RAM (`echo mem > /sys/power/state`) + `upower` + `power-profiles-daemon`
- [ ] **Telephony (M6):** `cpif` + `ModemManager`/`oFono` + ABOX/HFP voice routing → `rabbiz-dialer` / `rabbiz-sms` ใช้งานจริง
- [ ] **Sensors:** `iio-sensor-proxy` + Auto-brightness (ALS)
- [ ] **Base Apps (L2):** ทดสอบ Rust `rabbiz-apps` ทั้ง 13 ตัวบน A21s จริง และเพิ่ม `gvfs` + `udisks2` + `tumbler` สำหรับ `rabbiz-files`

---

## 3. Part B — Desktop Mode Architecture & Implementation Plan

ต่อยอดจาก [DE-DESKTOP-MODE.md](DE-DESKTOP-MODE.md) โดยให้ **1 Compositor (`rabbiz-de`)** เปลี่ยนพฤติกรรมตาม `FormFactor::{Handset, Tablet, Desktop}`:

### B1. 3 ช่องทางในการใช้งาน Desktop Mode
1. **On-Device Desktop Toggle (A21s / Tablet):**
   - สลับ `FormFactor::Desktop` ผ่าน Quick Settings หรือ `rabbiz-de ctl set-form-factor desktop`
   - บน A21s (720×1600): หมุน logical output เป็น **Landscape (`1600×720`)** อัตโนมัติ เมื่อต่อเมาส์/คีย์บอร์ด Bluetooth หรือ USB OTG
2. **External Display (Pad 7 Pro `muyu` / USB-C DP Alt Mode):**
   - Multi-output DRM hotplug → จอนอกเป็น `FormFactor::Desktop` (1920×1080+), จอมือถือเป็น Touchpad/Second screen ได้
3. **Virtual Desktop Output via `rabbiz-mirror` (DeX-on-PC สำหรับ A21s):**
   - เนื่องจาก A21s USB-C ไม่มี DP Alt Mode จึงสร้าง **Virtual Output** (เช่น `1920×1080` @ 60 Hz) ใน `de-core` ส่งเฟรมผ่าน USB-C RNDIS (`172.16.42.1`) ไปยังแอป **Rabbiz Mirror** บน macOS/Linux/Windows โดยแยกจากหน้าจอมือถือ

### B2. Window Management Policy (`de-protocols` + `de-core`)
- เปลี่ยนจาก `configure_fullscreen()` แบบตายตัว เป็น **`WindowPolicy::for_form_factor(form_factor)`**:
  - `Handset`: `Fullscreen`, No SSD, gesture-only
  - `Desktop` / `Tablet`:
    - **Floating & Resizable (`xdg_toplevel`):** รองรับ `Activated`, `Maximized`, `Fullscreen`, `Resizing`, `min_size`/`max_size`
    - **Server-Side Decorations (SSD):** Titlebar (สูง ~32–36 px) วาดโดย `de-ui` มีชื่อแอป + ปุ่ม Minimize / Maximize / Close (เคารพ CSD หากแอปขอ `ClientSide`)
    - **Window Actions:** Drag titlebar เพื่อย้าย, Drag ขอบ/มุมเพื่อ resize, **Edge Snap** (ลากชนขอบซ้าย/ขวา = 50% Split, ชนขอบบน = Maximize), **Alt+Tab** window switcher, Focus-follows-click (ป้องกัน focus stealing)

### B3. Desktop Shell UI (`de-ui`) & Ecosystem
- **Unified Desktop Panel (Taskbar):**
  - แถบล่าง (KDE/ChromeOS style) สูง ~48 px:
    - ซ้าย: App Launcher icon + Search
    - กลาง: Pinned apps + Running window pills (คลิกเพื่อ focus/minimize, คลิกขวาเพื่อ Close/Pin)
    - ขวา: **System Tray (`org.freedesktop.StatusNotifierWatcher`)**, Quick Settings summary pill (Wi-Fi/BT/Audio/Battery), Notification counter, Clock
- **Context Menus & Pointer UX:**
  - `xdg_popup` grab + `wp_cursor_shape_manager_v1` (มีแล้ว) + วาด hardware/software cursor บน output + hover state บนปุ่ม/การ์ดใน `de-ui`
  - Shell Context Menu component: คลิกขวาบนพื้นที่ว่างของ Desktop (Wallpaper, Display/Resolution, Sort icons), บน Titlebar (Always on top, Move, Close), และบน Taskbar
- **XWayland (`M5-T1`):** เปิดใช้งาน feature `xwayland` ของ `smithay` ใน `de-core` เพื่อรันแอป X11/Steam/Electron เก่าในโหมด Desktop

### B4. ลำดับการ Implement แนะนำ (Phased)
1. **Phase D1 (Core Policy + Toggle + Landscape):** เพิ่ม `FormFactor` runtime switch ใน `de-core` + Quick Settings tile `Desktop Mode` (auto-landscape `1600×720` บน handset) + `WindowPolicy` (floating geometry + SSD titlebar + move/resize)
2. **Phase D2 (Taskbar + Cursor + Context Menu):** วาด software cursor + hover feedback, Desktop Taskbar (`de-ui`), `Alt+Tab` switcher, และ Right-click context menu component
3. **Phase D3 (Ecosystem & Virtual Output):** เปิด XWayland (`M5-T1`), `org.freedesktop.StatusNotifierWatcher` (tray icons), และ Virtual Output stream สำหรับ `rabbiz-mirror` (DeX-on-PC)
