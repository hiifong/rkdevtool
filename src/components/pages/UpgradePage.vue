<script setup lang="ts">
import { onMounted, ref } from "vue";
import AppButton from "../ui/AppButton.vue";
import PathField from "../ui/PathField.vue";
import { useAppState } from "../../composables/useAppState";
import { useToolCommand, toolApi } from "../../composables/useToolCommand";
import { pickFile } from "../../composables/useFilePicker";
import { useI18n } from "../../i18n";
import { logText } from "../../i18n/logText";

const { appendLog, busy, deviceState } = useAppState();
const { run } = useToolCommand();
const { t } = useI18n();

const firmwarePath = ref("");
const firmwareVersion = ref("");
const loaderVersion = ref("");
const chipInfo = ref("");

async function refreshFirmwareInfo() {
  const path = firmwarePath.value.trim();
  if (!path) {
    firmwareVersion.value = "";
    loaderVersion.value = "";
    chipInfo.value = "";
    return;
  }

  try {
    const info = await toolApi.parseFirmware(path);
    firmwareVersion.value = info.firmware_version || "";
    loaderVersion.value = info.loader_version || "";
    chipInfo.value = info.chip_family || "";
  } catch (err) {
    firmwareVersion.value = "";
    loaderVersion.value = "";
    chipInfo.value = "";
    appendLog(String(err), "error");
  }
}

async function browseFirmware() {
  const path = await pickFile(t("upgrade.pickFirmware"));
  if (path) {
    firmwarePath.value = path;
    await refreshFirmwareInfo();
  }
}

async function refreshChipInfo() {
  if (deviceState.value !== "loader") {
    chipInfo.value = t("upgrade.maskromChipHint");
    return;
  }

  try {
    const output = await toolApi.readChipInfo();
    const text = output.trim();
    chipInfo.value = text.includes("Fail") ? t("upgrade.chipReadFailed") : text;
  } catch {
    chipInfo.value = t("upgrade.chipReadError");
  }
}

async function upgrade() {
  if (!firmwarePath.value.trim()) {
    appendLog(logText("upgrade.selectFirmwareFirst"), "error");
    return;
  }

  try {
    await run(() => toolApi.upgradeFirmware(firmwarePath.value), logText("task.upgradeFirmware"));
    appendLog(logText("upgrade.upgradeSuccess"), "success");
  } catch (err) {
    appendLog(String(err), "error");
  }
}

onMounted(() => {
  if (deviceState.value === "loader") {
    refreshChipInfo();
  } else {
    chipInfo.value = "";
  }
});
</script>

<template>
  <div class="upgrade-page">
    <div class="firmware-row">
      <span class="firmware-row__label">{{ t("upgrade.firmware") }}</span>
      <PathField
        v-model="firmwarePath"
        browse-variant="inline"
        :placeholder="t('upgrade.firmwarePlaceholder')"
        @browse="browseFirmware"
      />
      <AppButton variant="primary" :disabled="busy" @click="upgrade">{{ t("upgrade.upgrade") }}</AppButton>
    </div>

    <div class="info-card">
      <div class="info-card__row">
        <span class="info-card__label">{{ t("upgrade.firmwareVersion") }}</span>
        <input v-model="firmwareVersion" class="info-card__value" readonly />
      </div>
      <div class="info-card__row">
        <span class="info-card__label">{{ t("upgrade.loaderVersion") }}</span>
        <input v-model="loaderVersion" class="info-card__value" readonly />
      </div>
      <div class="info-card__row">
        <span class="info-card__label">{{ t("upgrade.chipInfo") }}</span>
        <input v-model="chipInfo" class="info-card__value" readonly />
      </div>
      <div class="info-card__actions">
        <AppButton size="sm" :disabled="busy" @click="refreshChipInfo">{{ t("upgrade.refreshChipInfo") }}</AppButton>
        <AppButton size="sm" :disabled="busy" @click="refreshFirmwareInfo">{{ t("upgrade.refreshFirmwareInfo") }}</AppButton>
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
  display: grid;
  grid-template-columns: auto minmax(0, 1fr) auto;
  align-items: center;
  gap: 12px;
  padding: 16px;
  border-radius: var(--border-radius-lg);
  border: 1px solid var(--color-border);
  background: var(--color-surface);
}

.firmware-row__label {
  flex-shrink: 0;
  font-size: 13px;
  color: var(--color-text-secondary);
  white-space: nowrap;
}

.firmware-row :deep(.path-field) {
  min-width: 0;
}

.info-card {
  display: grid;
  grid-template-columns: max-content minmax(0, 1fr);
  column-gap: 16px;
  row-gap: 12px;
  padding: 20px;
  border-radius: var(--border-radius-lg);
  border: 1px solid var(--color-border);
  background: var(--color-surface);
}

.info-card__row {
  display: contents;
}

.info-card__label {
  font-size: 13px;
  color: var(--color-text-secondary);
  text-align: left;
  white-space: nowrap;
}

.info-card__value {
  width: 100%;
  height: 36px;
  padding: 0 12px;
  border-radius: var(--border-radius-md);
  border: 1px solid var(--color-border);
  background: var(--color-surface-hover);
  font-size: 13px;
  color: var(--color-text-primary);
}

.info-card__actions {
  grid-column: 1 / -1;
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}
</style>
