export function formatSize(bytes: number, isDir: boolean): string {
  if (isDir) return "--";
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let v = bytes / 1024;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v.toFixed(v < 10 ? 1 : 0)} ${units[i]}`;
}

export function formatSpeed(bytesPerSec: number): string {
  if (!bytesPerSec) return "";
  return `${formatSize(bytesPerSec, false)}/s`;
}

export function formatEta(secs: number): string {
  if (!secs) return "";
  if (secs < 60) return `${secs}s left`;
  const m = Math.floor(secs / 60);
  const s = secs % 60;
  if (m < 60) return `${m}m ${s}s left`;
  const h = Math.floor(m / 60);
  return `${h}h ${m % 60}m left`;
}

export function formatDate(epochSeconds: number): string {
  if (!epochSeconds) return "";
  const d = new Date(epochSeconds * 1000);
  return d.toLocaleDateString(undefined, {
    year: "numeric",
    month: "short",
    day: "numeric",
  });
}

export function joinPath(dir: string, name: string): string {
  if (dir.endsWith("/")) return dir + name;
  return dir + "/" + name;
}

export function parentPath(path: string): string {
  const trimmed = path.replace(/\/+$/, "");
  const idx = trimmed.lastIndexOf("/");
  if (idx <= 0) return "/";
  return trimmed.slice(0, idx);
}

export function baseName(path: string): string {
  const trimmed = path.replace(/\/+$/, "");
  const idx = trimmed.lastIndexOf("/");
  return idx < 0 ? trimmed : trimmed.slice(idx + 1);
}

let counter = 0;
export function uid(): string {
  counter += 1;
  return `t${Date.now().toString(36)}-${counter}`;
}

/** Icon glyph for a file entry, chosen by extension / dir flag. */
export function fileIcon(name: string, isDir: boolean): string {
  if (isDir) return "📁";
  const ext = name.split(".").pop()?.toLowerCase() ?? "";
  if (["jpg", "jpeg", "png", "gif", "heic", "webp", "bmp"].includes(ext))
    return "🖼️";
  if (["mp4", "mov", "mkv", "avi", "webm", "m4v"].includes(ext)) return "🎬";
  if (["mp3", "wav", "flac", "aac", "ogg", "m4a"].includes(ext)) return "🎵";
  if (["pdf"].includes(ext)) return "📕";
  if (["zip", "tar", "gz", "rar", "7z", "apk"].includes(ext)) return "🗜️";
  if (["txt", "md", "json", "xml", "csv", "log"].includes(ext)) return "📄";
  return "📄";
}
