#!/bin/sh
# Builds dist/kliknload.app: the Rust menu bar app plus the native SwiftUI settings
# window as a helper app inside it.
#
# Usage: scripts/bundle.sh [--universal] [--install]
#   --universal  build for Apple Silicon and Intel (needs both rustup targets)
#   --install    copy the app to ~/Applications
set -eu
cd "$(dirname "$0")/.."

UNIVERSAL=0
INSTALL=0
for arg in "$@"; do
    case "$arg" in
        --universal) UNIVERSAL=1 ;;
        --install) INSTALL=1 ;;
        *) echo "unknown option $arg" >&2; exit 2 ;;
    esac
done

VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
BUNDLE_ID="io.github.woodfairy.kliknload"
MIN_MACOS="13.0"
BUILD=target/bundle
rm -rf "$BUILD"
mkdir -p "$BUILD"

# --- Rust core ---------------------------------------------------------------
if [ "$UNIVERSAL" = 1 ]; then
    for t in aarch64-apple-darwin x86_64-apple-darwin; do
        MACOSX_DEPLOYMENT_TARGET=$MIN_MACOS cargo build --release --locked --target "$t"
    done
    lipo -create -output "$BUILD/kliknload" \
        target/aarch64-apple-darwin/release/kliknload target/x86_64-apple-darwin/release/kliknload
else
    MACOSX_DEPLOYMENT_TARGET=$MIN_MACOS cargo build --release --locked
    cp target/release/kliknload "$BUILD/kliknload"
fi

# --- SwiftUI settings window -------------------------------------------------
swift_build() {
    swiftc -swift-version 5 -O -parse-as-library -target "$1-apple-macos$MIN_MACOS" \
        settings/Sources/*.swift -o "$2"
}
if [ "$UNIVERSAL" = 1 ]; then
    swift_build arm64 "$BUILD/settings-arm64"
    swift_build x86_64 "$BUILD/settings-x86_64"
    lipo -create -output "$BUILD/kliknload-settings" "$BUILD/settings-arm64" "$BUILD/settings-x86_64"
else
    swift_build "$(uname -m)" "$BUILD/kliknload-settings"
fi

# --- Icon from assets/app-icon.svg -------------------------------------------
ICONSET="$BUILD/AppIcon.iconset"
mkdir -p "$ICONSET"
for size in 16 32 128 256 512; do
    "$BUILD/kliknload" render-icon "$size" "$ICONSET/icon_${size}x${size}.png"
    "$BUILD/kliknload" render-icon "$((size * 2))" "$ICONSET/icon_${size}x${size}@2x.png"
done
iconutil -c icns "$ICONSET" -o "$BUILD/AppIcon.icns"

plist() { # name, bundle id, executable, background-only
    cat <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>$1</string>
    <key>CFBundleDisplayName</key><string>$1</string>
    <key>CFBundleIdentifier</key><string>$2</string>
    <key>CFBundleExecutable</key><string>$3</string>
    <key>CFBundleIconFile</key><string>AppIcon</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleVersion</key><string>$VERSION</string>
    <key>CFBundleShortVersionString</key><string>$VERSION</string>
    <key>LSMinimumSystemVersion</key><string>$MIN_MACOS</string>
    <key>LSUIElement</key><$4/>
    <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST
}

# --- Assemble ----------------------------------------------------------------
APP=dist/kliknload.app
HELPER="$APP/Contents/Helpers/kliknload Settings.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources" "$HELPER/Contents/MacOS" "$HELPER/Contents/Resources"

cp "$BUILD/kliknload" "$APP/Contents/MacOS/kliknload"
cp "$BUILD/AppIcon.icns" "$APP/Contents/Resources/AppIcon.icns"
plist kliknload "$BUNDLE_ID" kliknload true > "$APP/Contents/Info.plist"

cp "$BUILD/kliknload-settings" "$HELPER/Contents/MacOS/kliknload-settings"
cp "$BUILD/AppIcon.icns" "$HELPER/Contents/Resources/AppIcon.icns"
plist "kliknload Settings" "$BUNDLE_ID.settings" kliknload-settings false > "$HELPER/Contents/Info.plist"

# Ad-hoc signature, inner bundle first.
codesign --force --sign - "$HELPER" >/dev/null
codesign --force --sign - "$APP" >/dev/null
echo "built $APP ($VERSION)"

if [ "$INSTALL" = 1 ]; then
    mkdir -p "$HOME/Applications"
    rm -rf "$HOME/Applications/kliknload.app"
    cp -R "$APP" "$HOME/Applications/"
    echo "installed to ~/Applications/kliknload.app"
fi
