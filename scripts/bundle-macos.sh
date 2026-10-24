#!/bin/sh
# Builds target/bundle/Vextor.app (spec 13c): the release binary, the app
# icon, and the .vxtr document type with its icon, so Finder shows Vextor
# documents with their own icon once the app is installed.
#
#   scripts/bundle-macos.sh             build target/bundle/Vextor.app
#   scripts/bundle-macos.sh --install   ... and copy it to ~/Applications,
#                                       registered with Launch Services
#
# macOS only. Uses only tools that ship with macOS (iconutil, codesign,
# plutil, lsregister); no network. Signed ad hoc: for this machine, not
# for distribution (spec 15).
set -eu

cd "$(dirname "$0")/.."

if [ "$(uname)" != "Darwin" ]; then
    echo "bundle-macos.sh builds a macOS app; run it on a Mac." >&2
    exit 1
fi

out=target/bundle
app="$out/Vextor.app"
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)

echo "Building Vextor $version (release)..."
cargo build --release -p vextor-app

echo "Rendering icons..."
rm -rf "$out/AppIcon.iconset" "$out/Document.iconset"
VEXTOR_ICONSET_DIR="$PWD/$out" cargo test -q -p vextor-app -- --ignored write_iconsets >/dev/null

echo "Assembling $app..."
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp target/release/vextor "$app/Contents/MacOS/vextor"
iconutil -c icns "$out/AppIcon.iconset" -o "$app/Contents/Resources/AppIcon.icns"
iconutil -c icns "$out/Document.iconset" -o "$app/Contents/Resources/Document.icns"
sed "s/__VERSION__/$version/g" crates/vextor-app/macos/Info.plist >"$app/Contents/Info.plist"
plutil -lint -s "$app/Contents/Info.plist"
codesign --force --sign - "$app" 2>/dev/null

echo "Built $app"

if [ "${1:-}" = "--install" ]; then
    dest="$HOME/Applications/Vextor.app"
    mkdir -p "$HOME/Applications"
    rm -rf "$dest"
    cp -R "$app" "$dest"
    lsregister=/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister
    "$lsregister" -f "$dest"
    # The system already hands out the new icons, but the icon service,
    # the Dock and Finder (which draws the Desktop) keep what they cached.
    echo "Installed $dest."
    echo "To see the new icons, refresh the icon caches once:"
    echo "  killall iconservicesagent Dock Finder"
    echo "If a .vxtr file still shows the old icon, clear the system's icon"
    echo "store and restart the Mac:"
    echo "  sudo rm -rf /Library/Caches/com.apple.iconservices.store"
fi
