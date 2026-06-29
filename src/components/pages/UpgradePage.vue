<script setup lang="ts">
import { onMounted, ref } from "vue";
import AppButton from "../ui/AppButton.vue";
import PathField from "../ui/PathField.vue";
import { useAppState } from "../../composables/useAppState";
import { useToolCommand, toolApi } from "../../composables/useToolCommand";
import { pickFile } from "../../composables/useFilePicker";

const { appendLog, busy, deviceState } = useAppState();
const { run } = useToolCommand();

const firmwarePath = ref("");
const firmwareVersion = ref("—");
const loaderVersion = ref("—");
const chipInfo = ref("—");

async function refreshFirmwareInfo() {
  const path = firmwarePath.value.trim();
  if (!path) {
    firmwareVersion.value = "—";
    loaderVersion.value = "—";
    chipInfo.value = "—";
    return;
  }

  try {
    const info = await toolApi.parseFirmware(path);
    firmwareVersion.value = info.firmware_version || "—";
    loaderVersion.value = info.loader_version || "—";
    chipInfo.value = info.chip_family || "—";
  } catch (err) {
    firmwareVersion.value = "—";
    loaderVersion.value = "—";
    chipInfo.value = "—";
    appendLog(String(err), "error");
  }
}

async function browseFirmware() {
  const path = await pickFile("选择固件文件");
  if (path) {
    firmwarePath.value = path;
    await refreshFirmwareInfo();
  }
}

async function refreshChipInfo() {
  if (deviceState.value !== "loader") {
    chipInfo.value = "Maskrom 模式下需先下载 Loader 后才能读取";
    return;
  }

  try {
    const output = await toolApi.readChipInfo();
    const text = output.trim();
    chipInfo.value = text.includes("Fail") ? "读取失败，请检查设备连接" : text || "—";
  } catch {
    chipInfo.value = "读取失败";
  }
}

async function upgrade() {
  if (!firmwarePath.value.trim()) {
    appendLog("请先选择固件路径", "error");
    return;
  }

  try {
    await run(() => toolApi.upgradeFirmware(firmwarePath.value), "升级固件");
    appendLog("升级固件成功", "success");
  } catch (err) {
    appendLog(String(err), "error");
  }
}

onMounted(() => {
  if (deviceState.value === "loader") {
    refreshChipInfo();
  } else {
    chipInfo.value = "—";
  }
});
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
      <AppButton variant="primary" :disabled="busy" @click="upgrade">升级</AppButton>
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
      <div class="info-card__actions">
        <AppButton size="sm" :disabled="busy" @click="refreshChipInfo">刷新芯片信息</AppButton>
        <AppButton size="sm" :disabled="busy" @click="refreshFirmwareInfo">刷新固件信息</AppButton>
      </div>
    </div>
  </div>
</template>

<style scoped>
.upgrade-page {
  display: flex;
  flex-direction: column;
  gap: 16px;
  width: 100%;
  max-width: 652px;
}

.firmware-row {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 16px;
  border-radius: var(--border-radius-lg);
  border: 1px solid var(--color-border);
  background: var(--color-surface);
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

.info-card__actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}
</style>
