# RabbizOS Shared On-Screen Keyboard (OSK) Behavior API

`rabbiz-de` welcomes both the built-in keyboard (`rabbiz-osk`) and third-party / market Wayland keyboards (e.g., `squeekboard`, `stevia`, `maliit-keyboard`, `fcitx5`, or custom keyboards).

## 1. Selection & Security Policy

- **Settings UI:** `Settings → Keyboard` (`de-ui::settings::keyboard`) lets the user:
  - Toggle **Allow 3rd-party keyboards** (`Prefs.allow_third_party_keyboards`, persisted as `deAllowThirdPartyKeyboards` in `~/.config/rabbiz-de/prefs.json`).
  - Choose the active keyboard (`Prefs.keyboard_id`, persisted as `deKeyboard`).
- **Privileged Socket:** When `rabbiz-de` spawns the selected keyboard, it passes a private Wayland socket (`WAYLAND_SOCKET=0`, `session_input_method = true`), granting access to:
  - `zwp_input_method_v2`
  - `zwp_virtual_keyboard_v1`
  - `zwlr_layer_shell_v1`
  - `zwlr_data_control_manager_v1`
- When **Allow 3rd-party keyboards** is `false`, `rabbiz-de` forces the built-in `rabbiz-osk` even if another id is stored in `prefs.json`.

## 2. Wayland Layer & Gesture Bar Contract

To avoid colliding with RabbizOS's bottom 32 px system gesture strip (`Zone::NavBar`, `GESTURE_BAR_HEIGHT = 32`), an OSK using `zwlr_layer_shell_v1` should:

1. Anchor to `BOTTOM | LEFT | RIGHT` on `Layer::Top` with `KeyboardInteractivity::None`.
2. When **visible**: set `margin_bottom = 32` and `exclusive_zone = keyboard_height + 32` so the bottom key row (Space / Return) sits cleanly above `Zone::NavBar`.
3. When **hidden**: set `margin_bottom = -(keyboard_height + 32)` (or unmap) and `exclusive_zone = 0` so no bottom space is reserved.

## 3. Text Delivery & D-Bus Control

- **App text input:** Standard `zwp_input_method_v2` (`activate`, `deactivate`, `surrounding_text`, `content_type`, `done`, `commit_string`, `delete_surrounding_text`, `commit`).
- **Non-text keys / fallback:** `zwp_virtual_keyboard_v1` (`KEY_ENTER`, `KEY_BACKSPACE`, ASCII fallback).
- **Manual show/hide:** D-Bus `sm.puri.OSK0` (`/sm/puri/OSK0`, `SetVisible(b)`, `Visible` property).

## 4. Shell Search Bridge (`RABBIZ_OSK_IM_SOCK`)

Compositor-rendered shell inputs (`Screen::Search`, `Screen::Drawer` search bar) have no client `zwp_text_input_v3` surface. `rabbiz-de` passes `RABBIZ_OSK_IM_SOCK=<unix-stream-path>` in the spawned keyboard's environment. Keyboards that support it speak newline-delimited UTF-8 messages:

- **Compositor → OSK:** `SHOW\n`, `HIDE\n`
- **OSK → Compositor:** `C <utf8-text>\n` (commit text), `B <bytes>\n` (backspace), `E\n` (enter/submit)

Third-party keyboards that do not connect to `RABBIZ_OSK_IM_SOCK` still work normally in all Wayland apps via `zwp_input_method_v2`.

## 5. Registering Custom / Market Keyboards

Besides built-in detection (`rabbiz-osk`, `squeekboard`, `stevia`, `maliit-keyboard`, `fcitx5`), third-party packages can drop a `.toml` manifest in `/usr/share/rabbiz-de/keyboards/<id>.toml` or `~/.config/rabbiz-de/keyboards/<id>.toml`:

```toml
id = "my-keyboard"
name = "My Market Keyboard"
exec = "/usr/bin/my-keyboard"
```
