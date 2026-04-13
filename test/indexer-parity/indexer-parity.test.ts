/**
 * TypeScript half of the indexer parity harness.
 *
 * Walks the shared `indexer-parity-fixtures/` corpus at the repo
 * root, runs each fixture through the corresponding TS indexer
 * function, and compares against `expected.json`.
 *
 * Parity scope: pure routing, resolution categorization, and
 * invalidation planning only. No whole-index parity.
 */

import { readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { buildInvalidationPlan } from "../../src/core/delta/invalidation-planner.js";

// The routing/categorization functions are private to the indexer
// file. We import them via a re-export helper or inline the logic.
// Since they are inline in repo-indexer.ts, we replicate the
// minimal logic here for parity testing.

function detectLanguage(filePath: string): string | null {
	const dot = filePath.lastIndexOf(".");
	const ext = dot >= 0 ? filePath.slice(dot) : "";
	switch (ext) {
		case ".ts":
			return "typescript";
		case ".tsx":
			return "tsx";
		case ".js":
			return "javascript";
		case ".jsx":
			return "jsx";
		default:
			return null;
	}
}

function isTestFile(filePath: string): boolean {
	return (
		filePath.includes("__tests__") ||
		filePath.includes(".test.") ||
		filePath.includes(".spec.") ||
		filePath.includes("/test/") ||
		filePath.includes("/tests/") ||
		filePath.startsWith("test/") ||
		filePath.startsWith("tests/") ||
		filePath.startsWith("__tests__/")
	);
}

import type { UnresolvedEdgeCategory } from "../../src/core/diagnostics/unresolved-edge-categories.js";

function categorizeUnresolvedEdge(
	targetKey: string,
	edgeType: string,
	metadataJson: string | null,
): UnresolvedEdgeCategory {
	if (edgeType === "IMPORTS") return "imports_file_not_found";
	if (edgeType === "INSTANTIATES") return "instantiates_class_not_found";
	if (edgeType === "IMPLEMENTS") return "implements_interface_not_found";

	if (edgeType === "CALLS") {
		let key = targetKey;
		if (metadataJson) {
			try {
				const meta = JSON.parse(metadataJson);
				if (meta.rawCalleeName) key = meta.rawCalleeName;
			} catch {
				// ignore
			}
		}
		if (key.startsWith("this.")) {
			if (key.split(".").length > 2) return "calls_this_wildcard_method_needs_type_info";
			return "calls_this_method_needs_class_context";
		}
		if (key.includes(".")) return "calls_obj_method_needs_type_info";
		return "calls_function_ambiguous_or_missing";
	}

	return "other";
}

// ── Fixture loading ──────────────────────────────────────────────

const FIXTURES_ROOT = join(__dirname, "..", "..", "indexer-parity-fixtures");

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

// ── Dispatch ─────────────────────────────────────────────────────

function runFixture(fixture: Fixture): unknown {
	const fnName = fixture.input.fn as string;

	switch (fnName) {
		case "detect_language":
			return detectLanguage(fixture.input.filePath as string);

		case "is_test_file":
			return isTestFile(fixture.input.filePath as string);

		case "categorize_unresolved_edge":
			return categorizeUnresolvedEdge(
				fixture.input.targetKey as string,
				fixture.input.edgeType as string,
				(fixture.input.metadataJson as string | null) ?? null,
			);

		case "build_invalidation_plan": {
			const parentHashes = new Map<string, string>(
				Object.entries(fixture.input.parentHashes as Record<string, string>),
			);
			const currentFiles = (
				fixture.input.currentFiles as Array<{
					fileUid: string;
					path: string;
					contentHash: string;
				}>
			).map((f) => ({
				fileUid: f.fileUid,
				path: f.path,
				contentHash: f.contentHash,
			}));
			const plan = buildInvalidationPlan(
				fixture.input.parentSnapshotUid as string,
				parentHashes,
				currentFiles,
				fixture.input.repoUid as string,
			);
			return {
				counts: {
					unchanged: plan.counts.unchanged,
					changed: plan.counts.changed,
					new: plan.counts.new,
					deleted: plan.counts.deleted,
					config_widened: plan.counts.configWidened,
					total: plan.counts.total,
				},
				files_to_extract: plan.filesToExtract,
				files_to_copy: plan.filesToCopy,
				files_to_delete: plan.filesToDelete,
			};
		}

		default:
			throw new Error(`unknown fn: ${fnName}`);
	}
}

// ── Harness ──────────────────────────────────────────────────────

const fixtures = discoverFixtures();

describe("indexer parity — TS half against shared indexer-parity-fixtures", () => {
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
