/**
 * Trust rules — pure function tests.
 *
 * Every function in rules.ts is pure. These tests exercise the
 * detection rules and reliability formulas with synthetic inputs.
 * No storage, no fixtures.
 */

import { describe, expect, it } from "vitest";
import {
	computeCallGraphReliability,
	computeChangeImpactReliability,
	computeDeadCodeReliability,
	computeImportGraphReliability,
	countSuspiciousZeroConnectivityModules,
	detectAliasResolutionSuspicion,
	detectFrameworkHeavySuspicion,
	detectMissingEntrypointDeclarations,
	detectNextjsConventions,
	detectReactHeavy,
	detectRegistryPatternSuspicion,
	groupPathPrefixCyclesByAncestor,
	sumUnresolvedCalls,
	sumUnresolvedImports,
} from "../../../src/core/trust/rules.js";

// ── Framework detection ─────────────────────────────────────────

describe("detectNextjsConventions", () => {
	it("detects app router page.tsx", () => {
		expect(
			detectNextjsConventions(["src/app/home/page.tsx", "lib/util.ts"]),
		).toBe(true);
	});

	it("detects app router layout.tsx", () => {
		expect(detectNextjsConventions(["app/[id]/layout.tsx"])).toBe(true);
	});

	it("detects app router route.ts", () => {
		expect(detectNextjsConventions(["app/api/users/route.ts"])).toBe(true);
	});

	it("returns false with no matches", () => {
		expect(
			detectNextjsConventions([
				"src/index.ts",
				"lib/util.ts",
				"components/Button.tsx",
			]),
		).toBe(false);
	});

	it("returns false for app/ without the specific files", () => {
		expect(detectNextjsConventions(["app/README.md"])).toBe(false);
	});
});

describe("detectReactHeavy", () => {
	it("true when >= 20% of files are .tsx", () => {
		const files = [
			"a.tsx",
			"b.tsx",
			"c.ts",
			"d.ts",
			"e.ts",
			"f.ts",
			"g.ts",
			"h.ts",
			"i.ts",
			"j.ts",
		];
		// 2 tsx / 10 = 20%
		expect(detectReactHeavy(files)).toBe(true);
	});

	it("false when < 20% of files are .tsx", () => {
		const files = ["a.tsx", "b.ts", "c.ts", "d.ts", "e.ts", "f.ts"];
		// 1 / 6 = ~16.7%
		expect(detectReactHeavy(files)).toBe(false);
	});

	it("false for empty input", () => {
		expect(detectReactHeavy([])).toBe(false);
	});
});

describe("detectFrameworkHeavySuspicion", () => {
	it("triggered by Next.js conventions alone", () => {
		const result = detectFrameworkHeavySuspicion({
			filePaths: ["app/home/page.tsx"],
		});
		expect(result.triggered).toBe(true);
		expect(result.reasons).toContain("nextjs_app_router_detected");
	});

	it("triggered by react-heavy ratio alone", () => {
		const files = Array.from({ length: 10 }, (_, i) =>
			i < 3 ? `Button${i}.tsx` : `util${i}.ts`,
		);
		const result = detectFrameworkHeavySuspicion({ filePaths: files });
		expect(result.triggered).toBe(true);
		expect(result.reasons).toContain("react_heavy_tsx_ratio");
	});

	it("not triggered for plain TS backend", () => {
		const files = ["src/index.ts", "src/util.ts", "src/service.ts"];
		const result = detectFrameworkHeavySuspicion({ filePaths: files });
		expect(result.triggered).toBe(false);
		expect(result.reasons).toEqual([]);
	});
});

// ── Registry pattern detection ──────────────────────────────────

describe("detectRegistryPatternSuspicion", () => {
	it("triggered when one ancestor has >= 3 parent-child cycles", () => {
		const result = detectRegistryPatternSuspicion({
			pathPrefixCyclesByAncestor: {
				"repo:packages/plugins/src:MODULE": 5,
				"repo:other:MODULE": 1,
			},
			pathPrefixCyclesTotal: 6,
		});
		expect(result.triggered).toBe(true);
		expect(result.reasons.some((r) => r.startsWith("ancestor_with_5"))).toBe(
			true,
		);
	});

	it("triggered when total cycles >= 5 even without dominant ancestor", () => {
		const result = detectRegistryPatternSuspicion({
			pathPrefixCyclesByAncestor: {
				a: 1,
				b: 1,
				c: 1,
				d: 1,
				e: 1,
			},
			pathPrefixCyclesTotal: 5,
		});
		expect(result.triggered).toBe(true);
		expect(result.reasons).toContain("total_parent_child_cycles=5");
	});

	it("not triggered for small parent-child cycle counts", () => {
		const result = detectRegistryPatternSuspicion({
			pathPrefixCyclesByAncestor: { a: 1, b: 2 },
			pathPrefixCyclesTotal: 3,
		});
		expect(result.triggered).toBe(false);
	});

	it("not triggered for empty input", () => {
		const result = detectRegistryPatternSuspicion({
			pathPrefixCyclesByAncestor: {},
			pathPrefixCyclesTotal: 0,
		});
		expect(result.triggered).toBe(false);
	});
});

