/**
 * Provisional annotations — attribution rules.
 *
 * Pure functions that map extracted source artifacts to exactly one
 * target stable_key. Normative contract:
 * docs/architecture/annotations-contract.txt.
 *
 * All functions in this module are PURE. No I/O, no storage access,
 * no filesystem calls. Inputs are plain data; outputs are
 * attribution decisions or null (drop).
 *
 * Attribution is deterministic. Two runs with the same inputs MUST
 * produce identical outputs.
 */

import {
	BOILERPLATE_REGEXES,
	BOILERPLATE_WINDOW_CHARS,
	CONTENT_MAX_BYTES,
	CONTENT_TRUNCATION_MARKER,
	type AnnotationKind,
	type AnnotationTargetKind,
} from "./types.js";

// ── Content normalization ───────────────────────────────────────────

/**
 * Strip JSDoc/TSDoc decorations from a raw comment block.
 *
 * Removes `/**`, ` * ` (leading on each line), and trailing `*\/`.
 * Preserves interior line breaks and markdown formatting.
 *
 * Safe on non-JSDoc inputs: if the block doesn't start with `/**`,
 * returns the input with only leading/trailing `/* ... *\/` stripped.
 */
export function stripJsdocDecorations(raw: string): string {
	// Strip /* or /** opener and */ closer first
	let body = raw.trim();
	if (body.startsWith("/**")) {
		body = body.slice(3);
	} else if (body.startsWith("/*")) {
		body = body.slice(2);
	}
	if (body.endsWith("*/")) {
		body = body.slice(0, -2);
	}
	// Strip leading ` * ` or ` *` per line
	const lines = body.split("\n");
	const stripped = lines.map((line) => {
		// Match leading whitespace then ` *`, optionally followed by one space
		const m = line.match(/^\s*\*\s?(.*)$/);
		return m ? m[1] : line;
	});
	return stripped.join("\n").trim();
}

/**
 * Strip line-comment decorations from a block of consecutive `//` lines.
 * Removes the `//` prefix and one optional following space per line.
 */
export function stripLineCommentDecorations(raw: string): string {
	const lines = raw.split("\n");
	const stripped = lines.map((line) => {
		const m = line.match(/^\s*\/\/\s?(.*)$/);
		return m ? m[1] : line;
	});
	return stripped.join("\n").trim();
}

/**
 * Truncate content to the byte cap, appending the truncation marker
 * if the cap was exceeded.
 */
export function applyContentTruncation(content: string): string {
	// Measure in bytes (UTF-8) to match storage-layer cap semantics
	const bytes = Buffer.byteLength(content, "utf-8");
	if (bytes <= CONTENT_MAX_BYTES) return content;
	// Truncate conservatively: slice by chars until under the cap,
	// leaving room for the marker
	const markerBytes = Buffer.byteLength(CONTENT_TRUNCATION_MARKER, "utf-8");
	const budget = CONTENT_MAX_BYTES - markerBytes;
	// Binary-safe-ish truncation by repeatedly halving
	let lo = 0;
	let hi = content.length;
	while (lo < hi) {
		const mid = Math.floor((lo + hi + 1) / 2);
		const size = Buffer.byteLength(content.slice(0, mid), "utf-8");
		if (size <= budget) lo = mid;
		else hi = mid - 1;
	}
	return content.slice(0, lo) + CONTENT_TRUNCATION_MARKER;
}

/**
 * Determine whether the content should be DROPPED on emptiness grounds.
 * Empty / whitespace-only → true (drop). Anything else → false (keep).
 *
 * Contract §4 rule 3: the ONLY universal drop rule. No length
 * triviality filter is applied.
 */
export function isEmptyContent(content: string): boolean {
	return content.trim().length === 0;
}

/**
 * Determine whether file_header_comment content is boilerplate (license
 * notice, copyright header, SPDX identifier) and should be dropped.
 *
 * Applies ONLY to file_header_comment. Callers for other kinds
 * should not invoke this function. Contract §4 boilerplate filter.
 */
export function isFileHeaderBoilerplate(content: string): boolean {
	const window = content.slice(0, BOILERPLATE_WINDOW_CHARS);
	for (const re of BOILERPLATE_REGEXES) {
		if (re.test(window)) return true;
	}
	return false;
}

// ── Target attribution ─────────────────────────────────────────────

/**
 * Input for attributing an exported-symbol JSDoc block.
 *
 * A declaration is "exported" in v1 iff it has a literal `export`
 * keyword modifier. Re-exports are NOT carried. This is documented
 * incompleteness; extractor callers must have already verified the
 * export keyword before calling this function.
 */
export interface JsdocAttributionInput {
	/** True iff the declaration has an `export` keyword modifier. */
	declarationExported: boolean;
	/** Stable key of the declared symbol (e.g. `<repo>:path.ts#Name:SYMBOL:FUNCTION`). */
	symbolStableKey: string;
}

export interface AttributionDecision {
	target_kind: AnnotationTargetKind;
	target_stable_key: string;
}

/**
 * Attribute a JSDoc block to its exported symbol.
 * Returns null if the declaration is not exported (drop).
 */
