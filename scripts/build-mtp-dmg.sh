#!/usr/bin/env bash
# Build a self-contained, MTP-enabled Freedroid DMG (Apple Silicon).
# Requires: libmtp installed (brew install libmtp), the adb sidecar fetched, and
# npm deps installed. Produces dist/Freedroid_<ver>_aarch64-mtp.dmg.
set -euo pipefail
cd "$(dirname "$0")/.."

# 1. Build the .app (app bundle only — we make the DMG ourselves after fixups).
npm run tauri build -- --features mtp --bundles app

APP="src-tauri/target/release/bundle/macos/Freedroid.app"

# 2. Make it self-contained: bundle libmtp + libusb, rewrite install names.
bash scripts/bundle-mtp-dylibs.sh "$APP"

# 3. Assemble a drag-to-install DMG.
VER=$(node -p "require('./package.json').version")
OUT="dist/Freedroid_${VER}_aarch64-mtp.dmg"
mkdir -p dist
STAGING="$(mktemp -d)"
cp -R "$APP" "$STAGING/"
ln -s /Applications "$STAGING/Applications"
rm -f "$OUT"
hdiutil create -volname "Freedroid" -srcfolder "$STAGING" -ov -format UDZO "$OUT" >/dev/null
rm -rf "$STAGING"

echo "Built $OUT"
echo "Self-containment check (want no /opt/homebrew):"
otool -L "$APP/Contents/MacOS/freedroid" | grep -i homebrew && echo "  !! still references homebrew" || echo "  OK — no homebrew refs"
