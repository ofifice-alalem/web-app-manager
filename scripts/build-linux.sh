#!/usr/bin/env bash
set -euo pipefail

# Build script for Linux Mint (Ubuntu-based, webkit2gtk-4.1)
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="$ROOT_DIR/deploay/linux"

mkdir -p "$OUT_DIR" "$ROOT_DIR/deploay/windows"

echo "Building Seller App (desktop-application-host) for Linux Mint..."
cd "$ROOT_DIR"
npm install
npm run tauri build -- --bundles deb,appimage

echo "Building Settings App (desktop-host-settings) for Linux Mint..."
cd "$ROOT_DIR/settings-app"
npm install
npm run tauri build -- --bundles deb,appimage

cp -f "$ROOT_DIR"/src-tauri/target/release/bundle/deb/*.deb "$OUT_DIR"/ 2>/dev/null || true
cp -f "$ROOT_DIR"/src-tauri/target/release/bundle/appimage/*.AppImage "$OUT_DIR"/ 2>/dev/null || true
cp -f "$ROOT_DIR"/settings-app/src-tauri/target/release/bundle/deb/*.deb "$OUT_DIR"/ 2>/dev/null || true
cp -f "$ROOT_DIR"/settings-app/src-tauri/target/release/bundle/appimage/*.AppImage "$OUT_DIR"/ 2>/dev/null || true

echo "Linux Mint artifacts deployed to: $OUT_DIR"
ls -lh "$OUT_DIR"
