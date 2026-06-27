<script setup lang="ts">
defineProps<{
  variant?: "primary" | "secondary" | "destructive";
  size?: "md" | "sm";
  type?: "button" | "submit";
  disabled?: boolean;
}>();
</script>

<template>
  <button
    class="app-btn"
    :class="[`app-btn--${variant ?? 'secondary'}`, size === 'sm' && 'app-btn--sm']"
    :type="type ?? 'button'"
    :disabled="disabled"
  >
    <slot />
  </button>
</template>

<style scoped>
.app-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  height: 36px;
  padding: 0 16px;
  border-radius: var(--border-radius-md);
  border: 1px solid transparent;
  font-size: 13px;
  font-weight: 600;
  white-space: nowrap;
  transition: background 0.15s, border-color 0.15s;
}

.app-btn--sm {
  height: 32px;
  padding: 0 12px;
  font-size: 12px;
}

.app-btn--primary {
  background: var(--color-primary);
  color: #fff;
}

.app-btn--primary:hover:not(:disabled) {
  background: var(--color-primary-hover);
}

.app-btn--secondary {
  background: var(--color-surface);
  color: var(--color-text-primary);
  border-color: var(--color-border);
}

.app-btn--secondary:hover:not(:disabled) {
  background: var(--color-surface-hover);
}

.app-btn--destructive {
  background: var(--color-surface);
  color: var(--color-danger);
  border-color: var(--color-danger-light);
}

.app-btn--destructive:hover:not(:disabled) {
  background: var(--color-danger-light);
}

.app-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
</style>
