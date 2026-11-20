#!/usr/bin/env bash
# Build the macOS emulator app in dist/. Pass --run to open it after building.
# Set RABBIZ_EMU_BINARY to package a previously built viewer.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

SDK="tools/rabbiz-sdk"
BINARY="rabbiz-emu-viewer"
APP="dist/RabbizOS Emulator.app"
CONTENTS="$APP/Contents"

VIEWER="${RABBIZ_EMU_BINARY:-}"
if [ -z "$VIEWER" ]; then
    cargo build --release --locked --manifest-path "$SDK/Cargo.toml" --target-dir "$SDK/target" -p "$BINARY"
    VIEWER="$SDK/target/release/$BINARY"
fi
if [ ! -x "$VIEWER" ]; then
    echo "Viewer executable not found at $VIEWER" >&2
    exit 1
fi
mkdir -p "$CONTENTS/MacOS" "$CONTENTS/Resources"
install -m 755 "$VIEWER" "$CONTENTS/MacOS/$BINARY"
install -m 644 assets/branding/rabbizos-emulator-icon.icns "$CONTENTS/Resources/AppIcon.icns"

cat > "$CONTENTS/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>RabbizOS Emulator</string>
    <key>CFBundleDisplayName</key><string>RabbizOS Emulator</string>
    <key>CFBundleExecutable</key><string>$BINARY</string>
    <key>CFBundleIconFile</key><string>AppIcon</string>
    <key>CFBundleIdentifier</key><string>com.devded.rabbizos-emulator</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>0.1.0</string>
    <key>CFBundleVersion</key><string>1</string>
    <key>LSApplicationCategoryType</key><string>public.app-category.developer-tools</string>
</dict>
</plist>
PLIST

codesign --force --sign "${CODESIGN_IDENTITY:--}" "$APP"
touch "$APP"
echo "Built $APP"

if [ "${1:-}" = "--run" ]; then
    open "$APP"
fi
