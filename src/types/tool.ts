export type LogLevel = "default" | "info" | "success" | "error";

export interface RockusbDevice {
  location_id: string;
  mode: string;
  label: string;
}

export interface ToolInfo {
  version: string;
  platform_dir: string;
  tool_path: string;
}

export interface FirmwareInfo {
  format: string;
  firmware_version: string;
  loader_version: string;
  chip_family: string;
}

export interface DownloadRowPayload {
  enabled: boolean;
  storage: string;
  address: string;
  name: string;
  path: string;
}

export interface DownloadExecutePayload {
  rows: DownloadRowPayload[];
  force_by_address: boolean;
}

export interface ActionParams {
  boot_path?: string;
  start_sector?: string;
  sector_count?: string;
  output_path?: string;
}

export interface ToolLogEvent {
  text: string;
  level: LogLevel;
  update?: boolean;
}
