<script setup lang="ts">
import { computed } from "vue";
import { provideAppState } from "../../composables/useAppState";
import { useDevicePoll, useToolBusyPoll } from "../../composables/useDevicePoll";
import Sidebar from "./Sidebar.vue";
import LogPanel from "./LogPanel.vue";
import StatusBar from "./StatusBar.vue";
import PageHeader from "./PageHeader.vue";
import DownloadPage from "../pages/DownloadPage.vue";
import UpgradePage from "../pages/UpgradePage.vue";
import AdvancedPage from "../pages/AdvancedPage.vue";
import { useI18n } from "../../i18n";

const state = provideAppState();
const { activePage } = state;
const { onDeviceChange } = useDevicePoll(2000, state);
useToolBusyPoll(state);
const { t } = useI18n();

const pageTitle = computed(() => {
  switch (activePage.value) {
    case "download":
      return t("page.download");
    case "upgrade":
      return t("page.upgrade");
    case "advanced":
      return t("page.advanced");
  }
});
</script>

<template>
  <div class="app-shell">
    <div class="app-body">
      <Sidebar :active-page="activePage" @navigate="activePage = $event" />

      <div class="main-area">
        <PageHeader :title="pageTitle" />
        <div class="left-panel">
          <DownloadPage v-if="activePage === 'download'" />
          <UpgradePage v-else-if="activePage === 'upgrade'" />
          <AdvancedPage v-else />
        </div>
      </div>
      <LogPanel />
    </div>
    <StatusBar @device-change="onDeviceChange" />
  </div>
</template>

<style scoped>
.app-shell {
  display: flex;
  flex-direction: column;
  width: 100%;
  height: 100%;
  background: var(--color-bg);
}

.app-body {
  display: flex;
  flex: 1;
  min-height: 0;
}

.main-area {
  flex: 1;
  display: flex;
  flex-direction: column;
  min-width: 0;
  min-height: 0;
}

.left-panel {
  flex: 1;
  min-width: 0;
  min-height: 0;
  padding: 16px 24px;
  display: flex;
  flex-direction: column;
  gap: 16px;
  overflow: auto;
}
</style>
