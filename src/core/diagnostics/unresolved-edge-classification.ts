/**
 * Classification vocabulary for unresolved edges.
 *
 * This file holds THREE related vocabularies, all orthogonal to the
 * extraction failure taxonomy in `unresolved-edge-categories.ts`:
 *
 *   1. UnresolvedEdgeClassification — semantic meaning of the gap.
 *      "Where does this unresolved reference POINT to?"
 *      {external_library_candidate, internal_candidate, unknown}
 *
 *   2. UnresolvedEdgeBasisCode — the specific rule that produced a
 *      classification. Makes each classification verdict auditable
 *      without re-running the classifier.
 *
 *   3. CURRENT_CLASSIFIER_VERSION — integer stamp of the rule set
 *      that produced a classification. Rows carrying an older
 *      version than CURRENT_CLASSIFIER_VERSION are eligible for
 *      backfill when that slice is introduced.
 *
 * Orthogonality with UnresolvedEdgeCategory:
 *
 *   Category         = extraction failure mode (why the extractor gave up)
 *   Classification   = semantic bucket (what the gap most likely means)
 *   BasisCode        = classifier reason (which rule matched)
 *
 * Name collision to be aware of:
 *
 *   UnresolvedEdgeCategory.OTHER
 *       "extraction failure mode not captured by existing categories"
 *   UnresolvedEdgeClassification.UNKNOWN
 *       "no classifier signal supported any semantic bucket"
 *
 * These are different concepts on different axes. Kept distinct on
 * purpose. The OTHER/UNKNOWN parallel is a consequence of orthogonal
 * axes, not an error.
 *
 * Vocabulary evolution discipline:
 *
 *   Adding a new value is non-breaking.
 *   Removing or renaming a value is breaking and MUST bump
 *   CURRENT_CLASSIFIER_VERSION to signal backfill.
 *   Changing the semantic rules that produce a basis code ALSO
 *   requires a bump, even if the identifiers are unchanged.
 */

// ── Semantic classification buckets ─────────────────────────────────

export const UnresolvedEdgeClassification = {
	EXTERNAL_LIBRARY_CANDIDATE: "external_library_candidate",
	INTERNAL_CANDIDATE: "internal_candidate",
	UNKNOWN: "unknown",
} as const;

export type UnresolvedEdgeClassification =
	(typeof UnresolvedEdgeClassification)[keyof typeof UnresolvedEdgeClassification];

// ── Classifier basis codes ──────────────────────────────────────────

export const UnresolvedEdgeBasisCode = {
	/** import specifier is a bare name matching a package.json dependency */
	SPECIFIER_MATCHES_PACKAGE_DEPENDENCY: "specifier_matches_package_dependency",
	/**
	 * Import binding specifier matches a project-level path alias
	 * (e.g. tsconfig `paths` entry). Renamed from the v1
	 * `specifier_matches_tsconfig_alias` to keep the persisted basis
	 * semantic rather than config-file-specific.
	 */
	SPECIFIER_MATCHES_PROJECT_ALIAS: "specifier_matches_project_alias",
	/**
	 * Import binding specifier matches a known runtime/stdlib module
	 * (e.g. "path", "fs", "node:crypto"). The module is part of the
	 * execution runtime, not a declared package dependency.
	 */
	SPECIFIER_MATCHES_RUNTIME_MODULE: "specifier_matches_runtime_module",
	/** obj.method() receiver came in via external import in this file */
	RECEIVER_MATCHES_EXTERNAL_IMPORT: "receiver_matches_external_import",
	/** obj.method() receiver came in via internal (relative/alias) import */
	RECEIVER_MATCHES_INTERNAL_IMPORT: "receiver_matches_internal_import",
	/** obj.method() receiver is a symbol declared in the same source file */
	RECEIVER_MATCHES_SAME_FILE_SYMBOL: "receiver_matches_same_file_symbol",
	/** obj.method() receiver is a known runtime global (Map, Date, etc.) */
	RECEIVER_MATCHES_RUNTIME_GLOBAL: "receiver_matches_runtime_global",
	/** fn() callee is declared in the same source file */
	CALLEE_MATCHES_SAME_FILE_SYMBOL: "callee_matches_same_file_symbol",
	/** fn() callee came in via external import in this file */
	CALLEE_MATCHES_EXTERNAL_IMPORT: "callee_matches_external_import",
	/** fn() callee came in via internal import in this file */
	CALLEE_MATCHES_INTERNAL_IMPORT: "callee_matches_internal_import",
	/** fn() / new Foo() callee is a known runtime global (Map, Date, etc.) */
	CALLEE_MATCHES_RUNTIME_GLOBAL: "callee_matches_runtime_global",
	/** this.m() or this.x.m() — receiver is on the current class */
	THIS_RECEIVER_IMPLIES_INTERNAL: "this_receiver_implies_internal",
	/**
	 * An `imports_file_not_found` observation whose specifier is
	 * path-relative (starts with "."). Current extractor behavior
	 * only emits unresolved IMPORTS edges for relative specifiers,
	 * so this basis fires for every such observation.
	 */
	RELATIVE_IMPORT_TARGET_UNRESOLVED: "relative_import_target_unresolved",
	/** no classification signal matched */
	NO_SUPPORTING_SIGNAL: "no_supporting_signal",
} as const;

export type UnresolvedEdgeBasisCode =
	(typeof UnresolvedEdgeBasisCode)[keyof typeof UnresolvedEdgeBasisCode];

// ── Rule-set version ────────────────────────────────────────────────

/**
 * Current classifier rule-set version.
 *
 * Bump when any of the following changes:
 *   - a classification value is renamed or removed
 *   - a basis code is renamed or removed
 *   - a rule's semantic changes (what a basis code's match implies)
 *
 * Adding values does NOT require a bump.
 *
 * Bumping marks every persisted row with an older classifier_version
 * as eligible for backfill. Backfill tooling is deferred to a later
 * slice; the version column is already in place so that tooling can
 * detect and rewrite stale rows without a migration.
 */
/**
 * Version 2 changes (from v1):
 *   - RENAMED: specifier_matches_tsconfig_alias → specifier_matches_project_alias
 *   - ADDED: specifier_matches_runtime_module, callee_matches_runtime_global,
 *     receiver_matches_runtime_global
 *   - NEW RULE: runtime-builtins check (last-resort before unknown)
 *   - Alias-basis fidelity: alias matches now emit specifier_matches_project_alias
 *     directly instead of reusing the internal-import basis
 */
export const CURRENT_CLASSIFIER_VERSION = 2;
