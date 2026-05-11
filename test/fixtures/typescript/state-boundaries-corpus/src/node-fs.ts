// FS via "node:fs" module specifier
import { readFile, writeFile } from "node:fs";

export function loadData() {
  readFile("/data/input.json", () => {});
}

export function saveData() {
  writeFile("/data/output.json", "result", () => {});
}
