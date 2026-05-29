<script lang="ts">
  import type { Snippet } from "svelte";
  import type { FileEntry } from "../ipc";
  import { formatSize, formatDate, fileIcon, parentPath } from "../util";

  interface Props {
    title: string;
    icon: string;
    path: string;
    entries: FileEntry[];
    loading: boolean;
    error?: string | null;
    canWrite: boolean;
    selected: Set<string>;
    onNavigate: (path: string) => void;
    onRefresh: () => void;
    onNewFolder: () => void;
    onDelete: () => void;
    onRename: (entry: FileEntry) => void;
    onOpen?: (entry: FileEntry) => void; // double-click a file to open it
    onDragOut?: () => void; // a row drag started from this pane
    onDropIn?: () => void; // something was dropped onto this pane
    rootPath: string;
    rootLabel?: string; // breadcrumb label for the root (else inferred)
    headerExtra?: Snippet; // optional content in the header (e.g. volume picker)
  }

  let {
    title,
    icon,
    path,
    entries,
    loading,
    error = null,
    canWrite,
    selected = $bindable(),
    onNavigate,
    onRefresh,
    onNewFolder,
    onDelete,
    onRename,
    onOpen,
    onDragOut,
    onDropIn,
    rootPath,
    rootLabel,
    headerExtra,
  }: Props = $props();

  let dragOver = $state(false);
  let sortKey = $state<"name" | "size" | "mtime">("name");
  let sortDir = $state<1 | -1>(1);
  let filter = $state("");
  let anchorIndex: number | null = null;

  // Sort (folders first, then by the chosen column) and filter by name.
  let view = $derived.by(() => {
    const arr = [...entries];
    arr.sort((a, b) => {
      if (a.isDir !== b.isDir) return a.isDir ? -1 : 1;
      let c = 0;
      if (sortKey === "name")
        c = a.name.toLowerCase().localeCompare(b.name.toLowerCase());
      else if (sortKey === "size") c = a.size - b.size;
      else c = a.mtime - b.mtime;
      return c * sortDir;
    });
    const q = filter.trim().toLowerCase();
    return q ? arr.filter((e) => e.name.toLowerCase().includes(q)) : arr;
  });

  function setSort(key: "name" | "size" | "mtime") {
    if (sortKey === key) sortDir = sortDir === 1 ? -1 : 1;
    else {
      sortKey = key;
      sortDir = 1;
    }
  }

  // Clear the filter and selection anchor when the folder changes.
  $effect(() => {
    path;
    filter = "";
    anchorIndex = null;
  });

  function rowDragStart(e: DragEvent, entry: FileEntry) {
    // If dragging a row that isn't part of the selection, select just it.
    if (!selected.has(entry.path)) {
      selected.clear();
      selected.add(entry.path);
      selected = new Set(selected);
    }
    if (e.dataTransfer) {
      e.dataTransfer.effectAllowed = "copy";
      e.dataTransfer.setData("text/plain", entry.path);
    }
    onDragOut?.();
  }

  function paneDragOver(e: DragEvent) {
    // Only react to internal app drags (OS file drops are handled by Tauri).
    if (!onDropIn) return;
    e.preventDefault();
    if (e.dataTransfer) e.dataTransfer.dropEffect = "copy";
    dragOver = true;
  }

  function paneDrop(e: DragEvent) {
    if (!onDropIn) return;
    e.preventDefault();
    dragOver = false;
    onDropIn();
  }

  // Breadcrumb segments relative to root.
  let crumbs = $derived.by(() => {
    const out: { label: string; path: string }[] = [];
    const label = rootLabel ?? (rootPath === "/sdcard" ? "Device" : "Home");
    out.push({ label, path: rootPath });
    if (path.startsWith(rootPath) && path !== rootPath) {
      const rest = path.slice(rootPath.length).replace(/^\/+/, "");
      let acc = rootPath;
      for (const seg of rest.split("/")) {
        if (!seg) continue;
        acc = acc + "/" + seg;
        out.push({ label: seg, path: acc });
      }
    }
    return out;
  });

  function rowClick(e: MouseEvent, entry: FileEntry, index: number) {
    if (e.shiftKey && anchorIndex !== null) {
      const [a, b] = [anchorIndex, index].sort((x, y) => x - y);
      if (!(e.metaKey || e.ctrlKey)) selected.clear();
      for (const en of view.slice(a, b + 1)) selected.add(en.path);
    } else if (e.metaKey || e.ctrlKey) {
      selected.has(entry.path)
        ? selected.delete(entry.path)
        : selected.add(entry.path);
      anchorIndex = index;
    } else {
      selected.clear();
      selected.add(entry.path);
      anchorIndex = index;
    }
    selected = new Set(selected); // trigger reactivity
  }

  function rowDblClick(entry: FileEntry) {
    if (entry.isDir) {
      onNavigate(entry.path);
    } else {
      onOpen?.(entry);
    }
  }

  let canUp = $derived(path !== rootPath && path !== "/");
</script>

