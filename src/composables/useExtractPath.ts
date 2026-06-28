import { join } from "@tauri-apps/api/path";

function sanitizeFirmwareName(path: string): string {
  const base = path.split(/[/\\]/).pop() ?? "firmware";
  const withoutExt = base.includes(".") ? base.replace(/\.[^.]+$/, "") : base;
  return withoutExt.replace(/[<>:"/\\|?*\x00-\x1f]/g, "_").trim() || "firmware";
}

function formatExtractTimestamp(date = new Date()): string {
  const pad = (value: number) => String(value).padStart(2, "0");
  return `${date.getFullYear()}${pad(date.getMonth() + 1)}${pad(date.getDate())}-${pad(date.getHours())}${pad(date.getMinutes())}${pad(date.getSeconds())}`;
}

export function buildExtractDirName(firmwarePath: string): string {
  return `rkdevtool_${sanitizeFirmwareName(firmwarePath)}_${formatExtractTimestamp()}`;
}

export async function buildExtractOutputDir(
  parentDir: string,
  firmwarePath: string,
): Promise<string> {
  return join(parentDir, buildExtractDirName(firmwarePath));
}
