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
  bytesPerSec: number;
  etaSecs: number;
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

export interface Volume {
  label: string;
  path: string;
}

export const listVolumes = (serial: string) =>
  invoke<Volume[]>("list_volumes", { serial });

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

export const cancelTransfer = (id: string) =>
  invoke<void>("cancel_transfer", { id });

export const openLocal = (path: string) => invoke<void>("open_local", { path });

export const openDeviceFile = (serial: string, remote: string, name: string) =>
  invoke<void>("open_device_file", { serial, remote, name });

export const listLocalDir = (path: string) =>
  invoke<FileEntry[]>("list_local_dir", { path });

export const localHome = () => invoke<string>("local_home");

export const localMakeDir = (path: string) =>
  invoke<void>("local_make_dir", { path });

export const localRename = (from: string, to: string) =>
  invoke<void>("local_rename", { from, to });

export const localTrash = (paths: string[]) =>
  invoke<void>("local_trash", { paths });

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
