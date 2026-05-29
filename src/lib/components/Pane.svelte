<script lang="ts">
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
    onActivate?: () => void; // double-click transfer of selection toward other pane
    rootPath: string;
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
    onActivate,
    rootPath,
  }: Props = $props();

  // Breadcrumb segments relative to root.
  let crumbs = $derived.by(() => {
    const out: { label: string; path: string }[] = [];
    const rootLabel = rootPath === "/sdcard" ? "Device" : "Home";
    out.push({ label: rootLabel, path: rootPath });
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

  function rowClick(e: MouseEvent, entry: FileEntry) {
    if (e.metaKey || e.ctrlKey) {
      selected.has(entry.path)
        ? selected.delete(entry.path)
        : selected.add(entry.path);
    } else {
      selected.clear();
      selected.add(entry.path);
    }
    selected = new Set(selected); // trigger reactivity
  }

  function rowDblClick(entry: FileEntry) {
    if (entry.isDir) {
      onNavigate(entry.path);
    } else {
      onActivate?.();
    }
  }

  let canUp = $derived(path !== rootPath && path !== "/");
</script>

<section class="pane">
  <header class="pane-head">
    <span class="pane-title"><span class="pane-icon">{icon}</span>{title}</span>
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
  </nav>

  <div class="filelist" role="listbox" tabindex="0">
    {#if loading}
      <div class="placeholder">Loading…</div>
    {:else if error}
      <div class="placeholder error">{error}</div>
    {:else if entries.length === 0}
      <div class="placeholder">Empty folder</div>
    {:else}
      <div class="row head">
        <span class="col-name">Name</span>
        <span class="col-size">Size</span>
        <span class="col-date">Modified</span>
      </div>
      {#each entries as entry (entry.path)}
        <div
          class="row"
          class:selected={selected.has(entry.path)}
          role="option"
          aria-selected={selected.has(entry.path)}
          tabindex="-1"
          onclick={(e) => rowClick(e, entry)}
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

  .filelist {
    flex: 1;
    overflow-y: auto;
    outline: none;
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
