import { onMounted, onUnmounted, watch } from "vue";
import { listen } from "@tauri-apps/api/event";
import { toolApi } from "./useToolCommand";
import { useAppState, type AppState } from "./useAppState";
import type { LogLevel } from "../types/app";
import type { RockusbDevice, ToolLogEvent } from "../types/tool";

export function useDevicePoll(intervalMs = 2000, appState?: AppState) {
  const { appendLog, setDevices, selectedDeviceId, busy } = appState ?? useAppState();

  let timer: ReturnType<typeof setInterval> | null = null;
  const unlisteners: Array<() => void> = [];

  async function refreshDevices() {
    if (busy.value) return;
    try {
      const list = await toolApi.listDevices();
      setDevices(list);
    } catch {
      // 静默：无设备或工具不可用时保持 disconnected
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
          setDevices(event.payload);
        }),
      );
    } catch (err) {
      appendLog(String(err), "error");
    }

    await refreshDevices();
    timer = setInterval(refreshDevices, intervalMs);

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
    if (timer) clearInterval(timer);
    unlisteners.forEach((off) => off());
    document.removeEventListener("visibilitychange", onVisibilityChange);
  });

  async function onDeviceChange(locationId: string) {
    selectedDeviceId.value = locationId;
    await toolApi.selectDevice(locationId);
    await refreshDevices();
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
