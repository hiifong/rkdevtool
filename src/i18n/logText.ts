import en from "./messages/en";

type MessageTree = Record<string, unknown>;

function lookup(tree: MessageTree, key: string): string | undefined {
  const parts = key.split(".");
  let node: unknown = tree;
  for (const part of parts) {
    if (node == null || typeof node !== "object" || !(part in node)) return undefined;
    node = (node as Record<string, unknown>)[part];
  }
  return typeof node === "string" ? node : undefined;
}

function applyParams(text: string, params?: Record<string, string>) {
  if (!params) return text;
  return Object.entries(params).reduce(
    (result, [name, value]) => result.replace(new RegExp(`\\{${name}\\}`, "g"), value),
    text,
  );
}

/** Log panel messages are always English regardless of UI locale. */
export function logText(key: string, params?: Record<string, string>): string {
  const text = lookup(en, key) ?? key;
  return applyParams(text, params);
}
