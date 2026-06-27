<script setup lang="ts">
import { ref } from "vue";
import AppButton from "../ui/AppButton.vue";
import PathField from "../ui/PathField.vue";
import { useAppState } from "../../composables/useAppState";
import { pickFile } from "../../composables/useFilePicker";

const { appendLog } = useAppState();

const bootPath = ref("/Users/hiifong/Downloads/MiniLoaderAll.bin");
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
const selectedStorage = ref(5);

const actions = [
  "读取FlashID",
  "读取Flash信息",
  "读取Chip信息",
  "读取Capability",
  "测试设备",
  "重启设备",
  "进入Maskrom",
  "切换存储",
  "清空序列号",
  "检测安全模式",
  "导出串口日志",
  "获取当前存储",
  "导出镜像",
  "擦除扇区",
  "擦除所有",
  "切换USB3",
];

const destructiveActions = new Set(["擦除扇区", "擦除所有"]);

async function browseFile(target: "boot" | "firmware") {
  const title = target === "boot" ? "选择 Boot 文件" : "选择固件文件";
  const path = await pickFile(title);
  if (!path) return;
  if (target === "boot") bootPath.value = path;
  else firmwarePath.value = path;
}

function runAction(action: string) {
  appendLog(`${action}开始`);
  appendLog(`${action}成功`, "success");
}
</script>

<template>
  <div class="advanced-page">
    <div class="file-section">
      <div class="file-row">
        <span class="file-row__label">Boot:</span>
        <PathField
          v-model="bootPath"
          browse-variant="inline"
          placeholder="输入或选择 Boot 路径"
          @browse="browseFile('boot')"
        />
        <AppButton variant="primary">下载</AppButton>
      </div>
      <div class="file-row">
        <span class="file-row__label">固件:</span>
        <PathField
          v-model="firmwarePath"
          browse-variant="inline"
          placeholder="输入或选择固件路径"
          @browse="browseFile('firmware')"
        />
        <AppButton>解包</AppButton>
      </div>
    </div>

    <div class="grid-area">
      <div class="action-grid">
        <button
          v-for="action in actions"
          :key="action"
          type="button"
          class="action-btn"
          :class="{ 'action-btn--destructive': destructiveActions.has(action) }"
          @click="runAction(action)"
        >
          {{ action }}
        </button>
      </div>

      <div class="storage-list">
        <div class="storage-list__title">存储类型</div>
        <button
          v-for="(item, index) in storageItems"
          :key="item"
          type="button"
          class="storage-list__item"
          :class="{ 'storage-list__item--active': selectedStorage === index }"
          @click="selectedStorage = index"
        >
          {{ index + 1 }}. {{ item }}
        </button>
      </div>
    </div>

    <div class="sector-card">
      <div class="sector-row">
        <span class="sector-row__label">起始扇区:</span>
        <input v-model="startSector" class="sector-row__input" type="text" />
      </div>
      <div class="sector-row">
        <span class="sector-row__label">扇区数:</span>
        <input v-model="sectorCount" class="sector-row__input" type="text" />
      </div>
    </div>
  </div>
</template>

<style scoped>
.advanced-page {
  display: flex;
  flex-direction: column;
  gap: 24px;
  width: 652px;
}

.file-section {
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding: 20px;
  border-radius: var(--border-radius-lg);
  border: 1px solid var(--color-border);
  background: var(--color-surface);
}

.file-row {
  display: flex;
  align-items: center;
  gap: 12px;
}

.file-row__label {
  width: 40px;
  flex-shrink: 0;
  font-size: 13px;
  color: var(--color-text-secondary);
}

.file-row :deep(.path-field) {
  flex: 1;
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

.action-btn:hover {
  background: var(--color-surface-hover);
  border-color: var(--color-border-strong);
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

.storage-list__item:hover {
  background: var(--color-surface-hover);
}

.storage-list__item--active {
  background: var(--color-primary-light);
  color: var(--color-primary);
  font-weight: 600;
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
