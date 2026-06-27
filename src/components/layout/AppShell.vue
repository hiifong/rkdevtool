<script setup lang="ts">
import { computed } from "vue";
import { provideAppState } from "../../composables/useAppState";
import Sidebar from "./Sidebar.vue";
import LogPanel from "./LogPanel.vue";
import StatusBar from "./StatusBar.vue";
import PageHeader from "./PageHeader.vue";
import DownloadPage from "../pages/DownloadPage.vue";
import UpgradePage from "../pages/UpgradePage.vue";
import AdvancedPage from "../pages/AdvancedPage.vue";

const { activePage } = provideAppState();

const pageTitle = computed(() => {
  switch (activePage.value) {
    case "download":
      return "下载镜像";
    case "upgrade":
      return "升级固件";
    case "advanced":
      return "高级功能";
  }
});
</script>

<template>
  <div class="app-shell">
    <div class="app-body">
      <Sidebar :active-page="activePage" @navigate="activePage = $event" />

      <div class="main-area">
        <PageHeader :title="pageTitle" />
        <div class="content-row">
          <div class="left-panel">
            <DownloadPage v-if="activePage === 'download'" />
            <UpgradePage v-else-if="activePage === 'upgrade'" />
            <AdvancedPage v-else />
          </div>
          <LogPanel />
        </div>
      </div>
    </div>
    <StatusBar />
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
}

.content-row {
  display: flex;
  flex: 1;
  min-height: 0;
}

.left-panel {
  flex: 1;
  min-width: 0;
  padding: 24px;
  display: flex;
  flex-direction: column;
  gap: 24px;
  overflow-y: auto;
}
</style>
