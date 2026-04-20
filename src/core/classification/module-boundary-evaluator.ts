/**
 * Module boundary evaluator — pure policy core for discovered-module boundaries.
 *
 * Evaluates boundary declarations against derived module dependency edges.
 * Detects violations (forbidden edges that exist) and stale declarations
 * (boundaries referencing modules that no longer exist in the snapshot).
 *
 * Design decisions:
 *   - Evaluator receives pre-parsed typed DTOs, not storage shapes
 *   - Stale declarations are excluded from violation evaluation
 *   - Output ordering is deterministic for stable tests/CLI
 *
 * This is pure policy. No DB access. No side effects.
 */

// ── Evaluator input DTOs ───────────────────────────────────────────

/**
 * Normalized boundary declaration for evaluation.
 * Adapter parses Declaration + filters discovered_module + extracts paths.
 */
export interface EvaluatableDiscoveredModuleBoundary {
	readonly declarationUid: string;
	readonly sourceCanonicalPath: string;
	readonly targetCanonicalPath: string;
	readonly reason?: string;
}

/**
 * Normalized module dependency edge for evaluation.
 * Adapter derives from cross-module import graph.
 */
export interface EvaluatableModuleDependencyEdge {
	readonly sourceCanonicalPath: string;
	readonly targetCanonicalPath: string;
	readonly importCount: number;
	readonly sourceFileCount: number;
}

/**
 * Evaluator input bundle.
 */
export interface ModuleBoundaryInput {
	/** Boundary declarations (discovered_module domain, pre-parsed) */
	readonly boundaries: readonly EvaluatableDiscoveredModuleBoundary[];
	/** Derived cross-module edges */
	readonly edges: readonly EvaluatableModuleDependencyEdge[];
	/** Current snapshot module set: canonicalRootPath → moduleUid */
	readonly moduleIndex: ReadonlyMap<string, string>;
}

// ── Evaluator output DTOs ──────────────────────────────────────────

/**
 * A boundary violation: a forbidden edge exists in the module graph.
 */
export interface ModuleBoundaryViolation {
	readonly declarationUid: string;
	readonly sourceCanonicalPath: string;
	readonly targetCanonicalPath: string;
	readonly importCount: number;
	readonly sourceFileCount: number;
	readonly reason?: string;
}

/**
 * A stale boundary declaration: one or both modules no longer exist.
 */
export interface StaleBoundaryDeclaration {
	readonly declarationUid: string;
	readonly staleSide: "source" | "target" | "both";
	readonly missingPaths: readonly string[];
}

/**
 * Evaluation result.
 */
export interface ModuleBoundaryEvaluation {
	readonly violations: readonly ModuleBoundaryViolation[];
	readonly staleDeclarations: readonly StaleBoundaryDeclaration[];
}

// ── Pure evaluator ─────────────────────────────────────────────────

/**
 * Evaluate module boundaries against the current module graph.
 *
 * For each boundary declaration:
 *   1. Check if source and target modules exist in the current snapshot
 *   2. If either is missing → report as stale, skip violation check
 *   3. If both exist → check if a forbidden edge exists
 *   4. If edge exists → report as violation
 *
 * Output is deterministically ordered:
 *   - staleDeclarations: sorted by declarationUid
 *   - violations: sorted by (sourceCanonicalPath, targetCanonicalPath, declarationUid)
 */
export function evaluateModuleBoundaries(
	input: ModuleBoundaryInput,
): ModuleBoundaryEvaluation {
	const { boundaries, edges, moduleIndex } = input;

	// Build edge lookup: "source|target" → edge
	const edgeMap = new Map<string, EvaluatableModuleDependencyEdge>();
	for (const edge of edges) {
		const key = `${edge.sourceCanonicalPath}|${edge.targetCanonicalPath}`;
		edgeMap.set(key, edge);
	}

	const violations: ModuleBoundaryViolation[] = [];
	const staleDeclarations: StaleBoundaryDeclaration[] = [];

	for (const boundary of boundaries) {
		const sourceExists = moduleIndex.has(boundary.sourceCanonicalPath);
		const targetExists = moduleIndex.has(boundary.targetCanonicalPath);

		// Check for staleness first — stale declarations are not evaluated for violations
		if (!sourceExists || !targetExists) {
			const missingPaths: string[] = [];
			let staleSide: "source" | "target" | "both";

			if (!sourceExists && !targetExists) {
				staleSide = "both";
				missingPaths.push(
					boundary.sourceCanonicalPath,
					boundary.targetCanonicalPath,
				);
			} else if (!sourceExists) {
				staleSide = "source";
				missingPaths.push(boundary.sourceCanonicalPath);
			} else {
				staleSide = "target";
				missingPaths.push(boundary.targetCanonicalPath);
			}

			staleDeclarations.push({
				declarationUid: boundary.declarationUid,
				staleSide,
				missingPaths,
			});
			continue; // Do not evaluate for violations
		}

		// Both modules exist — check if forbidden edge exists
		const edgeKey = `${boundary.sourceCanonicalPath}|${boundary.targetCanonicalPath}`;
		const edge = edgeMap.get(edgeKey);

		if (edge) {
			violations.push({
				declarationUid: boundary.declarationUid,
				sourceCanonicalPath: boundary.sourceCanonicalPath,
				targetCanonicalPath: boundary.targetCanonicalPath,
				importCount: edge.importCount,
				sourceFileCount: edge.sourceFileCount,
				reason: boundary.reason,
			});
		}
	}

	// Sort for deterministic output
	staleDeclarations.sort((a, b) =>
		a.declarationUid.localeCompare(b.declarationUid),
	);

	violations.sort((a, b) => {
		const sourceCmp = a.sourceCanonicalPath.localeCompare(
			b.sourceCanonicalPath,
		);
		if (sourceCmp !== 0) return sourceCmp;
		const targetCmp = a.targetCanonicalPath.localeCompare(
			b.targetCanonicalPath,
		);
		if (targetCmp !== 0) return targetCmp;
		return a.declarationUid.localeCompare(b.declarationUid);
	});

	return { violations, staleDeclarations };
}
