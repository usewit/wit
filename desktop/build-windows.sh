#!/usr/bin/env bash
# Cross-build the Windows desktop app from WSL. Output:
#   target/x86_64-pc-windows-gnu/release/bundle/nsis/Wit_<version>_x64-setup.exe,
#     the installer (installs WebView2 too if Windows lacks it);
#   target/Wit/, a folder to copy and run without installing: Wit.exe,
#     WebView2Loader.dll, web/ and fonts/.
# (A windows-gnu build loads WebView2 through that DLL, so it must sit next to
# the exe; without it Windows says the WebView2 DLL can't be found.)
# Needs the x86_64-pc-windows-gnu Rust target, NSIS (`sudo apt install nsis`),
# and either MinGW (`sudo apt install mingw-w64`) or cargo-zigbuild with `zig`
# on PATH and the x86_64-w64-mingw32-windres shim (wraps `zig rc`) in
# ~/.local/bin. Release installers come from the Windows workflow instead.
set -euo pipefail
cd "$(dirname "$0")/.."
target=x86_64-pc-windows-gnu
out=target/$target/release
(cd web && { [ -d node_modules ] || npm install; } && npm run build >/dev/null)
if command -v x86_64-w64-mingw32-gcc >/dev/null; then runner=cargo; else runner=cargo-zigbuild; fi
# The DLL only exists after the exe is built, so build that first, then bundle
# it into the installer beside Wit.exe.
$runner build --release -p wit-desktop --target $target
config='{"build":{"beforeBuildCommand":""},"bundle":{"resources":{"../'$out'/WebView2Loader.dll":"WebView2Loader.dll"}}}'
(cd desktop && cargo tauri build --runner $runner --target $target --bundles nsis --config "$config")
rm -rf target/Wit
mkdir -p target/Wit/fonts
cp "$out/wit-desktop.exe" target/Wit/Wit.exe
cp "$out/WebView2Loader.dll" target/Wit/
cp -r web/dist target/Wit/web
# The open fonts and their licences, as tauri.conf.json bundles them.
cp -r fonts/*.ttf fonts/README.md fonts/licenses target/Wit/fonts/
echo "Built:"
ls -1 "$out"/bundle/nsis/*-setup.exe
echo "target/Wit/ (Wit.exe, WebView2Loader.dll, web/, fonts/): copy the whole folder."
