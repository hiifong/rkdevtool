<script setup lang="ts">
import { computed, nextTick, ref, watch } from "vue";
import { useAppState } from "../../composables/useAppState";
import { useI18n } from "../../i18n";

const { logs, clearLogs } = useAppState();
const { t } = useI18n();

const contentRef = ref<HTMLElement | null>(null);
const stickToBottom = ref(true);

const levelClass = computed(() => (level: string) => `log-line--${level}`);

function onScroll() {
  const el = contentRef.value;
  if (!el) return;
  stickToBottom.value = el.scrollHeight - el.scrollTop - el.clientHeight < 48;
}

async function scrollToBottom() {
  await nextTick();
  const el = contentRef.value;
  if (!el || !stickToBottom.value) return;
  el.scrollTop = el.scrollHeight;
}

watch(
  () => logs.value.map((entry) => `${entry.id}:${entry.text}`).join("\n"),
  () => {
    scrollToBottom();
  },
);

function handleClear() {
  clearLogs();
  stickToBottom.value = true;
  scrollToBottom();
}
</script>

<template>
  <aside class="log-panel">
    <header class="log-panel__header">
      <span>{{ t("log.title") }}</span>
      <button type="button" class="log-panel__clear" @click="handleClear">{{ t("log.clear") }}</button>
    </header>
    <div ref="contentRef" class="log-panel__content" @scroll="onScroll">
      <p
        v-for="entry in logs"
        :key="entry.id"
        class="log-line"
        :class="[levelClass(entry.level), entry.kind === 'progress' && 'log-line--progress']"
      >
        {{ entry.text }}
      </p>
    </div>
  </aside>
</template>

<style scoped>
.log-panel {
  flex: 0 1 var(--log-panel-width);
  width: var(--log-panel-width);
  min-width: var(--log-panel-min-width);
  max-width: var(--log-panel-max-width);
  min-height: 0;
  align-self: stretch;
  background: var(--color-log-bg);
  display: flex;
  flex-direction: column;
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
  flex-shrink: 0;
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
  min-width: 0;
  min-height: 0;
  overflow-y: auto;
  overflow-x: hidden;
  padding: 16px;
  display: flex;
  flex-direction: column;
  gap: 6px;
  scrollbar-gutter: stable;
}

.log-panel__content::-webkit-scrollbar {
  width: 8px;
}

.log-panel__content::-webkit-scrollbar-track {
  background: rgba(255, 255, 255, 0.04);
}

.log-panel__content::-webkit-scrollbar-thumb {
  background: rgba(148, 163, 184, 0.35);
  border-radius: 4px;
}

.log-panel__content::-webkit-scrollbar-thumb:hover {
  background: rgba(148, 163, 184, 0.55);
}

.log-line {
  margin: 0;
  max-width: 100%;
  font-family: var(--font-family-mono);
  font-size: 12px;
  line-height: 18px;
  color: var(--color-log-text);
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  word-break: break-word;
  flex-shrink: 0;
}

.log-line--progress {
  color: #fbbf24;
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
