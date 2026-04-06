/**
 * Trust reporting types.
 *
 * Pure domain types. No I/O. No CLI concerns.
 *
 * The trust surface answers: before acting on a `rgr` result, how
 * reliable is that result on THIS repo, given known extraction
 * limitations? The output shape is stable so an agent can compare
 * trust reports across snapshots or across repos.
 *
 * Architectural principle: trust is a DERIVED INTERPRETATION layer
 * over snapshot facts. It is NOT persisted as measurement or
 * inference in the first slice. Compute-on-read from snapshot
 * extraction diagnostics plus deterministic rules.
 */

/**
 * Extraction diagnostics payload stored on each snapshot.
 * See src/adapters/storage/sqlite/migrations/005-extraction-diagnostics.ts.
 *
 * diagnostics_version: 1 (bump on breaking key changes).
 */
export interface ExtractionDiagnostics {
	diagnostics_version: number;
	edges_total: number;
	unresolved_total: number;
	/**
	 * Machine-stable keys from UnresolvedEdgeCategory.
	 * See src/core/diagnostics/unresolved-edge-categories.ts.
	 */
	unresolved_breakdown: Record<string, number>;
}

export type ReliabilityLevel = "HIGH" | "MEDIUM" | "LOW";

/**
 * Reliability assessment on one axis (import graph, call graph,
 * dead-code, change impact). Reasons are machine-stable identifiers
 * explaining why the level is not HIGH. Empty reasons array
 * means the axis is HIGH and no downgrade was triggered.
 */
export interface ReliabilityAxisScore {
	level: ReliabilityLevel;
	reasons: string[];
}

/**
 * A downgrade trigger indicates the trust report detected a
 * repo-level pattern that lowers confidence in certain results.
 */
export interface DowngradeTrigger {
	triggered: boolean;
	reasons: string[];
}

export interface TrustDowngrades {
	framework_heavy_suspicion: DowngradeTrigger;
	registry_pattern_suspicion: DowngradeTrigger;
	missing_entrypoint_declarations: DowngradeTrigger;
	alias_resolution_suspicion: DowngradeTrigger;
}

export interface TrustReliability {
	import_graph: ReliabilityAxisScore;
	call_graph: ReliabilityAxisScore;
	dead_code: ReliabilityAxisScore;
	change_impact: ReliabilityAxisScore;
}

export interface TrustSummary {
	edges_total: number;
	edges_resolved: number;
	unresolved_total: number;
	/** Count of resolved CALLS edges in the snapshot. */
	resolved_calls: number;
	/** Sum of ALL unresolved CALLS-family categories (total). */
	unresolved_calls: number;
	/**
	 * Unresolved CALLS classified as external_library_candidate.
	 * These are calls to code outside the repo (npm packages,
	 * runtime builtins, stdlib modules). Excluded from the
	 * call_resolution_rate denominator because they are not
	 * internal call-graph failures.
	 *
	 * Zero when the snapshot predates migration 007 (no
	 * classification data available).
	 */
	unresolved_calls_external: number;
	/**
	 * Unresolved CALLS NOT classified as external: internal_candidate
	 * + unknown. These remain in the call_resolution_rate denominator
	 * because they represent genuine internal call-graph uncertainty.
	 *
	 * Equal to unresolved_calls - unresolved_calls_external.
	 */
	unresolved_calls_internal_like: number;
	/**
	 * resolved_calls / (resolved_calls + unresolved_calls_internal_like).
	 *
	 * The denominator excludes external-library unresolved calls
	 * (Variant A reweighting, introduced with classifier v2).
	 * The result is a cleaner measure of internal call-graph quality.
	 */
	call_resolution_rate: number;
	reliability: TrustReliability;
	triggered_downgrades: TrustDowngrades;
}

export interface TrustCategoryRow {
	/** Machine-stable category key. */
	category: string;
	/** Human-readable label rendered at display time. */
	label: string;
	unresolved: number;
}

/**
 * Classifier-bucket count row. Parallel to TrustCategoryRow but on
 * the ORTHOGONAL classification axis (semantic meaning of the gap,
 * not extraction failure mode).
 *
 * See docs/architecture/schema.txt (unresolved_edges table) and
 * src/core/classification/unresolved-classifier.ts.
 */
/**
 * Blast-radius breakdown for unknown-classified CALLS edges.
 * Scoped: only counts CALLS-family edges with classification=unknown.
 */
export interface UnknownCallsBlastRadiusBreakdown {
	low: number;
	medium: number;
	/** Currently always 0 (entrypoint-path detection deferred). */
	high: number;
}

export interface TrustClassificationRow {
	/** Machine-stable classification key (e.g. "external_library_candidate"). */
	classification: string;
	count: number;
}

export interface ModuleTrustRow {
	module_stable_key: string;
	qualified_name: string;
	fan_in: number;
	fan_out: number;
	file_count: number;
	/** file_count >= 2, fan_in = 0, fan_out = 0, not repo root. */
	suspicious_zero_connectivity: boolean;
	/** Machine-stable trust note keys applying to this module. */
	trust_notes: string[];
}

export interface TrustReport {
	snapshot_uid: string;
	basis_commit: string | null;
	/**
	 * Snapshot toolchain provenance. Reliability levels emitted by
	 * this report are DERIVED FROM EXTRACTOR BEHAVIOR, so consumers
	 * comparing trust reports across snapshots must verify that
	 * this block matches (or tolerate differences as meaningful).
	 *
	 * Null only for snapshots created before the toolchain_json
	 * column existed (migration 002).
	 */
	toolchain: Record<string, unknown> | null;
	/**
	 * Version of the extraction_diagnostics payload schema. Bumps
	 * when unresolved-category keys are renamed or the payload
	 * shape changes. A different diagnostics_version between two
	 * reports means category counts are NOT directly comparable.
	 *
	 * Null when diagnostics_available is false.
	 */
	diagnostics_version: number | null;
	summary: TrustSummary;
	categories: TrustCategoryRow[];
	/**
	 * Classifier bucket counts (Tier 1a). Present when the snapshot
	 * was indexed after migration 007; empty otherwise. The axis is
	 * orthogonal to `categories`: a single unresolved edge has both
	 * a category (failure mode) and a classification (semantic meaning).
	 */
	classifications: TrustClassificationRow[];
	/**
	 * Blast-radius breakdown for unknown-classified CALLS edges.
	 * Scoped to the subset that motivated the axis: CALLS where
	 * classification=unknown (the dominant residual). Non-CALLS
	 * unresolveds are excluded. Null when the snapshot predates
	 * migration 007.
	 */
	unknown_calls_blast_radius: UnknownCallsBlastRadiusBreakdown | null;
	modules: ModuleTrustRow[];
	/** Human-readable caveats summarizing the trust posture. */
	caveats: string[];
	/**
	 * True iff the snapshot was indexed before migration 005 and
	 * lacks persisted diagnostics. The report is still produced but
	 * fields derived from diagnostics are placeholder/zero.
	 */
	diagnostics_available: boolean;
}
