/**
 * CLI integration tests for the operational dependency seam contract.
 *
 * Pins:
 *  - `surfaces show` env + fs sections (JSON + human)
 *  - `modules show` rollup section (JSON + human)
 *  - per-surface env + fs in `modules show` (JSON shape)
 *  - cross-surface aggregation wiring (surfaceCount > 1)
 *  - destinationPaths preservation through both rendering paths
 *  - dynamic-path summary aggregation through both rendering paths
 *
 * The pure aggregation rules (accessKind precedence, hasConflictingDefaults
 * semantics, default-value selection, sort order) are exhaustively
 * unit-tested in test/core/seams/module-seam-rollup.test.ts. This file
 * verifies the wiring from storage → rollup core → CLI render and the
 * stability of the user-facing JSON contract.
 *
 * Fixture: test/fixtures/seam-multisurf produces TWO surfaces in ONE
 * module (cli + backend_service from package.json bin + express),
 * sharing file ownership over both src/cli.ts and src/server.ts. Every
 * env var and fs mutation in either file is therefore visible to both
 * surfaces, which exercises surfaceCount=2 in the rollup.
 */

import { join } from "node:path";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { createTestHarness, type TestHarness } from "./harness.js";

const SEAM_FIXTURE = join(
	import.meta.dirname,
	"../fixtures/seam-multisurf",
);

let h: TestHarness;

beforeAll(async () => {
	h = await createTestHarness();
	await h.run("repo", "add", SEAM_FIXTURE, "--name", "seam-multisurf");
	await h.run("repo", "index", "seam-multisurf");
}, 30000);

afterAll(() => {
	h.cleanup();
});

// ── surfaces show: env section ─────────────────────────────────────

describe("surfaces show — env dependencies", () => {
	it("includes envDependencies array in JSON", async () => {
		const r = await h.run("surfaces", "show", "seam-multisurf", "cli", "--json");
		expect(r.exitCode).toBe(0);
		const json = JSON.parse(r.stdout) as Record<string, unknown>;
		expect(Array.isArray(json.envDependencies)).toBe(true);
		const envs = json.envDependencies as Array<Record<string, unknown>>;
		expect(envs.length).toBeGreaterThanOrEqual(4);
	});

	it("env rows have required fields and correct shape", async () => {
		const r = await h.run("surfaces", "show", "seam-multisurf", "cli", "--json");
		const json = JSON.parse(r.stdout) as Record<string, unknown>;
		const envs = json.envDependencies as Array<Record<string, unknown>>;
		for (const row of envs) {
			expect(typeof row.envName).toBe("string");
			expect(typeof row.accessKind).toBe("string");
			expect(["required", "optional", "unknown"]).toContain(row.accessKind);
			expect(typeof row.evidenceCount).toBe("number");
			expect(typeof row.confidence).toBe("number");
			expect("defaultValue" in row).toBe(true);
		}
	});

	it("env rows are sorted by envName ascending", async () => {
		const r = await h.run("surfaces", "show", "seam-multisurf", "cli", "--json");
		const json = JSON.parse(r.stdout) as Record<string, unknown>;
		const envs = json.envDependencies as Array<{ envName: string }>;
		const names = envs.map((e) => e.envName);
		const sorted = [...names].sort();
		expect(names).toEqual(sorted);
	});

	it("captures default value for optional env access (API_KEY=dev-key)", async () => {
		const r = await h.run("surfaces", "show", "seam-multisurf", "cli", "--json");
		const json = JSON.parse(r.stdout) as Record<string, unknown>;
		const envs = json.envDependencies as Array<{
			envName: string;
			accessKind: string;
			defaultValue: string | null;
		}>;
		const apiKey = envs.find((e) => e.envName === "API_KEY");
		expect(apiKey).toBeDefined();
		expect(apiKey!.accessKind).toBe("optional");
		expect(apiKey!.defaultValue).toBe("dev-key");
	});

	it("marks DATABASE_URL as required (no default)", async () => {
		const r = await h.run("surfaces", "show", "seam-multisurf", "cli", "--json");
		const json = JSON.parse(r.stdout) as Record<string, unknown>;
		const envs = json.envDependencies as Array<{
			envName: string;
			accessKind: string;
			defaultValue: string | null;
		}>;
		const dbUrl = envs.find((e) => e.envName === "DATABASE_URL");
		expect(dbUrl).toBeDefined();
		expect(dbUrl!.accessKind).toBe("required");
		expect(dbUrl!.defaultValue).toBeNull();
	});

	it("human output includes Env Dependencies header", async () => {
		const r = await h.run("surfaces", "show", "seam-multisurf", "cli");
		expect(r.exitCode).toBe(0);
		expect(r.stdout).toContain("Env Dependencies");
		expect(r.stdout).toContain("API_KEY=dev-key");
		expect(r.stdout).toContain("DATABASE_URL");
	});
});

