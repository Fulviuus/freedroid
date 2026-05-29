<script lang="ts">
  import { app } from "../state.svelte";

  let active = $derived(app.transfers.filter((t) => t.status === "active").length);
</script>

{#if app.transfers.length > 0}
  <div class="queue">
    <div class="queue-head">
      <span>Transfers {active > 0 ? `· ${active} active` : ""}</span>
      <button class="clear" onclick={() => app.clearFinished()}>Clear finished</button>
    </div>
    <div class="items">
      {#each app.transfers as t (t.id)}
        <div class="item">
          <span class="arrow">{t.direction === "push" ? "→" : "←"}</span>
          <div class="meta">
            <div class="name">{t.name}</div>
            <div class="bar">
              <div
                class="fill"
                class:error={t.status === "error"}
                class:done={t.status === "done"}
                style="width: {t.status === 'error' ? 100 : t.percent}%"
              ></div>
            </div>
            {#if t.status === "error"}<div class="err">{t.error}</div>{/if}
          </div>
          <span class="pct">
            {#if t.status === "error"}Failed{:else if t.status === "done"}Done{:else}{t.percent}%{/if}
          </span>
        </div>
      {/each}
    </div>
  </div>
{/if}

<style>
  .queue {
    border-top: 1px solid var(--border);
    background: var(--head-bg);
    max-height: 180px;
    display: flex;
    flex-direction: column;
  }
  .queue-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 6px 12px;
    font-size: 12px;
    color: var(--muted);
  }
  .clear {
    border: none;
    background: transparent;
    color: var(--accent);
    cursor: pointer;
    font-size: 12px;
  }
  .items { overflow-y: auto; padding: 0 12px 8px; }
  .item {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 4px 0;
  }
  .arrow { color: var(--accent); font-weight: 700; }
  .meta { flex: 1; min-width: 0; }
  .name {
    font-size: 12px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .bar {
    height: 5px;
    background: var(--border);
    border-radius: 3px;
    overflow: hidden;
    margin-top: 3px;
  }
  .fill { height: 100%; background: var(--accent); transition: width 0.2s; }
  .fill.done { background: #34c759; }
  .fill.error { background: #ff453a; }
  .err { font-size: 11px; color: #ff453a; margin-top: 2px; }
  .pct {
    font-size: 11px;
    color: var(--muted);
    width: 52px;
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
</style>
