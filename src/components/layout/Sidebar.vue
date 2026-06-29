<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { getVersion } from "@tauri-apps/api/app";
import { openUrl } from "@tauri-apps/plugin-opener";
import type { PageId } from "../../types/app";
import type { Locale } from "../../types/locale";
import { GITHUB_REPO_URL } from "../../constants/app";
import { useI18n } from "../../i18n";
import packageJson from "../../../package.json";

defineProps<{
  activePage: PageId;
}>();

const emit = defineEmits<{ navigate: [page: PageId] }>();

const { locale, setLocale, t } = useI18n();
const appVersion = ref(`v${packageJson.version}`);

const navItems = computed(() => [
  { id: "download" as const, label: t("nav.download") },
  { id: "upgrade" as const, label: t("nav.upgrade") },
  { id: "advanced" as const, label: t("nav.advanced") },
]);

function switchLocale(next: Locale) {
  setLocale(next);
}

async function openGithub() {
  try {
    await openUrl(GITHUB_REPO_URL);
  } catch {
    window.open(GITHUB_REPO_URL, "_blank", "noopener,noreferrer");
  }
}

onMounted(async () => {
  try {
    const version = await getVersion();
    appVersion.value = `v${version}`;
  } catch {
    // 浏览器预览等非 Tauri 环境沿用 package.json 版本
  }
});
</script>

<template>
  <aside class="sidebar">
    <div class="sidebar__logo">
      <span class="sidebar__title">RKDevTool</span>
      <span class="sidebar__subtitle">{{ t("app.subtitle") }}</span>
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
    <div class="sidebar__footer">
      <div class="sidebar__lang">
        <button
          type="button"
          class="sidebar__lang-btn"
          :class="{ 'sidebar__lang-btn--active': locale === 'zh-CN' }"
          @click="switchLocale('zh-CN')"
        >
          {{ t("lang.zh") }}
        </button>
        <button
          type="button"
          class="sidebar__lang-btn"
          :class="{ 'sidebar__lang-btn--active': locale === 'en' }"
          @click="switchLocale('en')"
        >
          {{ t("lang.en") }}
        </button>
      </div>
      <div class="sidebar__version">{{ appVersion }}</div>
      <a
        class="sidebar__github"
        :href="GITHUB_REPO_URL"
        @click.prevent="openGithub"
      >
        <svg class="sidebar__github-icon" viewBox="0 0 24 24" aria-hidden="true">
          <path
            fill="currentColor"
            d="M12 0C5.37 0 0 5.37 0 12c0 5.31 3.435 9.795 8.205 11.385.6.105.825-.255.825-.57 0-.285-.015-1.23-.015-2.235-3.015.555-3.795-.735-4.035-1.41-.135-.345-.72-1.41-1.23-1.695-.42-.225-1.02-.78-.015-.795.945-.015 1.62.87 1.845 1.23 1.08 1.815 2.805 1.305 3.495.99.105-.78.42-1.305.765-1.605-2.67-.3-5.46-1.335-5.46-5.925 0-1.305.465-2.385 1.23-3.225-.12-.3-.54-1.53.12-3.18 0 0 1.005-.315 3.3 1.23.96-.27 1.98-.405 3-.405s2.04.135 3 .405c2.295-1.56 3.3-1.23 3.3-1.23.66 1.65.24 2.88.12 3.18.765.84 1.23 1.905 1.23 3.225 0 4.605-2.805 5.625-5.475 5.925.435.375.81 1.095.81 2.22 0 1.605-.015 2.895-.015 3.3 0 .315.225.69.825.57A12.02 12.02 0 0 0 24 12c0-6.63-5.37-12-12-12Z"
          />
        </svg>
        <span>GitHub</span>
      </a>
    </div>
  </aside>
</template>

<style scoped>
.sidebar {
  width: var(--sidebar-width);
  height: 100%;
  background: var(--color-sidebar);
  display: flex;
  flex-direction: column;
  padding: 8px 16px 12px;
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

.sidebar__footer {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
  width: 100%;
}

.sidebar__lang {
  display: flex;
  gap: 8px;
  justify-content: center;
}

.sidebar__lang-btn {
  min-width: 52px;
  height: 28px;
  padding: 0 10px;
  border-radius: 6px;
  border: 1px solid rgba(148, 163, 184, 0.35);
  background: transparent;
  color: #94a3b8;
  font-size: 12px;
  font-weight: 600;
  transition: background 0.15s, color 0.15s, border-color 0.15s;
}

.sidebar__lang-btn:hover {
  background: var(--color-sidebar-hover);
  color: #e2e8f0;
}

.sidebar__lang-btn--active {
  background: rgba(37, 99, 235, 0.2);
  border-color: #3b82f6;
  color: #fff;
}

.sidebar__version {
  margin: 0;
  text-align: center;
  font-size: 12px;
  line-height: 1;
  color: var(--color-text-muted);
}

.sidebar__github {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  height: 24px;
  padding: 0 8px;
  border-radius: 6px;
  color: var(--color-text-muted);
  font-size: 12px;
  font-weight: 500;
  text-decoration: none;
  transition: color 0.15s, background 0.15s;
}

.sidebar__github:hover {
  color: #e2e8f0;
  background: var(--color-sidebar-hover);
}

.sidebar__github-icon {
  width: 16px;
  height: 16px;
  flex-shrink: 0;
}
</style>
