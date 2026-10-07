import { ref, watch } from "vue";

const STORAGE_KEY = "rkdevtool-upgrade-form";

function restoreFirmwarePath(): string {
  try {
    const saved: unknown = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "null");
    if (saved !== null && typeof saved === "object") {
      const form = saved as Record<string, unknown>;
      if (form.version === 1 && typeof form.firmwarePath === "string") {
        return form.firmwarePath;
      }
    }
  } catch {
    // An unavailable store or invalid saved data must not prevent opening the page.
  }
  return "";
}

export function useUpgradeForm() {
  const firmwarePath = ref(restoreFirmwarePath());

  watch(
    firmwarePath,
    (path) => {
      try {
        localStorage.setItem(STORAGE_KEY, JSON.stringify({ version: 1, firmwarePath: path }));
      } catch (error) {
        console.warn("Failed to save upgrade form", error);
      }
    },
    { flush: "sync" },
  );

  return { firmwarePath };
}
