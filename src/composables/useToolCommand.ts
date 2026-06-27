import { ref } from "vue";
import * as toolApi from "../api/tool";

const busy = ref(false);

function extractError(err: unknown): string {
  if (typeof err === "string") return err;
  if (err instanceof Error) return err.message;
  return String(err);
}

export function useToolCommand() {
  async function run<T>(task: () => Promise<T>, label?: string): Promise<T | null> {
    if (busy.value) return null;
    busy.value = true;
    try {
      return await task();
    } catch (err) {
      throw new Error(label ? `${label}: ${extractError(err)}` : extractError(err));
    } finally {
      busy.value = false;
    }
  }

  return { busy, run };
}

export { toolApi };
