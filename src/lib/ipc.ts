// Typed wrappers around the Rust backend commands.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export interface Device {
  serial: string;
  state: string; // "device" | "unauthorized" | "offline" | ...
  model: string | null;
  product: string | null;
  wifi: boolean;
}

export interface FileEntry {
  name: string;
  path: string;
  size: number;
  mtime: number; // unix epoch seconds
  isDir: boolean;
  isSymlink: boolean;
}

export interface TransferProgress {
  id: string;
  percent: number;
  direction: "push" | "pull";
  name: string;
  indeterminate: boolean;
}

export interface TransferDone {
  id: string;
  success: boolean;
  error: string | null;
}

export const adbVersion = () => invoke<string>("adb_version");

export const listDevices = () => invoke<Device[]>("list_devices");

export const listDeviceDir = (serial: string, path: string) =>
  invoke<FileEntry[]>("list_device_dir", { serial, path });

export const deviceMakeDir = (serial: string, path: string) =>
  invoke<void>("device_make_dir", { serial, path });

export const deviceRemove = (serial: string, path: string) =>
  invoke<void>("device_remove", { serial, path });

export const deviceRename = (serial: string, from: string, to: string) =>
  invoke<void>("device_rename", { serial, from, to });

export const pullFile = (
  serial: string,
  remote: string,
  local: string,
  id: string,
  name: string,
  total: number,
  isDir: boolean,
) => invoke<void>("pull_file", { serial, remote, local, id, name, total, isDir });

export const pushFile = (
  serial: string,
  local: string,
  remote: string,
  id: string,
  name: string,
  total: number,
  isDir: boolean,
) => invoke<void>("push_file", { serial, local, remote, id, name, total, isDir });

export const listLocalDir = (path: string) =>
  invoke<FileEntry[]>("list_local_dir", { path });

export const localHome = () => invoke<string>("local_home");

export const wifiEnableTcpip = (serial: string) =>
  invoke<string>("wifi_enable_tcpip", { serial });

export const wifiConnect = (address: string) =>
  invoke<string>("wifi_connect", { address });

export const wifiDisconnect = (address: string) =>
  invoke<void>("wifi_disconnect", { address });

export const wifiPair = (address: string, code: string) =>
  invoke<string>("wifi_pair", { address, code });

export const onTransferProgress = (
  cb: (p: TransferProgress) => void,
): Promise<UnlistenFn> =>
  listen<TransferProgress>("transfer://progress", (e) => cb(e.payload));

export const onTransferDone = (
  cb: (d: TransferDone) => void,
): Promise<UnlistenFn> =>
  listen<TransferDone>("transfer://done", (e) => cb(e.payload));