export function attributeJsdoc(
	input: JsdocAttributionInput,
): AttributionDecision | null {
	if (!input.declarationExported) return null;
	return {
		target_kind: "symbol",
		target_stable_key: input.symbolStableKey,
	};
}

/**
 * Input for attributing a package.json `description` field.
 */
export interface PackageDescriptionAttributionInput {
	/** True iff the package.json lives at the repo root. */
	isRepoRoot: boolean;
	/** Stable key of the REPO (used when isRepoRoot). */
	repoStableKey: string;
	/**
	 * Stable key of the owning MODULE for the directory containing
	 * the package.json, OR null if no MODULE exists for that dir.
	 * If null AND not repo root, the annotation is dropped.
	 */
	owningModuleStableKey: string | null;
}

export function attributePackageDescription(
	input: PackageDescriptionAttributionInput,
): AttributionDecision | null {
	if (input.isRepoRoot) {
		return {
			target_kind: "repo",
			target_stable_key: input.repoStableKey,
		};
	}
	if (input.owningModuleStableKey === null) return null;
	return {
		target_kind: "module",
		target_stable_key: input.owningModuleStableKey,
	};
}

/**
 * Input for attributing a file-header comment.
 *
 * A file-header comment is the FIRST comment in the file, preceding
 * any code. The extractor is responsible for identifying it;
 * attribution just produces the target.
 */
export interface FileHeaderAttributionInput {
	/** Stable key of the FILE node. */
	fileStableKey: string;
}

export function attributeFileHeader(
	input: FileHeaderAttributionInput,
): AttributionDecision {
	return {
		target_kind: "file",
		target_stable_key: input.fileStableKey,
	};
}

/**
 * Input for attributing a README file.
 */
export interface ReadmeAttributionInput {
	/** True iff the README lives at the repo root. */
	isRepoRoot: boolean;
	/** Stable key of the REPO (used when isRepoRoot). */
	repoStableKey: string;
	/**
	 * Stable key of the MODULE owning the directory containing the
	 * README, OR null if no MODULE exists for that dir. Drop if null
	 * AND not repo root.
	 */
	owningModuleStableKey: string | null;
}

export function attributeReadme(
	input: ReadmeAttributionInput,
): AttributionDecision | null {
	if (input.isRepoRoot) {
		return {
			target_kind: "repo",
			target_stable_key: input.repoStableKey,
		};
	}
	if (input.owningModuleStableKey === null) return null;
	return {
		target_kind: "module",
		target_stable_key: input.owningModuleStableKey,
	};
}

/**
 * Preferred-README tiebreaker. When both README.md and README.txt
 * exist in the same directory, .md wins and .txt is dropped.
 *
 * Returns the filename that should be extracted, or null if no
 * README file is present.
 */
export function preferReadmeFile(
	presentFilenames: string[],
): string | null {
	const lower = presentFilenames.map((f) => f.toLowerCase());
	if (lower.includes("readme.md")) {
		// Return the original-cased filename if present
		const idx = lower.indexOf("readme.md");
		return presentFilenames[idx];
	}
	if (lower.includes("readme.txt")) {
		const idx = lower.indexOf("readme.txt");
		return presentFilenames[idx];
	}
	return null;
}

// ── Collision resolution ───────────────────────────────────────────

/**
 * Deterministic collision winner selection.
 *
 * When multiple annotations of the SAME kind target the SAME
 * stable_key, the winner is the one with the lowest
 * source_line_start. Ties are broken by source_file alphabetical
 * then source_line_end ascending for absolute determinism.
 *
 * Returns the indices (into the input array) of annotations to KEEP.
 * Dropped count = input.length - keptIndices.length.
 */
export function resolveCollisions<
	T extends {
		target_kind: AnnotationTargetKind;
		target_stable_key: string;
		annotation_kind: AnnotationKind;
		source_file: string;
		source_line_start: number;
		source_line_end: number;
	},
>(candidates: T[]): { keptIndices: number[]; droppedCount: number } {
	// Group by (target_kind, target_stable_key, annotation_kind)
	const groups = new Map<string, number[]>();
	for (let i = 0; i < candidates.length; i++) {
		const c = candidates[i];
		const key = `${c.target_kind}\t${c.target_stable_key}\t${c.annotation_kind}`;
		const arr = groups.get(key) ?? [];
		arr.push(i);
		groups.set(key, arr);
	}

	const keptIndices: number[] = [];
	let droppedCount = 0;
	for (const indices of groups.values()) {
		if (indices.length === 1) {
			keptIndices.push(indices[0]);
			continue;
		}
		// Sort to find the deterministic winner
		indices.sort((a, b) => {
			const ca = candidates[a];
			const cb = candidates[b];
			if (ca.source_line_start !== cb.source_line_start) {
				return ca.source_line_start - cb.source_line_start;
			}
			if (ca.source_file !== cb.source_file) {
				return ca.source_file < cb.source_file ? -1 : 1;
			}
			return ca.source_line_end - cb.source_line_end;
		});
		keptIndices.push(indices[0]);
		droppedCount += indices.length - 1;
	}
	keptIndices.sort((a, b) => a - b);
	return { keptIndices, droppedCount };
}
