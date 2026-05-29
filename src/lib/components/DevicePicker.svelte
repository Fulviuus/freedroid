<script lang="ts">
  import { app } from "../state.svelte";

  interface Props {
    onWifi: () => void;
  }
  let { onWifi }: Props = $props();

  function label(state: string): string {
    switch (state) {
      case "device": return "Connected";
      case "unauthorized": return "Authorize on phone";
      case "offline": return "Offline";
      default: return state;
    }
  }
</script>

<div class="picker">
  <select
    value={app.selectedSerial ?? ""}
    onchange={(e) => (app.selectedSerial = (e.currentTarget as HTMLSelectElement).value || null)}
  >
    {#if app.devices.length === 0}
      <option value="">No device connected</option>
    {/if}
    {#each app.devices as d (d.serial)}
      <option value={d.serial}>
        {(d.model ?? d.serial).replace(/_/g, " ")}{d.wifi ? " (Wi-Fi)" : ""} — {label(d.state)}
      </option>
    {/each}
  </select>

  {#if app.selectedDevice}
    <span class="dot" class:ok={app.ready} class:warn={app.selectedDevice.state === "unauthorized"}></span>
  {/if}

  <button class="ghost" onclick={() => app.refreshDevices()} title="Rescan">⟳</button>
  <button class="ghost" onclick={onWifi} title="Connect over Wi-Fi">Wi-Fi…</button>
</div>

{#if app.selectedDevice?.state === "unauthorized"}
  <div class="hint">Check your phone and tap <b>Allow</b> on the “Allow USB debugging?” prompt, then rescan.</div>
{:else if app.devices.length === 0}
  <div class="hint">Plug in your Android phone and enable <b>USB debugging</b> (Settings → Developer options).</div>
{/if}

<style>
  .picker { display: flex; align-items: center; gap: 8px; }
  select {
    background: var(--pane-bg);
    color: var(--text);
    border: 1px solid var(--border);
    border-radius: 7px;
    padding: 5px 8px;
    font-size: 13px;
    min-width: 240px;
    max-width: 360px;
  }
  .dot { width: 9px; height: 9px; border-radius: 50%; background: var(--muted); }
  .dot.ok { background: #34c759; }
  .dot.warn { background: #ff9f0a; }
  .ghost {
    border: 1px solid var(--border);
    background: var(--pane-bg);
    color: var(--text);
    border-radius: 7px;
    padding: 5px 10px;
    font-size: 12px;
    cursor: pointer;
  }
  .ghost:hover { background: var(--hover); }
  .hint {
    font-size: 12px;
    color: var(--muted);
    margin-top: 6px;
    padding: 6px 10px;
    background: var(--head-bg);
    border-radius: 7px;
  }
</style>
