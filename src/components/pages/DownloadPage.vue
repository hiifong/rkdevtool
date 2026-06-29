<script setup lang="ts">
import { ref } from "vue";
import AppButton from "../ui/AppButton.vue";
import PathField from "../ui/PathField.vue";
import { useAppState } from "../../composables/useAppState";
import { useToolCommand, toolApi } from "../../composables/useToolCommand";
import { pickFile } from "../../composables/useFilePicker";
import { useI18n } from "../../i18n";
import { logText } from "../../i18n/logText";
import type { PartitionRow, StorageType } from "../../types/app";

const { appendLog, busy } = useAppState();
const { run } = useToolCommand();
const { t } = useI18n();

const storageOptions: StorageType[] = [
  "",
  "FLASH",
  "EMMC",
  "SD",
  "SPINOR",
  "SPINAND",
  "SATA",
  "PCIE",
];

const storageIndexMap: Record<string, string> = {
  FLASH: "1",
  EMMC: "2",
  SD: "3",
  SPINOR: "5",
  SPINAND: "6",
  SATA: "9",
  PCIE: "10",
};

const forceByAddress = ref(false);
const selectedRowId = ref(1);
const loaderVersion = ref("");
let nextId = 2;

const rows = ref<PartitionRow[]>([
  {
    id: 1,
    enabled: true,
    storage: "",
    address: "0x00000000",
    name: "Loader",
    path: "",
  },
]);

function selectRow(id: number) {
  selectedRowId.value = id;
}

function addRow() {
  rows.value.push({
    id: nextId++,
    enabled: true,
    storage: "",
    address: "0x00000000",
    name: "",
    path: "",
  });
}

async function browsePath(row: PartitionRow) {
  const path = await pickFile(t("download.pickImage"));
  if (!path) return;
  row.path = path;
  if (row.name.toLowerCase().includes("loader")) {
    await refreshLoaderVersion(path);
  }
}

async function refreshLoaderVersion(path: string) {
  try {
    const info = await toolApi.parseFirmware(path);
    loaderVersion.value = info.loader_version || "";
  } catch {
    loaderVersion.value = "";
  }
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
      () => toolApi.runAction("切换存储", { start_sector: index }),
      logText("task.switchStorage"),
    );
  } catch (err) {
    appendLog(String(err), "error");
  }
}

async function showPartitionList() {
  try {
    const output = await run(() => toolApi.partitionList(), logText("task.partitionList"));
    if (output) appendLog(output, "info");
  } catch (err) {
    appendLog(String(err), "error");
  }
}

function clearRows() {
  rows.value = [];
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
  width: 56px;
}

.col--path {
  flex: 1;
  min-width: 0;
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
