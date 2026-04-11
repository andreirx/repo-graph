/**
 * TS parity test harness for the cross-runtime detector contract.
 *
 * Walks the shared `parity-fixtures/` corpus at the repo root,
 * runs each fixture's `input.<ext>` through the TS detector
 * pipeline (`detectEnvAccesses` + `detectFsMutations`), and
 * compares the output against the fixture's `expected.json`.
 *
 * This is the TypeScript half of the cross-runtime parity check.
 * The Rust half (R1-J) runs the same fixtures through the Rust
 * pipeline and compares against the same expected.json. Both
 * runtimes consume the SAME bytes from the SAME file; if one
 * passes and the other fails, the detector contract has drifted.
 *
 * Contract notes:
 *
 *  - Output shape: `{ env: [...], fs: [...] }` — both keys always
 *    present, arrays always present, nulls explicit.
 *
 *  - Field order inside each record: TS `JSON.stringify` preserves
 *    object key insertion order, and the interface field
 *    declarations in `env-dependency.ts` / `fs-mutation.ts` match
 *    the locked expected.json field order. No explicit ordering
 *    step is needed — structural `toEqual` comparison handles
 *    both.
 *
 *  - `destinationPath` normalization: TS `DetectedFsMutation`
 *    declares `destinationPath?: string | null` as OPTIONAL. When
 *    a detector does not set it, the field is MISSING from the
 *    serialized output. The locked expected.json contract
 *    requires the field to be ALWAYS PRESENT with value `null`.
 *    The harness fills in `destinationPath: null` when the
 *    detector leaves it undefined. This normalization happens in
 *    the harness's output shaping, NOT in the detector pipeline.
 *
 *  - `filePath` in expected.json is the fixture-local filename
 *    (e.g., `input.ts`). The harness passes the same filename to
 *    the detector so the reported filePath matches.
 */

import { readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import type { DetectedEnvDependency } from "../../src/core/seams/env-dependency.js";
import { detectEnvAccesses } from "../../src/core/seams/env-detectors.js";
import { detectFsMutations } from "../../src/core/seams/fs-mutation-detectors.js";
import type { DetectedFsMutation } from "../../src/core/seams/fs-mutation.js";

// ── Types ──────────────────────────────────────────────────────────

/**
 * The parity boundary output shape. Matches the locked
 * `expected.json` contract and the Rust `ParityOutput` struct.
 * Both keys always present, arrays always present.
 */
interface ParityOutput {
	env: DetectedEnvDependency[];
	fs: NormalizedFsMutation[];
}

/**
 * Fs mutation shape with `destinationPath` always present.
 * Mirrors the Rust `DetectedFsMutation` which uses `Option<String>`
 * (always serializing as either a string or `null`).
 */
interface NormalizedFsMutation {
	filePath: string;
	lineNumber: number;
	mutationKind: DetectedFsMutation["mutationKind"];
	mutationPattern: DetectedFsMutation["mutationPattern"];
	targetPath: string | null;
	destinationPath: string | null;
	dynamicPath: boolean;
	confidence: number;
}

// ── Fixture discovery ─────────────────────────────────────────────

const FIXTURES_ROOT = join(__dirname, "..", "..", "parity-fixtures");

interface Fixture {
	name: string;
	inputFileName: string;
	inputContent: string;
	expected: ParityOutput;
}

function discoverFixtures(): Fixture[] {
	const entries = readdirSync(FIXTURES_ROOT);
	const fixtures: Fixture[] = [];
	for (const entry of entries) {
		const fullPath = join(FIXTURES_ROOT, entry);
		if (!statSync(fullPath).isDirectory()) continue;
		// Find input.<ext>
		const files = readdirSync(fullPath);
		const inputFileName = files.find((f) => f.startsWith("input."));
		if (!inputFileName) {
			throw new Error(`fixture ${entry} has no input.<ext> file`);
		}
		const inputContent = readFileSync(join(fullPath, inputFileName), "utf-8");
		const expectedPath = join(fullPath, "expected.json");
		const expectedRaw = readFileSync(expectedPath, "utf-8");
		const expected: ParityOutput = JSON.parse(expectedRaw);
		fixtures.push({
			name: entry,
			inputFileName,
			inputContent,
			expected,
		});
	}
	fixtures.sort((a, b) => a.name.localeCompare(b.name));
	return fixtures;
}

// ── Output normalization ──────────────────────────────────────────

/**
 * Normalize fs detector output for parity comparison.
 *
 * The TS `DetectedFsMutation.destinationPath` is optional, so
 * detectors that don't handle two-ended ops leave it undefined.
 * The parity contract requires the field to be always present
 * with `null` when unset. This function rebuilds each record with
 * the field filled in, preserving the locked field order.
 *
 * Field declaration order in this literal matches the locked
 * expected.json field order exactly: filePath, lineNumber,
 * mutationKind, mutationPattern, targetPath, destinationPath,
 * dynamicPath, confidence.
 */
function normalizeFs(records: DetectedFsMutation[]): NormalizedFsMutation[] {
	return records.map((r) => ({
		filePath: r.filePath,
		lineNumber: r.lineNumber,
		mutationKind: r.mutationKind,
		mutationPattern: r.mutationPattern,
		targetPath: r.targetPath,
		destinationPath: r.destinationPath ?? null,
		dynamicPath: r.dynamicPath,
		confidence: r.confidence,
	}));
}

/**
 * Normalize env detector output for parity comparison.
 *
 * The TS `DetectedEnvDependency` has no optional fields, so no
 * structural normalization is needed. The function exists for
 * symmetry with `normalizeFs` and to rebuild each record in the
 * locked field order (in case a future TS detector reorders its
 * output object keys unintentionally).
 */
function normalizeEnv(
	records: DetectedEnvDependency[],
): DetectedEnvDependency[] {
	return records.map((r) => ({
		varName: r.varName,
		accessKind: r.accessKind,
		accessPattern: r.accessPattern,
		filePath: r.filePath,
		lineNumber: r.lineNumber,
		defaultValue: r.defaultValue,
		confidence: r.confidence,
	}));
}

// ── Harness ────────────────────────────────────────────────────────

const fixtures = discoverFixtures();

describe("parity — TS pipeline against shared parity-fixtures corpus", () => {
	it("discovered at least one fixture", () => {
		expect(fixtures.length).toBeGreaterThan(0);
	});

	for (const fixture of fixtures) {
		it(`fixture: ${fixture.name}`, () => {
			const envRecords = detectEnvAccesses(
				fixture.inputContent,
				fixture.inputFileName,
			);
			const fsRecords = detectFsMutations(
				fixture.inputContent,
				fixture.inputFileName,
			);
			const actual: ParityOutput = {
				env: normalizeEnv(envRecords),
				fs: normalizeFs(fsRecords),
			};
			expect(actual).toEqual(fixture.expected);
		});
	}
});
