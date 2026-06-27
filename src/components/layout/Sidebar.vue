<script setup lang="ts">
import type { PageId } from "../../types/app";

defineProps<{
  activePage: PageId;
}>();

const emit = defineEmits<{ navigate: [page: PageId] }>();

const navItems: { id: PageId; label: string }[] = [
  { id: "download", label: "下载镜像" },
  { id: "upgrade", label: "升级固件" },
  { id: "advanced", label: "高级功能" },
];
</script>

<template>
  <aside class="sidebar">
    <div class="sidebar__logo">
      <span class="sidebar__title">RKDevTool</span>
      <span class="sidebar__subtitle">瑞芯微开发工具</span>
    </div>

    <nav class="sidebar__nav">
      <button
        v-for="item in navItems"
        :key="item.id"
        type="button"
        class="sidebar__nav-item"
        :class="{ 'sidebar__nav-item--active': activePage === item.id }"
        @click="emit('navigate', item.id)"
      >
        <span class="sidebar__dot" />
        <span>{{ item.label }}</span>
      </button>
    </nav>

    <div class="sidebar__spacer" />
    <div class="sidebar__version">v1.0.0</div>
  </aside>
</template>

<style scoped>
.sidebar {
  width: var(--sidebar-width);
  height: 100%;
  background: var(--color-sidebar);
  display: flex;
  flex-direction: column;
  padding: 8px 16px 16px;
  flex-shrink: 0;
}

.sidebar__logo {
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding: 20px 4px 16px;
}

.sidebar__title {
  font-size: 18px;
  font-weight: 700;
  color: #fff;
}

.sidebar__subtitle {
  font-size: 11px;
  color: var(--color-text-muted);
}

.sidebar__nav {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.sidebar__nav-item {
  display: flex;
  align-items: center;
  gap: 12px;
  width: 188px;
  height: 40px;
  padding: 0 12px;
  border: none;
  border-radius: var(--border-radius-md);
  background: transparent;
  color: #cbd5e1;
  font-size: 13px;
  text-align: left;
  transition: background 0.15s, color 0.15s;
}

.sidebar__nav-item:hover {
  background: var(--color-sidebar-hover);
}

.sidebar__nav-item--active {
  background: var(--color-sidebar-active);
  color: #fff;
  font-weight: 600;
}

.sidebar__nav-item--active .sidebar__dot {
  background: #fff;
}

.sidebar__dot {
  width: 8px;
  height: 8px;
  border-radius: 4px;
  background: #64748b;
  flex-shrink: 0;
}

.sidebar__spacer {
  flex: 1;
}

.sidebar__version {
  text-align: center;
  font-size: 12px;
  color: var(--color-text-muted);
  padding-bottom: 8px;
}
</style>
