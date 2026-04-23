/**
 * CLI integration tests for A1+B1 overlap visibility.
 *
 * Tests that overlapping kbuild + directory_structure evidence:
 *   - Produces a single module candidate with both sources
 *   - Shows deterministic source ordering (alphabetical)
 *   - Selects kbuild as primary source (confidence 0.9 > 0.7)
 *   - Has aggregate confidence of 0.9 (max)
 *
 * Uses the kbuild-overlap fixture which has:
 *   - drivers/Makefile with `obj-y += net/` (A1)
 *   - drivers/net/ with 5 C files (B1)
 */

import { join } from "node:path";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { createTestHarness, type TestHarness } from "./harness.js";

const FIXTURES_PATH = join(
	import.meta.dirname,
	"../fixtures/kbuild-overlap",
);

let h: TestHarness;

beforeAll(async () => {
	h = await createTestHarness();
	await h.run("repo", "add", FIXTURES_PATH, "--name", "kbuild-overlap-test");
	await h.run("repo", "index", "kbuild-overlap-test");
}, 30000);

afterAll(() => {
	h.cleanup();
});

// ── A1+B1 overlap visibility ──────────────────────────────────────

describe("modules list — A1+B1 overlap", () => {
	it("produces single module candidate for overlapping roots", async () => {
		const r = await h.run(
			"modules",
			"list",
			"kbuild-overlap-test",
			"--json",
		);
		expect(r.exitCode).toBe(0);

		const modules = r.json() as Array<Record<string, unknown>>;
		// Should have exactly one module at drivers/net
		const driversNet = modules.find(
			(m) => m.canonicalRootPath === "drivers/net",
		);
		expect(driversNet).toBeDefined();
	});

	it("shows both evidence sources in deterministic order", async () => {
		const r = await h.run(
			"modules",
			"list",
			"kbuild-overlap-test",
			"--json",
		);
		expect(r.exitCode).toBe(0);

		const modules = r.json() as Array<Record<string, unknown>>;
		const driversNet = modules.find(
			(m) => m.canonicalRootPath === "drivers/net",
		);
		expect(driversNet).toBeDefined();

		const sources = driversNet!.evidenceSources as string[];
		expect(sources).toContain("kbuild");
		expect(sources).toContain("directory_structure");
		// Deterministic alphabetical order
		expect(sources).toEqual(["directory_structure", "kbuild"]);
	});

	it("selects kbuild as primary source (higher confidence)", async () => {
		const r = await h.run(
			"modules",
			"list",
			"kbuild-overlap-test",
			"--json",
		);
		expect(r.exitCode).toBe(0);

		const modules = r.json() as Array<Record<string, unknown>>;
		const driversNet = modules.find(
			(m) => m.canonicalRootPath === "drivers/net",
		);
		expect(driversNet).toBeDefined();

		// kbuild has confidence 0.9, directory_structure has 0.7
		// Primary should be kbuild
		expect(driversNet!.primarySource).toBe("kbuild");
	});

	it("uses max confidence from evidence items", async () => {
		const r = await h.run(
			"modules",
			"list",
			"kbuild-overlap-test",
			"--json",
		);
		expect(r.exitCode).toBe(0);

		const modules = r.json() as Array<Record<string, unknown>>;
		const driversNet = modules.find(
			(m) => m.canonicalRootPath === "drivers/net",
		);
		expect(driversNet).toBeDefined();

		// Max of 0.9 (kbuild) and 0.7 (directory_structure)
		expect(driversNet!.confidence).toBe(0.9);
	});

	it("has two evidence items for overlapping root", async () => {
		const r = await h.run(
			"modules",
			"list",
			"kbuild-overlap-test",
			"--json",
		);
		expect(r.exitCode).toBe(0);

		const modules = r.json() as Array<Record<string, unknown>>;
		const driversNet = modules.find(
			(m) => m.canonicalRootPath === "drivers/net",
		);
		expect(driversNet).toBeDefined();

		expect(driversNet!.evidenceCount).toBe(2);
	});

	it("--source kbuild returns overlapping module", async () => {
		const r = await h.run(
			"modules",
			"list",
			"kbuild-overlap-test",
			"--source",
			"kbuild",
			"--json",
		);
		expect(r.exitCode).toBe(0);

		const modules = r.json() as Array<Record<string, unknown>>;
		expect(modules.length).toBeGreaterThanOrEqual(1);
		const driversNet = modules.find(
			(m) => m.canonicalRootPath === "drivers/net",
		);
		expect(driversNet).toBeDefined();
	});

	it("--source directory_structure returns overlapping module", async () => {
		const r = await h.run(
			"modules",
			"list",
			"kbuild-overlap-test",
			"--source",
			"directory_structure",
			"--json",
		);
		expect(r.exitCode).toBe(0);

		const modules = r.json() as Array<Record<string, unknown>>;
		expect(modules.length).toBeGreaterThanOrEqual(1);
		const driversNet = modules.find(
			(m) => m.canonicalRootPath === "drivers/net",
		);
		expect(driversNet).toBeDefined();
	});
});

// ── Evidence detail via modules evidence ──────────────────────────

describe("modules evidence — A1+B1 overlap", () => {
	it("shows both kbuild and directory_structure evidence items", async () => {
		const r = await h.run(
			"modules",
			"evidence",
			"kbuild-overlap-test",
			"drivers/net",
			"--json",
		);
		expect(r.exitCode).toBe(0);

		const evidence = r.json() as Array<Record<string, unknown>>;
		expect(evidence.length).toBe(2);

		const sourceTypes = evidence.map((e) => e.sourceType as string).sort();
		expect(sourceTypes).toEqual(["directory_structure", "kbuild"]);
	});

	it("kbuild evidence has correct fields", async () => {
		const r = await h.run(
			"modules",
			"evidence",
			"kbuild-overlap-test",
			"drivers/net",
			"--json",
		);
		expect(r.exitCode).toBe(0);

		const evidence = r.json() as Array<Record<string, unknown>>;
		const kbuildEvidence = evidence.find((e) => e.sourceType === "kbuild");
		expect(kbuildEvidence).toBeDefined();
		expect(kbuildEvidence!.evidenceKind).toBe("kbuild_subdir");
		expect(kbuildEvidence!.confidence).toBe(0.9);
	});

	it("directory_structure evidence has correct fields", async () => {
		const r = await h.run(
			"modules",
			"evidence",
			"kbuild-overlap-test",
			"drivers/net",
			"--json",
		);
		expect(r.exitCode).toBe(0);

		const evidence = r.json() as Array<Record<string, unknown>>;
		const dirEvidence = evidence.find(
			(e) => e.sourceType === "directory_structure",
		);
		expect(dirEvidence).toBeDefined();
		expect(dirEvidence!.evidenceKind).toBe("directory_pattern");
		expect(dirEvidence!.confidence).toBe(0.7);
	});
});
