/**
 * Module dependency edge derivation — pure core logic.
 *
 * Derives module-to-module dependency edges from file-level IMPORTS edges
 * and module file ownership. This is fact generation, not policy evaluation.
 *
 * See docs/architecture/module-graph-contract.txt for the full specification.
 *
 * Derivation rule:
 *   For each resolved IMPORTS edge (nodeA -> nodeB):
 *     fileA = file containing nodeA
 *     fileB = file containing nodeB
 *     moduleA = owning module of fileA
 *     moduleB = owning module of fileB
 *     if moduleA exists AND moduleB exists AND moduleA != moduleB:
 *       emit edge (moduleA -> moduleB)
 *
 * Constraints:
 *   - File ownership based, not symbol ownership
 *   - Cross-module edges only (intra-module excluded)
 *   - Resolved imports only (unresolved excluded)
 *
 * No filesystem access. No storage calls. No side effects.
 */

// ── Output types ───────────────────────────────────────────────────

/**
 * A derived module-to-module dependency edge.
 *
 * Direction: sourceModule depends on targetModule
 * (sourceModule imports from targetModule)
 */
export interface ModuleDependencyEdge {
	/** Module containing the importing files. */
	readonly sourceModuleUid: string;
	/** Module containing the imported files/symbols. */
	readonly targetModuleUid: string;
	/** Total count of IMPORTS edges from source to target module. */
	readonly importCount: number;
	/** Distinct files in source module that import from target module. */
	readonly sourceFileCount: number;
}

/**
 * Result of module dependency edge derivation.
 */
export interface ModuleDependencyResult {
	/** Derived cross-module dependency edges. */
	readonly edges: ModuleDependencyEdge[];
	/** Diagnostics about the derivation process. */
	readonly diagnostics: DerivationDiagnostics;
}

export interface DerivationDiagnostics {
	/** Total IMPORTS edges considered. */
	readonly importsEdgesTotal: number;
	/** IMPORTS edges where source node has no file. */
	readonly importsSourceNoFile: number;
	/** IMPORTS edges where target node has no file. */
	readonly importsTargetNoFile: number;
	/** IMPORTS edges where source file has no module owner. */
	readonly importsSourceNoModule: number;
	/** IMPORTS edges where target file has no module owner. */
	readonly importsTargetNoModule: number;
	/** IMPORTS edges within the same module (excluded). */
	readonly importsIntraModule: number;
	/** IMPORTS edges that contributed to cross-module edges. */
	readonly importsCrossModule: number;
}

// ── Input types ────────────────────────────────────────────────────

/**
 * Minimal edge representation for derivation.
 * Only resolved IMPORTS edges should be passed.
 */
export interface ImportEdgeInput {
	readonly sourceNodeUid: string;
	readonly targetNodeUid: string;
}

/**
 * Minimal node representation for derivation.
 * Only need nodeUid -> fileUid mapping.
 */
export interface NodeFileMapping {
	readonly nodeUid: string;
	readonly fileUid: string | null;
}

/**
 * Minimal ownership representation for derivation.
 * Only need fileUid -> moduleCandidateUid mapping.
 */
export interface FileOwnershipMapping {
	readonly fileUid: string;
	readonly moduleCandidateUid: string;
}

// ── Derivation ─────────────────────────────────────────────────────

export interface DeriveModuleDependencyEdgesInput {
	/** Resolved IMPORTS edges only. */
	readonly importsEdges: ImportEdgeInput[];
	/** Node to file mapping. */
	readonly nodeFiles: NodeFileMapping[];
	/** File to module ownership mapping. */
	readonly fileOwnership: FileOwnershipMapping[];
}

/**
 * Derive module-to-module dependency edges from file-level IMPORTS.
 *
 * Pure function. No side effects.
 */
export function deriveModuleDependencyEdges(
	input: DeriveModuleDependencyEdgesInput,
): ModuleDependencyResult {
	const { importsEdges, nodeFiles, fileOwnership } = input;

	// Build lookup maps for O(1) access.
	const nodeToFile = new Map<string, string>();
	for (const nf of nodeFiles) {
		if (nf.fileUid) {
			nodeToFile.set(nf.nodeUid, nf.fileUid);
		}
	}

	const fileToModule = new Map<string, string>();
	for (const fo of fileOwnership) {
		fileToModule.set(fo.fileUid, fo.moduleCandidateUid);
	}

	// Track edge aggregation: (sourceModule, targetModule) -> { importCount, sourceFiles }
	const edgeMap = new Map<
		string,
		{ importCount: number; sourceFiles: Set<string> }
	>();

	// Mutable diagnostics counters.
	let sourceNoFile = 0;
	let targetNoFile = 0;
	let sourceNoModule = 0;
	let targetNoModule = 0;
	let intraModule = 0;
	let crossModule = 0;

	for (const edge of importsEdges) {
		// Resolve source node -> file -> module.
		const sourceFile = nodeToFile.get(edge.sourceNodeUid);
		if (!sourceFile) {
			sourceNoFile++;
			continue;
		}
		const sourceModule = fileToModule.get(sourceFile);
		if (!sourceModule) {
			sourceNoModule++;
			continue;
		}

		// Resolve target node -> file -> module.
		const targetFile = nodeToFile.get(edge.targetNodeUid);
		if (!targetFile) {
			targetNoFile++;
			continue;
		}
		const targetModule = fileToModule.get(targetFile);
		if (!targetModule) {
			targetNoModule++;
			continue;
		}

		// Skip intra-module imports.
		if (sourceModule === targetModule) {
			intraModule++;
			continue;
		}

		// Cross-module import: aggregate.
		crossModule++;
		const key = `${sourceModule}|${targetModule}`;
		const existing = edgeMap.get(key);
		if (existing) {
			existing.importCount++;
			existing.sourceFiles.add(sourceFile);
		} else {
			edgeMap.set(key, {
				importCount: 1,
				sourceFiles: new Set([sourceFile]),
			});
		}
	}

	// Convert aggregated map to output edges.
	const edges: ModuleDependencyEdge[] = [];
	for (const [key, agg] of edgeMap) {
		const [sourceModuleUid, targetModuleUid] = key.split("|");
		edges.push({
			sourceModuleUid,
			targetModuleUid,
			importCount: agg.importCount,
			sourceFileCount: agg.sourceFiles.size,
		});
	}

	// Sort for deterministic output:
	// 1. importCount DESC (most important edges first)
	// 2. sourceModuleUid ASC (stable tie-breaker)
	// 3. targetModuleUid ASC (stable tie-breaker)
	edges.sort((a, b) => {
		if (b.importCount !== a.importCount) {
			return b.importCount - a.importCount;
		}
		const srcCmp = a.sourceModuleUid.localeCompare(b.sourceModuleUid);
		if (srcCmp !== 0) return srcCmp;
		return a.targetModuleUid.localeCompare(b.targetModuleUid);
	});

	return {
		edges,
		diagnostics: {
			importsEdgesTotal: importsEdges.length,
			importsSourceNoFile: sourceNoFile,
			importsTargetNoFile: targetNoFile,
			importsSourceNoModule: sourceNoModule,
			importsTargetNoModule: targetNoModule,
			importsIntraModule: intraModule,
			importsCrossModule: crossModule,
		},
	};
}
