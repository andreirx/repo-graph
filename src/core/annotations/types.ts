/**
 * Provisional annotations — core types.
 *
 * Normative contract: docs/architecture/annotations-contract.txt.
 *
 * Annotations are PROVISIONAL author claims extracted from source
 * artifacts (JSDoc blocks, package descriptions, file headers,
 * READMEs). They are NOT structural truth. They MUST NOT be read
 * by policy, evaluator, gate, dead-code, trust, or change-impact
 * surfaces. See the isolation invariant in the contract.
 */

/**
 * Kind of the target an annotation attaches to.
 * Frozen enum; adding values is non-breaking, removing is breaking.
 */
export const AnnotationTargetKind = {
	SYMBOL: "symbol",
	FILE: "file",
	MODULE: "module",
	REPO: "repo",
} as const;

export type AnnotationTargetKind =
	(typeof AnnotationTargetKind)[keyof typeof AnnotationTargetKind];

/**
 * Kind of the annotation itself — which source artifact produced it.
 * Frozen enum in v1; extending is additive.
 */
export const AnnotationKind = {
	JSDOC_BLOCK: "jsdoc_block",
	FILE_HEADER_COMMENT: "file_header_comment",
	MODULE_README: "module_readme",
	PACKAGE_DESCRIPTION: "package_description",
} as const;

export type AnnotationKind =
	(typeof AnnotationKind)[keyof typeof AnnotationKind];

/**
 * The deterministic rendering order for annotations in output.
 * Contract output-ordering rule 2 in annotations-contract.txt.
 */
export const ANNOTATION_KIND_ORDER: readonly AnnotationKind[] = Object.freeze(
	[
		AnnotationKind.PACKAGE_DESCRIPTION,
		AnnotationKind.MODULE_README,
		AnnotationKind.FILE_HEADER_COMMENT,
		AnnotationKind.JSDOC_BLOCK,
	],
);

/**
 * Contract class of an annotation. Output-only rendering discriminator.
 *   HINT     — orientation material; do not treat as verified
 *   CITATION — exact author claim with source location (reserved for v2)
 *
 * `EVIDENCE` is forbidden — annotations are never verified facts.
 * In v1 all annotations are stored with contract_class=HINT.
 */
export const AnnotationContractClass = {
	HINT: "HINT",
	CITATION: "CITATION",
} as const;

export type AnnotationContractClass =
	(typeof AnnotationContractClass)[keyof typeof AnnotationContractClass];

/**
 * Stored annotation — one row in the annotations table.
 *
 * `provisional` is always true in v1 and reserved for future
 * annotation classes that may be promoted via human declaration.
 *
 * `language` describes the SOURCE ARTIFACT (e.g. "typescript" for
 * JSDoc in .ts, "markdown" for README.md, "json" for package.json).
 * It is NOT the natural language of the prose content.
 */
export interface Annotation {
	annotation_uid: string;
	snapshot_uid: string;
	target_kind: AnnotationTargetKind;
	target_stable_key: string;
	annotation_kind: AnnotationKind;
	contract_class: AnnotationContractClass;
	content: string;
	content_hash: string;
	source_file: string;
	source_line_start: number;
	source_line_end: number;
	language: string;
	provisional: boolean;
	extracted_at: string;
}

/**
 * Suffix appended to content that exceeds the 8 KB size cap.
 * Contract §4 content normalization rule 5.
 */
export const CONTENT_TRUNCATION_MARKER = "\n...[TRUNCATED]";

/**
 * Hard cap on stored annotation content length (bytes).
 * Contract §4 content normalization rule 5.
 */
export const CONTENT_MAX_BYTES = 8 * 1024;

/**
 * Boilerplate-detection regexes applied to file_header_comment only.
 * Contract §4 boilerplate filter.
 *
 * If ANY of these match the first 500 characters of the normalized
 * content, the annotation is dropped.
 */
export const BOILERPLATE_REGEXES: readonly RegExp[] = Object.freeze([
	/copyright/i,
	/license/i,
	/spdx/i,
]);

/**
 * Window of content examined by the boilerplate filter.
 */
export const BOILERPLATE_WINDOW_CHARS = 500;

/**
 * Query resolution result for `rgr docs <target>`.
 *
 * See contract §6 target resolution rules. A query either resolves
 * to exactly one target or returns a resolution_error describing
 * why it failed.
 */
export type TargetResolution =
	| { kind: "resolved"; targetStableKey: string }
	| { kind: "not_found" }
	| {
			kind: "ambiguous";
			step: "path" | "module_name";
			candidates: string[];
	  };

/**
 * Canonical resolution-error codes emitted in the docs command
 * output envelope when the query fails. Stable strings, not prose.
 */
export const ResolutionErrorCode = {
	NOT_FOUND: "not_found",
	AMBIGUOUS_AT_STEP_PATH: "ambiguous_at_step_2",
	AMBIGUOUS_AT_STEP_MODULE_NAME: "ambiguous_at_step_3",
} as const;

export type ResolutionErrorCode =
	(typeof ResolutionErrorCode)[keyof typeof ResolutionErrorCode];
