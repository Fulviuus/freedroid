import * as ipc from "./ipc";
import { uid } from "./util";

export interface Transfer {
  id: string;
  name: string;
  direction: "push" | "pull";
  percent: number;
  status: "active" | "done" | "error";
  error?: string;
}

class AppState {
  devices = $state<ipc.Device[]>([]);
  selectedSerial = $state<string | null>(null);
  adbVersion = $state<string>("");
  transfers = $state<Transfer[]>([]);
  toast = $state<{ msg: string; kind: "info" | "error" } | null>(null);

  get selectedDevice(): ipc.Device | undefined {
    return this.devices.find((d) => d.serial === this.selectedSerial);
  }

  get ready(): boolean {
    return this.selectedDevice?.state === "device";
  }

  notify(msg: string, kind: "info" | "error" = "info") {
    this.toast = { msg, kind };
    setTimeout(() => {
      if (this.toast?.msg === msg) this.toast = null;
    }, 4000);
  }

  async refreshDevices() {
    try {
      this.devices = await ipc.listDevices();
      // Auto-select first ready device if nothing selected (or selection gone).
      if (
        !this.selectedSerial ||
        !this.devices.some((d) => d.serial === this.selectedSerial)
      ) {
        this.selectedSerial =
          this.devices.find((d) => d.state === "device")?.serial ??
          this.devices[0]?.serial ??
          null;
      }
    } catch (e) {
      this.notify(String(e), "error");
    }
  }

  startTransfer(name: string, direction: "push" | "pull"): string {
    const id = uid();
    this.transfers = [
      { id, name, direction, percent: 0, status: "active" },
      ...this.transfers,
    ];
    return id;
  }

  updateProgress(id: string, percent: number) {
    const t = this.transfers.find((x) => x.id === id);
    if (t) t.percent = percent;
  }

  finishTransfer(id: string, success: boolean, error?: string) {
    const t = this.transfers.find((x) => x.id === id);
    if (t) {
      t.status = success ? "done" : "error";
      t.percent = success ? 100 : t.percent;
      if (error) t.error = error;
    }
  }

  clearFinished() {
    this.transfers = this.transfers.filter((t) => t.status === "active");
  }
}

export const app = new AppState();
