/**
 * CLI integration tests for `rgr modules list` Layer 3 visibility.
 *
 * Tests the module listing command with Layer 3 visibility fields:
 *   - Evidence sources exposed in JSON output
 *   - Primary source (highest confidence) identified
 *   - isInferred convenience field
 *   - --kind filter for module kind
 *   - --source filter for evidence source type
 *
 * Uses the directory-module-b1 fixture which has a directory-inferred
 * module at src/core with 5 TypeScript files.
 */

import { join } from "node:path";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { createTestHarness, type TestHarness } from "./harness.js";

const FIXTURES_PATH = join(
	import.meta.dirname,
	"../fixtures/directory-module-b1",
);

let h: TestHarness;

beforeAll(async () => {
	h = await createTestHarness();
	await h.run("repo", "add", FIXTURES_PATH, "--name", "b1-visibility-test");
	await h.run("repo", "index", "b1-visibility-test");
}, 30000);

afterAll(() => {
	h.cleanup();
});

// ── Layer 3 visibility fields ─────────────────────────────────────

describe("modules list — Layer 3 visibility", () => {
	it("exposes evidenceSources array in JSON output", async () => {
		const r = await h.run("modules", "list", "b1-visibility-test", "--json");
		expect(r.exitCode).toBe(0);

		const modules = r.json() as Array<Record<string, unknown>>;
		expect(modules.length).toBe(1);

		const mod = modules[0];
		expect(mod.evidenceSources).toBeInstanceOf(Array);
		expect(mod.evidenceSources).toContain("directory_structure");
	});

	it("exposes primarySource in JSON output", async () => {
		const r = await h.run("modules", "list", "b1-visibility-test", "--json");
		expect(r.exitCode).toBe(0);

		const modules = r.json() as Array<Record<string, unknown>>;
		const mod = modules[0];
		expect(mod.primarySource).toBe("directory_structure");
	});

	it("exposes isInferred convenience field", async () => {
		const r = await h.run("modules", "list", "b1-visibility-test", "--json");
		expect(r.exitCode).toBe(0);

		const modules = r.json() as Array<Record<string, unknown>>;
		const mod = modules[0];
		expect(mod.isInferred).toBe(true);
		expect(mod.moduleKind).toBe("inferred");
	});

	it("shows directory_structure module with correct confidence", async () => {
		const r = await h.run("modules", "list", "b1-visibility-test", "--json");
		expect(r.exitCode).toBe(0);

		const modules = r.json() as Array<Record<string, unknown>>;
		const mod = modules[0];
		expect(mod.confidence).toBe(0.7);
		expect(mod.canonicalRootPath).toBe("src/core");
	});

	it("includes SOURCES column in human-readable output", async () => {
		const r = await h.run("modules", "list", "b1-visibility-test");
		expect(r.exitCode).toBe(0);

		expect(r.stdout).toContain("SOURCES");
		expect(r.stdout).toContain("directory_structure");
	});

	it("includes original rollup columns in human-readable output", async () => {
		const r = await h.run("modules", "list", "b1-visibility-test");
		expect(r.exitCode).toBe(0);

		// Original rollup columns preserved
		expect(r.stdout).toContain("SYMBOLS");
		expect(r.stdout).toContain("TESTS");
		expect(r.stdout).toContain("LANGS");
		expect(r.stdout).toContain("DIR_MOD");
	});
});

// ── Filter by kind ────────────────────────────────────────────────

describe("modules list — --kind filter", () => {
	it("--kind inferred returns inferred modules", async () => {
		const r = await h.run(
			"modules",
			"list",
			"b1-visibility-test",
			"--kind",
			"inferred",
			"--json",
		);
		expect(r.exitCode).toBe(0);

		const modules = r.json() as Array<Record<string, unknown>>;
		expect(modules.length).toBe(1);
		expect(modules[0].moduleKind).toBe("inferred");
	});

	it("--kind declared returns empty for B1 fixture", async () => {
		const r = await h.run(
			"modules",
			"list",
			"b1-visibility-test",
			"--kind",
			"declared",
			"--json",
		);
		expect(r.exitCode).toBe(0);

		const modules = r.json() as Array<Record<string, unknown>>;
		expect(modules.length).toBe(0);
	});

	it("--kind operational returns empty for B1 fixture", async () => {
		const r = await h.run(
			"modules",
			"list",
			"b1-visibility-test",
			"--kind",
			"operational",
			"--json",
		);
		expect(r.exitCode).toBe(0);

		const modules = r.json() as Array<Record<string, unknown>>;
		expect(modules.length).toBe(0);
	});

	it("rejects invalid --kind value", async () => {
		const r = await h.run(
			"modules",
			"list",
			"b1-visibility-test",
			"--kind",
			"invalid",
		);
		expect(r.exitCode).toBe(1);
		expect(r.stderr).toMatch(/invalid.*--kind/i);
	});
});

// ── Filter by source ──────────────────────────────────────────────

describe("modules list — --source filter", () => {
	it("--source directory_structure returns B1 modules", async () => {
		const r = await h.run(
			"modules",
			"list",
			"b1-visibility-test",
			"--source",
			"directory_structure",
			"--json",
		);
		expect(r.exitCode).toBe(0);

		const modules = r.json() as Array<Record<string, unknown>>;
		expect(modules.length).toBe(1);
		expect(modules[0].evidenceSources).toContain("directory_structure");
	});

	it("--source kbuild returns empty for B1 fixture", async () => {
		const r = await h.run(
			"modules",
			"list",
			"b1-visibility-test",
			"--source",
			"kbuild",
			"--json",
		);
		expect(r.exitCode).toBe(0);

		const modules = r.json() as Array<Record<string, unknown>>;
		expect(modules.length).toBe(0);
	});

	it("--source package_json_workspaces returns empty for B1 fixture", async () => {
		const r = await h.run(
			"modules",
			"list",
			"b1-visibility-test",
			"--source",
			"package_json_workspaces",
			"--json",
		);
		expect(r.exitCode).toBe(0);

		const modules = r.json() as Array<Record<string, unknown>>;
		expect(modules.length).toBe(0);
	});
});

// ── Combined filters ──────────────────────────────────────────────

describe("modules list — combined filters", () => {
	it("--kind inferred --source directory_structure returns B1 modules", async () => {
		const r = await h.run(
			"modules",
			"list",
			"b1-visibility-test",
			"--kind",
			"inferred",
			"--source",
			"directory_structure",
			"--json",
		);
		expect(r.exitCode).toBe(0);

		const modules = r.json() as Array<Record<string, unknown>>;
		expect(modules.length).toBe(1);
	});

	it("--kind declared --source directory_structure returns empty", async () => {
		const r = await h.run(
			"modules",
			"list",
			"b1-visibility-test",
			"--kind",
			"declared",
			"--source",
			"directory_structure",
			"--json",
		);
		expect(r.exitCode).toBe(0);

		const modules = r.json() as Array<Record<string, unknown>>;
		expect(modules.length).toBe(0);
	});
});

// ── Error handling ────────────────────────────────────────────────

describe("modules list — error handling", () => {
	it("returns error for unknown repo", async () => {
		const r = await h.run("modules", "list", "nonexistent-repo");
		expect(r.exitCode).toBe(1);
		expect(r.stderr).toMatch(/not found|not indexed/i);
	});

	it("returns empty message with filter note when no matches", async () => {
		const r = await h.run(
			"modules",
			"list",
			"b1-visibility-test",
			"--kind",
			"declared",
		);
		expect(r.exitCode).toBe(0);
		expect(r.stdout).toContain("with current filters");
	});
});
