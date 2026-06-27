import { open } from "@tauri-apps/plugin-dialog";

function pickFileViaInput(): Promise<string | null> {
  return new Promise((resolve) => {
    const input = document.createElement("input");
    input.type = "file";
    input.style.display = "none";
    input.onchange = () => {
      const file = input.files?.[0];
      if (!file) {
        resolve(null);
        return;
      }
      resolve((file as File & { path?: string }).path ?? file.name);
      input.remove();
    };
    document.body.appendChild(input);
    input.click();
  });
}

export async function pickFile(title?: string): Promise<string | null> {
  try {
    const selected = await open({
      multiple: false,
      directory: false,
      title,
    });
    if (typeof selected === "string") return selected;
    return null;
  } catch {
    return pickFileViaInput();
  }
}
