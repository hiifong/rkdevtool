import { reactive, toRefs, watch } from "vue";
import type { PartitionRow, StorageType } from "../types/app";
import type { DevicePartitionTable } from "../types/tool";

const STORAGE_KEY = "rkdevtool-download-form";

export const downloadStorageOptions: StorageType[] = [
  "",
  "FLASH",
  "EMMC",
  "SD",
  "SPINOR",
  "SPINAND",
  "SATA",
  "PCIE",
];

interface DownloadForm {
  version: 1;
  rows: PartitionRow[];
  forceByAddress: boolean;
  selectedRowId: number | null;
}

function defaultForm(): DownloadForm {
  return {
    version: 1,
    rows: [
      {
        id: 1,
        enabled: true,
        storage: "",
        address: "0x00000000",
        name: "Loader",
        path: "",
      },
    ],
    forceByAddress: false,
    selectedRowId: 1,
  };
}

function isPartitionRow(value: unknown): value is PartitionRow {
  if (value === null || typeof value !== "object") return false;
  const row = value as Record<string, unknown>;
  return (
    typeof row.id === "number"
    && Number.isSafeInteger(row.id)
    && row.id > 0
    && typeof row.enabled === "boolean"
    && downloadStorageOptions.some((storage) => storage === row.storage)
    && typeof row.address === "string"
    && typeof row.name === "string"
    && typeof row.path === "string"
  );
}

function restoreForm(): DownloadForm {
  try {
    const saved: unknown = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "null");
    if (saved === null || typeof saved !== "object") return defaultForm();
    const form = saved as Record<string, unknown>;
    if (
      form.version !== 1
      || !Array.isArray(form.rows)
      || !form.rows.every(isPartitionRow)
      || new Set(form.rows.map((row) => row.id)).size !== form.rows.length
      || typeof form.forceByAddress !== "boolean"
    ) {
      return defaultForm();
    }
    return {
      version: 1,
      rows: form.rows,
      forceByAddress: form.forceByAddress,
      selectedRowId: form.rows.find((row) => row.id === form.selectedRowId)?.id
        ?? form.rows[0]?.id
        ?? null,
    };
  } catch {
    return defaultForm();
  }
}

export function useDownloadForm() {
  const form = reactive(restoreForm());
  let nextId = form.rows.reduce((max, row) => Math.max(max, row.id), 0) + 1;

  watch(
    form,
    () => {
      try {
        localStorage.setItem(STORAGE_KEY, JSON.stringify(form));
      } catch (error) {
        console.warn("Failed to save download form", error);
      }
    },
    { flush: "sync" },
  );

  function addRow() {
    const id = nextId++;
    form.rows.push({
      id,
      enabled: true,
      storage: "",
      address: "0x00000000",
      name: "",
      path: "",
    });
    form.selectedRowId ??= id;
  }

  function clearRows() {
    form.rows = [];
    form.selectedRowId = null;
    nextId = 1;
  }

  function removeRow(id: number) {
    const index = form.rows.findIndex((row) => row.id === id);
    if (index === -1) return;
    form.rows.splice(index, 1);
    if (form.selectedRowId === id) {
      form.selectedRowId = form.rows[index]?.id ?? form.rows[index - 1]?.id ?? null;
    }
  }

  function fillFromPartitionTable(table: DevicePartitionTable) {
    if (table.partitions.length === 0) return;
    const storage = downloadStorageOptions.find((option) => option === table.storage?.name) ?? "";
    const updated = new Map<number, PartitionRow>();
    const added: PartitionRow[] = [];

    for (const partition of table.partitions) {
      const name = partition.name.trim().toLowerCase();
      const matches = form.rows.filter((row) =>
        row.name.trim().toLowerCase() !== "loader"
        && row.name.trim().toLowerCase() === name
        && !updated.has(row.id),
      );
      const existing = matches.find((row) => row.storage === storage)
        ?? matches.find((row) => row.storage === "");
      const row: PartitionRow = {
        id: existing?.id ?? nextId++,
        enabled: existing?.enabled ?? true,
        storage: storage || existing?.storage || "",
        address: partition.address,
        name: partition.name,
        path: existing?.path ?? "",
      };
      if (existing) updated.set(existing.id, row);
      else added.push(row);
    }

    form.rows = [...form.rows.map((row) => updated.get(row.id) ?? row), ...added];
    form.selectedRowId ??= form.rows[0]?.id ?? null;
  }

  const { rows, forceByAddress, selectedRowId } = toRefs(form);
  return { rows, forceByAddress, selectedRowId, addRow, removeRow, clearRows, fillFromPartitionTable };
}
