export type PageId = "download" | "upgrade" | "advanced";

export type LogLevel = "default" | "info" | "success" | "error";

export interface LogEntry {
  id: number;
  level: LogLevel;
  text: string;
}

export type StorageType =
  | ""
  | "FLASH"
  | "EMMC"
  | "SD"
  | "SPINOR"
  | "SPINAND"
  | "SATA"
  | "PCIE";

export interface PartitionRow {
  id: number;
  enabled: boolean;
  storage: StorageType;
  address: string;
  name: string;
  path: string;
}

export type DeviceState = "connected" | "disconnected" | "loader";
