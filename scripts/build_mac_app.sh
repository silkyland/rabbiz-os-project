#!/usr/bin/env bash
# build_mac_app.sh — build Rabbiz Mirror and assemble a double-clickable .app
#   ./scripts/build_mac_app.sh            # release build -> dist/Rabbiz Mirror.app
#   ./scripts/build_mac_app.sh --run      # build, then open the app
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

PACKAGE="tools/rabbiz-mirror"
APP="dist/Rabbiz Mirror.app"
BUNDLE_ID="com.devded.mirrorapp"
VERSION="1.0"

if ! command -v swift >/dev/null 2>&1; then
    echo "swift not found — install Xcode Command Line Tools" >&2
    exit 1
fi

echo "==> Building RabbizMirror (release)"
swift build --package-path "$PACKAGE" -c release

BINARY="$(swift build --package-path "$PACKAGE" -c release --show-bin-path)/RabbizMirror"
[ -x "$BINARY" ] || { echo "binary not found at $BINARY" >&2; exit 1; }

echo "==> Assembling $APP"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BINARY" "$APP/Contents/MacOS/RabbizMirror"
cp "$PACKAGE/Assets/AppIcon.icns" "$APP/Contents/Resources/AppIcon.icns"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>Rabbiz Mirror</string>
    <key>CFBundleDisplayName</key><string>Rabbiz Mirror</string>
    <key>CFBundleExecutable</key><string>RabbizMirror</string>
    <key>CFBundleIconFile</key><string>AppIcon</string>
    <key>CFBundleIdentifier</key><string>$BUNDLE_ID</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>$VERSION</string>
    <key>CFBundleVersion</key><string>1</string>
    <key>LSMinimumSystemVersion</key><string>13.0</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>NSLocalNetworkUsageDescription</key><string>Rabbiz Mirror connects to the phone's VNC server and SSH on your local USB or Wi-Fi network.</string>
    <key>NSSupportsAutomaticGraphicsSwitching</key><true/>
    <key>LSApplicationCategoryType</key><string>public.app-category.developer-tools</string>
</dict>
</plist>
PLIST

# A stable identity matters for Local Network privacy (macOS 15+): use the first
# Apple Development identity when present, otherwise fall back to ad-hoc.
IDENTITY=${CODESIGN_IDENTITY:-}
if [ -z "$IDENTITY" ]; then
    IDENTITY=$(security find-identity -v -p codesigning 2>/dev/null \
        | sed -n 's/.*"\(Apple Development:.*\)"/\1/p' | head -1)
fi
if [ -n "$IDENTITY" ]; then
    echo "==> Signing with $IDENTITY (Hardened Runtime)"
    codesign --force --options runtime --sign "$IDENTITY" "$APP"
else
    echo "==> Signing ad-hoc"
    codesign --force --sign - "$APP"
fi

echo "==> Built $APP"
if [ "${1:-}" = "--run" ]; then
    open "$APP"
fi
