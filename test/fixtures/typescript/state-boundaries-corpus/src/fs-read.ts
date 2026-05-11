// FS read via named import from "fs"
import { readFile } from "fs";

export function loadConfig() {
  readFile("/etc/app.yaml", () => {});
}

export function loadMultiple() {
  readFile("/etc/db.conf", () => {});
  readFile("/etc/cache.conf", () => {});
}
