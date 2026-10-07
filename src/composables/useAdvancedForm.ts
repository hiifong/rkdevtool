import { reactive, toRefs, watch } from "vue";

const STORAGE_KEY = "rkdevtool-advanced-form";

interface AdvancedForm {
  version: 1;
  bootPath: string;
  firmwarePath: string;
}

function restoreForm(): AdvancedForm {
  try {
    const saved: unknown = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "null");
    if (saved !== null && typeof saved === "object") {
      const form = saved as Record<string, unknown>;
      if (
        form.version === 1
        && typeof form.bootPath === "string"
        && typeof form.firmwarePath === "string"
      ) {
        return {
          version: 1,
          bootPath: form.bootPath,
          firmwarePath: form.firmwarePath,
        };
      }
    }
  } catch {
    // An unavailable store or invalid saved data must not prevent opening the page.
  }
  return { version: 1, bootPath: "", firmwarePath: "" };
}

export function useAdvancedForm() {
  const form = reactive(restoreForm());

  watch(
    form,
    () => {
      try {
        localStorage.setItem(STORAGE_KEY, JSON.stringify(form));
      } catch (error) {
        console.warn("Failed to save advanced form", error);
      }
    },
    { flush: "sync" },
  );

  const { bootPath, firmwarePath } = toRefs(form);
  return { bootPath, firmwarePath };
}
