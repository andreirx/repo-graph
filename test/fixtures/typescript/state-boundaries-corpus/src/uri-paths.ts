// URI-shaped FS paths (file:// URLs)
import { readFile } from "fs";

export function loadFromUri() {
  // file:// URL should emit NormalizedUrl evidence
  readFile("file:///etc/ssl/certs/ca-bundle.crt", () => {});
}

export function loadWindowsPath() {
  // Windows path with drive letter - stays NormalizedPath
  readFile("C:\\Windows\\System32\\config", () => {});
}
