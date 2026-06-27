import { inject, provide, ref, type InjectionKey } from "vue";
import type { DeviceState, LogEntry, LogLevel, PageId } from "../types/app";
import type { RockusbDevice, ToolInfo } from "../types/tool";

const APP_STATE_KEY: InjectionKey<ReturnType<typeof createAppState>> = Symbol("app-state");

const PROGRESS_RE = /\d+\s*%/;
const STEP_DONE_RE = /\b(success|fail(ed)?|成功|失败)\s*$/i;
const STEP_START_RE = /^(start to|begin)\b/i;

function isProgressLine(text: string): boolean {
  const t = text.trim();
  if (STEP_DONE_RE.test(t) || STEP_START_RE.test(t)) {
    return false;
  }
  return PROGRESS_RE.test(t) || t.endsWith("...") || /progress/i.test(t);
}

function shouldUpdateLastLine(last: LogEntry, next: string, update: boolean): boolean {
  if (!isProgressLine(next)) return false;
  if (last.kind !== "progress" && !isProgressLine(last.text)) return false;
  if (update) return true;

  const normalize = (value: string) =>
    value.trim().replace(/\.+$/, "").toLowerCase();
  const lastBase = normalize(last.text);
  const nextBase = normalize(next);
  return lastBase.startsWith(nextBase) || nextBase.startsWith(lastBase);
}

let logId = 0;

function createAppState() {
  const activePage = ref<PageId>("download");
  const deviceState = ref<DeviceState>("disconnected");
  const devices = ref<RockusbDevice[]>([]);
  const selectedDeviceId = ref<string | null>(null);
  const toolInfo = ref<ToolInfo | null>(null);
  const logs = ref<LogEntry[]>([]);
  const busy = ref(false);

  const deviceSelector = ref("");

  function appendLog(text: string, level: LogLevel = "default", update = false) {
    const trimmed = text.trim();
    if (!trimmed) return;

    const last = logs.value[logs.value.length - 1];
    const progress = isProgressLine(trimmed);

    if (last && shouldUpdateLastLine(last, trimmed, update)) {
      logs.value[logs.value.length - 1] = {
        ...last,
        text: trimmed,
        level,
        kind: "progress",
      };
      logs.value = [...logs.value];
      return;
    }

    logs.value.push({
      id: ++logId,
      level,
      text: trimmed,
      kind: progress ? "progress" : "line",
    });
  }

  function clearLogs() {
    logs.value = [];
  }

  function setDevices(list: RockusbDevice[]) {
    devices.value = list;
    if (list.length === 0) {
      deviceState.value = "disconnected";
      selectedDeviceId.value = null;
      deviceSelector.value = "";
      return;
    }

    if (!selectedDeviceId.value || !list.some((d) => d.location_id === selectedDeviceId.value)) {
      selectedDeviceId.value = list[0].location_id;
    }

    const current = list.find((d) => d.location_id === selectedDeviceId.value) ?? list[0];
    deviceSelector.value = current.label;
    deviceState.value = current.mode.toUpperCase().includes("LOADER") ? "loader" : "connected";
  }

  function setBusy(value: boolean) {
    busy.value = value;
  }

  return {
    activePage,
    deviceState,
    devices,
    selectedDeviceId,
    deviceSelector,
    toolInfo,
    logs,
    busy,
    appendLog,
    clearLogs,
    setDevices,
    setBusy,
  };
}

export function provideAppState() {
  const state = createAppState();
  provide(APP_STATE_KEY, state);
  return state;
}

export function useAppState() {
  const state = inject(APP_STATE_KEY);
  if (!state) throw new Error("useAppState must be used within AppShell");
  return state;
}

export type AppState = ReturnType<typeof createAppState>;
