<script lang="ts">
  import { app } from "../state.svelte";
  import * as ipc from "../ipc";

  interface Props {
    onClose: () => void;
  }
  let { onClose }: Props = $props();

  let mode = $state<"usb" | "pair">("usb");
  let busy = $state(false);
  let connectAddr = $state("");
  let pairAddr = $state("");
  let pairCode = $state("");

  async function enableFromUsb() {
    if (!app.selectedSerial) return;
    busy = true;
    try {
      const addr = await ipc.wifiEnableTcpip(app.selectedSerial);
      await ipc.wifiConnect(addr);
      app.notify(`Connected over Wi-Fi: ${addr}`);
      await app.refreshDevices();
      onClose();
    } catch (e) {
      app.notify(String(e), "error");
    } finally {
      busy = false;
    }
  }

  async function connectManual() {
    busy = true;
    try {
      const msg = await ipc.wifiConnect(connectAddr.trim());
      app.notify(msg || "Connected");
      await app.refreshDevices();
      onClose();
    } catch (e) {
      app.notify(String(e), "error");
    } finally {
      busy = false;
    }
  }

  async function pair() {
    busy = true;
    try {
      await ipc.wifiPair(pairAddr.trim(), pairCode.trim());
      app.notify("Paired. Now enter the connect address above and connect.");
    } catch (e) {
      app.notify(String(e), "error");
    } finally {
      busy = false;
    }
  }
</script>

<div class="overlay" onclick={onClose} role="presentation">
  <div class="dialog" onclick={(e) => e.stopPropagation()} role="dialog" aria-label="Wi-Fi connection">
    <h2>Connect over Wi-Fi</h2>

    <div class="tabs">
      <button class:active={mode === "usb"} onclick={() => (mode = "usb")}>From USB</button>
      <button class:active={mode === "pair"} onclick={() => (mode = "pair")}>Wireless pairing</button>
    </div>

    {#if mode === "usb"}
      <p class="desc">
        Easiest method. Keep the phone plugged in via USB and authorized, then click below.
        Freedroid switches it to wireless mode and connects automatically. (Works until the phone reboots.)
      </p>
      <button class="primary" disabled={busy || !app.ready} onclick={enableFromUsb}>
        {busy ? "Connecting…" : "Enable Wi-Fi & connect"}
      </button>
      {#if !app.ready}
        <p class="muted">Connect & authorize a device over USB first.</p>
      {/if}
    {:else}
      <p class="desc">
        Android 11+: on the phone go to <b>Developer options → Wireless debugging → Pair device with pairing code</b>.
      </p>
      <label>Pairing address (IP:port shown under the code)
        <input bind:value={pairAddr} placeholder="192.168.1.5:37123" />
      </label>
      <label>Pairing code
        <input bind:value={pairCode} placeholder="123456" />
      </label>
      <button class="primary" disabled={busy || !pairAddr || !pairCode} onclick={pair}>Pair</button>
    {/if}

    <hr />
    <label>Connect address (IP:port from the Wireless debugging screen)
      <input bind:value={connectAddr} placeholder="192.168.1.5:5555" />
    </label>
    <button class="primary" disabled={busy || !connectAddr} onclick={connectManual}>Connect</button>

    <div class="footer">
      <button class="ghost" onclick={onClose}>Close</button>
    </div>
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.35);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 50;
  }
  .dialog {
    background: var(--pane-bg);
    border: 1px solid var(--border);
    border-radius: 12px;
    padding: 20px;
    width: 420px;
    max-width: 90vw;
    box-shadow: 0 20px 60px rgba(0, 0, 0, 0.4);
  }
  h2 { font-size: 16px; margin: 0 0 12px; }
  .tabs { display: flex; gap: 4px; margin-bottom: 12px; }
  .tabs button {
    flex: 1;
    padding: 6px;
    border: 1px solid var(--border);
    background: transparent;
    color: var(--text);
    border-radius: 7px;
    cursor: pointer;
    font-size: 12px;
  }
  .tabs button.active { background: var(--accent); color: #fff; border-color: var(--accent); }
  .desc { font-size: 12px; color: var(--muted); line-height: 1.5; }
  label { display: block; font-size: 12px; color: var(--muted); margin: 10px 0 0; }
  input {
    width: 100%;
    box-sizing: border-box;
    margin-top: 4px;
    padding: 7px 9px;
    border: 1px solid var(--border);
    border-radius: 7px;
    background: var(--bg);
    color: var(--text);
    font-size: 13px;
  }
  .primary {
    margin-top: 12px;
    width: 100%;
    padding: 8px;
    border: none;
    border-radius: 8px;
    background: var(--accent);
    color: #fff;
    font-size: 13px;
    font-weight: 600;
    cursor: pointer;
  }
  .primary:disabled { opacity: 0.4; cursor: default; }
  .muted { font-size: 11px; color: var(--muted); margin-top: 6px; }
  hr { border: none; border-top: 1px solid var(--border); margin: 16px 0 0; }
  .footer { display: flex; justify-content: flex-end; margin-top: 14px; }
  .ghost {
    border: 1px solid var(--border);
    background: transparent;
    color: var(--text);
    border-radius: 7px;
    padding: 6px 14px;
    cursor: pointer;
    font-size: 12px;
  }
</style>
