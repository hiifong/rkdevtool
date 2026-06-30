<script setup lang="ts">
import { ref } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import AppButton from "../ui/AppButton.vue";
import PathField from "../ui/PathField.vue";
import { useAppState } from "../../composables/useAppState";
import { useToolCommand, toolApi } from "../../composables/useToolCommand";
import { pickFile } from "../../composables/useFilePicker";
import { buildExtractOutputDir, buildSerialLogPath } from "../../composables/useExtractPath";
import { ADVANCED_ACTIONS } from "../../constants/advancedActions";
import { useI18n } from "../../i18n";
import { logText } from "../../i18n/logText";

const { appendLog, busy } = useAppState();
const { run } = useToolCommand();
const { t } = useI18n();

const bootPath = ref("");
const firmwarePath = ref("");
const startSector = ref("");
const sectorCount = ref("");

const storageItems = [
  "FLASH",
  "EMMC",
  "SD",
  "SD1",
  "SPINOR",
  "SPINAND",
  "RAM",
  "USB",
  "SATA",
  "PCIE",
];
const selectedStorage = ref(0);

async function browseFile(target: "boot" | "firmware") {
  const title = target === "boot" ? t("advanced.pickBoot") : t("advanced.pickFirmware");
  const path = await pickFile(title);
  if (!path) return;
  if (target === "boot") bootPath.value = path;
  else firmwarePath.value = path;
}

function actionParams(command: string) {
  if (command === "切换存储" || command === "获取当前存储") {
    return { start_sector: String(selectedStorage.value + 1) };
  }
  if (command === "擦除扇区") {
    return {
      start_sector: startSector.value || "0",
      sector_count: sectorCount.value || "1",
    };
  }
  if (command === "擦除所有") {
    return { boot_path: bootPath.value };
  }
  return undefined;
}

async function exportSerialLog(labelKey: string) {
  const parentDir = await open({
    directory: true,
    multiple: false,
    title: t("advanced.pickSerialLogDir"),
  });

  if (!parentDir || Array.isArray(parentDir)) return;

  const outputPath = await buildSerialLogPath(parentDir);

  try {
    await run(
      () => toolApi.runAction("导出串口日志", { output_path: outputPath }),
      logText(labelKey),
    );
  } catch (err) {
    appendLog(String(err), "error");
  }
}

async function runAction(command: string, labelKey: string) {
  if (command === "导出镜像") {
    appendLog(logText("advanced.exportImageTodo"), "error");
    return;
  }

  if (command === "导出串口日志") {
    await exportSerialLog(labelKey);
    return;
  }

  try {
    await run(
      () => toolApi.runAction(command, actionParams(command)),
      logText(labelKey),
    );
  } catch (err) {
    appendLog(String(err), "error");
  }
}

async function downloadBoot() {
  if (!bootPath.value.trim()) {
    appendLog(logText("advanced.selectBootFirst"), "error");
    return;
  }

  const fileName = bootPath.value.split(/[/\\]/).pop()?.toLowerCase() ?? "";
  if (fileName === "download.bin") {
    appendLog(logText("advanced.downloadBinHint"), "info");
  }

  try {
    await run(() => toolApi.downloadBoot(bootPath.value), logText("task.downloadBoot"));
    appendLog(logText("advanced.downloadBootSuccess"), "success");
  } catch (err) {
    appendLog(String(err), "error");
  }
}

async function extractFirmware() {
  if (!firmwarePath.value.trim()) {
    appendLog(logText("advanced.selectFirmwareFirst"), "error");
    return;
  }

  const parentDir = await open({
    directory: true,
    multiple: false,
    title: t("advanced.pickExtractDir"),
  });

  if (!parentDir || Array.isArray(parentDir)) return;

  const outputDir = await buildExtractOutputDir(parentDir, firmwarePath.value);

  try {
    const extractLog = await run(
      () => toolApi.extractFirmware(firmwarePath.value, outputDir),
      logText("task.extractFirmware"),
    );
    if (extractLog) {
      for (const line of extractLog.split("\n")) {
        if (line.trim()) appendLog(line, "info");
      }
    }
    appendLog(logText("advanced.extractSuccess", { path: outputDir }), "success");
  } catch (err) {
    appendLog(String(err), "error");
  }
}
</script>

