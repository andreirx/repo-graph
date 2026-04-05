/**
 * Trust reporting — deterministic detection rules + reliability formulas.
 *
 * Every function in this module is PURE. No storage access, no I/O.
 * Inputs are plain data; outputs are flags and reasons.
 *
 * This isolation serves two purposes:
 *   1. rules are unit-testable without fixtures
 *   2. the formulas are auditable — a consumer can read this file
 *      and know exactly why a score was assigned
 *
 * The reliability level semantics:
 *   HIGH    — the command's claimed behavior is reliable on this repo
 *   MEDIUM  — reliable enough for orientation but not decision-critical action
 *   LOW     — do not act on this command's output without manual verification
 */

import {
	CALLS_CATEGORIES,
	IMPORTS_CATEGORIES,
	UnresolvedEdgeCategory,
} from "../diagnostics/unresolved-edge-categories.js";
import type {
	DowngradeTrigger,
	ExtractionDiagnostics,
	ReliabilityAxisScore,
} from "./types.js";

// ── Downgrade detection ─────────────────────────────────────────────

/**
 * Next.js app-router convention detection.
 * Matches any file under `app/` ending in page.tsx/page.jsx,
 * layout.tsx/layout.jsx, or route.ts/route.js.
 */
export function detectNextjsConventions(filePaths: string[]): boolean {
	const patterns = [
		/(^|\/)app\/.*\/page\.(tsx|jsx)$/,
		/(^|\/)app\/.*\/layout\.(tsx|jsx)$/,
		/(^|\/)app\/.*\/route\.(ts|js)$/,
	];
	for (const path of filePaths) {
		for (const re of patterns) {
			if (re.test(path)) return true;
		}
	}
	return false;
}

/**
 * React-heavy UI surface detection.
 * First-slice proxy: (tsx + jsx) file ratio >= 0.20.
 *
 * Counts both .tsx and .jsx extensions because both imply React
 * component code regardless of TS vs JS. The "framework_heavy"
 * downgrade applies equally to JS-React codebases.
 *
 * The full rule also requires package manifest to indicate `react`
 * as a dependency. That subcheck is deferred until the manifest
 * extractor captures dependencies.
 */
export function detectReactHeavy(filePaths: string[]): boolean {
	if (filePaths.length === 0) return false;
	let reactCount = 0;
	for (const path of filePaths) {
		if (path.endsWith(".tsx") || path.endsWith(".jsx")) reactCount++;
	}
	const ratio = reactCount / filePaths.length;
	return ratio >= 0.2;
}

/**
 * Framework-heavy suspicion: Next.js conventions OR React-heavy ratio.
 */
export function detectFrameworkHeavySuspicion(input: {
	filePaths: string[];
}): DowngradeTrigger {
	const reasons: string[] = [];
	if (detectNextjsConventions(input.filePaths)) {
		reasons.push("nextjs_app_router_detected");
	}
	if (detectReactHeavy(input.filePaths)) {
		reasons.push("react_heavy_tsx_ratio");
	}
	return { triggered: reasons.length > 0, reasons };
}

/**
 * Registry-pattern suspicion: 2-node module cycles where one module
 * is a strict path-prefix ancestor of the other.
 *
 * Triggers:
 *   - any single ancestor participates in >= 3 such parent-child cycles, OR
 *   - total such cycles across the repo >= 5.
 */
export function detectRegistryPatternSuspicion(input: {
	pathPrefixCyclesByAncestor: Record<string, number>;
	pathPrefixCyclesTotal: number;
}): DowngradeTrigger {
	const reasons: string[] = [];
	let maxPerAncestor = 0;
	let topAncestor: string | null = null;
	for (const [ancestor, count] of Object.entries(
		input.pathPrefixCyclesByAncestor,
	)) {
		if (count > maxPerAncestor) {
			maxPerAncestor = count;
			topAncestor = ancestor;
		}
	}
	if (maxPerAncestor >= 3) {
		reasons.push(
			`ancestor_with_${maxPerAncestor}_parent_child_cycles:${topAncestor}`,
		);
	}
	if (input.pathPrefixCyclesTotal >= 5) {
		reasons.push(`total_parent_child_cycles=${input.pathPrefixCyclesTotal}`);
	}
	return { triggered: reasons.length > 0, reasons };
}

