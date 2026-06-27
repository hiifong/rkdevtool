<script setup lang="ts">
const model = defineModel<string>({ default: "" });

defineProps<{
  placeholder?: string;
  browseVariant?: "table" | "inline";
}>();

const emit = defineEmits<{ browse: [] }>();
</script>

<template>
  <div class="path-field" :class="browseVariant === 'inline' && 'path-field--inline'">
    <input
      v-model="model"
      type="text"
      class="path-field__input"
      :placeholder="placeholder"
      spellcheck="false"
      autocomplete="off"
    />
    <slot name="suffix">
      <button type="button" class="path-field__browse" @click="emit('browse')">...</button>
    </slot>
  </div>
</template>

<style scoped>
.path-field {
  display: flex;
  align-items: center;
  gap: 8px;
  flex: 1;
  min-width: 0;
}

.path-field--inline {
  gap: 12px;
}

.path-field__input {
  flex: 1;
  min-width: 0;
  width: 100%;
  height: 28px;
  padding: 0 4px;
  border: none;
  background: transparent;
  font-size: 12px;
  font-family: var(--font-family-mono);
  color: var(--color-text-primary);
  white-space: nowrap;
  overflow-x: auto;
  text-overflow: clip;
}

.path-field--inline .path-field__input {
  height: 36px;
  padding: 0 12px;
  border-radius: var(--border-radius-md);
  border: 1px solid var(--color-border);
  background: var(--color-surface-hover);
  font-size: 13px;
  font-family: var(--font-family);
}

.path-field__input::-webkit-scrollbar {
  height: 4px;
}

.path-field__input::-webkit-scrollbar-thumb {
  background: var(--color-border-strong);
  border-radius: 2px;
}

.path-field__browse {
  flex-shrink: 0;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 32px;
  height: 28px;
  border-radius: 6px;
  border: 1px solid var(--color-border);
  background: var(--color-surface);
  color: var(--color-text-secondary);
  font-size: 12px;
  font-weight: 600;
}

.path-field--inline .path-field__browse {
  width: 36px;
  height: 36px;
}

.path-field__browse:hover {
  background: var(--color-surface-hover);
}
</style>
