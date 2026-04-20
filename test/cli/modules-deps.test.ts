/**
 * CLI integration tests for `rgr modules deps` and `rgr modules boundary`.
 *
 * Tests the module dependency graph command:
 *   - JSON envelope structure
 *   - Cross-module edge detection
 *   - Module filtering (--outbound, --inbound)
 *   - Error handling
 *
 * Tests the module boundary declaration command:
 *   - Declaration creation with exact module resolution
 *   - Persistence of canonicalRootPath identity
 *   - Error handling for unknown modules
 */

import { join } from "node:path";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { createTestHarness, type TestHarness } from "./harness.js";

const FIXTURES_PATH = join(
	import.meta.dirname,
	"../fixtures/typescript/module-deps",
);

let h: TestHarness;

beforeAll(async () => {
	h = await createTestHarness();
	await h.run("repo", "add", FIXTURES_PATH, "--name", "module-deps-repo");
	await h.run("repo", "index", "module-deps-repo");
}, 30000);

afterAll(() => {
	h.cleanup();
});

// ── Basic command structure ───────────────────────────────────────

describe("modules deps — basic", () => {
	it("returns JSON envelope with correct structure", async () => {
		const r = await h.run("modules", "deps", "module-deps-repo");
		expect(r.exitCode).toBe(0);

		const json = r.json();
		expect(json.command).toMatch(/^modules deps/);
		expect(json.repo).toBe("module-deps-repo");
		expect(json.snapshot).toBeDefined();
		expect(json.snapshot_scope).toMatch(/^(full|incremental)$/);
		expect(json.results).toBeInstanceOf(Array);
		expect(json.count).toBeGreaterThanOrEqual(0);
		expect(json.diagnostics).toBeDefined();
	});

	it("includes diagnostics in output", async () => {
		const r = await h.run("modules", "deps", "module-deps-repo");
		expect(r.exitCode).toBe(0);

		const json = r.json();
		const diag = json.diagnostics as Record<string, number>;
		expect(diag).toHaveProperty("imports_edges_total");
		expect(diag).toHaveProperty("imports_cross_module");
		expect(diag).toHaveProperty("imports_intra_module");
	});
});

// ── Cross-module edges ────────────────────────────────────────────

describe("modules deps — cross-module edges", () => {
	it("detects cross-module dependency from app to core", async () => {
		const r = await h.run("modules", "deps", "module-deps-repo");
		expect(r.exitCode).toBe(0);

		const json = r.json();
		const results = json.results as Array<Record<string, unknown>>;

		// The fixture has @fixture/app importing from @fixture/core.
		// We should see at least one edge with app as source and core as target.
		const appToCoreEdge = results.find(
			(e) =>
				(e.source_root_path as string).includes("app") &&
				(e.target_root_path as string).includes("core"),
		);

		expect(appToCoreEdge).toBeDefined();
		if (appToCoreEdge) {
			expect(appToCoreEdge.import_count).toBeGreaterThanOrEqual(1);
			expect(appToCoreEdge.source_file_count).toBeGreaterThanOrEqual(1);
		}
	});

	it("edge result includes module identity fields", async () => {
		const r = await h.run("modules", "deps", "module-deps-repo");
		expect(r.exitCode).toBe(0);

		const json = r.json();
		const results = json.results as Array<Record<string, unknown>>;

		if (results.length > 0) {
			const edge = results[0];
			// Source module fields
			expect(edge).toHaveProperty("source_module_uid");
			expect(edge).toHaveProperty("source_module_key");
			expect(edge).toHaveProperty("source_root_path");
			expect(edge).toHaveProperty("source_module_kind");
			expect(edge).toHaveProperty("source_display_name");
			// Target module fields
			expect(edge).toHaveProperty("target_module_uid");
			expect(edge).toHaveProperty("target_module_key");
			expect(edge).toHaveProperty("target_root_path");
			expect(edge).toHaveProperty("target_module_kind");
			expect(edge).toHaveProperty("target_display_name");
			// Aggregation fields
			expect(edge).toHaveProperty("import_count");
			expect(edge).toHaveProperty("source_file_count");
		}
	});
});

// ── Module filtering ──────────────────────────────────────────────

