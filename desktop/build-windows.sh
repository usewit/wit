#!/usr/bin/env bash
# Cross-build the Windows desktop app from WSL. Output: target/Wit/, a folder
# ready to copy to Windows: Wit.exe, WebView2Loader.dll, web/ and fonts/.
# (A windows-gnu build loads WebView2 through that DLL, so it must sit next to
# the exe; without it Windows says the WebView2 DLL can't be found.)
# Needs the x86_64-pc-windows-gnu Rust target and either MinGW
# (`sudo apt install mingw-w64`) or cargo-zigbuild with `zig` on PATH and the
# x86_64-w64-mingw32-windres shim (wraps `zig rc`) in ~/.local/bin.
set -euo pipefail
cd "$(dirname "$0")/.."
(cd web && { [ -d node_modules ] || npm install; } && npm run build >/dev/null)
if command -v x86_64-w64-mingw32-gcc >/dev/null; then
  cargo build --release -p wit-desktop --target x86_64-pc-windows-gnu
else
  cargo zigbuild --release -p wit-desktop --target x86_64-pc-windows-gnu
fi
out=target/x86_64-pc-windows-gnu/release
rm -rf target/Wit
mkdir -p target/Wit
cp "$out/wit-desktop.exe" target/Wit/Wit.exe
cp "$out/WebView2Loader.dll" target/Wit/
cp -r web/dist target/Wit/web
cp -r fonts target/Wit/fonts
echo "Built target/Wit/ (Wit.exe, WebView2Loader.dll, web/, fonts/). Copy the whole folder."