<section class="pane">
  <header class="pane-head">
    <span class="pane-title"><span class="pane-icon">{icon}</span>{title}</span>
    {#if headerExtra}<div class="header-extra">{@render headerExtra()}</div>{/if}
    <div class="pane-actions">
      <button class="icon-btn" title="Up" disabled={!canUp} onclick={() => onNavigate(parentPath(path))}>↑</button>
      <button class="icon-btn" title="Refresh" onclick={onRefresh}>⟳</button>
      <button class="icon-btn" title="New folder" disabled={!canWrite} onclick={onNewFolder}>＋</button>
      <button class="icon-btn" title="Rename" disabled={!canWrite || selected.size !== 1} onclick={() => { const ent = entries.find(e => selected.has(e.path)); if (ent) onRename(ent); }}>✎</button>
      <button class="icon-btn danger" title="Delete" disabled={!canWrite || selected.size === 0} onclick={onDelete}>🗑</button>
    </div>
  </header>

  <nav class="breadcrumb">
    {#each crumbs as c, i}
      {#if i > 0}<span class="sep">›</span>{/if}
      <button class="crumb" onclick={() => onNavigate(c.path)}>{c.label}</button>
    {/each}
    <input class="filter" placeholder="Filter…" bind:value={filter} />
  </nav>

  <div
    class="filelist"
    class:drag-over={dragOver}
    role="listbox"
    tabindex="0"
    ondragover={paneDragOver}
    ondragleave={() => (dragOver = false)}
    ondrop={paneDrop}
  >
    {#if loading}
      <div class="placeholder">Loading…</div>
    {:else if error}
      <div class="placeholder error">{error}</div>
    {:else if entries.length === 0}
      <div class="placeholder">Empty folder</div>
    {:else}
      <div class="row head">
        <button class="col-name sortbtn" onclick={() => setSort("name")}>
          Name {sortKey === "name" ? (sortDir === 1 ? "▲" : "▼") : ""}
        </button>
        <button class="col-size sortbtn" onclick={() => setSort("size")}>
          Size {sortKey === "size" ? (sortDir === 1 ? "▲" : "▼") : ""}
        </button>
        <button class="col-date sortbtn" onclick={() => setSort("mtime")}>
          Modified {sortKey === "mtime" ? (sortDir === 1 ? "▲" : "▼") : ""}
        </button>
      </div>
      {#if view.length === 0}
        <div class="placeholder">No matches</div>
      {/if}
      {#each view as entry, i (entry.path)}
        <div
          class="row"
          class:selected={selected.has(entry.path)}
          role="option"
          aria-selected={selected.has(entry.path)}
          tabindex="-1"
          draggable="true"
          ondragstart={(e) => rowDragStart(e, entry)}
          onclick={(e) => rowClick(e, entry, i)}
          ondblclick={() => rowDblClick(entry)}
        >
          <span class="col-name">
            <span class="ficon">{fileIcon(entry.name, entry.isDir)}</span>
            <span class="fname">{entry.name}</span>
          </span>
          <span class="col-size">{formatSize(entry.size, entry.isDir)}</span>
          <span class="col-date">{formatDate(entry.mtime)}</span>
        </div>
      {/each}
    {/if}
  </div>
</section>

<style>
  .pane {
    display: flex;
    flex-direction: column;
    min-width: 0;
    background: var(--pane-bg);
    border-radius: 10px;
    border: 1px solid var(--border);
    overflow: hidden;
  }
  .pane-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 10px;
    border-bottom: 1px solid var(--border);
    background: var(--head-bg);
  }
  .pane-title {
    font-weight: 600;
    font-size: 13px;
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .pane-icon { font-size: 14px; }
  .header-extra { margin-left: auto; margin-right: 8px; display: flex; align-items: center; }
  .pane-actions { display: flex; gap: 2px; }
  .icon-btn {
    border: none;
    background: transparent;
    color: var(--text);
    width: 26px;
    height: 26px;
    border-radius: 6px;
    cursor: pointer;
    font-size: 14px;
    line-height: 1;
  }
  .icon-btn:hover:not(:disabled) { background: var(--hover); }
  .icon-btn:disabled { opacity: 0.3; cursor: default; }
  .icon-btn.danger:hover:not(:disabled) { background: #ff3b3022; }

  .breadcrumb {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 6px 10px;
    border-bottom: 1px solid var(--border);
    overflow-x: auto;
    white-space: nowrap;
  }
  .crumb {
    border: none;
    background: transparent;
    color: var(--accent);
    cursor: pointer;
    font-size: 12px;
    padding: 2px 4px;
    border-radius: 4px;
  }
  .crumb:hover { background: var(--hover); }
  .sep { color: var(--muted); font-size: 12px; }
  .filter {
    margin-left: auto;
    background: var(--bg);
    border: 1px solid var(--border);
    color: var(--text);
    border-radius: 6px;
    padding: 2px 8px;
    font-size: 12px;
    width: 110px;
    flex-shrink: 0;
  }
  .sortbtn {
    border: none;
    background: transparent;
    color: var(--muted);
    font: inherit;
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    cursor: pointer;
    text-align: left;
    padding: 0;
  }
  .sortbtn:hover { color: var(--text); }

  .filelist {
    flex: 1;
    overflow-y: auto;
    outline: none;
  }
  .filelist.drag-over {
    box-shadow: inset 0 0 0 2px var(--accent);
    background: color-mix(in srgb, var(--accent) 8%, transparent);
  }
  .placeholder {
    padding: 24px;
    text-align: center;
    color: var(--muted);
    font-size: 13px;
  }
  .placeholder.error { color: #ff453a; white-space: pre-wrap; }

  .row {
    display: grid;
    grid-template-columns: 1fr 80px 110px;
    align-items: center;
    padding: 5px 10px;
    font-size: 13px;
    cursor: default;
    user-select: none;
  }
  .row.head {
    position: sticky;
    top: 0;
    background: var(--pane-bg);
    color: var(--muted);
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    border-bottom: 1px solid var(--border);
    cursor: default;
  }
  .row:not(.head):nth-child(even) { background: var(--zebra); }
  .row:not(.head):hover { background: var(--hover); }
  .row.selected { background: var(--accent) !important; color: white; }
  .col-name {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  .fname {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .ficon { flex-shrink: 0; }
  .col-size, .col-date { color: var(--muted); font-variant-numeric: tabular-nums; }
  .row.selected .col-size, .row.selected .col-date { color: #ffffffcc; }
</style>
