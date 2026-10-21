#!/usr/bin/env bash
# FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
# Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
# SPDX-License-Identifier: MIT
#
# Builds the universal macOS disk image from the Apple Silicon and Intel release builds:
#
#   packaging/macos/bundle.sh <arm64-bin-dir> <x86_64-bin-dir> <version> <output.dmg>
#
# `lipo` merges each executable, FastTail.app gets both (`fasttail` starts the window,
# `fasttail-tui` is in Contents/MacOS for a terminal) and the icon, and is signed ad hoc
# (no Developer ID: Gatekeeper still asks once). The image holds the app, an
# Applications link, README.md and LICENSE.
set -euo pipefail

arm=$1
intel=$2
version=$3
output=$4

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

app="$work/image/FastTail.app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
for bin in fasttail fasttail-tui; do
  lipo -create "$arm/$bin" "$intel/$bin" -output "$app/Contents/MacOS/$bin"
done
sed "s/@VERSION@/$version/g" "$here/Info.plist" > "$app/Contents/Info.plist"
plutil -lint "$app/Contents/Info.plist"
cp "$root/packaging/icons/FastTail.icns" "$app/Contents/Resources/FastTail.icns"
codesign --force --deep --sign - "$app"

cp "$root/README.md" "$root/LICENSE" "$work/image/"
ln -s /Applications "$work/image/Applications"
hdiutil create -volname "FastTail $version" -srcfolder "$work/image" -fs HFS+ \
  -format UDZO -ov "$output"
