<script setup lang="ts">
defineProps<{
  variant?: "table" | "inline";
}>();

const emit = defineEmits<{ click: [] }>();
</script>

<template>
  <button
    type="button"
    class="browse-btn"
    :class="variant === 'inline' ? 'browse-btn--inline' : 'browse-btn--table'"
    @click="emit('click')"
  >
    ...
  </button>
</template>

<style scoped>
.browse-btn {
  flex-shrink: 0;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border-radius: 6px;
  border: 1px solid var(--color-border);
  background: var(--color-surface);
  color: var(--color-text-secondary);
  font-size: 12px;
  font-weight: 600;
  transition: background 0.15s;
}

.browse-btn:hover {
  background: var(--color-surface-hover);
}

.browse-btn--table {
  width: 32px;
  height: 28px;
}

.browse-btn--inline {
  width: 36px;
  height: 36px;
}
</style>
