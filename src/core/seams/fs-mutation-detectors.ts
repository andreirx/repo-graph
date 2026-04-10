/**
 * Filesystem mutation detectors — wrapper over the externalized
 * detector graph.
 *
 * The actual detector declarations live in `detectors/detectors.toml`
 * and the runtime walker in `detectors/walker.ts`. This file is the
 * thin compatibility wrapper that preserves the legacy public API
 * (`detectFsMutations(content, filePath)`) so callers do not need
 * to change.
 *
 * Behavior:
 *   1. Comment-mask the source via `maskCommentsForFile` (unchanged)
 *   2. Map the file extension to a `DetectorLanguage`
 *   3. Dispatch to the generic walker with the production graph
 *
 * Byte-identical to the legacy hand-rolled detectors against the
 * existing test corpus. Position-overlap dedup is preserved via
 * `position_dedup_group` declarations on the Rust fs records;
 * single-match-per-line semantics are preserved via the
 * `single_match_per_line` flag on the affected records.
 */

import { maskCommentsForFile } from "./comment-masker.js";
import {
	PRODUCTION_DETECTOR_GRAPH,
	PRODUCTION_HOOK_REGISTRY,
} from "./detectors/production-graph.js";
import type { DetectorLanguage } from "./detectors/types.js";
import { walkFsDetectors } from "./detectors/walker.js";
import type { DetectedFsMutation } from "./fs-mutation.js";

/**
 * Detect filesystem mutation occurrences in a source file.
 *
 * Public API preserved from the legacy implementation. The legacy
 * per-language detector functions (detectJsMutations, etc.) have
 * been replaced by walker dispatch over the externalized graph.
 */
export function detectFsMutations(
	content: string,
	filePath: string,
): DetectedFsMutation[] {
	const language = languageFromExtension(filePath);
	if (language === null) return [];
	const masked = maskCommentsForFile(filePath, content);
	return walkFsDetectors(
		PRODUCTION_DETECTOR_GRAPH,
		PRODUCTION_HOOK_REGISTRY,
		language,
		masked,
		filePath,
	);
}

/** See env-detectors.ts for the matching mapping policy. */
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
