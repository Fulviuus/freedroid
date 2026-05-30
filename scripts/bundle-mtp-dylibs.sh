#!/usr/bin/env bash
# Make an MTP-enabled Freedroid.app self-contained by copying libmtp + libusb
# into Contents/Frameworks and rewriting their install names to @rpath. Run after
# `tauri build --features mtp`. Pass the .app path (defaults to the native release).
set -euo pipefail

APP="${1:-src-tauri/target/release/bundle/macos/Freedroid.app}"
BIN="$APP/Contents/MacOS/freedroid"
FW="$APP/Contents/Frameworks"

[ -f "$BIN" ] || { echo "binary not found: $BIN" >&2; exit 1; }
mkdir -p "$FW"

# Resolve the libmtp the binary was linked against, and libmtp's libusb dep.
LIBMTP_SRC=$(otool -L "$BIN" | grep -oE '/[^ ]*libmtp[^ ]*\.dylib' | head -1)
[ -n "$LIBMTP_SRC" ] || { echo "binary doesn't link libmtp (build with --features mtp)" >&2; exit 1; }
LIBUSB_SRC=$(otool -L "$LIBMTP_SRC" | grep -oE '/[^ ]*libusb[^ ]*\.dylib' | head -1)

MTP=$(basename "$LIBMTP_SRC")   # libmtp.9.dylib
USB=$(basename "$LIBUSB_SRC")   # libusb-1.0.0.dylib

echo "Bundling $MTP + $USB into $FW"
cp -L "$LIBMTP_SRC" "$FW/$MTP"
cp -L "$LIBUSB_SRC" "$FW/$USB"
chmod u+w "$FW/$MTP" "$FW/$USB"

# Binary: reference libmtp via @rpath (the rpath is baked in by build.rs).
install_name_tool -change "$LIBMTP_SRC" "@rpath/$MTP" "$BIN"

# libmtp: set its id, and point its libusb dep at the sibling in Frameworks.
install_name_tool -id "@rpath/$MTP" "$FW/$MTP"
install_name_tool -change "$LIBUSB_SRC" "@loader_path/$USB" "$FW/$MTP"

# libusb: set its id.
install_name_tool -id "@rpath/$USB" "$FW/$USB"

# Re-sign (ad-hoc here; the release workflow re-signs with the real identity).
codesign --force -s - "$FW/$USB"
codesign --force -s - "$FW/$MTP"
codesign --force -s - "$BIN" 2>/dev/null || true

echo "Done. Verifying:"
otool -L "$BIN" | grep -iE "libmtp|libusb" || true