// ── Missing entrypoint declarations ─────────────────────────────

describe("detectMissingEntrypointDeclarations", () => {
	it("triggered when count is 0", () => {
		const result = detectMissingEntrypointDeclarations({
			activeEntrypointCount: 0,
		});
		expect(result.triggered).toBe(true);
		expect(result.reasons).toContain("active_entrypoint_count=0");
	});

	it("not triggered when count >= 1", () => {
		const result = detectMissingEntrypointDeclarations({
			activeEntrypointCount: 1,
		});
		expect(result.triggered).toBe(false);
	});
});

// ── Alias resolution suspicion ──────────────────────────────────

describe("detectAliasResolutionSuspicion", () => {
	it("triggered when >= 3 suspicious modules", () => {
		const result = detectAliasResolutionSuspicion({
			suspiciousModuleCount: 3,
		});
		expect(result.triggered).toBe(true);
	});

	it("not triggered below threshold", () => {
		expect(
			detectAliasResolutionSuspicion({ suspiciousModuleCount: 2 }).triggered,
		).toBe(false);
		expect(
			detectAliasResolutionSuspicion({ suspiciousModuleCount: 0 }).triggered,
		).toBe(false);
	});
});

// ── Import graph reliability ────────────────────────────────────

describe("computeImportGraphReliability", () => {
	it("LOW when alias_resolution_suspicion triggered", () => {
		const result = computeImportGraphReliability({
			aliasResolutionSuspicion: true,
			registryPatternSuspicion: false,
			unresolvedImportsCount: 0,
		});
		expect(result.level).toBe("LOW");
		expect(result.reasons).toContain("alias_resolution_suspicion");
	});

	it("LOW when any unresolved IMPORTS > 0", () => {
		const result = computeImportGraphReliability({
			aliasResolutionSuspicion: false,
			registryPatternSuspicion: false,
			unresolvedImportsCount: 1,
		});
		expect(result.level).toBe("LOW");
	});

	it("MEDIUM when registry_pattern_suspicion only", () => {
		const result = computeImportGraphReliability({
			aliasResolutionSuspicion: false,
			registryPatternSuspicion: true,
			unresolvedImportsCount: 0,
		});
		expect(result.level).toBe("MEDIUM");
	});

	it("HIGH when nothing triggered", () => {
		const result = computeImportGraphReliability({
			aliasResolutionSuspicion: false,
			registryPatternSuspicion: false,
			unresolvedImportsCount: 0,
		});
		expect(result.level).toBe("HIGH");
		expect(result.reasons).toEqual([]);
	});
});

// ── Call graph reliability ──────────────────────────────────────

describe("computeCallGraphReliability", () => {
	it("LOW when rate < 50%", () => {
		const result = computeCallGraphReliability({
			resolvedCalls: 4,
			unresolvedCalls: 6,
		});
		expect(result.level).toBe("LOW");
	});

	it("MEDIUM when 50% <= rate < 85%", () => {
		const result = computeCallGraphReliability({
			resolvedCalls: 7,
			unresolvedCalls: 3,
		});
		expect(result.level).toBe("MEDIUM");
	});

	it("HIGH when rate >= 85%", () => {
		const result = computeCallGraphReliability({
			resolvedCalls: 9,
			unresolvedCalls: 1,
		});
		expect(result.level).toBe("HIGH");
	});

	it("HIGH when no CALLS edges at all (nothing to fail)", () => {
		const result = computeCallGraphReliability({
			resolvedCalls: 0,
			unresolvedCalls: 0,
		});
		expect(result.level).toBe("HIGH");
	});
});

// ── Dead code reliability ───────────────────────────────────────

describe("computeDeadCodeReliability", () => {
	it("LOW if missing_entrypoint_declarations", () => {
		const result = computeDeadCodeReliability({
			missingEntrypointDeclarations: true,
			registryPatternSuspicion: false,
			frameworkHeavySuspicion: false,
			callGraphLevel: "HIGH",
		});
		expect(result.level).toBe("LOW");
	});

	it("LOW if registry_pattern_suspicion", () => {
		const result = computeDeadCodeReliability({
			missingEntrypointDeclarations: false,
			registryPatternSuspicion: true,
			frameworkHeavySuspicion: false,
			callGraphLevel: "HIGH",
		});
		expect(result.level).toBe("LOW");
	});

	it("LOW if framework_heavy_suspicion", () => {
		const result = computeDeadCodeReliability({
			missingEntrypointDeclarations: false,
			registryPatternSuspicion: false,
			frameworkHeavySuspicion: true,
			callGraphLevel: "HIGH",
		});
		expect(result.level).toBe("LOW");
	});

	it("MEDIUM if call_graph is LOW but no other flags", () => {
		const result = computeDeadCodeReliability({
			missingEntrypointDeclarations: false,
			registryPatternSuspicion: false,
			frameworkHeavySuspicion: false,
			callGraphLevel: "LOW",
		});
		expect(result.level).toBe("MEDIUM");
	});

	it("HIGH otherwise", () => {
		const result = computeDeadCodeReliability({
			missingEntrypointDeclarations: false,
			registryPatternSuspicion: false,
			frameworkHeavySuspicion: false,
			callGraphLevel: "HIGH",
		});
		expect(result.level).toBe("HIGH");
	});
});

