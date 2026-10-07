<script setup lang="ts">
import { ref, watch } from "vue";
import AppButton from "../ui/AppButton.vue";
import PathField from "../ui/PathField.vue";
import { useAppState } from "../../composables/useAppState";
import { downloadStorageOptions as storageOptions } from "../../composables/useDownloadForm";
import { useToolCommand, toolApi } from "../../composables/useToolCommand";
import { pickFile } from "../../composables/useFilePicker";
import { useI18n } from "../../i18n";
import { logText } from "../../i18n/logText";
import type { PartitionRow } from "../../types/app";

const { appendLog, busy, downloadForm } = useAppState();
const { rows, forceByAddress, selectedRowId, addRow, removeRow, clearRows: clearFormRows, fillFromPartitionTable } = downloadForm;
const { run } = useToolCommand();
const { t } = useI18n();

const storageIndexMap: Record<string, string> = {
  FLASH: "1",
  EMMC: "2",
  SD: "3",
  SPINOR: "5",
  SPINAND: "6",
  SATA: "9",
  PCIE: "10",
};

const loaderVersion = ref("");

watch(
  () => rows.value.find((row) => row.name.toLowerCase().includes("loader"))?.path ?? "",
  async (path, _previousPath, onCleanup) => {
    let active = true;
    onCleanup(() => {
      active = false;
    });
    loaderVersion.value = "";
    if (!path) return;
    try {
      const info = await toolApi.parseFirmware(path);
      if (active) loaderVersion.value = info.loader_version || "";
    } catch {
      // The saved file may have been moved or removed since the previous session.
    }
  },
  { immediate: true },
);

function selectRow(id: number) {
  selectedRowId.value = id;
}

async function browsePath(row: PartitionRow) {
  const path = await pickFile(t("download.pickImage"));
  if (!path) return;
  row.path = path;
}

async function execute() {
  try {
    await run(
      () =>
        toolApi.downloadExecute({
          rows: rows.value.map((row) => ({
            enabled: row.enabled,
            storage: row.storage,
            address: row.address,
            name: row.name,
            path: row.path,
          })),
          force_by_address: forceByAddress.value,
        }),
      logText("task.execute"),
    );
  } catch (err) {
    appendLog(String(err), "error");
  }
}

async function switchStorage() {
  const row = rows.value.find((r) => r.id === selectedRowId.value);
  const index = row?.storage ? storageIndexMap[row.storage] : undefined;
  if (!index) {
    appendLog(logText("download.selectStorageFirst"), "error");
    return;
  }

  try {
    await run(
      () => toolApi.runAction("switch-storage", { start_sector: index }),
      logText("task.switchStorage"),
    );
  } catch (err) {
    appendLog(String(err), "error");
  }
}

async function showPartitionList() {
  try {
    const table = await run(() => toolApi.partitionList(), logText("task.partitionList"));
    if (!table) return;
    fillFromPartitionTable(table);
    appendLog(logText("download.partitionsLoaded", { count: String(table.partitions.length) }), "success");
  } catch (err) {
    appendLog(String(err), "error");
  }
}

function clearRows() {
  clearFormRows();
  appendLog(logText("download.cleared"));
}
</script>

<template>
  <div class="download-page">
    <div class="partition-table">
      <div class="partition-table__header">
        <span class="col col--index">#</span>
        <span class="col col--check" />
        <span class="col col--storage">{{ t("download.storage") }}</span>
        <span class="col col--address">{{ t("download.address") }}</span>
        <span class="col col--name">{{ t("download.name") }}</span>
        <span class="col col--path">{{ t("download.path") }}</span>
        <span class="col col--actions" aria-hidden="true" />
      </div>

      <div
        v-for="(row, index) in rows"
        :key="row.id"
        class="partition-table__row"
        @click="selectRow(row.id)"
      >
        <span class="col col--index">{{ index + 1 }}</span>
        <span class="col col--check">
          <input v-model="row.enabled" type="checkbox" @click.stop />
        </span>
        <span class="col col--storage">
          <select v-model="row.storage" class="storage-select" @click.stop>
            <option v-for="opt in storageOptions" :key="opt || 'empty'" :value="opt">
              {{ opt }}
            </option>
          </select>
        </span>
        <span class="col col--address">
          <input v-model="row.address" class="cell-input" @click.stop />
        </span>
        <span class="col col--name">
          <input v-model="row.name" class="cell-input" @click.stop />
        </span>
        <span class="col col--path" @click.stop>
          <PathField v-model="row.path" @browse="browsePath(row)" />
        </span>
        <span class="col col--actions">
          <button
            type="button"
            class="partition-table__remove"
            :title="t('download.removeRow', { index: String(index + 1) })"
            :aria-label="t('download.removeRow', { index: String(index + 1) })"
            :disabled="busy"
            @click.stop="removeRow(row.id)"
          >
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
              <path d="M3 6h18M9 6V4h6v2M5 6l1 14h12l1-14M10 10v6M14 10v6" />
            </svg>
          </button>
        </span>
      </div>

      <button type="button" class="partition-table__add" @click="addRow">
        {{ t("download.addRow") }}
      </button>
    </div>

    <div class="toolbar">
      <span class="toolbar__loader">{{ t("download.loaderVer") }}: {{ loaderVersion }}</span>
      <div class="toolbar__actions">
        <label class="toolbar__checkbox">
          <input v-model="forceByAddress" type="checkbox" :disabled="busy" />
          {{ t("download.forceByAddress") }}
        </label>
        <AppButton variant="primary" :disabled="busy" @click="execute">{{ t("download.execute") }}</AppButton>
        <AppButton :disabled="busy" @click="switchStorage">{{ t("download.switch") }}</AppButton>
        <AppButton :disabled="busy" @click="showPartitionList">{{ t("download.partitionTable") }}</AppButton>
        <AppButton :disabled="busy" @click="clearRows">{{ t("download.clear") }}</AppButton>
      </div>
    </div>
  </div>
