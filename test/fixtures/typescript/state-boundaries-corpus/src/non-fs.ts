// Non-FS import - should NOT produce state-boundary edges
import { join } from "path";
import { createHash } from "crypto";

export function buildPath() {
  return join("/base", "sub", "file.txt");
}

export function hashData(data: string) {
  return createHash("sha256").update(data).digest("hex");
}
