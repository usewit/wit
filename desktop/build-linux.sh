#!/usr/bin/env bash
# Build the Linux desktop app: an AppImage, a .deb and an .rpm in
# target/release/bundle/{appimage,deb,rpm}/. Works in WSL too.
# Needs the WebKitGTK dev packages (Ubuntu/Debian):
#   sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file \
#     libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev patchelf
# Binaries built here need this machine's glibc or newer; release builds come
# from the Linux workflow, which builds on Ubuntu 22.04 for that reason.
set -euo pipefail
cd "$(dirname "$0")/.."
if ! pkg-config --exists webkit2gtk-4.1; then
  echo "webkit2gtk-4.1 not found. Install the packages listed at the top of $0." >&2
  exit 1
fi
(cd web && { [ -d node_modules ] || npm install; } && npm run build >/dev/null)
# linuxdeploy (the AppImage step) is itself an AppImage; without FUSE, as in
# WSL, it has to unpack itself instead of mounting.
export APPIMAGE_EXTRACT_AND_RUN=1
cd desktop
# The web app is already built above.
if command -v cargo-tauri >/dev/null; then
  cargo tauri build --config '{"build":{"beforeBuildCommand":""}}'
else
  npx -y @tauri-apps/cli@2 build --config '{"build":{"beforeBuildCommand":""}}'
fi
echo "Built:"
ls -1 ../target/release/bundle/appimage/*.AppImage ../target/release/bundle/deb/*.deb ../target/release/bundle/rpm/*.rpm