/**
 * Missing entrypoint declarations: active entrypoint count === 0.
 */
export function detectMissingEntrypointDeclarations(input: {
	activeEntrypointCount: number;
}): DowngradeTrigger {
	if (input.activeEntrypointCount === 0) {
		return {
			triggered: true,
			reasons: ["active_entrypoint_count=0"],
		};
	}
	return { triggered: false, reasons: [] };
}

/**
 * Alias-resolution suspicion: count of modules with fan_in=0,
 * fan_out=0, file_count>=2, and non-root path is >= 3.
 */
export function detectAliasResolutionSuspicion(input: {
	suspiciousModuleCount: number;
}): DowngradeTrigger {
	if (input.suspiciousModuleCount >= 3) {
		return {
			triggered: true,
			reasons: [`suspicious_zero_connectivity_modules=${input.suspiciousModuleCount}`],
		};
	}
	return { triggered: false, reasons: [] };
}

// ── Reliability formulas ────────────────────────────────────────────

/**
 * Import graph reliability.
 *   LOW     if alias_resolution_suspicion OR any unresolved IMPORTS
 *   MEDIUM  if registry_pattern_suspicion triggered
 *   HIGH    otherwise
 */
export function computeImportGraphReliability(input: {
	aliasResolutionSuspicion: boolean;
	registryPatternSuspicion: boolean;
	unresolvedImportsCount: number;
}): ReliabilityAxisScore {
	const reasons: string[] = [];
	if (input.aliasResolutionSuspicion) {
		reasons.push("alias_resolution_suspicion");
	}
	if (input.unresolvedImportsCount > 0) {
		reasons.push(`unresolved_imports=${input.unresolvedImportsCount}`);
	}
	if (reasons.length > 0) return { level: "LOW", reasons };
	if (input.registryPatternSuspicion) {
		return {
			level: "MEDIUM",
			reasons: ["registry_pattern_suspicion"],
		};
	}
	return { level: "HIGH", reasons: [] };
}

/**
 * Call graph reliability.
 *   rate = resolved_calls / (resolved_calls + unresolved_calls)
 *   LOW     if rate < 0.50
 *   MEDIUM  if 0.50 <= rate < 0.85
 *   HIGH    if rate >= 0.85
 *
 * Edge case: if resolved + unresolved == 0 (no CALLS edges at all),
 * the rate is undefined. We return HIGH — there is nothing to fail.
 */
export function computeCallGraphReliability(input: {
	resolvedCalls: number;
	unresolvedCalls: number;
}): ReliabilityAxisScore {
	const total = input.resolvedCalls + input.unresolvedCalls;
	if (total === 0) {
		return { level: "HIGH", reasons: [] };
	}
	const rate = input.resolvedCalls / total;
	const ratePct = (rate * 100).toFixed(1);
	if (rate < 0.5) {
		return {
			level: "LOW",
			reasons: [`call_resolution_rate=${ratePct}%_below_50%`],
		};
	}
	if (rate < 0.85) {
		return {
			level: "MEDIUM",
			reasons: [`call_resolution_rate=${ratePct}%_below_85%`],
		};
	}
	return { level: "HIGH", reasons: [] };
}

/**
 * Dead-code reliability.
 *   LOW     if missing_entrypoint OR registry_pattern OR framework_heavy
 *   MEDIUM  if call_graph_reliability is LOW but the above three are false
 *   HIGH    otherwise
 */
export function computeDeadCodeReliability(input: {
	missingEntrypointDeclarations: boolean;
	registryPatternSuspicion: boolean;
	frameworkHeavySuspicion: boolean;
	callGraphLevel: ReliabilityAxisScore["level"];
}): ReliabilityAxisScore {
	const reasons: string[] = [];
	if (input.missingEntrypointDeclarations) {
		reasons.push("missing_entrypoint_declarations");
	}
	if (input.registryPatternSuspicion) {
		reasons.push("registry_pattern_suspicion");
	}
	if (input.frameworkHeavySuspicion) {
		reasons.push("framework_heavy_suspicion");
	}
	if (reasons.length > 0) return { level: "LOW", reasons };
	if (input.callGraphLevel === "LOW") {
		return {
			level: "MEDIUM",
			reasons: ["call_graph_reliability_low"],
		};
	}
	return { level: "HIGH", reasons: [] };
}