// ── surfaces show: fs section ──────────────────────────────────────

describe("surfaces show — fs mutations", () => {
	it("includes fsMutations.literal and fsMutations.dynamic in JSON", async () => {
		const r = await h.run("surfaces", "show", "seam-multisurf", "cli", "--json");
		expect(r.exitCode).toBe(0);
		const json = JSON.parse(r.stdout) as Record<string, unknown>;
		const fs = json.fsMutations as Record<string, unknown>;
		expect(fs).toBeDefined();
		expect(Array.isArray(fs.literal)).toBe(true);
		expect(typeof fs.dynamic).toBe("object");
	});

	it("literal rows have required fields and correct shape", async () => {
		const r = await h.run("surfaces", "show", "seam-multisurf", "cli", "--json");
		const json = JSON.parse(r.stdout) as Record<string, unknown>;
		const literal = (json.fsMutations as Record<string, unknown>).literal as Array<Record<string, unknown>>;
		expect(literal.length).toBeGreaterThanOrEqual(4);
		for (const row of literal) {
			expect(typeof row.targetPath).toBe("string");
			expect(typeof row.mutationKind).toBe("string");
			expect(typeof row.evidenceCount).toBe("number");
			expect(typeof row.confidence).toBe("number");
			expect(Array.isArray(row.destinationPaths)).toBe(true);
		}
	});

	it("literal rows sorted by targetPath then mutationKind", async () => {
		const r = await h.run("surfaces", "show", "seam-multisurf", "cli", "--json");
		const json = JSON.parse(r.stdout) as Record<string, unknown>;
		const literal = (json.fsMutations as Record<string, unknown>).literal as Array<{
			targetPath: string;
			mutationKind: string;
		}>;
		const keys = literal.map((r) => `${r.targetPath}|${r.mutationKind}`);
		const sorted = [...keys].sort();
		expect(keys).toEqual(sorted);
	});

	it("contains expected literal mutations from the fixture", async () => {
		const r = await h.run("surfaces", "show", "seam-multisurf", "cli", "--json");
		const json = JSON.parse(r.stdout) as Record<string, unknown>;
		const literal = (json.fsMutations as Record<string, unknown>).literal as Array<{
			targetPath: string;
			mutationKind: string;
		}>;
		const keys = literal.map((r) => `${r.targetPath}|${r.mutationKind}`);
		expect(keys).toContain("logs/app.log|write_file");
		expect(keys).toContain("data/cache.json|delete_path");
		expect(keys).toContain("uploads|create_dir");
		expect(keys).toContain("tmp/staging.txt|rename_path");
	});

	it("preserves rename destinationPaths in surface-direct rendering", async () => {
		const r = await h.run("surfaces", "show", "seam-multisurf", "cli", "--json");
		const json = JSON.parse(r.stdout) as Record<string, unknown>;
		const literal = (json.fsMutations as Record<string, unknown>).literal as Array<{
			targetPath: string;
			mutationKind: string;
			destinationPaths: string[];
		}>;
		const rename = literal.find(
			(r) => r.targetPath === "tmp/staging.txt" && r.mutationKind === "rename_path",
		);
		expect(rename).toBeDefined();
		expect(rename!.destinationPaths).toEqual(["data/final.txt"]);
	});

	it("destinationPaths defaults to [] for non-rename rows", async () => {
		const r = await h.run("surfaces", "show", "seam-multisurf", "cli", "--json");
		const json = JSON.parse(r.stdout) as Record<string, unknown>;
		const literal = (json.fsMutations as Record<string, unknown>).literal as Array<{
			mutationKind: string;
			destinationPaths: string[];
		}>;
		for (const row of literal) {
			if (row.mutationKind !== "rename_path" && row.mutationKind !== "copy_path") {
				expect(row.destinationPaths).toEqual([]);
			}
		}
	});

	it("dynamic summary has totalCount, distinctFileCount, byKind", async () => {
		const r = await h.run("surfaces", "show", "seam-multisurf", "cli", "--json");
		const json = JSON.parse(r.stdout) as Record<string, unknown>;
		const dyn = (json.fsMutations as Record<string, unknown>).dynamic as Record<string, unknown>;
		expect(typeof dyn.totalCount).toBe("number");
		expect(typeof dyn.distinctFileCount).toBe("number");
		expect(typeof dyn.byKind).toBe("object");
		expect(dyn.totalCount).toBeGreaterThanOrEqual(1);
		expect(dyn.distinctFileCount).toBeGreaterThanOrEqual(1);
	});

	it("human output includes Filesystem Mutations header and rename destination", async () => {
		const r = await h.run("surfaces", "show", "seam-multisurf", "cli");
		expect(r.exitCode).toBe(0);
		expect(r.stdout).toContain("Filesystem Mutations");
		expect(r.stdout).toContain("logs/app.log");
		expect(r.stdout).toContain("-> data/final.txt");
		expect(r.stdout).toContain("Dynamic-path mutations");
	});
});

