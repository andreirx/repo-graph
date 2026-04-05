/**
 * AnnotationsPort — dedicated port for provisional annotations.
 *
 * This port is DELIBERATELY SEPARATE from StoragePort. Annotations
 * are provisional author claims; policy/evaluator/gate/trust/impact
 * surfaces must not read them.
 *
 * The isolation invariant (docs/architecture/annotations-contract.txt
 * §7) is enforced by a grep-based architecture test: no imports
 * of this module from computed-truth paths.
 *
 * Callers allowed to import this port:
 *   - src/adapters/annotations/*       (implementations)
 *   - src/adapters/indexer/*           (write-only during extraction)
 *   - src/cli/commands/docs.ts         (read-only for `rgr docs`)
 *   - src/main.ts                      (composition root)
 *
 * Callers FORBIDDEN from importing this port:
 *   - src/core/evaluator/*
 *   - src/core/gate/*
 *   - src/core/impact/*
 *   - src/core/trust/*
 *   - src/cli/commands/gate.ts
 *   - src/cli/commands/evidence.ts
 *   - src/cli/commands/graph/obligations.ts
 *   - src/cli/commands/graph/queries.ts
 *   - src/cli/commands/change.ts
 */

import type { Annotation } from "../annotations/types.js";

export interface AnnotationsPort {
	/**
	 * Bulk insert annotations for a snapshot. Called by the indexer
	 * at the end of extraction. Not idempotent — callers must clear
	 * annotations for the snapshot first if re-running.
	 */
	insertAnnotations(annotations: Annotation[]): void;

	/**
	 * Delete all annotations for a snapshot. Used before re-insert
	 * during refresh/re-index.
	 */
	deleteAnnotationsBySnapshot(snapshotUid: string): void;

	/**
	 * Read annotations attached to exactly one target stable_key.
	 * Used by `rgr docs <target>` after target resolution.
	 *
	 * Returns results in the deterministic ordering specified by
	 * the contract (§6 output-ordering rules).
	 */
	getAnnotationsByTarget(
		snapshotUid: string,
		targetStableKey: string,
	): Annotation[];

	/**
	 * Resolve a query string to a target stable_key via exact match.
	 * Implements the precedence order from contract §6:
	 *   1. exact stable_key match
	 *   2. exact repo-relative path match (file OR module nodes)
	 *   3. exact module name / qualified name match
	 * Ambiguity at step 2 or 3 returns { kind: 'ambiguous', ... }.
	 */
	resolveDocsTarget(
		snapshotUid: string,
		query: string,
	): DocsTargetResolution;

	/**
	 * Count all annotations for a snapshot. Used by smoke-run /
	 * trust integration for diagnostic visibility.
	 */
	countAnnotationsBySnapshot(snapshotUid: string): number;
}

export type DocsTargetResolution =
	| { kind: "resolved"; targetStableKey: string }
	| { kind: "not_found" }
	| {
			kind: "ambiguous";
			step: "path" | "module_name";
			candidates: string[];
	  };
