#!/usr/bin/env bash
# FastTail -- Ultra-fast multi-stream log monitor and tail viewer.
# Copyright (c) Matteo Baccan -- https://github.com/matteobaccan/FastTail
# SPDX-License-Identifier: MIT
#
# Builds an AppImage from the release binaries of one target:
#
#   packaging/linux/appimage.sh <bin-dir> <x86_64|aarch64> <output.AppImage>
#
# The AppImage runs the window (`fasttail`); `fasttail-tui` travels in usr/bin
# (`--appimage-extract`). No graphics library is bundled: OpenGL / Vulkan come from the
# host driver. appimagetool and the type-2 runtime are pinned and checked by SHA-256.
set -euo pipefail

bin_dir=$1
arch=$2
output=$3

APPIMAGETOOL_VERSION=1.9.1
RUNTIME_VERSION=20251108
case "$arch" in
  x86_64)
    tool_sha=ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0
    runtime_sha=2fca8b443c92510f1483a883f60061ad09b46b978b2631c807cd873a47ec260d
    ;;
  aarch64)
    tool_sha=f0837e7448a0c1e4e650a93bb3e85802546e60654ef287576f46c71c126a9158
    runtime_sha=00cbdfcf917cc6c0ff6d3347d59e0ca1f7f45a6df1a428a0d6d8a78664d87444
    ;;
  *) echo "unknown architecture $arch" >&2; exit 2 ;;
esac

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

fetch() { # url file sha256
  curl -fsSL --retry 3 -o "$2" "$1"
  echo "$3  $2" | sha256sum -c -
}
fetch "https://github.com/AppImage/appimagetool/releases/download/$APPIMAGETOOL_VERSION/appimagetool-$arch.AppImage" \
  "$work/appimagetool" "$tool_sha"
fetch "https://github.com/AppImage/type2-runtime/releases/download/$RUNTIME_VERSION/runtime-$arch" \
  "$work/runtime" "$runtime_sha"
chmod +x "$work/appimagetool"

app="$work/FastTail.AppDir"
mkdir -p "$app/usr/bin" "$app/usr/share/applications" "$app/usr/share/icons/hicolor/256x256/apps"
install -m 755 "$bin_dir/fasttail" "$bin_dir/fasttail-tui" "$app/usr/bin/"
install -m 644 "$here/fasttail.desktop" "$app/usr/share/applications/"
install -m 644 "$root/packaging/icons/fasttail-256.png" "$app/usr/share/icons/hicolor/256x256/apps/fasttail.png"
cp "$here/fasttail.desktop" "$app/fasttail.desktop"
cp "$root/packaging/icons/fasttail-256.png" "$app/fasttail.png"
cp "$root/packaging/icons/fasttail-256.png" "$app/.DirIcon"
cat > "$app/AppRun" <<'RUN'
#!/bin/sh
here=$(dirname "$(readlink -f "$0")")
exec "$here/usr/bin/fasttail" "$@"
RUN
chmod 755 "$app/AppRun"

# The runners have no FUSE: appimagetool (an AppImage itself) runs extracted.
ARCH=$arch "$work/appimagetool" --appimage-extract-and-run --no-appstream \
  --runtime-file "$work/runtime" "$app" "$output"
