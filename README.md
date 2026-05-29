# Freedroid

**Free & open-source Android file transfer for macOS.**

Freedroid lets you browse your Android phone's storage and move files to and from
your Mac through a clean, native-feeling dual-pane interface — over USB or Wi-Fi.
It's a community alternative to closed-source tools like MacDroid and the
discontinued Android File Transfer.

> Freedroid is an independent project. It is not affiliated with, endorsed by, or
> derived from MacDroid or its assets — only its functionality is reimplemented.

## Features

- 📱 **Dual-pane file manager** — Mac on the left, Android on the right, copy either way.
- 🔌 **USB transfer over ADB** — fast, reliable, scriptable.
- 📶 **Wi-Fi mode** — switch a USB-connected device to wireless, or pair directly
  (Android 11+ wireless debugging).
- 📂 **File operations** — create folders, rename, delete on the device.
- 📊 **Live transfer progress** with a transfer queue.
- 🆓 **MIT-licensed**, no telemetry, no account, **nothing extra to install**.

## How it works

Freedroid is a [Tauri 2](https://v2.tauri.app) app: a Rust backend drives the
Android Debug Bridge (`adb`), and a Svelte frontend renders the UI. The `adb`
binary is bundled as a sidecar, so there's nothing to install separately.

## Requirements

- macOS 11+ (Apple Silicon or Intel)
- On your phone: **Settings → Developer options → USB debugging** enabled
  (tap *Build number* 7× to reveal Developer options). Approve the
  "Allow USB debugging?" prompt when you first connect.

## Development

```bash
# 1. Install toolchains: Rust (rustup), Node 20+, Xcode Command Line Tools
# 2. Fetch the bundled adb sidecar
./scripts/fetch-adb.sh
# 3. Install JS deps and run
npm install
npm run tauri dev
```

Build a release bundle:

```bash
npm run tauri build
```

### Project layout

```
src/                  Svelte frontend
  lib/ipc.ts          typed wrappers over the Rust commands
  lib/state.svelte.ts central app state (Svelte 5 runes)
  lib/components/      Pane, DevicePicker, WifiDialog, TransferQueue
src-tauri/src/
  adb/                adb wrapper: devices, files, transfer, wifi
  local.rs            local (Mac) filesystem listing
  commands.rs         #[tauri::command] surface
```

## Roadmap

- [x] Device detection (USB) with authorization states
- [x] Dual-pane browsing of `/sdcard` and local files
- [x] Push / pull with live progress
- [x] Device file ops (mkdir / rename / delete)
- [x] Wi-Fi mode (tcpip + wireless pairing)
- [ ] Drag-and-drop between panes and from Finder
- [ ] Folder (recursive) transfers in the queue UI
- [ ] Code signing & notarization

> **Not planned:** "Mount as a disk in Finder." On macOS that requires a
> privileged filesystem driver (macFUSE/fuse-t) or a signed FSKit/File-Provider
> extension — all of which mean an extra system install or a paid Apple Developer
> account. Freedroid intentionally stays install-free; the dual-pane manager
> covers the same use cases.

## License

[MIT](LICENSE). `adb` is redistributed under the Apache License 2.0 as part of
the Android SDK platform-tools.