// ── modules show: rollup section ───────────────────────────────────

describe("modules show — rollup", () => {
	it("includes rollup.envDependencies and rollup.fsMutations in JSON", async () => {
		const r = await h.run("modules", "show", "seam-multisurf", ".", "--json");
		expect(r.exitCode).toBe(0);
		const json = JSON.parse(r.stdout) as Record<string, unknown>;
		const rollup = json.rollup as Record<string, unknown>;
		expect(rollup).toBeDefined();
		expect(Array.isArray(rollup.envDependencies)).toBe(true);
		const fs = rollup.fsMutations as Record<string, unknown>;
		expect(Array.isArray(fs.literal)).toBe(true);
		expect(typeof fs.dynamic).toBe("object");
	});

	it("env rollup rows include cross-surface fields", async () => {
		const r = await h.run("modules", "show", "seam-multisurf", ".", "--json");
		const json = JSON.parse(r.stdout) as Record<string, unknown>;
		const envs = (json.rollup as Record<string, unknown>).envDependencies as Array<Record<string, unknown>>;
		expect(envs.length).toBeGreaterThanOrEqual(4);
		for (const row of envs) {
			expect(typeof row.envName).toBe("string");
			expect(typeof row.accessKind).toBe("string");
			expect("defaultValue" in row).toBe(true);
			expect(typeof row.hasConflictingDefaults).toBe("boolean");
			expect(typeof row.surfaceCount).toBe("number");
			expect(typeof row.evidenceCount).toBe("number");
			expect(typeof row.maxConfidence).toBe("number");
		}
	});

	it("env rollup surfaceCount = 2 for vars seen on both surfaces", async () => {
		// Both surfaces own both source files, so every env var in the
		// fixture appears on both the cli and backend_service surfaces.
		const r = await h.run("modules", "show", "seam-multisurf", ".", "--json");
		const json = JSON.parse(r.stdout) as Record<string, unknown>;
		const envs = (json.rollup as Record<string, unknown>).envDependencies as Array<{
			envName: string;
			surfaceCount: number;
		}>;
		const dbUrl = envs.find((e) => e.envName === "DATABASE_URL");
		expect(dbUrl).toBeDefined();
		expect(dbUrl!.surfaceCount).toBe(2);
	});

	it("env rollup evidenceCount sums across contributing surfaces", async () => {
		// DATABASE_URL appears on both surfaces, with 2 evidence rows
		// per surface (one in cli.ts, one in server.ts) → total = 4.
		const r = await h.run("modules", "show", "seam-multisurf", ".", "--json");
		const json = JSON.parse(r.stdout) as Record<string, unknown>;
		const envs = (json.rollup as Record<string, unknown>).envDependencies as Array<{
			envName: string;
			evidenceCount: number;
		}>;
		const dbUrl = envs.find((e) => e.envName === "DATABASE_URL");
		expect(dbUrl).toBeDefined();
		expect(dbUrl!.evidenceCount).toBe(4);
	});

	it("fs rollup rows include cross-surface fields", async () => {
		const r = await h.run("modules", "show", "seam-multisurf", ".", "--json");
		const json = JSON.parse(r.stdout) as Record<string, unknown>;
		const literal = ((json.rollup as Record<string, unknown>).fsMutations as Record<string, unknown>).literal as Array<Record<string, unknown>>;
		expect(literal.length).toBeGreaterThanOrEqual(4);
		for (const row of literal) {
			expect(typeof row.targetPath).toBe("string");
			expect(typeof row.mutationKind).toBe("string");
			expect(typeof row.surfaceCount).toBe("number");
			expect(typeof row.evidenceCount).toBe("number");
			expect(Array.isArray(row.destinationPaths)).toBe(true);
			expect(typeof row.maxConfidence).toBe("number");
		}
	});

	it("fs rollup surfaceCount = 2 for shared fs paths", async () => {
		const r = await h.run("modules", "show", "seam-multisurf", ".", "--json");
		const json = JSON.parse(r.stdout) as Record<string, unknown>;
		const literal = ((json.rollup as Record<string, unknown>).fsMutations as Record<string, unknown>).literal as Array<{
			targetPath: string;
			mutationKind: string;
			surfaceCount: number;
		}>;
		const writeAppLog = literal.find(
			(r) => r.targetPath === "logs/app.log" && r.mutationKind === "write_file",
		);
		expect(writeAppLog).toBeDefined();
		expect(writeAppLog!.surfaceCount).toBe(2);
	});

	it("fs rollup preserves rename destinationPaths through aggregation", async () => {
		const r = await h.run("modules", "show", "seam-multisurf", ".", "--json");
		const json = JSON.parse(r.stdout) as Record<string, unknown>;
		const literal = ((json.rollup as Record<string, unknown>).fsMutations as Record<string, unknown>).literal as Array<{
			targetPath: string;
			mutationKind: string;
			destinationPaths: string[];
		}>;
		const rename = literal.find(
			(r) => r.targetPath === "tmp/staging.txt" && r.mutationKind === "rename_path",
		);
		expect(rename).toBeDefined();
		expect(rename!.destinationPaths).toEqual(["data/final.txt"]);
	});

	it("fs dynamic summary aggregates across surfaces", async () => {
		// 1 dynamic occurrence in server.ts × 2 surfaces (shared file
		// ownership) = 2 dynamic evidence rows total.
		const r = await h.run("modules", "show", "seam-multisurf", ".", "--json");
		const json = JSON.parse(r.stdout) as Record<string, unknown>;
		const dyn = ((json.rollup as Record<string, unknown>).fsMutations as Record<string, unknown>).dynamic as {
			totalCount: number;
			distinctFileCount: number;
			byKind: Record<string, number>;
		};
		expect(dyn.totalCount).toBe(2);
		expect(dyn.distinctFileCount).toBe(1);
		expect(dyn.byKind.write_file).toBe(2);
	});

	it("rollup env rows sorted by envName", async () => {
		const r = await h.run("modules", "show", "seam-multisurf", ".", "--json");
		const json = JSON.parse(r.stdout) as Record<string, unknown>;
		const envs = (json.rollup as Record<string, unknown>).envDependencies as Array<{ envName: string }>;
		const names = envs.map((e) => e.envName);
		const sorted = [...names].sort();
		expect(names).toEqual(sorted);
	});

	it("rollup fs rows sorted by targetPath then mutationKind", async () => {
		const r = await h.run("modules", "show", "seam-multisurf", ".", "--json");
		const json = JSON.parse(r.stdout) as Record<string, unknown>;
		const literal = ((json.rollup as Record<string, unknown>).fsMutations as Record<string, unknown>).literal as Array<{
			targetPath: string;
			mutationKind: string;
		}>;
		const keys = literal.map((r) => `${r.targetPath}|${r.mutationKind}`);
		const sorted = [...keys].sort();
		expect(keys).toEqual(sorted);
	});
});

