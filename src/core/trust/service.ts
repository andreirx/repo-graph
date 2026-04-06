/**
 * Trust reporting orchestrator.
 *
 * Composes storage queries with pure rules (rules.ts) to produce
 * a TrustReport for a snapshot. No I/O outside the storage port;
 * no CLI formatting. The CLI layer adapts the report into JSON or
 * human output.
 */

import { EdgeType } from "../model/index.js";
import { DeclarationKind } from "../model/index.js";
import type { UnresolvedEdgeClassification } from "../diagnostics/unresolved-edge-classification.js";
import type { StoragePort } from "../ports/storage.js";
import { deriveBlastRadius } from "../classification/blast-radius.js";
import {
	CALLS_CATEGORIES,
	humanLabelForCategory,
} from "../diagnostics/unresolved-edge-categories.js";
import {
	computeCallGraphReliability,
	computeChangeImpactReliability,
	computeDeadCodeReliability,
	computeImportGraphReliability,
	countSuspiciousZeroConnectivityModules,
	detectAliasResolutionSuspicion,
	detectFrameworkHeavySuspicion,
	detectMissingEntrypointDeclarations,
	detectRegistryPatternSuspicion,
	groupPathPrefixCyclesByAncestor,
	sumUnresolvedCalls,
	sumUnresolvedImports,
} from "./rules.js";
import type {
	EnrichmentStatus,
	ExtractionDiagnostics,
	ModuleTrustRow,
	TrustCategoryRow,
	TrustClassificationRow,
	TrustReport,
	UnknownCallsBlastRadiusBreakdown,
} from "./types.js";

export interface ComputeTrustReportInput {
	storage: StoragePort;
	repoUid: string;
	snapshotUid: string;
	snapshotBasisCommit: string | null;
	/**
	 * Raw toolchain_json string from the snapshot row. Parsed into
	 * the report's `toolchain` field for cross-snapshot comparability.
	 * Pass null for snapshots without toolchain provenance.
	 */
	snapshotToolchainJson: string | null;
}

/**
 * Compute a TrustReport for the given snapshot.
 *
 * Errors: throws if the snapshot does not exist.
 * Gracefully handles snapshots indexed before migration 005 by
 * setting diagnostics_available=false and zeroing derived fields.
 */
