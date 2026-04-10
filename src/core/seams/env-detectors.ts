/**
 * Environment variable access detectors — wrapper over the
 * externalized detector graph.
 *
 * The actual detector declarations live in `detectors/detectors.toml`
 * and the runtime walker in `detectors/walker.ts`. This file is the
 * thin compatibility wrapper that preserves the legacy public API
 * (`detectEnvAccesses(content, filePath)`) so callers do not need
 * to change.
 *
 * Behavior:
 *   1. Comment-mask the source via `maskCommentsForFile` (unchanged)
 *   2. Map the file extension to a `DetectorLanguage`
 *   3. Dispatch to the generic walker with the production graph
 *
 * Byte-identical to the legacy hand-rolled detectors against the
 * existing test corpus. The walker + hook registry preserve all
 * legacy semantics (per-language pattern order, dedup-by-var-and-file,
 * single-match-per-line for destructure, etc.).
 *
 * No filesystem access here — the production graph is loaded once
 * at module-import time by `production-graph.ts`. Callers see no
 * I/O surface.
 */

import { maskCommentsForFile } from "./comment-masker.js";
import {
	PRODUCTION_DETECTOR_GRAPH,
	PRODUCTION_HOOK_REGISTRY,
} from "./detectors/production-graph.js";
import type { DetectorLanguage } from "./detectors/types.js";
import { walkEnvDetectors } from "./detectors/walker.js";
import type { DetectedEnvDependency } from "./env-dependency.js";

/**
 * Detect env var accesses in a source file.
 *
 * Public API preserved from the legacy implementation. The legacy
 * per-language detector functions (detectJsEnvAccesses, etc.) have
 * been replaced by walker dispatch over the externalized graph.
 */
export function detectEnvAccesses(
	content: string,
	filePath: string,
): DetectedEnvDependency[] {
	const language = languageFromExtension(filePath);
	if (language === null) return [];
	const masked = maskCommentsForFile(filePath, content);
	return walkEnvDetectors(
		PRODUCTION_DETECTOR_GRAPH,
		PRODUCTION_HOOK_REGISTRY,
		language,
		masked,
		filePath,
	);
}

/**
 * Map a file extension to its DetectorLanguage bucket.
 *
 * Mirrors the legacy detector dispatch table:
 *   .ts/.tsx/.js/.jsx → "ts" (the JS/TS family)
 *   .py               → "py"
 *   .rs               → "rs"
 *   .java             → "java"
 *   .c/.h/.cpp/.hpp/.cc/.cxx → "c"
 *   anything else     → null (no detection)
 *
 * Extension comparison is case-sensitive, matching the legacy
 * behavior exactly. Files with no extension or unknown extensions
 * return null and the walker is never called.
 */
function languageFromExtension(filePath: string): DetectorLanguage | null {
	const dot = filePath.lastIndexOf(".");
	if (dot < 0) return null;
	const ext = filePath.slice(dot);
	switch (ext) {
		case ".ts":
		case ".tsx":
		case ".js":
		case ".jsx":
			return "ts";
		case ".py":
			return "py";
		case ".rs":
			return "rs";
		case ".java":
			return "java";
		case ".c":
		case ".h":
		case ".cpp":
		case ".hpp":
		case ".cc":
		case ".cxx":
			return "c";
		default:
			return null;
	}
}