// ── modules show: per-surface direct data ──────────────────────────

describe("modules show — per-surface direct data", () => {
	it("each surface JSON entry includes projectSurfaceUid", async () => {
		const r = await h.run("modules", "show", "seam-multisurf", ".", "--json");
		const json = JSON.parse(r.stdout) as Record<string, unknown>;
		const surfaces = json.surfaces as Array<Record<string, unknown>>;
		expect(surfaces.length).toBe(2);
		for (const s of surfaces) {
			expect(typeof s.projectSurfaceUid).toBe("string");
			expect((s.projectSurfaceUid as string).length).toBeGreaterThan(0);
		}
	});

	it("each surface JSON entry includes envDependencies array", async () => {
		const r = await h.run("modules", "show", "seam-multisurf", ".", "--json");
		const json = JSON.parse(r.stdout) as Record<string, unknown>;
		const surfaces = json.surfaces as Array<Record<string, unknown>>;
		for (const s of surfaces) {
			expect(Array.isArray(s.envDependencies)).toBe(true);
			const envs = s.envDependencies as Array<Record<string, unknown>>;
			expect(envs.length).toBeGreaterThanOrEqual(4);
		}
	});

	it("each surface JSON entry includes fsMutations.literal and .dynamic", async () => {
		const r = await h.run("modules", "show", "seam-multisurf", ".", "--json");
		const json = JSON.parse(r.stdout) as Record<string, unknown>;
		const surfaces = json.surfaces as Array<Record<string, unknown>>;
		for (const s of surfaces) {
			const fs = s.fsMutations as Record<string, unknown>;
			expect(Array.isArray(fs.literal)).toBe(true);
			expect(typeof fs.dynamic).toBe("object");
		}
	});

	it("per-surface env rows have direct (non-aggregated) shape", async () => {
		const r = await h.run("modules", "show", "seam-multisurf", ".", "--json");
		const json = JSON.parse(r.stdout) as Record<string, unknown>;
		const surfaces = json.surfaces as Array<Record<string, unknown>>;
		for (const s of surfaces) {
			const envs = s.envDependencies as Array<Record<string, unknown>>;
			for (const row of envs) {
				// Per-surface rows must not have cross-surface fields.
				expect("surfaceCount" in row).toBe(false);
				expect("hasConflictingDefaults" in row).toBe(false);
				expect("maxConfidence" in row).toBe(false);
				// They must have the direct fields.
				expect(typeof row.envName).toBe("string");
				expect(typeof row.evidenceCount).toBe("number");
				expect(typeof row.confidence).toBe("number");
			}
		}
	});

	it("per-surface fs rows have direct (non-aggregated) shape", async () => {
		const r = await h.run("modules", "show", "seam-multisurf", ".", "--json");
		const json = JSON.parse(r.stdout) as Record<string, unknown>;
		const surfaces = json.surfaces as Array<Record<string, unknown>>;
		for (const s of surfaces) {
			const literal = (s.fsMutations as Record<string, unknown>).literal as Array<Record<string, unknown>>;
			for (const row of literal) {
				expect("surfaceCount" in row).toBe(false);
				expect("maxConfidence" in row).toBe(false);
				expect(typeof row.evidenceCount).toBe("number");
				expect(typeof row.confidence).toBe("number");
			}
		}
	});
});