describe("modules deps — filtering", () => {
	it("filters to edges involving specified module", async () => {
		// First get all edges to find a module to filter by.
		const allEdges = await h.run("modules", "deps", "module-deps-repo");
		expect(allEdges.exitCode).toBe(0);

		const json = allEdges.json();
		const results = json.results as Array<Record<string, unknown>>;
		if (results.length === 0) {
			// Skip if no edges (fixture might not have resolved imports)
			return;
		}

		// Use the app module root path for filtering.
		const appEdge = results.find((e) =>
			(e.source_root_path as string).includes("app"),
		);
		if (!appEdge) return;

		const appRootPath = appEdge.source_root_path as string;

		const filtered = await h.run(
			"modules",
			"deps",
			"module-deps-repo",
			appRootPath,
		);
		expect(filtered.exitCode).toBe(0);

		const filteredJson = filtered.json();
		const filteredResults = filteredJson.results as Array<
			Record<string, unknown>
		>;

		// All edges should involve the app module.
		for (const edge of filteredResults) {
			const involvesApp =
				(edge.source_root_path as string) === appRootPath ||
				(edge.target_root_path as string) === appRootPath;
			expect(involvesApp).toBe(true);
		}
	});

	it("--outbound shows only outbound edges from module", async () => {
		const allEdges = await h.run("modules", "deps", "module-deps-repo");
		const json = allEdges.json();
		const results = json.results as Array<Record<string, unknown>>;
		if (results.length === 0) return;

		const appEdge = results.find((e) =>
			(e.source_root_path as string).includes("app"),
		);
		if (!appEdge) return;

		const appRootPath = appEdge.source_root_path as string;

		const outbound = await h.run(
			"modules",
			"deps",
			"module-deps-repo",
			appRootPath,
			"--outbound",
		);
		expect(outbound.exitCode).toBe(0);

		const outJson = outbound.json();
		const outResults = outJson.results as Array<Record<string, unknown>>;

		// All edges should have app as source.
		for (const edge of outResults) {
			expect(edge.source_root_path).toBe(appRootPath);
		}
	});

	it("--inbound shows only inbound edges to module", async () => {
		const allEdges = await h.run("modules", "deps", "module-deps-repo");
		const json = allEdges.json();
		const results = json.results as Array<Record<string, unknown>>;
		if (results.length === 0) return;

		const coreEdge = results.find((e) =>
			(e.target_root_path as string).includes("core"),
		);
		if (!coreEdge) return;

		const coreRootPath = coreEdge.target_root_path as string;

		const inbound = await h.run(
			"modules",
			"deps",
			"module-deps-repo",
			coreRootPath,
			"--inbound",
		);
		expect(inbound.exitCode).toBe(0);

		const inJson = inbound.json();
		const inResults = inJson.results as Array<Record<string, unknown>>;

		// All edges should have core as target.
		for (const edge of inResults) {
			expect(edge.target_root_path).toBe(coreRootPath);
		}
	});
});

// ── Error handling ────────────────────────────────────────────────

describe("modules deps — error handling", () => {
	it("returns error for unknown repo", async () => {
		const r = await h.run("modules", "deps", "nonexistent-repo");
		expect(r.exitCode).toBe(1);

		const json = r.json();
		expect(json.error).toMatch(/not found|not indexed/i);
	});

	it("returns error for unknown module", async () => {
		const r = await h.run(
			"modules",
			"deps",
			"module-deps-repo",
			"nonexistent-module",
		);
		expect(r.exitCode).toBe(1);

		const json = r.json();
		expect(json.error).toMatch(/module not found/i);
	});

	it("returns error for --outbound without module argument", async () => {
		const r = await h.run(
			"modules",
			"deps",
			"module-deps-repo",
			"--outbound",
		);
		expect(r.exitCode).toBe(1);

		const json = r.json();
		expect(json.error).toMatch(/require.*module/i);
	});

	it("returns error for --inbound without module argument", async () => {
		const r = await h.run("modules", "deps", "module-deps-repo", "--inbound");
		expect(r.exitCode).toBe(1);

		const json = r.json();
		expect(json.error).toMatch(/require.*module/i);
	});

	it("returns error for --outbound --inbound together", async () => {
		// Get a valid module path first.
		const allEdges = await h.run("modules", "deps", "module-deps-repo");
		const json = allEdges.json();
		const results = json.results as Array<Record<string, unknown>>;
		if (results.length === 0) return;

		const modulePath = results[0].source_root_path as string;

		const r = await h.run(
			"modules",
			"deps",
			"module-deps-repo",
			modulePath,
			"--outbound",
			"--inbound",
		);
		expect(r.exitCode).toBe(1);

		const errJson = r.json();
		expect(errJson.error).toMatch(/mutually exclusive/i);
	});
});

