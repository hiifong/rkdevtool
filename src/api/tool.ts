import { invoke } from "@tauri-apps/api/core";
import type {
  ActionParams,
  DownloadExecutePayload,
  RockusbDevice,
  ToolInfo,
} from "../types/tool";

export function getToolInfo() {
  return invoke<ToolInfo>("get_tool_info");
}

export function listDevices() {
  return invoke<RockusbDevice[]>("list_devices");
}

export function selectDevice(locationId: string | null) {
  return invoke<void>("select_device", { locationId });
}

export function partitionList() {
  return invoke<string>("partition_list");
}

export function upgradeFirmware(path: string, noReset = false) {
  return invoke<void>("upgrade_firmware", { path, noReset });
}

export function downloadBoot(path: string) {
  return invoke<void>("download_boot", { path });
}

export function downloadExecute(payload: DownloadExecutePayload) {
  return invoke<void>("download_execute", { payload });
}

export function extractFirmware(path: string, outputDir: string) {
  return invoke<void>("extract_firmware", { path, outputDir });
}

export function readChipInfo() {
  return invoke<string>("read_chip_info");
}

export function runAction(action: string, params?: ActionParams) {
  return invoke<string>("run_action", { action, params: params ?? null });
}

export function isToolBusy() {
  return invoke<boolean>("is_tool_busy");
}
