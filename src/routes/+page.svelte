<script lang="ts">
  import { onMount } from "svelte";
  import { app } from "$lib/state.svelte";
  import * as ipc from "$lib/ipc";
  import type { FileEntry } from "$lib/ipc";
  import { joinPath } from "$lib/util";
  import Pane from "$lib/components/Pane.svelte";
  import DevicePicker from "$lib/components/DevicePicker.svelte";
  import WifiDialog from "$lib/components/WifiDialog.svelte";
  import TransferQueue from "$lib/components/TransferQueue.svelte";

  const DEVICE_ROOT = "/sdcard";

  // ----- Local (Mac) pane state -----
  let localRoot = $state("/");
  let localPath = $state("/");
  let localEntries = $state<FileEntry[]>([]);
  let localLoading = $state(false);
  let localError = $state<string | null>(null);
  let localSelected = $state<Set<string>>(new Set());

  // ----- Device (Android) pane state -----
  let devicePath = $state(DEVICE_ROOT);
  let deviceEntries = $state<FileEntry[]>([]);
  let deviceLoading = $state(false);
  let deviceError = $state<string | null>(null);
  let deviceSelected = $state<Set<string>>(new Set());

  let showWifi = $state(false);

  // ----- Finder mount (FUSE) -----
  let fuseOk = $state(false);
  let mountpoint = $state<string | null>(null);
  let mounting = $state(false);

  async function toggleMount() {
    if (!app.selectedSerial || !app.ready) return;
    mounting = true;
    try {
      if (mountpoint) {
        await ipc.unmountDevice();
        mountpoint = null;
        app.notify("Device unmounted");
      } else {
        const name =
          (app.selectedDevice?.model ?? app.selectedSerial).replace(/_/g, " ");
        mountpoint = await ipc.mountDevice(app.selectedSerial, name, DEVICE_ROOT);
        app.notify(`Mounted in Finder at ${mountpoint}`);
      }
    } catch (e) {
      app.notify(String(e), "error");
    } finally {
      mounting = false;
    }
  }

  async function loadLocal() {
    localLoading = true;
    localError = null;
    try {
      localEntries = await ipc.listLocalDir(localPath);
    } catch (e) {
      localError = String(e);
      localEntries = [];
    } finally {
      localLoading = false;
    }
  }

  async function loadDevice() {
    if (!app.ready || !app.selectedSerial) {
      deviceEntries = [];
      deviceError =
        app.selectedDevice && !app.ready ? "Device not authorized" : null;
      return;
    }
    deviceLoading = true;
    deviceError = null;
    try {
      deviceEntries = await ipc.listDeviceDir(app.selectedSerial, devicePath);
    } catch (e) {
      deviceError = String(e);
      deviceEntries = [];
    } finally {
      deviceLoading = false;
    }
  }

  // Reactively reload panes when their path / selected device changes.
  $effect(() => {
    localPath;
    loadLocal();
  });
  $effect(() => {
    devicePath;
    app.selectedSerial;
    app.ready;
    loadDevice();
  });

  function navLocal(p: string) {
    localSelected = new Set();
    localPath = p;
  }
  function navDevice(p: string) {
    deviceSelected = new Set();
    devicePath = p;
  }

  // ----- Transfers -----
  async function pushSelected() {
    if (!app.selectedSerial) return;
    const items = localEntries.filter((e) => localSelected.has(e.path) && !e.isDir);
    const dirs = localEntries.filter((e) => localSelected.has(e.path) && e.isDir);
    if (dirs.length)
      app.notify("Folder transfer isn't supported yet — select files", "info");
    for (const entry of items) {
      const id = app.startTransfer(entry.name, "push");
      const remote = joinPath(devicePath, entry.name);
      try {
        await ipc.pushFile(app.selectedSerial, entry.path, remote, id, entry.name);
      } catch (e) {
        app.finishTransfer(id, false, String(e));
      }
    }
    loadDevice();
  }

  async function pullSelected() {
    if (!app.selectedSerial) return;
    const items = deviceEntries.filter((e) => deviceSelected.has(e.path) && !e.isDir);
    const dirs = deviceEntries.filter((e) => deviceSelected.has(e.path) && e.isDir);
    if (dirs.length)
      app.notify("Folder transfer isn't supported yet — select files", "info");
    for (const entry of items) {
      const id = app.startTransfer(entry.name, "pull");
      const local = joinPath(localPath, entry.name);
      try {
        await ipc.pullFile(app.selectedSerial, entry.path, local, id, entry.name);
      } catch (e) {
        app.finishTransfer(id, false, String(e));
      }
    }
    loadLocal();
  }

  // ----- Device file ops -----
  async function newFolderDevice() {
    if (!app.selectedSerial) return;
    const name = prompt("New folder name:");
    if (!name) return;
    try {
      await ipc.deviceMakeDir(app.selectedSerial, joinPath(devicePath, name));
      loadDevice();
    } catch (e) {
      app.notify(String(e), "error");
    }
  }
  async function deleteDevice() {
    if (!app.selectedSerial || deviceSelected.size === 0) return;
    if (
      !confirm(
        `Delete ${deviceSelected.size} item(s) from the device? This cannot be undone.`,
      )
    )
      return;
    for (const path of deviceSelected) {
      try {
        await ipc.deviceRemove(app.selectedSerial, path);
      } catch (e) {
        app.notify(String(e), "error");
      }
    }
    deviceSelected = new Set();
    loadDevice();
  }
  async function renameDevice(entry: FileEntry) {
    if (!app.selectedSerial) return;
    const next = prompt("Rename to:", entry.name);
    if (!next || next === entry.name) return;
    try {
      await ipc.deviceRename(
        app.selectedSerial,
        entry.path,
        joinPath(devicePath, next),
      );
      loadDevice();
    } catch (e) {
      app.notify(String(e), "error");
    }
  }

  onMount(() => {
    (async () => {
      app.adbVersion = await ipc.adbVersion().catch(() => "");
      localRoot = await ipc.localHome().catch(() => "/");
      localPath = localRoot;
      fuseOk = await ipc.fuseAvailable().catch(() => false);
      mountpoint = await ipc.currentMountpoint().catch(() => null);
      await app.refreshDevices();
    })();

    const poll = setInterval(() => app.refreshDevices(), 3000);

    const unsubs: Array<Promise<() => void>> = [
      ipc.onTransferProgress((p) => app.updateProgress(p.id, p.percent)),
      ipc.onTransferDone((d) =>
        app.finishTransfer(d.id, d.success, d.error ?? undefined),
      ),
    ];

    return () => {
      clearInterval(poll);
      unsubs.forEach((u) => u.then((fn) => fn()));
    };
  });
