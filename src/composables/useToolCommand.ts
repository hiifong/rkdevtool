import * as toolApi from "../api/tool";
import { useAppState } from "./useAppState";

function extractError(err: unknown): string {
  if (typeof err === "string") return err;
  if (err instanceof Error) return err.message;
  return String(err);
}

export function useToolCommand() {
  const { busy, setBusy } = useAppState();

  async function run<T>(task: () => Promise<T>, label?: string): Promise<T | null> {
    if (busy.value) return null;
    setBusy(true);
    try {
      return await task();
    } catch (err) {
      throw new Error(label ? `${label}: ${extractError(err)}` : extractError(err));
    } finally {
      setBusy(false);
    }
  }

  return { busy, run };
}

export { toolApi };
