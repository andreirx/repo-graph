/**
 * TypeScript half of the ts-extractor parity harness.
 *
 * Walks `ts-extractor-parity-fixtures/`, runs each fixture through
 * the real TS TypeScriptExtractor, normalizes to the canonical
 * comparison shape, and compares against `expected.json`.
 */

import { readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { TypeScriptExtractor } from "../../src/adapters/extractors/typescript/ts-extractor.js";

const FIXTURES_ROOT = join(__dirname, "..", "..", "ts-extractor-parity-fixtures");

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
		fixtures.push({
			name: entry,
			input: JSON.parse(readFileSync(inputPath, "utf-8")),
			expected: JSON.parse(readFileSync(expectedPath, "utf-8")),
		});
	}
	fixtures.sort((a, b) => a.name.localeCompare(b.name));
	return fixtures;
}

/**
 * Normalize extraction result to the canonical comparison shape.
 * Strips volatile fields (UIDs, extractor version).
 * Retains location on nodes, edges, and import bindings (stable
 * for the same source text and grammar version).
 */
function normalizeResult(result: Awaited<ReturnType<TypeScriptExtractor["extract"]>>): unknown {
	const uidToKey = new Map<string, string>();
	for (const n of result.nodes) {
		uidToKey.set(n.nodeUid, n.stableKey);
	}

	// Nodes keyed by stable_key.
	const nodes: Record<string, unknown> = {};
	for (const n of result.nodes) {
		nodes[n.stableKey] = {
			kind: n.kind,
			subtype: n.subtype,
			name: n.name,
			qualified_name: n.qualifiedName,
			location: n.location,
			signature: n.signature,
			visibility: n.visibility,
			doc_comment: n.docComment,
		};
	}

	// Edges sorted by (source, type, target).
	const edges = result.edges
		.map((e) => ({
			source: uidToKey.get(e.sourceNodeUid) ?? e.sourceNodeUid,
			type: e.type,
			target: e.targetKey,
			location: e.location,
			metadata: e.metadataJson ? JSON.parse(e.metadataJson) : null,
		}))
		.sort((a, b) => {
			const ka = `${a.type}|${a.source}|${a.target}`;
			const kb = `${b.type}|${b.source}|${b.target}`;
			return ka < kb ? -1 : ka > kb ? 1 : 0;
		});

	// Import bindings sorted by (identifier, specifier).
	const importBindings = result.importBindings
		.map((b) => ({
			identifier: b.identifier,
			specifier: b.specifier,
			is_relative: b.isRelative,
			is_type_only: b.isTypeOnly,
			location: b.location,
		}))
		.sort((a, b) => {
			const ka = `${a.identifier}|${a.specifier}`;
			const kb = `${b.identifier}|${b.specifier}`;
			return ka < kb ? -1 : ka > kb ? 1 : 0;
		});

	// Metrics keyed by stable_key.
	const metrics: Record<string, unknown> = {};
	for (const [key, m] of result.metrics) {
		metrics[key] = {
			cyclomatic_complexity: m.cyclomaticComplexity,
			parameter_count: m.parameterCount,
			max_nesting_depth: m.maxNestingDepth,
		};
	}

	return { nodes, edges, import_bindings: importBindings, metrics };
}

const fixtures = discoverFixtures();
let extractor: TypeScriptExtractor;

describe("ts-extractor parity — TS half", () => {
	it("discovered at least one fixture", () => {
		expect(fixtures.length).toBeGreaterThan(0);
	});

	it("initializes extractor", async () => {
		extractor = new TypeScriptExtractor();
		await extractor.initialize();
	});

	for (const fixture of fixtures) {
		it(`fixture: ${fixture.name}`, async () => {
			const result = await extractor.extract(
				fixture.input.source as string,
				fixture.input.filePath as string,
				fixture.input.fileUid as string,
				fixture.input.repoUid as string,
				fixture.input.snapshotUid as string,
			);
			const normalized = JSON.parse(JSON.stringify(normalizeResult(result)));
			expect(normalized).toEqual(fixture.expected);
		});
	}
});
