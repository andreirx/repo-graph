/**
 * TypeScript receiver resolver — unit + integration tests.
 *
 * Uses a dedicated fixture (receiver-types/) with known call sites
 * and verifiable receiver types.
 */

import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { resolveReceiverTypes } from "../../../src/adapters/enrichment/typescript-receiver-resolver.js";

const FIXTURE_ROOT = join(
	import.meta.dirname,
	"../../fixtures/typescript/receiver-types",
);
const FILE_PATH = "src/calls.ts";

/**
 * Helper: run the resolver on a single call site and return the result.
 * lineStart is 1-based.
 */
async function resolveAt(line: number, col = 0) {
	const results = await resolveReceiverTypes(FIXTURE_ROOT, [
		{ edgeUid: `test-${line}`, sourceFilePath: FILE_PATH, lineStart: line, colStart: col, targetKey: "x" },
	]);
	expect(results.length).toBe(1);
	return results[0];
}

describe("resolveReceiverTypes — type resolution on fixture", () => {
	it("resolves Array receiver type on lines.map()", async () => {
		// Line 5: lines.map(...)
		const r = await resolveAt(5);
		expect(r.receiverTypeOrigin).toBe("compiler");
		expect(r.receiverType).toContain("string");
		expect(r.failureReason).toBeNull();
	});

	it("resolves Map receiver on scores.set()", async () => {
		// Line 9: scores.set(...)
		const r = await resolveAt(9);
		expect(r.receiverTypeOrigin).toBe("compiler");
		expect(r.receiverType).toContain("Map");
	});

	it("resolves string receiver on greeting.toUpperCase()", async () => {
		// Line 14: greeting.toUpperCase()
		const r = await resolveAt(14);
		expect(r.receiverType).toBe("string");
	});

	it("resolves Date receiver on now.toISOString()", async () => {
		// Line 18: now.toISOString()
		const r = await resolveAt(18);
		expect(r.receiverType).toBe("Date");
	});

	it("resolves class instance receiver on g.greet()", async () => {
		// Line 27: g.greet()
		const r = await resolveAt(27);
		expect(r.receiverType).toBe("Greeter");
	});

	it("returns failed for a line with no call expression", async () => {
		// Line 1: comment — no call expression
		const r = await resolveAt(1);
		expect(r.receiverTypeOrigin).toBe("failed");
		expect(r.failureReason).toBeTruthy();
	});
});

describe("resolveReceiverTypes — batch processing", () => {
	it("resolves multiple sites in one call", async () => {
		const results = await resolveReceiverTypes(FIXTURE_ROOT, [
			{ edgeUid: "e1", sourceFilePath: FILE_PATH, lineStart: 5, colStart: 0, targetKey: "lines.map" },
			{ edgeUid: "e2", sourceFilePath: FILE_PATH, lineStart: 14, colStart: 0, targetKey: "greeting.toUpperCase" },
			{ edgeUid: "e3", sourceFilePath: FILE_PATH, lineStart: 18, colStart: 0, targetKey: "now.toISOString" },
		]);
		expect(results.length).toBe(3);
		const resolved = results.filter((r) => r.receiverType !== null);
		expect(resolved.length).toBe(3);
	});

	it("returns empty for empty input", async () => {
		const results = await resolveReceiverTypes(FIXTURE_ROOT, []);
		expect(results).toEqual([]);
	});
});

describe("resolveReceiverTypes — edge cases", () => {
	it("handles nonexistent source file gracefully", async () => {
		const results = await resolveReceiverTypes(FIXTURE_ROOT, [
			{ edgeUid: "e1", sourceFilePath: "src/nonexistent.ts", lineStart: 1, colStart: 0, targetKey: "x" },
		]);
		expect(results.length).toBe(1);
		expect(results[0].receiverTypeOrigin).toBe("failed");
		expect(results[0].failureReason).toContain("not in program");
	});
});
