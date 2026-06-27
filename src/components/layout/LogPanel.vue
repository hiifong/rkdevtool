<script setup lang="ts">
import { computed } from "vue";
import { useAppState } from "../../composables/useAppState";

const { logs, clearLogs } = useAppState();

const levelClass = computed(() => (level: string) => `log-line--${level}`);
</script>

<template>
  <aside class="log-panel">
    <header class="log-panel__header">
      <span>输出日志</span>
      <button type="button" class="log-panel__clear" @click="clearLogs">清空</button>
    </header>
    <div class="log-panel__content">
      <p
        v-for="entry in logs"
        :key="entry.id"
        class="log-line"
        :class="levelClass(entry.level)"
      >
        {{ entry.text }}
      </p>
    </div>
  </aside>
</template>

<style scoped>
.log-panel {
  width: var(--log-panel-width);
  height: 100%;
  background: var(--color-log-bg);
  display: flex;
  flex-direction: column;
  flex-shrink: 0;
}

.log-panel__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  height: 44px;
  padding: 0 16px;
  background: #1e293b;
  color: #94a3b8;
  font-size: 13px;
  font-weight: 600;
}

.log-panel__clear {
  border: none;
  background: transparent;
  color: #94a3b8;
  font-size: 12px;
  padding: 4px 8px;
  border-radius: 4px;
}

.log-panel__clear:hover {
  color: #e2e8f0;
  background: rgba(255, 255, 255, 0.06);
}

.log-panel__content {
  flex: 1;
  overflow-y: auto;
  padding: 16px;
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.log-line {
  margin: 0;
  font-family: var(--font-family-mono);
  font-size: 12px;
  line-height: 18px;
  color: var(--color-log-text);
  word-break: break-all;
}

.log-line--success {
  color: var(--color-log-success);
  font-weight: 600;
}

.log-line--info {
  color: var(--color-log-info);
}

.log-line--error {
  color: #f87171;
}
</style>
