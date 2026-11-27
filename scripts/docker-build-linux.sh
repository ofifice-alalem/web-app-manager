#!/usr/bin/env bash
set -euo pipefail

export DEBIAN_FRONTEND=noninteractive
apt-get update
apt-get install -y curl wget file build-essential libssl-dev libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libgtk-3-dev patchelf ca-certificates gnupg

if ! command -v node >/dev/null 2>&1; then
  curl -fsSL https://deb.nodesource.com/setup_22.x | bash -
  apt-get install -y nodejs
fi

if ! command -v cargo >/dev/null 2>&1; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
fi
export PATH="/root/.cargo/bin:$PATH"

mkdir -p /work/deploay/linux

rm -rf /tmp/build_host
mkdir -p /tmp/build_host
cp -r /work/src /work/src-tauri /work/package.json /work/package-lock.json /work/tsconfig.json /work/vite.config.ts /work/index.html /work/config /tmp/build_host/
cp -r /work/settings-app /tmp/build_host/settings-app

echo "=== Building desktop-application-host (.deb) ==="
cd /tmp/build_host
npm ci --silent
CARGO_TARGET_DIR=/tmp/target-host npm run tauri build -- --bundles deb
cp -f /tmp/target-host/release/bundle/deb/*.deb /work/deploay/linux/

echo "=== Building desktop-host-settings (.deb) ==="
cd /tmp/build_host/settings-app
npm ci --silent
CARGO_TARGET_DIR=/tmp/target-settings npm run tauri build -- --bundles deb
cp -f /tmp/target-settings/release/bundle/deb/*.deb /work/deploay/linux/

ls -lh /work/deploay/linux/
