<script setup lang="ts">
import { ref } from "vue";
import AppButton from "../ui/AppButton.vue";
import PathField from "../ui/PathField.vue";
import { useAppState } from "../../composables/useAppState";
import { pickFile } from "../../composables/useFilePicker";
import type { PartitionRow, StorageType } from "../../types/app";

const { appendLog } = useAppState();

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

const forceByAddress = ref(false);
const selectedRowId = ref(1);
let nextId = 3;

const rows = ref<PartitionRow[]>([
  {
    id: 1,
    enabled: true,
    storage: "EMMC",
    address: "0xCCCCCCCC",
    name: "Loader",
    path: "/Users/hiifong/Downloads/MiniLoaderAll.bin",
  },
  {
    id: 2,
    enabled: true,
    storage: "EMMC",
    address: "0x00000000",
    name: "linux",
    path: "/Users/hiifong/Downloads/rootfs.img",
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
  const path = await pickFile("选择镜像文件");
  if (path) row.path = path;
}

function execute() {
  appendLog("执行开始");
  appendLog("执行成功", "success");
}

function clearRows() {
  rows.value = [];
  appendLog("已清空配置");
}
</script>

<template>
  <div class="download-page">
    <div class="partition-table">
      <div class="partition-table__header">
        <span class="col col--index">#</span>
        <span class="col col--check" />
        <span class="col col--storage">存储</span>
        <span class="col col--address">地址</span>
        <span class="col col--name">名字</span>
        <span class="col col--path">路径</span>
      </div>

      <div
        v-for="(row, index) in rows"
        :key="row.id"
        class="partition-table__row"
        :class="{ 'partition-table__row--selected': selectedRowId === row.id }"
        @click="selectRow(row.id)"
      >
        <span class="col col--index">{{ index + 1 }}</span>
        <span class="col col--check">
          <input v-model="row.enabled" type="checkbox" @click.stop />
        </span>
        <span class="col col--storage">
          <select v-model="row.storage" class="storage-select" @click.stop>
            <option v-for="opt in storageOptions" :key="opt" :value="opt">
              {{ opt || " " }}
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
        + 新增配置项
      </button>
    </div>

    <div class="toolbar">
      <span class="toolbar__loader">Loader Ver: 1.11</span>
      <div class="toolbar__actions">
        <label class="toolbar__checkbox">
          <input v-model="forceByAddress" type="checkbox" />
          强制按地址写
        </label>
        <AppButton variant="primary" @click="execute">执行</AppButton>
        <AppButton>切换</AppButton>
        <AppButton>设备分区表</AppButton>
        <AppButton @click="clearRows">清空</AppButton>
      </div>
    </div>
  </div>
</template>

<style scoped>
.download-page {
  display: flex;
  flex-direction: column;
  gap: 24px;
}

.partition-table {
  width: 648px;
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
}

.partition-table__row--selected {
  background: var(--color-table-row-selected);
  box-shadow: inset 2px 0 0 var(--color-table-row-selected-border);
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
  width: 648px;
}

.toolbar__loader {
  font-size: 12px;
  color: var(--color-text-secondary);
  white-space: nowrap;
}

.toolbar__actions {
  display: flex;
  align-items: center;
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
