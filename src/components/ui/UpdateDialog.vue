<script setup lang="ts">
import { nextTick, onMounted, onUnmounted, ref, useId } from "vue";
import AppButton from "./AppButton.vue";
import ReleaseNotes from "./ReleaseNotes.vue";
import { useI18n } from "../../i18n";

const props = defineProps<{
  version: string;
  notes: string;
  returnFocus?: HTMLElement | null;
}>();

const emit = defineEmits<{ confirm: []; cancel: [] }>();
const { t } = useI18n();
const dialog = ref<HTMLDialogElement | null>(null);
const titleId = useId();
const descriptionId = useId();
let previousFocus: HTMLElement | null = null;

onMounted(() => {
  previousFocus = props.returnFocus
    ?? (document.activeElement instanceof HTMLElement ? document.activeElement : null);
  dialog.value?.showModal();
});

onUnmounted(async () => {
  dialog.value?.close();
  await nextTick();
  if (previousFocus?.isConnected) previousFocus.focus();
});
</script>

<template>
  <Teleport to="body">
    <dialog
      ref="dialog"
      class="update-dialog"
      :aria-labelledby="titleId"
      :aria-describedby="descriptionId"
      @cancel.prevent="emit('cancel')"
    >
      <header class="update-dialog__header">
        <h2 :id="titleId">{{ t("update.title") }}</h2>
        <p :id="descriptionId">{{ t("update.available", { version }) }}</p>
      </header>

      <div
        v-if="notes.trim()"
        class="update-dialog__notes"
        role="region"
        :aria-label="t('update.releaseNotes')"
        tabindex="0"
      >
        <ReleaseNotes :content="notes" />
      </div>

      <footer class="update-dialog__footer">
        <AppButton autofocus @click="emit('cancel')">{{ t("update.later") }}</AppButton>
        <AppButton variant="primary" @click="emit('confirm')">{{ t("update.install") }}</AppButton>
      </footer>
    </dialog>
  </Teleport>
</template>

<style scoped>
.update-dialog {
  width: min(720px, calc(100vw - 48px));
  max-height: min(680px, calc(100dvh - 48px));
  margin: auto;
  padding: 0;
  overflow: hidden;
  border: 1px solid var(--color-border);
  border-radius: var(--border-radius-lg);
  background: var(--color-surface);
  color: var(--color-text-primary);
  box-shadow: 0 24px 64px rgb(15 23 42 / 24%);
}

.update-dialog[open] {
  display: flex;
  flex-direction: column;
}

.update-dialog::backdrop {
  background: rgb(15 23 42 / 42%);
}

.update-dialog__header {
  flex-shrink: 0;
  padding: 24px 24px 16px;
}

.update-dialog__header h2 {
  margin: 0 0 12px;
  font-size: 18px;
  font-weight: 600;
}

.update-dialog__header p {
  margin: 0;
  font-size: 13px;
  line-height: 1.6;
  overflow-wrap: anywhere;
}

.update-dialog__notes {
  flex: 1 1 auto;
  min-height: 0;
  padding: 0 24px 20px;
  overflow: auto;
  overscroll-behavior: contain;
  scrollbar-gutter: stable;
}

.update-dialog__notes:focus-visible {
  outline: 2px solid var(--color-primary);
  outline-offset: -3px;
}

.update-dialog__notes::-webkit-scrollbar {
  width: 8px;
  height: 8px;
}

.update-dialog__notes::-webkit-scrollbar-thumb {
  border-radius: 4px;
  background: var(--color-border-strong);
}

.update-dialog__footer {
  flex-shrink: 0;
  display: flex;
  justify-content: flex-end;
  flex-wrap: wrap;
  gap: 12px;
  padding: 16px 24px;
  border-top: 1px solid var(--color-border);
}

.update-dialog__footer :deep(button:focus-visible) {
  outline: 2px solid var(--color-primary);
  outline-offset: 3px;
}
</style>