// ── Change impact reliability ───────────────────────────────────

describe("computeChangeImpactReliability", () => {
	it("LOW if alias_resolution_suspicion", () => {
		const result = computeChangeImpactReliability({
			aliasResolutionSuspicion: true,
			registryPatternSuspicion: false,
			importGraphLevel: "HIGH",
		});
		expect(result.level).toBe("LOW");
	});

	it("LOW if registry_pattern_suspicion", () => {
		const result = computeChangeImpactReliability({
			aliasResolutionSuspicion: false,
			registryPatternSuspicion: true,
			importGraphLevel: "HIGH",
		});
		expect(result.level).toBe("LOW");
	});

	it("inherits HIGH from import_graph when flags false", () => {
		const result = computeChangeImpactReliability({
			aliasResolutionSuspicion: false,
			registryPatternSuspicion: false,
			importGraphLevel: "HIGH",
		});
		expect(result.level).toBe("HIGH");
	});

	it("inherits MEDIUM from import_graph when flags false", () => {
		const result = computeChangeImpactReliability({
			aliasResolutionSuspicion: false,
			registryPatternSuspicion: false,
			importGraphLevel: "MEDIUM",
		});
		expect(result.level).toBe("MEDIUM");
	});

	it("inherits LOW from import_graph when flags false (gap-fill rule)", () => {
		const result = computeChangeImpactReliability({
			aliasResolutionSuspicion: false,
			registryPatternSuspicion: false,
			importGraphLevel: "LOW",
		});
		expect(result.level).toBe("LOW");
		expect(result.reasons).toContain("import_graph_reliability_low");
	});
});

// ── Aggregation helpers ─────────────────────────────────────────

describe("countSuspiciousZeroConnectivityModules", () => {
	it("counts modules matching all criteria", () => {
		const modules = [
			// match
			{
				qualified_name: "src/admin",
				fan_in: 0,
				fan_out: 0,
				file_count: 3,
			},
			// no match: has connectivity
			{ qualified_name: "src/util", fan_in: 1, fan_out: 0, file_count: 2 },
			// no match: too few files
			{ qualified_name: "src/tiny", fan_in: 0, fan_out: 0, file_count: 1 },
			// no match: repo root
			{ qualified_name: ".", fan_in: 0, fan_out: 0, file_count: 5 },
			// match
			{
				qualified_name: "src/orphan",
				fan_in: 0,
				fan_out: 0,
				file_count: 4,
			},
		];
		expect(countSuspiciousZeroConnectivityModules(modules)).toBe(2);
	});
});

describe("groupPathPrefixCyclesByAncestor", () => {
	it("groups counts per ancestor stable_key", () => {
		const cycles = [
			{ ancestorStableKey: "a" },
			{ ancestorStableKey: "a" },
			{ ancestorStableKey: "b" },
		];
		expect(groupPathPrefixCyclesByAncestor(cycles)).toEqual({
			a: 2,
			b: 1,
		});
	});

	it("returns empty for empty input", () => {
		expect(groupPathPrefixCyclesByAncestor([])).toEqual({});
	});
});

describe("sumUnresolvedCalls / sumUnresolvedImports", () => {
	it("sums CALLS-family categories", () => {
		const diagnostics = {
			diagnostics_version: 1,
			edges_total: 100,
			unresolved_total: 40,
			unresolved_breakdown: {
				calls_obj_method_needs_type_info: 10,
				calls_function_ambiguous_or_missing: 20,
				calls_this_method_needs_class_context: 5,
				calls_this_wildcard_method_needs_type_info: 3,
				imports_file_not_found: 2,
			},
		};
		expect(sumUnresolvedCalls(diagnostics)).toBe(38); // 10+20+5+3
		expect(sumUnresolvedImports(diagnostics)).toBe(2);
	});

	it("returns 0 for missing categories", () => {
		const diagnostics = {
			diagnostics_version: 1,
			edges_total: 100,
			unresolved_total: 0,
			unresolved_breakdown: {},
		};
		expect(sumUnresolvedCalls(diagnostics)).toBe(0);
		expect(sumUnresolvedImports(diagnostics)).toBe(0);
	});
});
