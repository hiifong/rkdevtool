import { inject, provide, ref, type InjectionKey } from "vue";
import type { DeviceState, LogEntry, LogLevel, PageId } from "../types/app";

const APP_STATE_KEY: InjectionKey<ReturnType<typeof createAppState>> = Symbol("app-state");

let logId = 0;

function createAppState() {
  const activePage = ref<PageId>("download");
  const deviceState = ref<DeviceState>("connected");
  const deviceSelector = ref("1-11-3-1-1-3 : MASKROM");
  const logs = ref<LogEntry[]>([
    { id: ++logId, level: "success", text: "下载固件成功" },
    { id: ++logId, level: "default", text: "下载固件开始" },
    { id: ++logId, level: "default", text: "准备IDB开始" },
    { id: ++logId, level: "default", text: "准备IDB成功" },
    { id: ++logId, level: "default", text: "下载IDB开始" },
    { id: ++logId, level: "default", text: "下载IDB成功" },
    { id: ++logId, level: "default", text: "测试设备开始" },
    { id: ++logId, level: "default", text: "测试设备成功" },
    { id: ++logId, level: "default", text: "校验芯片开始" },
    { id: ++logId, level: "default", text: "校验芯片成功" },
    { id: ++logId, level: "default", text: "获取FlashInfo开始" },
    { id: ++logId, level: "default", text: "获取FlashInfo成功" },
  ]);

  function appendLog(text: string, level: LogLevel = "default") {
    logs.value.unshift({ id: ++logId, level, text });
  }

  function clearLogs() {
    logs.value = [];
  }

  return {
    activePage,
    deviceState,
    deviceSelector,
    logs,
    appendLog,
    clearLogs,
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