</script>

<div class="app">
  <header class="titlebar" data-tauri-drag-region>
    <div class="brand">
      <span class="logo">🤖</span>
      <span class="title">Freedroid</span>
    </div>
    <div class="head-right">
      {#if fuseOk}
        <button
          class="mount-btn"
          class:mounted={mountpoint}
          disabled={!app.ready || mounting}
          title={mountpoint ? `Mounted at ${mountpoint}` : "Mount device in Finder"}
          onclick={toggleMount}
        >
          {mounting ? "…" : mountpoint ? "⏏ Unmount" : "🗂 Mount in Finder"}
        </button>
      {/if}
      <DevicePicker onWifi={() => (showWifi = true)} />
    </div>
  </header>

  <main class="workspace">
    <Pane
      title="This Mac"
      icon="💻"
      path={localPath}
      rootPath={localRoot}
      entries={localEntries}
      loading={localLoading}
      error={localError}
      canWrite={false}
      bind:selected={localSelected}
      onNavigate={navLocal}
      onRefresh={loadLocal}
      onNewFolder={() => {}}
      onDelete={() => {}}
      onRename={() => {}}
      onActivate={pushSelected}
    />

    <div class="transfer-col">
      <button
        class="xfer"
        title="Copy to device"
        disabled={!app.ready || localSelected.size === 0}
        onclick={pushSelected}>→</button
      >
      <button
        class="xfer"
        title="Copy to Mac"
        disabled={!app.ready || deviceSelected.size === 0}
        onclick={pullSelected}>←</button
      >
    </div>

    <Pane
      title={app.selectedDevice
        ? (app.selectedDevice.model ?? app.selectedDevice.serial).replace(/_/g, " ")
        : "Android device"}
      icon="📱"
      path={devicePath}
      rootPath={DEVICE_ROOT}
      entries={deviceEntries}
      loading={deviceLoading}
      error={deviceError}
      canWrite={app.ready}
      bind:selected={deviceSelected}
      onNavigate={navDevice}
      onRefresh={loadDevice}
      onNewFolder={newFolderDevice}
      onDelete={deleteDevice}
      onRename={renameDevice}
      onActivate={pullSelected}
    />
  </main>

  <TransferQueue />

  <footer class="statusbar">
    <span>{app.adbVersion || "adb not found"}</span>
    <span>{app.devices.length} device(s)</span>
  </footer>

  {#if app.toast}
    <div class="toast" class:error={app.toast.kind === "error"}>
      {app.toast.msg}
    </div>
  {/if}

  {#if showWifi}
    <WifiDialog onClose={() => (showWifi = false)} />
  {/if}
</div>

<style>
  .app {
    display: flex;
    flex-direction: column;
    height: 100vh;
    overflow: hidden;
  }
  .titlebar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 14px 8px 80px; /* room for macOS traffic lights */
    background: var(--head-bg);
    border-bottom: 1px solid var(--border);
    -webkit-user-select: none;
    user-select: none;
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .head-right {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .mount-btn {
    border: 1px solid var(--border);
    background: var(--pane-bg);
    color: var(--text);
    border-radius: 7px;
    padding: 5px 10px;
    font-size: 12px;
    cursor: pointer;
    white-space: nowrap;
  }
  .mount-btn:hover:not(:disabled) {
    background: var(--hover);
  }
  .mount-btn.mounted {
    background: var(--accent);
    color: #fff;
    border-color: var(--accent);
  }
  .mount-btn:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .logo {
    font-size: 18px;
  }
  .title {
    font-weight: 700;
    font-size: 14px;
    letter-spacing: -0.01em;
  }

  .workspace {
    flex: 1;
    display: grid;
    grid-template-columns: 1fr auto 1fr;
    gap: 12px;
    padding: 12px;
    min-height: 0;
  }
  .transfer-col {
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 12px;
  }
  .xfer {
    width: 40px;
    height: 40px;
    border-radius: 10px;
    border: 1px solid var(--border);
    background: var(--pane-bg);
    color: var(--accent);
    font-size: 20px;
    cursor: pointer;
  }
  .xfer:hover:not(:disabled) {
    background: var(--accent);
    color: #fff;
  }
  .xfer:disabled {
    opacity: 0.3;
    cursor: default;
  }

  .statusbar {
    display: flex;
    justify-content: space-between;
    padding: 4px 14px;
    font-size: 11px;
    color: var(--muted);
    background: var(--head-bg);
    border-top: 1px solid var(--border);
  }
  .toast {
    position: fixed;
    bottom: 56px;
    left: 50%;
    transform: translateX(-50%);
    background: #323232;
    color: #fff;
    padding: 9px 16px;
    border-radius: 9px;
    font-size: 13px;
    box-shadow: 0 8px 30px rgba(0, 0, 0, 0.4);
    z-index: 60;
    max-width: 80vw;
  }
  .toast.error {
    background: #c0392b;
  }
</style>