export function computeTrustReport(
	input: ComputeTrustReportInput,
): TrustReport {
	const {
		storage,
		repoUid,
		snapshotUid,
		snapshotBasisCommit,
		snapshotToolchainJson,
	} = input;

	// ── Read snapshot-level diagnostics ─────────────────────────
	const diagnosticsJson = storage.getSnapshotExtractionDiagnostics(snapshotUid);
	const diagnostics: ExtractionDiagnostics | null = diagnosticsJson
		? (JSON.parse(diagnosticsJson) as ExtractionDiagnostics)
		: null;
	const diagnosticsAvailable = diagnostics !== null;

	// ── Parse toolchain provenance ──────────────────────────────
	const toolchain: Record<string, unknown> | null = snapshotToolchainJson
		? (JSON.parse(snapshotToolchainJson) as Record<string, unknown>)
		: null;

	// ── Read signals needed by the rule set ─────────────────────

	// File paths for framework detection
	const files = storage.getFilesByRepo(repoUid);
	const filePaths = files.map((f) => f.path);

	// Module stats for suspicious-zero-connectivity + module rows
	const moduleStats = storage.computeModuleStats(snapshotUid);

	// Path-prefix cycles for registry-pattern detection
	const pathPrefixCycles = storage.findPathPrefixModuleCycles(snapshotUid);
	const pathPrefixCyclesByAncestor =
		groupPathPrefixCyclesByAncestor(pathPrefixCycles);

	// Entrypoint declaration count
	const entrypointDecls = storage.getActiveDeclarations({
		repoUid,
		kind: DeclarationKind.ENTRYPOINT,
	});

	// Resolved CALLS count (on-demand, not persisted)
	const resolvedCalls = storage.countEdgesByType(
		snapshotUid,
		EdgeType.CALLS,
	);

	// ── Apply detection rules ───────────────────────────────────
	const frameworkHeavy = detectFrameworkHeavySuspicion({ filePaths });

	// Map ModuleStats to the field names expected by rules.ts
	const moduleStatsForRules = moduleStats.map((m) => ({
		qualified_name: m.path,
		fan_in: m.fanIn,
		fan_out: m.fanOut,
		file_count: m.fileCount,
	}));
	const suspiciousModuleCount = countSuspiciousZeroConnectivityModules(
		moduleStatsForRules,
	);

	const aliasResolution = detectAliasResolutionSuspicion({
		suspiciousModuleCount,
	});

	const registryPattern = detectRegistryPatternSuspicion({
		pathPrefixCyclesByAncestor,
		pathPrefixCyclesTotal: pathPrefixCycles.length,
	});

	const missingEntrypoints = detectMissingEntrypointDeclarations({
		activeEntrypointCount: entrypointDecls.length,
	});

	// ── Apply reliability formulas ──────────────────────────────
	const unresolvedCalls = diagnostics ? sumUnresolvedCalls(diagnostics) : 0;
	const unresolvedImports = diagnostics
		? sumUnresolvedImports(diagnostics)
		: 0;

	// Variant A reweighting: compute how many unresolved CALLS are
	// classified as external_library_candidate. These are excluded
	// from the call-graph rate denominator.
	const callsClassifiedByBucket = storage.countUnresolvedEdges({
		snapshotUid,
		groupBy: "classification",
		filterCategories: [...CALLS_CATEGORIES] as string[],
	});
	const unresolvedCallsExternal =
		callsClassifiedByBucket.find(
			(r) => r.key === "external_library_candidate",
		)?.count ?? 0;
	const unresolvedCallsInternalLike = unresolvedCalls - unresolvedCallsExternal;

	const importGraphReliability = computeImportGraphReliability({
		aliasResolutionSuspicion: aliasResolution.triggered,
		registryPatternSuspicion: registryPattern.triggered,
		unresolvedImportsCount: unresolvedImports,
	});

	const callGraphReliability = computeCallGraphReliability({
		resolvedCalls,
		unresolvedCallsInternalLike,
	});

	const deadCodeReliability = computeDeadCodeReliability({
		missingEntrypointDeclarations: missingEntrypoints.triggered,
		registryPatternSuspicion: registryPattern.triggered,
		frameworkHeavySuspicion: frameworkHeavy.triggered,
		callGraphLevel: callGraphReliability.level,
	});

	const changeImpactReliability = computeChangeImpactReliability({
		aliasResolutionSuspicion: aliasResolution.triggered,
		registryPatternSuspicion: registryPattern.triggered,
		importGraphLevel: importGraphReliability.level,
	});

	// ── Build category rows (sorted by unresolved count desc) ───
	const categories: TrustCategoryRow[] = diagnostics
		? Object.entries(diagnostics.unresolved_breakdown)
				.map(([category, unresolved]) => ({
					category,
					label: humanLabelForCategory(category),
					unresolved,
				}))
				.sort((a, b) => b.unresolved - a.unresolved)
		: [];

	// ── Build classification rows (Tier 1a) ─────────────────────
	// Reads from the unresolved_edges table (migration 007). For
	// snapshots indexed before that migration the query returns an
	// empty array and `classifications` is []. Sorted by count desc
	// so the dominant bucket leads. Tie-break by key ASC for
	// determinism.
	const classificationCounts = storage.countUnresolvedEdges({
		snapshotUid,
		groupBy: "classification",
	});
	const classifications: TrustClassificationRow[] = classificationCounts
		.map((row) => ({
			classification: row.key,
			count: row.count,
		}))
		.sort((a, b) => {
			if (b.count !== a.count) return b.count - a.count;
			return a.classification.localeCompare(b.classification);
		});

	// ── Blast-radius breakdown (scoped to unknown CALLS) ────────
	// Query-time derived: fetch unknown CALLS samples, derive
	// blast-radius per-row, aggregate counts.
	let unknownCallsBlastRadius: UnknownCallsBlastRadiusBreakdown | null = null;
	let enrichmentStatus: EnrichmentStatus | null = null;
	if (classificationCounts.length > 0) {
		const unknownCallsSamples = storage.queryUnresolvedEdges({
			snapshotUid,
			classification: "unknown" as UnresolvedEdgeClassification,
			limit: 100000,
		});
		// Filter to CALLS-family only.
		const callsFamilySet = new Set(CALLS_CATEGORIES as readonly string[]);
		const breakdown = { low: 0, medium: 0, high: 0 };
		// Enrichment tracking.
		let enrichedCount = 0;
		let eligibleCount = 0;
		let enrichmentWasRun = false;
		const typeCounts = new Map<string, { count: number; isExternal: boolean }>();

		for (const row of unknownCallsSamples) {
			if (!callsFamilySet.has(row.category)) continue;
			// Blast radius.
			const assessment = deriveBlastRadius({
				category: row.category,
				basisCode: row.basisCode,
				sourceNodeVisibility: row.sourceNodeVisibility,
			});
			if (assessment.blastRadius === "low") breakdown.low++;
			else if (assessment.blastRadius === "medium") breakdown.medium++;
			else if (assessment.blastRadius === "high") breakdown.high++;

			// Enrichment status: check metadata_json for enrichment key.
			if (row.category === "calls_obj_method_needs_type_info") {
				eligibleCount++;
				if (row.metadataJson) {
					try {
						const meta = JSON.parse(row.metadataJson) as Record<string, unknown>;
						const enrichment = meta.enrichment as Record<string, unknown> | undefined;
						if (enrichment) {
							// Enrichment marker present — enrichment WAS run on this edge.
							enrichmentWasRun = true;
							if (enrichment.receiverType) {
								enrichedCount++;
								const typeName = String(enrichment.typeDisplayName ?? enrichment.receiverType);
								const isExt = Boolean(enrichment.isExternalType);
								const existing = typeCounts.get(typeName);
								if (existing) {
									existing.count++;
								} else {
									typeCounts.set(typeName, { count: 1, isExternal: isExt });
								}
							}
						}
					} catch {
						// ignore malformed metadata
					}
				}
			}
		}
		unknownCallsBlastRadius = breakdown;

		// Build enrichment status.
		// Null ONLY if enrichment was never run (no enrichment markers
		// found on any edge). Populated with enriched=0 if enrichment
		// ran but resolved zero types. This distinction matters for
		// the trust display.
		if (enrichmentWasRun) {
			const topTypes = [...typeCounts.entries()]
				.sort((a, b) => b[1].count - a[1].count)
				.slice(0, 15)
				.map(([type, { count, isExternal }]) => ({ type, count, isExternal }));
			enrichmentStatus = { eligible: eligibleCount, enriched: enrichedCount, top_types: topTypes };
		}
	}

	// ── Build module rows ────────────────────────────────────────
	const modules: ModuleTrustRow[] = moduleStats.map((m) => {
		const suspicious =
			m.fanIn === 0 &&
			m.fanOut === 0 &&
			m.fileCount >= 2 &&
			m.path !== "." &&
			m.path !== "";
		const trustNotes: string[] = [];
		if (suspicious) {
			trustNotes.push("alias_resolution_candidate");
		}
		return {
			module_stable_key: m.stableKey,
			qualified_name: m.path,
			fan_in: m.fanIn,
			fan_out: m.fanOut,
			file_count: m.fileCount,
			suspicious_zero_connectivity: suspicious,
			trust_notes: trustNotes,
		};
	});

	// ── Caveats (human-readable summaries) ───────────────────────
	const caveats = buildCaveats({
		diagnosticsAvailable,
		importGraphLevel: importGraphReliability.level,
		callGraphLevel: callGraphReliability.level,
		deadCodeLevel: deadCodeReliability.level,
		changeImpactLevel: changeImpactReliability.level,
	});

	return {
		snapshot_uid: snapshotUid,
		basis_commit: snapshotBasisCommit,
		toolchain,
		diagnostics_version: diagnostics?.diagnostics_version ?? null,
		summary: {
			edges_total: diagnostics?.edges_total ?? 0,
			edges_resolved: diagnostics?.edges_total ?? 0,
			unresolved_total: diagnostics?.unresolved_total ?? 0,
			resolved_calls: resolvedCalls,
			unresolved_calls: unresolvedCalls,
			unresolved_calls_external: unresolvedCallsExternal,
			unresolved_calls_internal_like: unresolvedCallsInternalLike,
			call_resolution_rate:
				resolvedCalls + unresolvedCallsInternalLike > 0
					? resolvedCalls / (resolvedCalls + unresolvedCallsInternalLike)
					: 1,
			reliability: {
				import_graph: importGraphReliability,
				call_graph: callGraphReliability,
				dead_code: deadCodeReliability,
				change_impact: changeImpactReliability,
			},
			triggered_downgrades: {
				framework_heavy_suspicion: frameworkHeavy,
				registry_pattern_suspicion: registryPattern,
				missing_entrypoint_declarations: missingEntrypoints,
				alias_resolution_suspicion: aliasResolution,
			},
		},
		categories,
		classifications,
		unknown_calls_blast_radius: unknownCallsBlastRadius,
		enrichment_status: enrichmentStatus,
		modules,
		caveats,
		diagnostics_available: diagnosticsAvailable,
	};
}

