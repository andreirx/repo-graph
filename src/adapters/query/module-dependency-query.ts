/**
 * Module dependency graph query adapter.
 *
 * Loads data from storage, transforms to adapter DTOs, and calls
 * the pure core derivation function. Query-time derivation only —
 * no persistence of derived edges.
 *
 * See docs/architecture/module-graph-contract.txt §4.
 */

import type { ModuleKind } from "../../core/modules/module-candidate.js";
import {
	deriveModuleDependencyEdges,
	type FileOwnershipMapping,
	type ImportEdgeInput,
	type ModuleDependencyResult,
	type NodeFileMapping,
} from "../../core/modules/module-dependency-edges.js";
import type { StoragePort } from "../../core/ports/storage.js";

// ── Adapter DTOs ───────────────────────────────────────────────────

/**
 * Module candidate reference for display/reporting.
 * Includes identity fields needed for CLI output.
 */
export interface ModuleCandidateRef {
	readonly moduleCandidateUid: string;
	readonly moduleKey: string;
	readonly canonicalRootPath: string;
	readonly moduleKind: ModuleKind;
	readonly displayName: string | null;
}

/**
 * Enriched module dependency edge for reporting.
 * Adds module identity fields to the core edge.
 */
export interface EnrichedModuleDependencyEdge {
	readonly sourceModuleUid: string;
	readonly sourceModuleKey: string;
	readonly sourceRootPath: string;
	readonly sourceModuleKind: ModuleKind;
	readonly sourceDisplayName: string | null;

	readonly targetModuleUid: string;
	readonly targetModuleKey: string;
	readonly targetRootPath: string;
	readonly targetModuleKind: ModuleKind;
	readonly targetDisplayName: string | null;

	readonly importCount: number;
	readonly sourceFileCount: number;
}

/**
 * Result of module dependency graph query.
 */
export interface ModuleDependencyGraphResult {
	readonly snapshotUid: string;
	readonly edges: EnrichedModuleDependencyEdge[];
	readonly diagnostics: ModuleDependencyResult["diagnostics"];
}

/**
 * Filter options for module dependency graph query.
 */
export interface ModuleDependencyQueryFilter {
	/** Filter to edges involving this module (as source or target). */
	readonly moduleKey?: string;
	/** If true, only show outbound edges from the filtered module. */
	readonly outboundOnly?: boolean;
	/** If true, only show inbound edges to the filtered module. */
	readonly inboundOnly?: boolean;
}

// ── Storage query interface ────────────────────────────────────────

/**
 * Raw resolved IMPORTS edge from storage.
 * Only includes file UIDs, not full node data.
 */
export interface ResolvedImportEdgeRow {
	readonly sourceFileUid: string;
	readonly targetFileUid: string;
}

/**
 * Extension to StoragePort for module dependency queries.
 * These methods are added to SqliteStorage for query-time derivation.
 */
export interface ModuleDependencyStorageQueries {
	/**
	 * Query resolved IMPORTS edges with source/target file UIDs.
	 * Only returns edges where both source and target nodes have file_uid set.
	 */
	queryResolvedImportsWithFiles(snapshotUid: string): ResolvedImportEdgeRow[];
}

// ── Query function ─────────────────────────────────────────────────

/**
 * Query the module dependency graph for a snapshot.
 *
 * Loads IMPORTS edges, file ownership, and module candidates from storage,
 * runs the pure derivation algorithm, and enriches results with module
 * identity fields.
 *
 * Query-time derivation — no persistence.
 */
export function getModuleDependencyGraph(
	storage: StoragePort & ModuleDependencyStorageQueries,
	snapshotUid: string,
	filter?: ModuleDependencyQueryFilter,
): ModuleDependencyGraphResult {
	// 1. Load data from storage.
	const resolvedImports = storage.queryResolvedImportsWithFiles(snapshotUid);
	const ownership = storage.queryModuleFileOwnership(snapshotUid);
	const candidates = storage.queryModuleCandidates(snapshotUid);

	// 2. Transform to derivation input format.
	// For resolved imports, we already have file UIDs directly.
	// We need to create a synthetic node mapping where nodeUid = "file:<fileUid>"
	// so that the derivation algorithm can look up file ownership.
	const importsEdges: ImportEdgeInput[] = resolvedImports.map((_, i) => ({
		sourceNodeUid: `import-src-${i}`,
		targetNodeUid: `import-tgt-${i}`,
	}));

	const nodeFiles: NodeFileMapping[] = resolvedImports.flatMap((r, i) => [
		{ nodeUid: `import-src-${i}`, fileUid: r.sourceFileUid },
		{ nodeUid: `import-tgt-${i}`, fileUid: r.targetFileUid },
	]);

	const fileOwnership: FileOwnershipMapping[] = ownership.map((o) => ({
		fileUid: o.fileUid,
		moduleCandidateUid: o.moduleCandidateUid,
	}));

	// 3. Run pure derivation.
	const derivationResult = deriveModuleDependencyEdges({
		importsEdges,
		nodeFiles,
		fileOwnership,
	});

	// 4. Build module lookup for enrichment.
	const moduleMap = new Map<string, ModuleCandidateRef>();
	for (const c of candidates) {
		moduleMap.set(c.moduleCandidateUid, {
			moduleCandidateUid: c.moduleCandidateUid,
			moduleKey: c.moduleKey,
			canonicalRootPath: c.canonicalRootPath,
			moduleKind: c.moduleKind,
			displayName: c.displayName,
		});
	}

	// 5. Enrich edges with module identity.
	let enrichedEdges: EnrichedModuleDependencyEdge[] = derivationResult.edges
		.map((e) => {
			const srcMod = moduleMap.get(e.sourceModuleUid);
			const tgtMod = moduleMap.get(e.targetModuleUid);
			if (!srcMod || !tgtMod) return null; // Should not happen if data is consistent.
			return {
				sourceModuleUid: e.sourceModuleUid,
				sourceModuleKey: srcMod.moduleKey,
				sourceRootPath: srcMod.canonicalRootPath,
				sourceModuleKind: srcMod.moduleKind,
				sourceDisplayName: srcMod.displayName,

				targetModuleUid: e.targetModuleUid,
				targetModuleKey: tgtMod.moduleKey,
				targetRootPath: tgtMod.canonicalRootPath,
				targetModuleKind: tgtMod.moduleKind,
				targetDisplayName: tgtMod.displayName,

				importCount: e.importCount,
				sourceFileCount: e.sourceFileCount,
			};
		})
		.filter((e): e is EnrichedModuleDependencyEdge => e !== null);

	// 6. Validate and apply filter if specified.
	if (filter) {
		// Directional flags require moduleKey — reject invalid combinations.
		if ((filter.outboundOnly || filter.inboundOnly) && !filter.moduleKey) {
			throw new Error(
				"outboundOnly and inboundOnly filters require moduleKey to be set",
			);
		}

		if (filter.moduleKey) {
			const filterKey = filter.moduleKey;
			enrichedEdges = enrichedEdges.filter((e) => {
				const isSource = e.sourceModuleKey === filterKey;
				const isTarget = e.targetModuleKey === filterKey;

				if (filter.outboundOnly) return isSource;
				if (filter.inboundOnly) return isTarget;
				return isSource || isTarget;
			});
		}
	}

	return {
		snapshotUid,
		edges: enrichedEdges,
		diagnostics: derivationResult.diagnostics,
	};
}