</template>

<style scoped>
.download-page {
  display: flex;
  flex-direction: column;
  gap: 16px;
  width: 100%;
  max-width: 652px;
}

.partition-table {
  width: 100%;
  border: 1px solid var(--color-border);
  border-radius: var(--border-radius-lg);
  background: var(--color-surface);
  overflow: hidden;
}

.partition-table__header,
.partition-table__row {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 0 12px;
}

.partition-table__header {
  height: 40px;
  background: var(--color-table-header);
  color: var(--color-text-secondary);
  font-size: 12px;
  font-weight: 600;
}

.partition-table__row {
  height: 52px;
  border-top: 1px solid var(--color-border);
  cursor: pointer;
  transition: background 0.15s;
}

.partition-table__row:hover {
  background: var(--color-surface-hover);
}

.col {
  flex-shrink: 0;
  display: flex;
  align-items: center;
}

.col--index {
  width: 26px;
  justify-content: center;
  font-size: 12px;
}

.col--check {
  width: 26px;
  justify-content: center;
}

.col--storage {
  width: 68px;
}

.col--address {
  width: 88px;
}

.col--name {
  width: 88px;
}

.col--path {
  flex: 1;
  min-width: 0;
}

.col--actions {
  width: 32px;
  justify-content: center;
}

.partition-table__remove {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  padding: 0;
  border: 1px solid var(--color-border);
  border-radius: 6px;
  background: var(--color-surface);
  color: var(--color-danger);
}

.partition-table__remove:hover:not(:disabled) {
  background: var(--color-danger-light);
  border-color: var(--color-danger);
}

.partition-table__remove:focus-visible {
  outline: 2px solid var(--color-primary);
  outline-offset: 2px;
}

.partition-table__remove:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.storage-select {
  width: 68px;
  height: 28px;
  padding: 0 6px;
  border-radius: 6px;
  border: 1px solid var(--color-border);
  background: var(--color-surface);
  font-size: 11px;
  appearance: none;
  background-image: linear-gradient(45deg, transparent 50%, #64748b 50%),
    linear-gradient(135deg, #64748b 50%, transparent 50%);
  background-position: calc(100% - 12px) 50%, calc(100% - 8px) 50%;
  background-size: 4px 4px, 4px 4px;
  background-repeat: no-repeat;
}

.cell-input {
  width: 100%;
  height: 28px;
  padding: 0 4px;
  border: none;
  background: transparent;
  font-size: 12px;
  color: var(--color-text-primary);
}

.partition-table__add {
  width: 100%;
  height: 40px;
  border: none;
  border-top: 1px solid var(--color-border);
  background: var(--color-surface);
  color: var(--color-primary);
  font-size: 12px;
  font-weight: 600;
}

.partition-table__add:hover {
  background: var(--color-surface-hover);
}

.toolbar {
  display: flex;
  align-items: center;
  gap: 16px;
  padding: 16px;
  border-radius: var(--border-radius-lg);
  background: var(--color-surface);
  border: 1px solid var(--color-border);
  width: 100%;
  flex-wrap: wrap;
}

.toolbar__loader {
  font-size: 12px;
  color: var(--color-text-secondary);
  white-space: nowrap;
}

.toolbar__actions {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 12px;
  margin-left: auto;
}

.toolbar__checkbox {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-right: 4px;
  font-size: 12px;
  color: var(--color-text-secondary);
  white-space: nowrap;
  cursor: pointer;
}
</style>
