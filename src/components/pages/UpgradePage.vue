<script setup lang="ts">
import { ref } from "vue";
import AppButton from "../ui/AppButton.vue";
import PathField from "../ui/PathField.vue";
import { useAppState } from "../../composables/useAppState";
import { pickFile } from "../../composables/useFilePicker";

const { appendLog } = useAppState();

const firmwarePath = ref(
  "/Users/hiifong/Downloads/Luckfox_Pico_Pro_Max_Flash_250607/update.img",
);
const firmwareVersion = ref("1.0.00");
const loaderVersion = ref("1.01");
const chipInfo = ref("RK3568");

async function browseFirmware() {
  const path = await pickFile("选择固件文件");
  if (path) firmwarePath.value = path;
}

function upgrade() {
  appendLog("升级固件开始");
  appendLog("升级固件成功", "success");
}
</script>

<template>
  <div class="upgrade-page">
    <div class="firmware-row">
      <span class="firmware-row__label">固件</span>
      <PathField
        v-model="firmwarePath"
        browse-variant="inline"
        placeholder="输入或选择固件路径"
        @browse="browseFirmware"
      />
      <AppButton variant="primary" @click="upgrade">升级</AppButton>
    </div>

    <div class="info-card">
      <div class="info-card__row">
        <span class="info-card__label">固件版本</span>
        <input v-model="firmwareVersion" class="info-card__value" readonly />
      </div>
      <div class="info-card__row">
        <span class="info-card__label">Loader版本</span>
        <input v-model="loaderVersion" class="info-card__value" readonly />
      </div>
      <div class="info-card__row">
        <span class="info-card__label">芯片信息</span>
        <input v-model="chipInfo" class="info-card__value" readonly />
      </div>
    </div>
  </div>
</template>

<style scoped>
.upgrade-page {
  display: flex;
  flex-direction: column;
  gap: 24px;
  width: 648px;
}

.firmware-row {
  display: flex;
  align-items: center;
  gap: 12px;
}

.firmware-row__label {
  width: 40px;
  flex-shrink: 0;
  font-size: 13px;
  color: var(--color-text-secondary);
}

.firmware-row :deep(.path-field) {
  flex: 1;
  min-width: 0;
}

.info-card {
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding: 20px;
  border-radius: var(--border-radius-lg);
  border: 1px solid var(--color-border);
  background: var(--color-surface);
}

.info-card__row {
  display: flex;
  align-items: center;
  gap: 16px;
}

.info-card__label {
  width: 88px;
  flex-shrink: 0;
  font-size: 13px;
  color: var(--color-text-secondary);
  text-align: left;
}

.info-card__value {
  flex: 1;
  height: 36px;
  padding: 0 12px;
  border-radius: var(--border-radius-md);
  border: 1px solid var(--color-border);
  background: var(--color-surface-hover);
  font-size: 13px;
  color: var(--color-text-primary);
}
</style>
