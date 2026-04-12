/**
 * TypeScript half of the classification parity harness.
 *
 * Walks the shared `classification-parity-fixtures/` corpus at the
 * repo root, runs each fixture's `input.json` through the
 * corresponding TS classification function, and compares against
 * `expected.json`.
 *
 * No normalization needed — classification is pure policy with no
 * generated UIDs or timestamps.
 */

import { readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { deriveBlastRadius } from "../../src/core/classification/blast-radius.js";
import {
	type MatchableConsumerFact,
	type MatchableProviderFact,
	matchBoundaryFacts,
	getMatchStrategy,
} from "../../src/core/classification/boundary-matcher.js";
import { detectFrameworkBoundary } from "../../src/core/classification/framework-boundary.js";
import { detectLambdaEntrypoints } from "../../src/core/classification/framework-entrypoints.js";
import { classifyUnresolvedEdge } from "../../src/core/classification/unresolved-classifier.js";
import type { UnresolvedEdgeCategory } from "../../src/core/diagnostics/unresolved-edge-categories.js";
import type { UnresolvedEdgeBasisCode } from "../../src/core/diagnostics/unresolved-edge-classification.js";

// ── Fixture loading ──────────────────────────────────────────────

const FIXTURES_ROOT = join(__dirname, "..", "..", "classification-parity-fixtures");

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
		case "classify_unresolved_edge": {
			const edge = fixture.input.edge as {
				targetKey: string;
				metadataJson: string | null;
			};
			const category = fixture.input
				.category as UnresolvedEdgeCategory;
			const snapshotSignals = fixture.input.snapshotSignals as Parameters<
				typeof classifyUnresolvedEdge
			>[2];
			// FileSignals conversion: the fixture JSON uses arrays
			// for sameFile*Symbols (per the D-A2 DTO collection rule:
			// no HashSet/HashMap in parity-boundary DTOs). The TS
			// FileSignals interface uses ReadonlySet<string>. Convert
			// the arrays to Sets at the harness level so the TS
			// classifier can call .has() on them.
			const rawFs = fixture.input.fileSignals as {
				importBindings: unknown[];
				sameFileValueSymbols: string[];
				sameFileClassSymbols: string[];
				sameFileInterfaceSymbols: string[];
				packageDependencies: { names: string[] };
				tsconfigAliases: { entries: unknown[] };
			};
			const fileSignals = {
				importBindings: rawFs.importBindings,
				sameFileValueSymbols: new Set(rawFs.sameFileValueSymbols),
				sameFileClassSymbols: new Set(rawFs.sameFileClassSymbols),
				sameFileInterfaceSymbols: new Set(rawFs.sameFileInterfaceSymbols),
				packageDependencies: rawFs.packageDependencies,
				tsconfigAliases: rawFs.tsconfigAliases,
			};
			// Build the edge object matching the TS UnresolvedEdge shape.
			// The parity fixture has a narrow ClassifierEdgeInput;
			// the TS function takes a full UnresolvedEdge. Fill in
			// the fields the classifier doesn't read with defaults.
			const fullEdge = {
				edgeUid: "parity-test",
				snapshotUid: "parity-snap",
				repoUid: "parity-repo",
				sourceNodeUid: "parity-source",
				targetKey: edge.targetKey,
				type: "CALLS" as const,
				resolution: "static" as const,
				extractor: "parity:0.0.0",
				location: null,
				metadataJson: edge.metadataJson,
			};
			return classifyUnresolvedEdge(
				fullEdge,
				category,
				snapshotSignals,
				fileSignals as Parameters<typeof classifyUnresolvedEdge>[3],
			);
		}

		case "derive_blast_radius": {
			return deriveBlastRadius({
				category: fixture.input.category as string,
				basisCode: fixture.input.basisCode as string,
				sourceNodeVisibility: (fixture.input.sourceNodeVisibility ??
					null) as string | null,
			});
		}

		case "compute_matcher_key": {
			const mechanism = fixture.input.mechanism as string;
			const address = fixture.input.address as string;
			const metadata = (fixture.input.metadata ?? {}) as Record<
				string,
				unknown
			>;
			const strategy = getMatchStrategy(
				mechanism as Parameters<typeof getMatchStrategy>[0],
			);
			if (!strategy) return null;
			return strategy.computeMatcherKey(address, metadata);
		}

		case "match_boundary_facts": {
			const providers = fixture.input.providers as MatchableProviderFact[];
			const consumers = fixture.input.consumers as MatchableConsumerFact[];
			return matchBoundaryFacts(providers, consumers);
		}

		case "detect_framework_boundary": {
			const targetKey = fixture.input.targetKey as string;
			const category = fixture.input.category as string;
			const importBindings = fixture.input.importBindings as Parameters<
				typeof detectFrameworkBoundary
			>[2];
			return detectFrameworkBoundary(targetKey, category, importBindings);
		}

		case "detect_lambda_entrypoints": {
			const importBindings = fixture.input.importBindings as Parameters<
				typeof detectLambdaEntrypoints
			>[0]["importBindings"];
			const exportedSymbols = fixture.input.exportedSymbols as Parameters<
				typeof detectLambdaEntrypoints
			>[0]["exportedSymbols"];
			return detectLambdaEntrypoints({ importBindings, exportedSymbols });
		}

		default:
			throw new Error(`unknown fn: ${fnName}`);
	}
}

// ── Harness ──────────────────────────────────────────────────────

const fixtures = discoverFixtures();

describe("classification parity — TS half against shared classification-parity-fixtures", () => {
	it("discovered at least one fixture", () => {
		expect(fixtures.length).toBeGreaterThan(0);
	});

	for (const fixture of fixtures) {
		it(`fixture: ${fixture.name}`, () => {
			const actual = runFixture(fixture);
			// JSON round-trip to normalize the actual output
			// (strips class semantics, collapses undefined → absent).
			const normalized =
				actual === null || actual === undefined
					? null
					: JSON.parse(JSON.stringify(actual));
			expect(normalized).toEqual(fixture.expected);
		});
	}
});