<template>
  <div class="advanced-page">
    <div class="file-section">
      <div class="file-row">
        <span class="file-row__label">{{ t("advanced.boot") }}</span>
        <PathField
          v-model="bootPath"
          browse-variant="inline"
          :placeholder="t('advanced.bootPlaceholder')"
          @browse="browseFile('boot')"
        />
        <AppButton variant="primary" :disabled="busy" @click="downloadBoot">{{ t("advanced.download") }}</AppButton>
      </div>
      <div class="file-row">
        <span class="file-row__label">{{ t("advanced.firmware") }}</span>
        <PathField
          v-model="firmwarePath"
          browse-variant="inline"
          :placeholder="t('advanced.firmwarePlaceholder')"
          @browse="browseFile('firmware')"
        />
        <AppButton :disabled="busy" @click="extractFirmware">{{ t("advanced.extract") }}</AppButton>
      </div>
    </div>

    <div class="grid-area">
      <div class="action-grid">
        <button
          v-for="action in ADVANCED_ACTIONS"
          :key="action.command"
          type="button"
          class="action-btn"
          :class="{ 'action-btn--destructive': action.destructive }"
          :disabled="busy"
          @click="runAction(action.command, action.labelKey)"
        >
          {{ t(action.labelKey) }}
        </button>
      </div>

      <div class="storage-list">
        <div class="storage-list__title">{{ t("advanced.storageType") }}</div>
        <button
          v-for="(item, index) in storageItems"
          :key="item"
          type="button"
          class="storage-list__item"
          :class="{ 'storage-list__item--active': selectedStorage === index }"
          :disabled="busy"
          @click="selectedStorage = index"
        >
          {{ index + 1 }}. {{ item }}
        </button>
      </div>
    </div>

    <div class="sector-card">
      <div class="sector-row">
        <span class="sector-row__label">{{ t("advanced.startSector") }}</span>
        <input v-model="startSector" class="sector-row__input" type="text" />
      </div>
      <div class="sector-row">
        <span class="sector-row__label">{{ t("advanced.sectorCount") }}</span>
        <input v-model="sectorCount" class="sector-row__input" type="text" />
      </div>
    </div>
  </div>
</template>

<style scoped>
.advanced-page {
  display: flex;
  flex-direction: column;
  gap: 16px;
  width: 100%;
  max-width: 652px;
}

.file-section {
  display: grid;
  grid-template-columns: max-content minmax(0, 1fr) auto;
  column-gap: 12px;
  row-gap: 12px;
  align-items: center;
  padding: 20px;
  border-radius: var(--border-radius-lg);
  border: 1px solid var(--color-border);
  background: var(--color-surface);
}

.file-row {
  display: contents;
}

.file-row__label {
  flex-shrink: 0;
  font-size: 13px;
  color: var(--color-text-secondary);
  white-space: nowrap;
}

.file-row :deep(.path-field) {
  min-width: 0;
}

.grid-area {
  display: flex;
  gap: 20px;
  padding: 20px;
  height: 396px;
  border-radius: var(--border-radius-lg);
  border: 1px solid var(--color-border);
  background: var(--color-surface);
}

.action-grid {
  display: grid;
  grid-template-columns: repeat(4, 100px);
  gap: 12px;
  align-content: start;
  flex: 1;
}

.action-btn {
  width: 100px;
  height: 40px;
  padding: 0 8px;
  border-radius: var(--border-radius-md);
  border: 1px solid var(--color-border);
  background: var(--color-surface);
  font-size: 12px;
  color: var(--color-text-primary);
  transition: background 0.15s, border-color 0.15s;
}

.action-btn:hover:not(:disabled) {
  background: var(--color-surface-hover);
  border-color: var(--color-border-strong);
}

.action-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.action-btn--destructive {
  color: var(--color-danger);
}

.storage-list {
  width: 148px;
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  gap: 4px;
  overflow-y: auto;
}

.storage-list__title {
  font-size: 12px;
  font-weight: 600;
  color: var(--color-text-secondary);
  margin-bottom: 4px;
}

.storage-list__item {
  height: 32px;
  padding: 0 10px;
  border: none;
  border-radius: 6px;
  background: transparent;
  text-align: left;
  font-size: 12px;
  color: var(--color-text-primary);
}

.storage-list__item:hover:not(:disabled) {
  background: var(--color-surface-hover);
}

.storage-list__item--active {
  background: var(--color-primary-light);
  color: var(--color-primary);
  font-weight: 600;
}

.storage-list__item:disabled {
  opacity: 0.5;
}

.sector-card {
  display: flex;
  gap: 24px;
  height: 72px;
  padding: 20px;
  border-radius: var(--border-radius-lg);
  border: 1px solid var(--color-border);
  background: var(--color-surface);
  align-items: center;
}

.sector-row {
  display: flex;
  align-items: center;
  gap: 12px;
}

.sector-row__label {
  font-size: 13px;
  color: var(--color-text-secondary);
  white-space: nowrap;
}

.sector-row__input {
  width: 120px;
  height: 36px;
  padding: 0 12px;
  border-radius: var(--border-radius-md);
  border: 1px solid var(--color-border);
  background: var(--color-surface-hover);
  font-size: 13px;
}
</style>
