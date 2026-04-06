/**
 * Blast-radius derivation for unresolved edges (query-time).
 *
 * Third orthogonal signal axis alongside:
 *   - category (extraction failure mode)
 *   - classification (semantic meaning of the gap)
 *   - blast_radius (how architecturally dangerous is this unresolved)
 *
 * Computed on demand — NOT persisted. The heuristic is derived from:
 *   - basis_code (determines receiver origin)
 *   - category (scoping: only defined for CALLS-family)
 *   - source node visibility (enclosing scope significance)
 *
 * Scope (first slice):
 *   - defined for CALLS-family unresolved edges only
 *   - other categories return { blastRadius: "not_applicable" }
 *   - entrypoint-path detection is deferred (uses visibility only)
 *
 * Naming: `local_like` is the honest catch-all for receivers
 * that are NOT import-bound, NOT same-file-declared, NOT runtime
 * globals, NOT `this`-bound. They are likely local variables,
 * parameters, destructured locals, closure captures, or
 * intermediate expression roots. The label does NOT overclaim
 * exact provenance.
 */

import { UnresolvedEdgeBasisCode } from "../diagnostics/unresolved-edge-classification.js";
import { UnresolvedEdgeCategory } from "../diagnostics/unresolved-edge-categories.js";

// ── Vocabulary ──────────────────────────────────────────────────────

export type ReceiverOrigin =
	| "this_bound"
	| "import_bound_external"
	| "import_bound_internal"
	| "same_file_declared"
	| "runtime_global"
	| "runtime_module"
	| "local_like"
	| "not_applicable";

export type EnclosingScopeSignificance =
	| "exported_public"
	| "internal_private"
	| "unknown_scope";

export type BlastRadiusLevel =
	| "low"
	| "medium"
	| "high"
	| "not_applicable";

export interface BlastRadiusAssessment {
	receiverOrigin: ReceiverOrigin;
	enclosingScopeSignificance: EnclosingScopeSignificance;
	blastRadius: BlastRadiusLevel;
}

// ── CALLS-family categories ─────────────────────────────────────────

const CALLS_FAMILY = new Set<string>([
	UnresolvedEdgeCategory.CALLS_FUNCTION_AMBIGUOUS_OR_MISSING,
	UnresolvedEdgeCategory.CALLS_OBJ_METHOD_NEEDS_TYPE_INFO,
	UnresolvedEdgeCategory.CALLS_THIS_METHOD_NEEDS_CLASS_CONTEXT,
	UnresolvedEdgeCategory.CALLS_THIS_WILDCARD_METHOD_NEEDS_TYPE_INFO,
]);

// ── Derivation ──────────────────────────────────────────────────────

/**
 * Derive blast-radius assessment for a single unresolved edge.
 *
 * Pure function. All inputs are plain data from the sample row.
 * Returns `not_applicable` for non-CALLS categories.
 */
export function deriveBlastRadius(input: {
	category: string;
	basisCode: string;
	sourceNodeVisibility: string | null;
}): BlastRadiusAssessment {
	if (!CALLS_FAMILY.has(input.category)) {
		return {
			receiverOrigin: "not_applicable",
			enclosingScopeSignificance: "unknown_scope",
			blastRadius: "not_applicable",
		};
	}

	const origin = deriveReceiverOrigin(input.basisCode);
	const scope = deriveEnclosingScopeSignificance(input.sourceNodeVisibility);
	const radius = computeRadius(origin, scope);

	return {
		receiverOrigin: origin,
		enclosingScopeSignificance: scope,
		blastRadius: radius,
	};
}

/**
 * Map basis_code to receiver origin.
 *
 * The basis_code already encodes what rule matched (or didn't).
 * This function makes that implicit knowledge explicit.
 */
function deriveReceiverOrigin(basisCode: string): ReceiverOrigin {
	switch (basisCode) {
		case UnresolvedEdgeBasisCode.THIS_RECEIVER_IMPLIES_INTERNAL:
			return "this_bound";
		case UnresolvedEdgeBasisCode.RECEIVER_MATCHES_EXTERNAL_IMPORT:
		case UnresolvedEdgeBasisCode.CALLEE_MATCHES_EXTERNAL_IMPORT:
			return "import_bound_external";
		case UnresolvedEdgeBasisCode.RECEIVER_MATCHES_INTERNAL_IMPORT:
		case UnresolvedEdgeBasisCode.CALLEE_MATCHES_INTERNAL_IMPORT:
		case UnresolvedEdgeBasisCode.SPECIFIER_MATCHES_PROJECT_ALIAS:
			return "import_bound_internal";
		case UnresolvedEdgeBasisCode.RECEIVER_MATCHES_SAME_FILE_SYMBOL:
		case UnresolvedEdgeBasisCode.CALLEE_MATCHES_SAME_FILE_SYMBOL:
			return "same_file_declared";
		case UnresolvedEdgeBasisCode.RECEIVER_MATCHES_RUNTIME_GLOBAL:
		case UnresolvedEdgeBasisCode.CALLEE_MATCHES_RUNTIME_GLOBAL:
			return "runtime_global";
		case UnresolvedEdgeBasisCode.SPECIFIER_MATCHES_RUNTIME_MODULE:
			return "runtime_module";
		case UnresolvedEdgeBasisCode.NO_SUPPORTING_SIGNAL:
			return "local_like";
		default:
			return "local_like";
	}
}

function deriveEnclosingScopeSignificance(
	visibility: string | null,
): EnclosingScopeSignificance {
	// Visibility values are lowercase in the DB (see model/types.ts Visibility).
	// "export" and "public" = externally reachable.
	// "private", "protected", "internal", null = not externally reachable.
	if (visibility === "export" || visibility === "public") return "exported_public";
	if (
		visibility === "private" ||
		visibility === "protected" ||
		visibility === "internal" ||
		visibility === null
	) {
		return "internal_private";
	}
	return "unknown_scope";
}

/**
 * Combine receiver origin + enclosing scope into a blast-radius level.
 *
 * Principles:
 *   - local_like + internal_private = LOW (function-local noise)
 *   - local_like + exported_public = MEDIUM (local uncertainty on a public path)
 *   - import_bound = MEDIUM (cross-module reference)
 *   - runtime_global / runtime_module = LOW (well-understood external)
 *   - same_file_declared = LOW (contained within file)
 *   - this_bound + internal = LOW, this_bound + exported = MEDIUM
 *
 * Entrypoint-path detection (→ HIGH override) is deferred to a
 * future slice. Currently no edge reaches HIGH.
 */
function computeRadius(
	origin: ReceiverOrigin,
	scope: EnclosingScopeSignificance,
): BlastRadiusLevel {
	switch (origin) {
		case "local_like":
			return scope === "exported_public" ? "medium" : "low";
		case "this_bound":
			return scope === "exported_public" ? "medium" : "low";
		case "import_bound_internal":
			return "medium";
		case "import_bound_external":
			return "low";
		case "same_file_declared":
			return "low";
		case "runtime_global":
		case "runtime_module":
			return "low";
		case "not_applicable":
			return "not_applicable";
	}
}
