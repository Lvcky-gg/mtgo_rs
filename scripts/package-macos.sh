#!/usr/bin/env bash
set -euo pipefail

version=${1:?release version required}
platform=${2:?macos-arm64 or macos-x64 required}
target=${3:?Rust target required}
app="dist/macos-staging/MTGO RS.app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "target/$target/release/mtg-gui" "$app/Contents/MacOS/mtg-gui"
chmod 755 "$app/Contents/MacOS/mtg-gui"
python3 - "$version" "$app/Contents/Info.plist" <<'PY'
import plistlib
import sys
with open(sys.argv[2], "wb") as file:
    plistlib.dump({
        "CFBundleName": "MTGO RS",
        "CFBundleDisplayName": "MTGO RS",
        "CFBundleIdentifier": "io.github.lvcky_gg.MtgoRs",
        "CFBundleExecutable": "mtg-gui",
        "CFBundlePackageType": "APPL",
        "CFBundleShortVersionString": sys.argv[1],
        "CFBundleVersion": sys.argv[1],
        "LSMinimumSystemVersion": "11.0",
        "NSHighResolutionCapable": True,
        "NSLocalNetworkUsageDescription": "Discover and host games with nearby players.",
    }, file)
PY
# Ad-hoc signing is required for locally built Apple Silicon executables. This
# does not imply Developer ID signing or Apple notarization.
codesign --force --deep --sign - "$app"
codesign --verify --deep --strict "$app"
# The updater replaces the signed bundle as a whole, using this archive.
ditto -c -k --sequesterRsrc --keepParent "$app" "dist/mtgo-rs-v$version-$platform.zip"
ln -s /Applications dist/macos-staging/Applications
hdiutil create -volname "MTGO RS $version" -srcfolder dist/macos-staging \
    -ov -format UDZO "dist/mtgo-rs-v$version-$platform.dmg"
