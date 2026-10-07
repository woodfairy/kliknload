#!/bin/sh
# Builds dist/kliknload-<version>.dmg from dist/kliknload.app with a background image
# (assets/dmg-background.svg) and the classic "drag to Applications" layout.
# Uses only macOS tools: hdiutil, tiffutil, osascript (Finder) and SetFile if present.
#
# Usage: scripts/dmg.sh        (run scripts/bundle.sh first)
set -eu
cd "$(dirname "$0")/.."

APP=dist/kliknload.app
[ -d "$APP" ] || { echo "missing $APP, run scripts/bundle.sh first" >&2; exit 1; }
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
VOLUME="kliknload"
OUT="dist/kliknload-$VERSION.dmg"
WORK=target/dmg
STAGING="$WORK/staging"

# Window geometry; must match the icon positions drawn in the background.
WIDTH=640
HEIGHT=400
APP_X=170
APP_Y=205
APPS_X=470
APPS_Y=205

rm -rf "$WORK"
mkdir -p "$STAGING/.background"

# Background at 1x and 2x, combined into one HiDPI tiff.
"$APP/Contents/MacOS/kliknload" render-svg assets/dmg-background.svg "$WIDTH" "$WORK/bg.png"
"$APP/Contents/MacOS/kliknload" render-svg assets/dmg-background.svg "$((WIDTH * 2))" "$WORK/bg@2x.png"
tiffutil -cathidpicheck "$WORK/bg.png" "$WORK/bg@2x.png" -out "$STAGING/.background/background.tiff" >/dev/null

cp -R "$APP" "$STAGING/"
ln -s /Applications "$STAGING/Applications"

# Detach a leftover volume with the same name from an earlier run.
[ -d "/Volumes/$VOLUME" ] && hdiutil detach "/Volumes/$VOLUME" -force >/dev/null 2>&1 || true

hdiutil create -quiet -volname "$VOLUME" -srcfolder "$STAGING" -fs HFS+ -format UDRW -ov "$WORK/rw.dmg"
DEVICE=$(hdiutil attach -readwrite -noverify -noautoopen "$WORK/rw.dmg" | awk '/Apple_HFS/ {print $1}')
MOUNT="/Volumes/$VOLUME"

# Finder window layout. Stored in the volume's .DS_Store.
if ! osascript <<APPLESCRIPT
tell application "Finder"
    tell disk "$VOLUME"
        open
        set current view of container window to icon view
        set toolbar visible of container window to false
        set statusbar visible of container window to false
        set the bounds of container window to {200, 120, $((200 + WIDTH)), $((120 + HEIGHT))}
        set opts to the icon view options of container window
        set arrangement of opts to not arranged
        set icon size of opts to 128
        set text size of opts to 13
        set background picture of opts to file ".background:background.tiff"
        set position of item "kliknload.app" of container window to {$APP_X, $APP_Y}
        set position of item "Applications" of container window to {$APPS_X, $APPS_Y}
        close
        open
        update without registering applications
        delay 2
        close
    end tell
end tell
APPLESCRIPT
then
    echo "warning: Finder layout failed (no automation permission?), the DMG works but looks plain" >&2
fi

# Volume icon, set after the Finder step (Finder drops it otherwise).
cp "$APP/Contents/Resources/AppIcon.icns" "$MOUNT/.VolumeIcon.icns"
if command -v SetFile >/dev/null 2>&1; then
    SetFile -c icnC "$MOUNT/.VolumeIcon.icns"
    SetFile -a C "$MOUNT"
fi

chmod -Rf go-w "$MOUNT" 2>/dev/null || true
sync
hdiutil detach "$DEVICE" -quiet || hdiutil detach "$DEVICE" -force -quiet
rm -f "$OUT"
hdiutil convert "$WORK/rw.dmg" -quiet -format UDZO -imagekey zlib-level=9 -o "$OUT"
codesign --force --sign - "$OUT" >/dev/null 2>&1 || true
echo "built $OUT"