/**
 * Build human-readable caveat strings from reliability levels.
 * These describe the trust posture to a human or agent reader.
 */
function buildCaveats(input: {
	diagnosticsAvailable: boolean;
	importGraphLevel: "HIGH" | "MEDIUM" | "LOW";
	callGraphLevel: "HIGH" | "MEDIUM" | "LOW";
	deadCodeLevel: "HIGH" | "MEDIUM" | "LOW";
	changeImpactLevel: "HIGH" | "MEDIUM" | "LOW";
}): string[] {
	const caveats: string[] = [];
	if (!input.diagnosticsAvailable) {
		caveats.push(
			"Extraction diagnostics unavailable for this snapshot. Re-index to populate.",
		);
	}
	if (input.callGraphLevel !== "HIGH") {
		caveats.push(
			"Call-graph reliability is " +
				input.callGraphLevel +
				" on this repo. Do not use callers/callees for safety-critical decisions without verification.",
		);
	}
	if (input.deadCodeLevel !== "HIGH") {
		caveats.push(
			"Dead-code reliability is " +
				input.deadCodeLevel +
				" on this repo. Treat `graph dead` results as 'graph orphans' requiring human arbitration, not deletion candidates.",
		);
	}
	if (input.importGraphLevel !== "HIGH") {
		caveats.push(
			"Import-graph reliability is " +
				input.importGraphLevel +
				". Module fan-in/fan-out and change-impact propagation may undercount relationships.",
		);
	}
	if (input.changeImpactLevel !== "HIGH") {
		caveats.push(
			"Change-impact reliability is " +
				input.changeImpactLevel +
				". Impacted-module sets may be incomplete on this repo.",
		);
	}
	caveats.push(
		"Cycle payloads currently emit leaf module names only; full stable keys are not in the user-facing `graph cycles` output.",
	);
	return caveats;
}

