/**
 * TypeScript half of the trust parity harness.
 *
 * Walks the shared `trust-parity-fixtures/` corpus at the repo root,
 * runs each fixture's `input.json` through the corresponding TS trust
 * function, and compares against `expected.json`.
 *
 * Two tiers:
 *   - `rules__*` fixtures test individual rule functions.
 *   - `report__*` fixtures test the full `computeTrustReport` via a
 *     mock StoragePort that returns the fixture's pre-baked data.
 *
 * No normalization needed — trust computation is pure policy with no
 * generated UIDs or timestamps.
 */

import { readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import {
	computeCallGraphReliability,
	computeChangeImpactReliability,
	computeDeadCodeReliability,
	computeImportGraphReliability,
	detectAliasResolutionSuspicion,
	detectFrameworkHeavySuspicion,
	detectMissingEntrypointDeclarations,
	detectRegistryPatternSuspicion,
} from "../../src/core/trust/rules.js";
import { computeTrustReport } from "../../src/core/trust/service.js";
import type { StoragePort } from "../../src/core/ports/storage.js";

// ── Fixture loading ──────────────────────────────────────────────

const FIXTURES_ROOT = join(__dirname, "..", "..", "trust-parity-fixtures");

interface Fixture {
	name: string;
	input: Record<string, unknown>;
	expected: unknown;
}

function discoverFixtures(): Fixture[] {
	const fixtures: Fixture[] = [];
	for (const entry of readdirSync(FIXTURES_ROOT)) {
		const fullPath = join(FIXTURES_ROOT, entry);
		if (!statSync(fullPath).isDirectory()) continue;
		const inputPath = join(fullPath, "input.json");
		const expectedPath = join(fullPath, "expected.json");
		try {
			statSync(inputPath);
			statSync(expectedPath);
		} catch {
			continue;
		}
		const input = JSON.parse(readFileSync(inputPath, "utf-8"));
		const expected = JSON.parse(readFileSync(expectedPath, "utf-8"));
		fixtures.push({ name: entry, input, expected });
	}
	fixtures.sort((a, b) => a.name.localeCompare(b.name));
	return fixtures;
}

// ── Dispatch ─────────────────────────────────────────────────────

function runFixture(fixture: Fixture): unknown {
	const fnName = fixture.input.fn as string;

	switch (fnName) {
		case "detect_framework_heavy_suspicion":
			return detectFrameworkHeavySuspicion({
				filePaths: fixture.input.filePaths as string[],
			});

		case "detect_alias_resolution_suspicion":
			return detectAliasResolutionSuspicion({
				suspiciousModuleCount: fixture.input.suspiciousModuleCount as number,
			});

		case "detect_missing_entrypoint_declarations":
			return detectMissingEntrypointDeclarations({
				activeEntrypointCount: fixture.input.activeEntrypointCount as number,
			});

		case "detect_registry_pattern_suspicion": {
			// Fixture stores as [[key, count], ...]; TS expects Record<string, number>.
			const pairs = fixture.input.pathPrefixCyclesByAncestor as Array<
				[string, number]
			>;
			const byAncestor: Record<string, number> = {};
			for (const [k, v] of pairs) {
				byAncestor[k] = v;
			}
			return detectRegistryPatternSuspicion({
				pathPrefixCyclesByAncestor: byAncestor,
				pathPrefixCyclesTotal: fixture.input.pathPrefixCyclesTotal as number,
			});
		}

		case "compute_call_graph_reliability":
			return computeCallGraphReliability({
				resolvedCalls: fixture.input.resolvedCalls as number,
				unresolvedCallsInternalLike:
					fixture.input.unresolvedCallsInternalLike as number,
			});

		case "compute_import_graph_reliability":
			return computeImportGraphReliability({
				aliasResolutionSuspicion: fixture.input.aliasResolutionSuspicion as boolean,
				registryPatternSuspicion: fixture.input.registryPatternSuspicion as boolean,
				unresolvedImportsCount: fixture.input.unresolvedImportsCount as number,
			});

		case "compute_dead_code_reliability":
			return computeDeadCodeReliability({
				missingEntrypointDeclarations:
					fixture.input.missingEntrypointDeclarations as boolean,
				registryPatternSuspicion: fixture.input.registryPatternSuspicion as boolean,
				frameworkHeavySuspicion: fixture.input.frameworkHeavySuspicion as boolean,
				callGraphLevel: fixture.input.callGraphLevel as "HIGH" | "MEDIUM" | "LOW",
			});

		case "compute_change_impact_reliability":
			return computeChangeImpactReliability({
				aliasResolutionSuspicion: fixture.input.aliasResolutionSuspicion as boolean,
				registryPatternSuspicion: fixture.input.registryPatternSuspicion as boolean,
				importGraphLevel: fixture.input.importGraphLevel as "HIGH" | "MEDIUM" | "LOW",
			});

		case "compute_trust_report":
			return dispatchComputeTrustReport(fixture.input);

		default:
			throw new Error(`unknown fn: ${fnName}`);
	}
}

// ── Full report dispatch via mock storage ─────────────────────────

function dispatchComputeTrustReport(
	input: Record<string, unknown>,
): unknown {
	const diagnosticsJson = input.diagnostics
		? JSON.stringify(input.diagnostics)
		: null;
	const toolchainJson = input.toolchain
		? JSON.stringify(input.toolchain)
		: null;

	const moduleStats = (
		input.moduleStats as Array<Record<string, unknown>>
	).map((m) => ({
		stableKey: m.stableKey as string,
		name: (m.stableKey as string).split(":").pop()?.replace(":MODULE", "") ?? "",
		path: m.path as string,
		fanIn: m.fanIn as number,
		fanOut: m.fanOut as number,
		instability: 0,
		abstractness: 0,
		distanceFromMainSequence: 0,
		fileCount: m.fileCount as number,
		symbolCount: 0,
	}));

	const pathPrefixCycles = (
		input.pathPrefixCycles as Array<Record<string, unknown>>
	).map((c) => ({
		ancestorStableKey: c.ancestorStableKey as string,
		descendantStableKey: c.descendantStableKey as string,
	}));

	const callsClassificationCounts = (
		input.callsClassificationCounts as Array<Record<string, unknown>>
	).map((r) => ({
		key: r.classification as string,
		count: r.count as number,
	}));

	const allClassificationCounts = (
		input.allClassificationCounts as Array<Record<string, unknown>>
	).map((r) => ({
		key: r.classification as string,
		count: r.count as number,
	}));

	const unknownCallsSamples = (
		input.unknownCallsSamples as Array<Record<string, unknown>>
	).map((s) => ({
		edgeUid: "parity-edge",
		classification: "unknown",
		category: s.category as string,
		basisCode: s.basisCode as string,
		targetKey: "parity-target",
		sourceNodeUid: "parity-source",
		sourceStableKey: "parity-stable-key",
		sourceFilePath: null,
		lineStart: null,
		colStart: null,
		sourceNodeVisibility: (s.sourceNodeVisibility as string | null) ?? null,
		metadataJson: (s.metadataJson as string | null) ?? null,
	}));

	// Build a mock storage that returns the fixture data.
	const CALLS_CATEGORIES = [
		"calls_this_wildcard_method_needs_type_info",
		"calls_this_method_needs_class_context",
		"calls_obj_method_needs_type_info",
		"calls_function_ambiguous_or_missing",
	];

	const mockStorage = {
		getSnapshotExtractionDiagnostics: () => diagnosticsJson,
		getFilesByRepo: () =>
			(input.filePaths as string[]).map((p) => ({ path: p })),
		computeModuleStats: () => moduleStats,
		findPathPrefixModuleCycles: () => pathPrefixCycles,
		getActiveDeclarations: () =>
			new Array(input.activeEntrypointCount as number),
		countEdgesByType: () => input.resolvedCalls as number,
		countUnresolvedEdges: (args: {
			snapshotUid: string;
			groupBy: string;
			filterCategories?: string[];
		}) => {
			if (
				args.filterCategories &&
				args.filterCategories.length > 0
			) {
				return callsClassificationCounts;
			}
			return allClassificationCounts;
		},
		queryUnresolvedEdges: () => unknownCallsSamples,
	} as unknown as StoragePort;

	return computeTrustReport({
		storage: mockStorage,
		repoUid: "r1",
		snapshotUid: input.snapshotUid as string,
		snapshotBasisCommit: (input.basisCommit as string | null) ?? null,
		snapshotToolchainJson: toolchainJson,
	});
}

// ── Harness ──────────────────────────────────────────────────────

const fixtures = discoverFixtures();

describe("trust parity — TS half against shared trust-parity-fixtures", () => {
	it("discovered at least one fixture", () => {
		expect(fixtures.length).toBeGreaterThan(0);
	});

	for (const fixture of fixtures) {
		it(`fixture: ${fixture.name}`, () => {
			const actual = runFixture(fixture);
			const normalized =
				actual === null || actual === undefined
					? null
					: JSON.parse(JSON.stringify(actual));
			expect(normalized).toEqual(fixture.expected);
		});
	}
});