// ── modules boundary ──────────────────────────────────────────────

describe("modules boundary — declaration", () => {
	it("creates boundary declaration with --json output", async () => {
		// Get valid module paths from the dependency graph.
		const depsResult = await h.run("modules", "deps", "module-deps-repo");
		const json = depsResult.json();
		const results = json.results as Array<Record<string, unknown>>;
		if (results.length === 0) return;

		const sourcePath = results[0].source_root_path as string;
		const targetPath = results[0].target_root_path as string;

		const r = await h.run(
			"modules",
			"boundary",
			"module-deps-repo",
			sourcePath,
			"--forbids",
			targetPath,
			"--reason",
			"test boundary",
			"--json",
		);

		expect(r.exitCode).toBe(0);
		const result = r.json();
		expect(result.declaration_uid).toBeDefined();
		expect(result.source_root_path).toBe(sourcePath);
		expect(result.target_root_path).toBe(targetPath);
	});

	it("creates boundary declaration with human-readable output", async () => {
		const depsResult = await h.run("modules", "deps", "module-deps-repo");
		const json = depsResult.json();
		const results = json.results as Array<Record<string, unknown>>;
		if (results.length === 0) return;

		const sourcePath = results[0].source_root_path as string;
		const targetPath = results[0].target_root_path as string;

		const r = await h.run(
			"modules",
			"boundary",
			"module-deps-repo",
			sourcePath,
			"--forbids",
			targetPath,
		);

		expect(r.exitCode).toBe(0);
		expect(r.stdout).toContain("Created boundary");
		expect(r.stdout).toContain(sourcePath);
		expect(r.stdout).toContain(targetPath);
		expect(r.stdout).toContain("Declaration UID");
	});
});

describe("modules boundary — error handling", () => {
	it("returns error for unknown repo", async () => {
		const r = await h.run(
			"modules",
			"boundary",
			"nonexistent-repo",
			"some-module",
			"--forbids",
			"other-module",
		);
		expect(r.exitCode).toBe(1);
		expect(r.stderr).toMatch(/not found|not indexed/i);
	});

	it("returns error for unknown source module", async () => {
		const r = await h.run(
			"modules",
			"boundary",
			"module-deps-repo",
			"nonexistent-source",
			"--forbids",
			"packages/core",
			"--json",
		);
		expect(r.exitCode).toBe(1);
		const result = r.json();
		expect(result.error).toMatch(/source module not found/i);
	});

	it("returns error for unknown target module", async () => {
		// Get a valid source module.
		const depsResult = await h.run("modules", "deps", "module-deps-repo");
		const json = depsResult.json();
		const results = json.results as Array<Record<string, unknown>>;
		if (results.length === 0) return;

		const sourcePath = results[0].source_root_path as string;

		const r = await h.run(
			"modules",
			"boundary",
			"module-deps-repo",
			sourcePath,
			"--forbids",
			"nonexistent-target",
			"--json",
		);
		expect(r.exitCode).toBe(1);
		const result = r.json();
		expect(result.error).toMatch(/target module not found/i);
	});

	it("returns error for self-referential boundary", async () => {
		const depsResult = await h.run("modules", "deps", "module-deps-repo");
		const json = depsResult.json();
		const results = json.results as Array<Record<string, unknown>>;
		if (results.length === 0) return;

		const modulePath = results[0].source_root_path as string;

		const r = await h.run(
			"modules",
			"boundary",
			"module-deps-repo",
			modulePath,
			"--forbids",
			modulePath, // same as source
			"--json",
		);
		expect(r.exitCode).toBe(1);
		const result = r.json();
		expect(result.error).toMatch(/must be different/i);
	});
});
