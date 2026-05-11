// FS write via named import from "fs"
import { writeFile } from "fs";

export function saveConfig() {
  writeFile("/var/log/app.log", "data", () => {});
}
