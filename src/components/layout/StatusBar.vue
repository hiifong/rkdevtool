<script setup lang="ts">
import { computed } from "vue";
import { useAppState } from "../../composables/useAppState";
import { useI18n } from "../../i18n";

const emit = defineEmits<{
  deviceChange: [locationId: string];
}>();

const { deviceState, devices, selectedDeviceId, busy } = useAppState();
const { t } = useI18n();

const statusLabel = computed(() => {
  switch (deviceState.value) {
    case "connected":
      return t("status.maskrom");
    case "loader":
      return t("status.loader");
    default:
      return t("status.disconnected");
  }
});

const dotColor = computed(() => {
  switch (deviceState.value) {
    case "connected":
      return "var(--color-success)";
    case "loader":
      return "var(--color-primary)";
    default:
      return "var(--color-danger)";
  }
});

function onSelect(event: Event) {
  const value = (event.target as HTMLSelectElement).value;
  if (value) emit("deviceChange", value);
}
</script>

<template>
  <footer class="status-bar">
    <div class="status-bar__left">
      <span class="status-bar__dot" :style="{ background: dotColor }" />
      <span class="status-bar__text">{{ statusLabel }}</span>
      <span v-if="busy" class="status-bar__busy">{{ t("status.busy") }}</span>
    </div>
    <select
      class="status-bar__select"
      :value="selectedDeviceId ?? ''"
      :disabled="devices.length === 0"
      @change="onSelect"
    >
      <option v-if="devices.length === 0" value="">{{ t("status.noDevice") }}</option>
      <option v-for="device in devices" :key="device.location_id" :value="device.location_id">
        {{ device.label }}
      </option>
    </select>
  </footer>
</template>

<style scoped>
.status-bar {
  height: var(--status-bar-height);
  background: var(--color-surface);
  border-top: 1px solid var(--color-border);
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 16px;
  flex-shrink: 0;
}

.status-bar__left {
  display: flex;
  align-items: center;
  gap: 10px;
}

.status-bar__dot {
  width: 10px;
  height: 10px;
  border-radius: 50%;
  flex-shrink: 0;
}

.status-bar__text {
  font-size: 14px;
  font-weight: 600;
  color: var(--color-text-primary);
}

.status-bar__busy {
  font-size: 12px;
  color: var(--color-primary);
}

.status-bar__select {
  min-width: 280px;
  height: 36px;
  padding: 0 10px;
  border-radius: var(--border-radius-md);
  border: 1px solid var(--color-border);
  background: var(--color-surface);
  color: var(--color-text-primary);
  font-size: 12px;
}

.status-bar__select:disabled {
  opacity: 0.6;
}
</style>