// ── modules show: human output ─────────────────────────────────────

describe("modules show — human output", () => {
	it("renders Module Rollup header before Surfaces section", async () => {
		const r = await h.run("modules", "show", "seam-multisurf", ".");
		expect(r.exitCode).toBe(0);
		expect(r.stdout).toContain("Module Rollup:");
		const rollupIdx = r.stdout.indexOf("Module Rollup:");
		const surfacesIdx = r.stdout.indexOf("Surfaces (");
		expect(rollupIdx).toBeGreaterThan(0);
		expect(surfacesIdx).toBeGreaterThan(rollupIdx);
	});

	it("rollup human output includes env vars with surfaceCount", async () => {
		const r = await h.run("modules", "show", "seam-multisurf", ".");
		expect(r.stdout).toContain("Env Dependencies");
		expect(r.stdout).toContain("DATABASE_URL");
		expect(r.stdout).toContain("surfaces=2");
	});

	it("rollup human output includes fs rename destination", async () => {
		const r = await h.run("modules", "show", "seam-multisurf", ".");
		expect(r.stdout).toContain("Filesystem Mutations");
		expect(r.stdout).toContain("tmp/staging.txt");
		expect(r.stdout).toContain("-> data/final.txt");
	});

	it("rollup human output includes dynamic-path summary", async () => {
		const r = await h.run("modules", "show", "seam-multisurf", ".");
		expect(r.stdout).toContain("Dynamic-path:");
		expect(r.stdout).toContain("write_file: 2");
	});

	it("per-surface human section uses compact Seams: line", async () => {
		const r = await h.run("modules", "show", "seam-multisurf", ".");
		expect(r.stdout).toContain("Surfaces (2):");
		// Both surfaces should have the compact Seams: breadcrumb.
		const seamLines = r.stdout
			.split("\n")
			.filter((line) => line.trim().startsWith("Seams:"));
		expect(seamLines.length).toBe(2);
		for (const line of seamLines) {
			expect(line).toMatch(/env=\d+/);
			expect(line).toMatch(/fs=\d+/);
		}
	});
});