/**
 * Change-impact reliability.
 *
 * Rules (with one assumption — see note below):
 *   LOW     if alias_resolution_suspicion OR registry_pattern_suspicion
 *   Otherwise inherit the import_graph reliability level, because
 *   change-impact propagation is bounded by import-graph completeness:
 *     HIGH   if import_graph is HIGH
 *     MEDIUM if import_graph is MEDIUM
 *     LOW    if import_graph is LOW
 *
 * Note: this measures reliability of the command's CLAIMED semantics
 * (reverse MODULE IMPORTS only), not completeness against every
 * runtime behavior. change impact can be HIGH despite excluding
 * CALLS because the command explicitly claims that scope.
 *
 * Assumption: the user's original formula specified HIGH when
 * import_graph is HIGH and both flags are false, MEDIUM when
 * import_graph is MEDIUM, and LOW when either suspicion flag is
 * triggered — but did not specify the case where import_graph is
 * LOW and both flags are false. Because change-impact is
 * derivatively bounded by import-graph correctness, this
 * implementation falls through to the import_graph level
 * ("cannot be more reliable than our primary input").
 */
export function computeChangeImpactReliability(input: {
	aliasResolutionSuspicion: boolean;
	registryPatternSuspicion: boolean;
	importGraphLevel: ReliabilityAxisScore["level"];
}): ReliabilityAxisScore {
	const reasons: string[] = [];
	if (input.aliasResolutionSuspicion) {
		reasons.push("alias_resolution_suspicion");
	}
	if (input.registryPatternSuspicion) {
		reasons.push("registry_pattern_suspicion");
	}
	if (reasons.length > 0) return { level: "LOW", reasons };
	// Inherit import_graph level as the upper bound for change_impact.
	switch (input.importGraphLevel) {
		case "HIGH":
			return { level: "HIGH", reasons: [] };
		case "MEDIUM":
			return {
				level: "MEDIUM",
				reasons: ["import_graph_reliability_medium"],
			};
		case "LOW":
			return {
				level: "LOW",
				reasons: ["import_graph_reliability_low"],
			};
	}
}

// ── Diagnostic aggregation helpers ──────────────────────────────────

/**
 * Sum counts for CALLS-family unresolved categories.
 */
export function sumUnresolvedCalls(
	diagnostics: ExtractionDiagnostics,
): number {
	let total = 0;
	for (const cat of CALLS_CATEGORIES) {
		total += diagnostics.unresolved_breakdown[cat] ?? 0;
	}
	return total;
}

/**
 * Sum counts for IMPORTS-family unresolved categories.
 */
export function sumUnresolvedImports(
	diagnostics: ExtractionDiagnostics,
): number {
	let total = 0;
	for (const cat of IMPORTS_CATEGORIES) {
		total += diagnostics.unresolved_breakdown[cat] ?? 0;
	}
	return total;
}

/**
 * Count modules matching the suspicious-zero-connectivity pattern:
 *   fan_in = 0 AND fan_out = 0 AND file_count >= 2 AND not repo root.
 *
 * "Repo root" is heuristically detected as qualified_name === "." or "".
 */
export function countSuspiciousZeroConnectivityModules(
	modules: Array<{
		qualified_name: string;
		fan_in: number;
		fan_out: number;
		file_count: number;
	}>,
): number {
	let count = 0;
	for (const m of modules) {
		if (
			m.fan_in === 0 &&
			m.fan_out === 0 &&
			m.file_count >= 2 &&
			m.qualified_name !== "." &&
			m.qualified_name !== ""
		) {
			count++;
		}
	}
	return count;
}

/**
 * Group path-prefix cycles by ancestor stable_key and return counts.
 */
export function groupPathPrefixCyclesByAncestor(
	cycles: Array<{ ancestorStableKey: string }>,
): Record<string, number> {
	const byAncestor: Record<string, number> = {};
	for (const c of cycles) {
		byAncestor[c.ancestorStableKey] =
			(byAncestor[c.ancestorStableKey] ?? 0) + 1;
	}
	return byAncestor;
}

// Re-export for convenience in the service
export { UnresolvedEdgeCategory };
