import { onMounted, onUnmounted, watch } from "vue";
import { listen } from "@tauri-apps/api/event";
import { toolApi } from "./useToolCommand";
import { useAppState, type AppState } from "./useAppState";
import type { LogLevel } from "../types/app";
import type { RockusbDevice, ToolLogEvent } from "../types/tool";
import { playDeviceConnected, playDeviceDisconnected } from "../utils/deviceSounds";

function deviceKeySet(list: RockusbDevice[]): Set<string> {
  return new Set(list.map((d) => d.location_id));
}

/** Subscribe to backend USB hotplug events (no polling). */
export function useDevicePoll(_intervalMs = 2000, appState?: AppState) {
  const { appendLog, setDevices, selectedDeviceId, busy } = appState ?? useAppState();

  const unlisteners: Array<() => void> = [];
  let hotplugPrimed = false;
  let prevKeys = new Set<string>();

  function applyDevices(list: RockusbDevice[], playSound: boolean) {
    const nextKeys = deviceKeySet(list);
    if (playSound && hotplugPrimed) {
      let added = 0;
      let removed = 0;
      for (const id of nextKeys) {
        if (!prevKeys.has(id)) added += 1;
      }
      for (const id of prevKeys) {
        if (!nextKeys.has(id)) removed += 1;
      }
      if (added > 0 && removed === 0) {
        playDeviceConnected();
      } else if (removed > 0 && added === 0) {
        playDeviceDisconnected();
      } else if (added > 0 && removed > 0) {
        // Swap / Maskrom↔Loader re-enumerate: treat as connect cue
        playDeviceConnected();
      }
    }
    hotplugPrimed = true;
    prevKeys = nextKeys;
    setDevices(list);
  }

  async function refreshDevices() {
    if (busy.value) return;
    try {
      const list = await toolApi.listDevices();
      // Manual / startup resync: update list without sounding
      applyDevices(list, false);
    } catch {
      // 静默：无设备或 USB 枚举失败时保持 disconnected
    }
  }

  onMounted(async () => {
    try {
      const info = await toolApi.getToolInfo();
      appendLog(`upgrade_tool ${info.version} (${info.platform_dir})`, "info");
    } catch (err) {
      appendLog(String(err), "error");
    }

    try {
      unlisteners.push(
        await listen<ToolLogEvent>("tool-log", (event) => {
          appendLog(
            event.payload.text,
            event.payload.level as LogLevel,
            event.payload.update ?? false,
          );
        }),
      );

      unlisteners.push(
        await listen<RockusbDevice[]>("devices-updated", (event) => {
          applyDevices(event.payload, true);
        }),
      );
    } catch (err) {
      appendLog(String(err), "error");
    }

    await refreshDevices();

    watch(busy, (next, prev) => {
      if (prev && !next) {
        refreshDevices();
      }
    });

    document.addEventListener("visibilitychange", onVisibilityChange);
  });

  function onVisibilityChange() {
    if (!document.hidden) {
      refreshDevices();
    }
  }

  onUnmounted(() => {
    unlisteners.forEach((off) => off());
    document.removeEventListener("visibilitychange", onVisibilityChange);
  });

  async function onDeviceChange(locationId: string) {
    selectedDeviceId.value = locationId;
    await toolApi.selectDevice(locationId);
  }

  return { refreshDevices, onDeviceChange };
}

export function useToolBusyPoll(appState?: AppState) {
  const { setBusy } = appState ?? useAppState();
  let timer: ReturnType<typeof setInterval> | null = null;

  onMounted(() => {
    timer = setInterval(async () => {
      try {
        setBusy(await toolApi.isToolBusy());
      } catch {
        setBusy(false);
      }
    }, 400);
  });

  onUnmounted(() => {
    if (timer) clearInterval(timer);
  });
}
