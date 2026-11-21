#!/usr/bin/env bash
# Install a Linux desktop launcher and icon for RabbizOS Emulator.
# Builds rabbiz-emu-viewer (--release) if missing, or pass --build to rebuild.
set -euo pipefail
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

SDK="tools/rabbiz-sdk"
BINARY="rabbiz-emu-viewer"
VIEWER="${RABBIZ_EMU_BINARY:-$REPO_ROOT/$SDK/target/release/$BINARY}"

if [ "${1:-}" = "--build" ] || [ ! -x "$VIEWER" ]; then
    cargo build --release --locked --manifest-path "$SDK/Cargo.toml" --target-dir "$SDK/target" -p "$BINARY"
    VIEWER="$REPO_ROOT/$SDK/target/release/$BINARY"
fi

if [ ! -x "$VIEWER" ]; then
    echo "Viewer executable not found at $VIEWER" >&2
    exit 1
fi

ICON_DIR_SVG="${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor/scalable/apps"
ICON_DIR_PNG="${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor/256x256/apps"
APPS_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/applications"

mkdir -p "$ICON_DIR_SVG" "$ICON_DIR_PNG" "$APPS_DIR"
install -m 644 assets/branding/rabbizos-emulator-icon.svg "$ICON_DIR_SVG/rabbizos-emulator.svg"
install -m 644 assets/branding/rabbizos-emulator-icon.png "$ICON_DIR_PNG/rabbizos-emulator.png"

DESKTOP_FILE="$APPS_DIR/rabbizos-emulator.desktop"
cat > "$DESKTOP_FILE" <<DESKTOP
[Desktop Entry]
Version=1.0
Type=Application
Name=RabbizOS Emulator
Comment=Live RabbizOS Wayland Emulator & Studio Dock
Exec=$VIEWER
Path=$REPO_ROOT
Icon=rabbizos-emulator
Terminal=false
Categories=Development;Emulator;
StartupNotify=true
DESKTOP
chmod 755 "$DESKTOP_FILE"

if [ -d "$HOME/Desktop" ]; then
    DESKTOP_SHORTCUT="$HOME/Desktop/RabbizOS Emulator.desktop"
    install -m 755 "$DESKTOP_FILE" "$DESKTOP_SHORTCUT"
    if command -v gio >/dev/null 2>&1; then
        gio set "$DESKTOP_SHORTCUT" metadata::trusted true 2>/dev/null || true
    fi
    echo "Installed desktop shortcut: $DESKTOP_SHORTCUT"
fi

if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$APPS_DIR" 2>/dev/null || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -f -t "${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor" 2>/dev/null || true
fi

echo "Installed app entry: $DESKTOP_FILE"
