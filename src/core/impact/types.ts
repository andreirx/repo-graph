/**
 * Core types for change impact analysis.
 *
 * Pure domain types. No I/O. No storage access. No CLI concerns.
 *
 * The impact service orchestrates ports to compute these results.
 * The CLI adapts them into JSON and human-readable output.
 *
 * Design constraint (first slice):
 *   - Seeds are FILE paths from git diff
 *   - Seeds map to indexed FILE nodes, which map to owning MODULE nodes
 *   - Propagation traverses REVERSE MODULE IMPORTS only
 *   - CALLS edges are NOT used — weaker trust signal, would inflate false positives
 *   - Dynamic / runtime / framework-registry edges are NOT modeled
 *
 * The trust metadata (TrustNote, caveats) is first-class output, not
 * commentary. Consumers must be able to detect when propagation is
 * coarse or incomplete.
 */

/**
 * How the change set was scoped against git history.
 * `against_snapshot` means: "diff working tree vs the basis_commit of
 *   the latest snapshot." This is the default because it matches the
 *   product's indexed reference frame.
 * `staged` means: "diff staged changes" (git diff --cached).
 * `since_ref` means: "diff working tree vs this git ref" (commit, tag,
 *   branch). Caller-specified.
 */
export type ImpactScope =
	| { kind: "against_snapshot"; basis_commit: string }
	| { kind: "staged" }
	| { kind: "since_ref"; ref: string };

/**
 * A single changed file discovered by the git diff.
 *
 * Resolution state:
 *   matched_to_index = true  -> file is present in the indexed snapshot
 *   matched_to_index = false -> file is NOT indexed (outside surface,
 *                                excluded, new file not yet indexed)
 *
 * Unmatched files are surfaced in output so the consumer knows the
 * impact set is derived from a subset of the real change set.
 */
export interface ChangedFileSeed {
	/** Repo-relative path as reported by git. */
	path: string;
	/** Whether the path maps to an indexed FILE node in this snapshot. */
	matched_to_index: boolean;
	/**
	 * Owning module stable key, if the file is indexed. Null when the
	 * file is not in the snapshot or has no resolved owning module.
	 */
	owning_module: string | null;
	/** Short reason when unmatched: "not_in_snapshot" | "no_owning_module". */
	unmatched_reason: string | null;
}

/**
 * A module reached by reverse-IMPORTS propagation.
 *
 * distance:
 *   0 is reserved for seed modules (never emitted via reverse_import)
 *   1 means "a module that directly imports a seed module"
 *   n means "n hops of reverse IMPORTS from some seed"
 *
 * reason:
 *   "seed"           -> this module owns at least one changed file
 *   "reverse_import" -> this module was reached by traversal
 */
export interface ImpactedModule {
	module: string;
	distance: number;
	reason: "seed" | "reverse_import";
}

/**
 * Honesty metadata. These fields are mandatory in every result and
 * must be echoed verbatim by the CLI output surface.
 *
 * Consumers (gate --changed, CI, agents) must treat these as part of
 * the result contract, not commentary.
 */
export interface TrustNote {
	/** What edges were used for propagation. */
	graph_basis: "reverse_module_imports_only";
	/** Explicit flag for whether CALLS were consulted. */
	calls_included: false;
	/** Human-readable caveats. Stable wording; extended only by adding. */
	caveats: string[];
}

export interface ImpactCounts {
	changed_files: number;
	changed_files_matched: number;
	changed_files_unmatched: number;
	seed_modules: number;
	impacted_modules: number;
	max_distance: number;
}

/**
 * The complete impact analysis result.
 *
 * Stable envelope. Additive changes allowed; field removal is breaking.
 */
export interface ImpactResult {
	scope: ImpactScope;
	snapshot_uid: string;
	/** Basis commit of the snapshot (not of the diff target). */
	snapshot_basis_commit: string | null;
	trust: TrustNote;
	changed_files: ChangedFileSeed[];
	seed_modules: string[];
	impacted_modules: ImpactedModule[];
	counts: ImpactCounts;
}

/**
 * Standard caveats emitted with every result. Scope-agnostic: these
 * describe what the IMPACT GRAPH is derived from, not what the DIFF
 * represents. The diff source is already communicated via
 * ImpactResult.scope, so caveats here must hold for every scope.
 *
 * Extending this list is a non-breaking change; modifying existing
 * entries is breaking.
 */
export const STANDARD_CAVEATS: readonly string[] = Object.freeze([
	"Propagation uses reverse MODULE IMPORTS only.",
	"Call edges (CALLS) are NOT included in this slice.",
	"Dynamic and runtime dispatch edges are NOT modeled.",
	"Registry/framework liveness (plugin registration, render maps) is NOT modeled; impact may be under-reported on such codebases.",
	"Impact is computed over the current indexed snapshot's module graph; changes to import edges in the working tree since indexing are not reflected.",
]);
