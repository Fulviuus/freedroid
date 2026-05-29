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
# file serves both target triples.
cp "$TMP/platform-tools/adb" "$DEST/adb-aarch64-apple-darwin"
cp "$TMP/platform-tools/adb" "$DEST/adb-x86_64-apple-darwin"
chmod +x "$DEST"/adb-*

rm -rf "$TMP"
echo "adb sidecars written to $DEST"
