<script setup lang="ts">
import { computed } from "vue";
import { openUrl } from "@tauri-apps/plugin-opener";
import { externalReleaseNotesUrl, renderReleaseNotes } from "../../utils/releaseNotes";

const props = defineProps<{ content: string }>();
const rendered = computed(() => renderReleaseNotes(props.content));

async function openLink(event: MouseEvent) {
  const target = event.target;
  if (!(target instanceof Element)) return;
  const link = target.closest("a");
  if (!link) return;

  event.preventDefault();
  if (event.type === "auxclick" && event.button !== 1) return;
  const url = externalReleaseNotesUrl(link.getAttribute("href") ?? "");
  if (!url) return;

  try {
    await openUrl(url);
  } catch {
    window.open(url, "_blank", "noopener,noreferrer");
  }
}
</script>

<template>
  <div
    class="release-notes"
    @click="openLink"
    @auxclick="openLink"
    v-html="rendered"
  />
</template>

<style scoped>
.release-notes {
  min-width: 0;
  color: var(--color-text-primary);
  font-size: 13px;
  line-height: 1.65;
  overflow-wrap: anywhere;
}

.release-notes :deep(> :first-child) {
  margin-top: 0;
}

.release-notes :deep(> :last-child) {
  margin-bottom: 0;
}

.release-notes :deep(h1),
.release-notes :deep(h2),
.release-notes :deep(h3),
.release-notes :deep(h4),
.release-notes :deep(h5),
.release-notes :deep(h6) {
  margin: 1.3em 0 0.55em;
  line-height: 1.35;
  font-weight: 600;
}

.release-notes :deep(h1) { font-size: 1.5em; }
.release-notes :deep(h2) { font-size: 1.3em; }
.release-notes :deep(h3) { font-size: 1.15em; }
.release-notes :deep(h4),
.release-notes :deep(h5),
.release-notes :deep(h6) { font-size: 1em; }

.release-notes :deep(p),
.release-notes :deep(ul),
.release-notes :deep(ol),
.release-notes :deep(blockquote),
.release-notes :deep(pre),
.release-notes :deep(table) {
  margin: 0 0 0.9em;
}

.release-notes :deep(ul),
.release-notes :deep(ol) {
  padding-left: 1.6em;
}

.release-notes :deep(li + li) {
  margin-top: 0.35em;
}

.release-notes :deep(li > ul),
.release-notes :deep(li > ol) {
  margin-top: 0.35em;
  margin-bottom: 0;
}

.release-notes :deep(a[href]) {
  color: var(--color-primary);
  text-decoration: underline;
  text-underline-offset: 2px;
}

.release-notes :deep(a[href]:focus-visible) {
  outline: 2px solid var(--color-primary);
  outline-offset: 2px;
  border-radius: 2px;
}

.release-notes :deep(code) {
  padding: 0.15em 0.35em;
  border-radius: var(--border-radius-sm);
  background: var(--color-surface-hover);
  font-family: var(--font-family-mono);
  font-size: 0.9em;
}

.release-notes :deep(pre) {
  max-width: 100%;
  overflow-x: auto;
  padding: 12px;
  border: 1px solid var(--color-border);
  border-radius: var(--border-radius-md);
  background: var(--color-surface-hover);
  white-space: pre;
  overflow-wrap: normal;
}

.release-notes :deep(pre code) {
  padding: 0;
  background: transparent;
}

.release-notes :deep(blockquote) {
  padding-left: 12px;
  margin-left: 0;
  margin-right: 0;
  border-left: 3px solid var(--color-border-strong);
  color: var(--color-text-secondary);
}

.release-notes :deep(table) {
  display: block;
  max-width: 100%;
  overflow-x: auto;
  border-collapse: collapse;
}

.release-notes :deep(th),
.release-notes :deep(td) {
  padding: 7px 10px;
  border: 1px solid var(--color-border);
  text-align: left;
}

.release-notes :deep(th) {
  background: var(--color-table-header);
  font-weight: 600;
}

.release-notes :deep(hr) {
  margin: 1.2em 0;
  border: 0;
  border-top: 1px solid var(--color-border);
}
</style>
