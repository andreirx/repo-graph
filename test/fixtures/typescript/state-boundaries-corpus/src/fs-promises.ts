// FS via "node:fs/promises" async API
import { readFile, writeFile } from "node:fs/promises";

export async function loadAsync() {
  const data = await readFile("/async/config.yaml", "utf-8");
  return data;
}

export async function saveAsync() {
  await writeFile("/async/output.txt", "async result");
}
