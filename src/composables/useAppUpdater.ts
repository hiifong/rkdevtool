import { onUnmounted, ref } from "vue";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { message } from "@tauri-apps/plugin-dialog";
import { useI18n } from "../i18n";

export function useAppUpdater() {
  const { t } = useI18n();
  const checking = ref(false);
  const updating = ref(false);
  const progressText = ref("");
  const updatePrompt = ref<{ version: string; notes: string } | null>(null);
  let pendingConfirmation: ((confirmed: boolean) => void) | null = null;
  let activeUpdate: Update | null = null;
  let disposed = false;

  function respondToUpdate(confirmed: boolean) {
    const resolve = pendingConfirmation;
    pendingConfirmation = null;
    resolve?.(confirmed);
  }

  async function closeUpdate() {
    const update = activeUpdate;
    activeUpdate = null;
    try {
      await update?.close();
    } catch {
      // Resource cleanup must not replace an update or installation error.
    }
  }

  onUnmounted(() => {
    disposed = true;
    respondToUpdate(false);
    void closeUpdate();
  });

  async function checkForUpdates({ silent = false }: { silent?: boolean } = {}) {
    if (disposed || checking.value || updating.value) return;

    checking.value = true;
    progressText.value = "";
    let userConfirmed = false;

    try {
      const update = await check();
      activeUpdate = update;
      if (disposed) return;
      if (!update) {
        if (!silent) {
          await message(t("update.upToDate"), {
            title: t("update.title"),
            kind: "info",
          });
        }
        return;
      }

      userConfirmed = await new Promise<boolean>((resolve) => {
        pendingConfirmation = resolve;
        updatePrompt.value = {
          version: update.version,
          notes: update.body?.trim() ?? "",
        };
      });
      updatePrompt.value = null;
      if (!userConfirmed || disposed) return;

      updating.value = true;
      let downloaded = 0;
      let contentLength = 0;

      await update.downloadAndInstall((event) => {
        if (disposed) return;
        switch (event.event) {
          case "Started":
            contentLength = event.data.contentLength ?? 0;
            progressText.value = t("update.downloading", { percent: "0" });
            break;
          case "Progress":
            downloaded += event.data.chunkLength;
            if (contentLength > 0) {
              const percent = Math.min(100, Math.round((downloaded / contentLength) * 100));
              progressText.value = t("update.downloading", {
                percent: String(percent),
              });
            } else {
              progressText.value = t("update.downloadingBytes", {
                bytes: String(downloaded),
              });
            }
            break;
          case "Finished":
            progressText.value = t("update.installing");
            break;
        }
      });

      if (!disposed) await relaunch();
    } catch (error) {
      if (!disposed && (!silent || userConfirmed)) {
        const detail = error instanceof Error ? error.message : String(error);
        await message(t("update.failed", { error: detail }), {
          title: t("update.title"),
          kind: "error",
        });
      }
    } finally {
      respondToUpdate(false);
      updatePrompt.value = null;
      const closing = closeUpdate();
      checking.value = false;
      updating.value = false;
      await closing;
    }
  }

  return {
    checking,
    updating,
    progressText,
    updatePrompt,
    respondToUpdate,
    checkForUpdates,
  };
}
