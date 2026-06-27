<script setup lang="ts">
import { computed } from "vue";
import { useAppState } from "../../composables/useAppState";

const { deviceState, deviceSelector } = useAppState();

const statusLabel = computed(() => {
  switch (deviceState.value) {
    case "connected":
      return "发现一个 MASKROM 设备";
    case "loader":
      return "发现一个 LOADER 设备";
    default:
      return "没有发现设备";
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
</script>

<template>
  <footer class="status-bar">
    <div class="status-bar__left">
      <span class="status-bar__dot" :style="{ background: dotColor }" />
      <span class="status-bar__text">{{ statusLabel }}</span>
    </div>
    <select v-model="deviceSelector" class="status-bar__select">
      <option>{{ deviceSelector }}</option>
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
</style>
