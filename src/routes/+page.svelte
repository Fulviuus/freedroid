<script lang="ts">
  import { onMount } from "svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { app } from "$lib/state.svelte";
  import * as ipc from "$lib/ipc";

  // Drag the window from the title bar, except when grabbing an interactive
  // control (the device dropdown / buttons).
  function startDrag(e: MouseEvent) {
    if (e.button !== 0) return;
    const el = e.target as HTMLElement;
    if (el.closest("button, select, input, a")) return;
    getCurrentWindow()
      .startDragging()
      .catch(() => {});
  }
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
  let deviceRoot = $state(DEVICE_ROOT);
  let deviceRootLabel = $state("Internal storage");
  let deviceVolumes = $state<ipc.Volume[]>([]);
  let devicePath = $state(DEVICE_ROOT);
  let deviceEntries = $state<FileEntry[]>([]);
  let deviceLoading = $state(false);
  let deviceError = $state<string | null>(null);
  let deviceSelected = $state<Set<string>>(new Set());

  function switchVolume(path: string) {
    const vol = deviceVolumes.find((v) => v.path === path);
    if (!vol) return;
    deviceRoot = vol.path;
    deviceRootLabel = vol.label;
    deviceSelected = new Set();
    devicePath = vol.path;
  }

  // ----- MTP mode (connect without USB debugging) -----
  let deviceBackend = $state<"adb" | "mtp">("adb");
  let mtpStorages = $state<ipc.MtpStorage[]>([]);
  let mtpStorageId = $state(0);
  let mtpName = $state("");
  // Synthetic path -> MTP object id ("/" -> 0 = storage root).
  let mtpIds = new Map<string, number>();

  let mtpConnected = $derived(deviceBackend === "mtp" && mtpStorages.length > 0);
  let deviceReady = $derived(deviceBackend === "mtp" ? mtpConnected : app.ready);

  function mtpChild(parent: string, name: string): string {
    return parent === "/" ? "/" + name : parent + "/" + name;
  }

  async function connectMtp() {
    try {
      const info = await ipc.mtpConnect();
      if (info.storages.length === 0) throw new Error("no MTP storage found");
      mtpName = info.name;
      mtpStorages = info.storages;
      mtpStorageId = info.storages[0].id;
      deviceBackend = "mtp";
      mtpIds = new Map([["/", 0]]);
      deviceRoot = "/";
      deviceRootLabel = info.storages[0].description;
      deviceSelected = new Set();
      devicePath = "/";
      loadDevice();
    } catch (e) {
      app.notify(String(e), "error");
    }
  }

  function switchToAdb() {
    deviceBackend = "adb";
    mtpStorages = [];
    ipc.mtpDisconnect().catch(() => {});
    deviceRoot = DEVICE_ROOT;
    deviceRootLabel = "Internal storage";
    deviceSelected = new Set();
    devicePath = DEVICE_ROOT;
    loadDevice();
  }

  function switchMtpStorage(idStr: string) {
    const s = mtpStorages.find((x) => x.id === Number(idStr));
    if (!s) return;
    mtpStorageId = s.id;
    mtpIds = new Map([["/", 0]]);
    deviceRootLabel = s.description;
    deviceSelected = new Set();
    devicePath = "/";
    loadDevice();
  }

  async function loadDeviceMtp() {
    const parent = mtpIds.get(devicePath) ?? 0;
    deviceLoading = true;
    deviceError = null;
    try {
      const items = await ipc.mtpList(mtpStorageId, parent);
      deviceEntries = items.map((it) => {
        const p = mtpChild(devicePath, it.name);
        mtpIds.set(p, it.id);
        return {
          name: it.name,
          path: p,
          size: it.size,
          mtime: it.mtime,
          isDir: it.isDir,
          isSymlink: false,
        };
      });
    } catch (e) {
      deviceError = String(e);
      deviceEntries = [];
    } finally {
      deviceLoading = false;
    }
  }

  let showWifi = $state(false);

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
    if (deviceBackend === "mtp") return loadDeviceMtp();
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
    deviceBackend;
    mtpStorageId;
    loadDevice();
  });

  // Load the device's storage volumes (internal + SD card) when it's ready.
  $effect(() => {
    const serial = app.selectedSerial;
    if (deviceBackend !== "adb") return; // MTP manages its own storages
    if (app.ready && serial) {
      ipc
        .listVolumes(serial)
        .then((v) => (deviceVolumes = v))
        .catch(() => (deviceVolumes = []));
    } else {
      deviceVolumes = [];
      // Reset to internal storage when the device goes away.
      deviceRoot = DEVICE_ROOT;
      deviceRootLabel = "Internal storage";
      devicePath = DEVICE_ROOT;
    }
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
  // adb push/pull handle directories recursively. We pass the destination
  // *directory* (not a full path) and let adb name the copy after the source,
  // which works uniformly for both files and folders.
  // MTP transfers — libmtp's progress callback drives the bar (see mtp_pull/push).
  async function pushSelectedMtp() {
    const parent = mtpIds.get(devicePath) ?? 0;
    const files = localEntries.filter((e) => localSelected.has(e.path) && !e.isDir);
    if (localEntries.some((e) => localSelected.has(e.path) && e.isDir))
      app.notify("MTP can't push folders yet — select files", "info");
    for (const entry of files) {
      const id = app.startTransfer(entry.name, "push");
      try {
        await ipc.mtpPush(entry.path, parent, mtpStorageId, entry.name, id);
        app.finishTransfer(id, true);
      } catch (e) {
        app.finishTransfer(id, false, String(e));
      }
    }
    loadDevice();
  }

  async function pullSelectedMtp() {
    const files = deviceEntries.filter((e) => deviceSelected.has(e.path) && !e.isDir);
    if (deviceEntries.some((e) => deviceSelected.has(e.path) && e.isDir))
      app.notify("MTP can't pull folders yet — select files", "info");
    for (const entry of files) {
      const oid = mtpIds.get(entry.path);
      if (oid == null) continue;
      const id = app.startTransfer(entry.name, "pull");
      try {
        await ipc.mtpPull(oid, joinPath(localPath, entry.name), id, entry.name);
        app.finishTransfer(id, true);
      } catch (e) {
        app.finishTransfer(id, false, String(e));
      }
    }
    loadLocal();
  }

  async function pushSelected() {
    if (deviceBackend === "mtp") return pushSelectedMtp();
    if (!app.selectedSerial) return;
    const items = localEntries.filter((e) => localSelected.has(e.path));
    for (const entry of items) {
      const id = app.startTransfer(entry.name, "push");
      try {
        await ipc.pushFile(
          app.selectedSerial,
          entry.path,
          devicePath,
          id,
          entry.name,
          entry.size,
          entry.isDir,
        );
      } catch (e) {
        app.finishTransfer(id, false, String(e));
      }
    }
    loadDevice();
  }

  async function pullSelected() {
    if (deviceBackend === "mtp") return pullSelectedMtp();
    if (!app.selectedSerial) return;
    const items = deviceEntries.filter((e) => deviceSelected.has(e.path));
    for (const entry of items) {
      const id = app.startTransfer(entry.name, "pull");
      try {
        await ipc.pullFile(
          app.selectedSerial,
          entry.path,
          localPath,
          id,
          entry.name,
          entry.size,
          entry.isDir,
        );
      } catch (e) {
        app.finishTransfer(id, false, String(e));
      }
    }
    loadLocal();
  }

  // ----- Drag and drop between panes -----
  let dragSource = $state<"local" | "device" | null>(null);

  // ----- Local (Mac) file ops -----
  async function newFolderLocal() {
    const name = prompt("New folder name:");
    if (!name) return;
    try {
      await ipc.localMakeDir(joinPath(localPath, name));
      loadLocal();
    } catch (e) {
      app.notify(String(e), "error");
    }
  }
  async function deleteLocal() {
    if (localSelected.size === 0) return;
    const n = localSelected.size;
    if (!confirm(`Move ${n} item(s) to the Trash?`)) return;
    app.notify(`Moving ${n} item${n === 1 ? "" : "s"} to Trash…`);
    try {
      await ipc.localTrash([...localSelected]);
      app.notify(`Moved ${n} item${n === 1 ? "" : "s"} to Trash`);
      localSelected = new Set();
      loadLocal();
    } catch (e) {
      app.notify(String(e), "error");
    }
  }
  async function renameLocal(entry: FileEntry) {
    const next = prompt("Rename to:", entry.name);
    if (!next || next === entry.name) return;
    try {
      await ipc.localRename(entry.path, joinPath(localPath, next));
      loadLocal();
    } catch (e) {
      app.notify(String(e), "error");
    }
  }

  // ----- Device file ops -----
  async function newFolderDevice() {
    const name = prompt("New folder name:");
    if (!name) return;
    try {
      if (deviceBackend === "mtp") {
        await ipc.mtpMkdir(name, mtpIds.get(devicePath) ?? 0, mtpStorageId);
      } else {
        if (!app.selectedSerial) return;
        await ipc.deviceMakeDir(app.selectedSerial, joinPath(devicePath, name));
      }
      loadDevice();
    } catch (e) {
      app.notify(String(e), "error");
    }
  }
  async function deleteDevice() {
    if (deviceSelected.size === 0) return;
    const n = deviceSelected.size;
    if (!confirm(`Delete ${n} item(s) from the device? This cannot be undone.`))
      return;
    app.notify(`Deleting ${n} item${n === 1 ? "" : "s"}…`);
    try {
      if (deviceBackend === "mtp") {
        for (const p of deviceSelected) {
          const oid = mtpIds.get(p);
          if (oid != null) await ipc.mtpDelete(oid);
        }
      } else {
        if (!app.selectedSerial) return;
        await ipc.deviceRemoveMany(app.selectedSerial, [...deviceSelected]);
      }
      app.notify(`Deleted ${n} item${n === 1 ? "" : "s"}`);
      deviceSelected = new Set();
    } catch (e) {
      app.notify(String(e), "error");
    }
    loadDevice();
  }
  async function renameDevice(entry: FileEntry) {
    if (deviceBackend === "mtp") {
      app.notify("Rename isn't supported in MTP mode yet", "info");
      return;
    }
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
  // Open a device file (download to temp, then open with the default app).
  async function openDeviceEntry(entry: FileEntry) {
    if (deviceBackend === "mtp") {
      const oid = mtpIds.get(entry.path);
      if (oid == null) return;
      app.notify(`Opening ${entry.name}…`);
      const tmp = `/tmp/freedroid-open-${entry.name}`;
      try {
        await ipc.mtpPull(oid, tmp, "open", entry.name);
        await ipc.openLocal(tmp);
      } catch (e) {
        app.notify(String(e), "error");
      }
      return;
    }
    if (!app.selectedSerial) return;
    app.notify(`Opening ${entry.name}…`);
    ipc
      .openDeviceFile(app.selectedSerial, entry.path, entry.name)
      .catch((e) => app.notify(String(e), "error"));
  }

  onMount(() => {
    (async () => {
      app.adbVersion = await ipc.adbVersion().catch(() => "");
      localRoot = await ipc.localHome().catch(() => "/");
      localPath = localRoot;
      await app.refreshDevices();
    })();

    const poll = setInterval(() => app.refreshDevices(), 3000);

    const unsubs: Array<Promise<() => void>> = [
      ipc.onTransferProgress((p) => app.updateProgress(p.id, p)),
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
  <header class="titlebar" onmousedown={startDrag}>
    <div class="brand">
      <span class="logo">🤖</span>
      <span class="title">Freedroid</span>
    </div>
    <DevicePicker onWifi={() => (showWifi = true)} />
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
      canWrite={true}
      bind:selected={localSelected}
      onNavigate={navLocal}
      onRefresh={loadLocal}
      onNewFolder={newFolderLocal}
      onDelete={deleteLocal}
      onRename={renameLocal}
      onOpen={(entry) => ipc.openLocal(entry.path).catch((e) => app.notify(String(e), "error"))}
      onDragOut={() => (dragSource = "local")}
      onDropIn={() => {
        if (dragSource === "device") pullSelected();
        dragSource = null;
      }}
    />

    <div class="transfer-col">
      <button
        class="xfer"
        title="Copy to device"
        disabled={!deviceReady || localSelected.size === 0}
        onclick={pushSelected}>→</button
      >
      <button
        class="xfer"
        title="Copy to Mac"
        disabled={!deviceReady || deviceSelected.size === 0}
        onclick={pullSelected}>←</button
      >
    </div>

    <Pane
      title={deviceBackend === "mtp"
        ? mtpName || "Android (MTP)"
        : app.selectedDevice
          ? (app.selectedDevice.model ?? app.selectedDevice.serial).replace(/_/g, " ")
          : "Android device"}
      icon="📱"
      path={devicePath}
      rootPath={deviceRoot}
      rootLabel={deviceRootLabel}
      entries={deviceEntries}
      loading={deviceLoading}
      error={deviceError}
      canWrite={deviceReady}
      bind:selected={deviceSelected}
      onNavigate={navDevice}
      onRefresh={loadDevice}
      onNewFolder={newFolderDevice}
      onDelete={deleteDevice}
      onRename={renameDevice}
      onOpen={openDeviceEntry}
      onDragOut={() => (dragSource = "device")}
      onDropIn={() => {
        if (dragSource === "local") pushSelected();
        dragSource = null;
      }}
    >
      {#snippet headerExtra()}
        <div class="mode-toggle">
          <button class:active={deviceBackend === "adb"} onclick={switchToAdb} title="USB debugging (ADB)">USB</button>
          <button class:active={deviceBackend === "mtp"} onclick={connectMtp} title="MTP — no USB debugging needed">MTP</button>
        </div>
        {#if deviceBackend === "mtp" && mtpStorages.length > 1}
          <select
            class="vol-picker"
            value={String(mtpStorageId)}
            onchange={(e) => switchMtpStorage((e.currentTarget as HTMLSelectElement).value)}
          >
            {#each mtpStorages as s (s.id)}
              <option value={String(s.id)}>{s.description}</option>
            {/each}
          </select>
        {:else if deviceBackend === "adb" && deviceVolumes.length > 1}
          <select
            class="vol-picker"
            value={deviceRoot}
            onchange={(e) => switchVolume((e.currentTarget as HTMLSelectElement).value)}
          >
            {#each deviceVolumes as v (v.path)}
              <option value={v.path}>{v.label}</option>
            {/each}
          </select>
        {/if}
      {/snippet}
    </Pane>
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
  :global(.vol-picker) {
    background: var(--bg);
    color: var(--text);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 2px 6px;
    font-size: 11px;
  }
  :global(.mode-toggle) {
    display: inline-flex;
    border: 1px solid var(--border);
    border-radius: 6px;
    overflow: hidden;
  }
  :global(.mode-toggle button) {
    border: none;
    background: var(--bg);
    color: var(--muted);
    font-size: 10px;
    font-weight: 600;
    padding: 3px 8px;
    cursor: pointer;
  }
  :global(.mode-toggle button.active) {
    background: var(--accent);
    color: #fff;
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
