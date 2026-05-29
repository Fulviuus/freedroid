#!/usr/bin/env bash
# Fetch Google's platform-tools `adb` and place it as Tauri sidecars for both
# macOS architectures. adb is Apache-2.0 (part of AOSP) and safe to bundle.
set -euo pipefail

DEST="$(cd "$(dirname "$0")/.." && pwd)/src-tauri/binaries"
mkdir -p "$DEST"

TMP="$(mktemp -d)"
URL="https://dl.google.com/android/repository/platform-tools-latest-darwin.zip"

echo "Downloading platform-tools…"
curl -fSL "$URL" -o "$TMP/pt.zip"
unzip -q "$TMP/pt.zip" -d "$TMP"

# adb from platform-tools is a universal (arm64 + x86_64) binary, so the same
# file serves every target triple. The `universal-apple-darwin` name is what the
# bundler looks for when building `--target universal-apple-darwin`.
for triple in aarch64-apple-darwin x86_64-apple-darwin universal-apple-darwin; do
  cp "$TMP/platform-tools/adb" "$DEST/adb-$triple"
done
chmod +x "$DEST"/adb-*

lipo -info "$DEST/adb-universal-apple-darwin" 2>/dev/null || true

rm -rf "$TMP"
echo "adb sidecars written to $DEST"
