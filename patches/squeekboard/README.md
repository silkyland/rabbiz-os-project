# squeekboard patches (RabbizOS)

## 0001-clipboard-paste.patch

Adds a `paste` key action to squeekboard 1.43.1 so the on-screen keyboard can paste the Wayland
clipboard. squeekboard's `zwp_input_method_v2` surface cannot read the clipboard, so the action
runs `wl-paste -n -t text/plain;charset=utf-8` and submits the text like typed input (no `wl-paste`
or empty clipboard → the key does nothing).

Layout usage:

```yaml
buttons:
    paste:
        action: "paste"
        outline: "special"
        label: "📋"
```

Build on the device (needs `rust gtk4 meson ninja wl-clipboard`):

```sh
git clone --depth 1 --branch v1.43.1 https://gitlab.gnome.org/World/Phosh/squeekboard.git
cd squeekboard && git apply /path/to/0001-clipboard-paste.patch
meson setup build --buildtype=release -Dtests=false && ninja -C build
# keep a rollback copy of the packaged binary first
cp /usr/bin/squeekboard /usr/bin/squeekboard.stock
ninja -C build install
```

Rollback: restore `/usr/bin/squeekboard.stock`, or `pacman -S squeekboard`.
