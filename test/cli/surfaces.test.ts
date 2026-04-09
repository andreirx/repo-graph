/**
 * CLI integration tests for `rgr surfaces` and `rgr modules show`.
 *
 * Uses the real CLI harness (subprocess execution against a temp DB).
 * Indexes the mixed-lang fixture which produces surfaces (backend_service
 * from express dep, library from Cargo.toml).
 */

import { join } from "node:path";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { createTestHarness, type TestHarness } from "./harness.js";

const MIXED_LANG_FIXTURE = join(
	import.meta.dirname,
	"../fixtures/mixed-lang",
);

let h: TestHarness;

beforeAll(async () => {
	h = await createTestHarness();
	await h.run("repo", "add", MIXED_LANG_FIXTURE, "--name", "mixed-lang");
	await h.run("repo", "index", "mixed-lang");
}, 30000);

afterAll(() => {
	h.cleanup();
});

// ── surfaces list ──────────────────────────────────────────────────

describe("surfaces list", () => {
	it("exits 0 and returns JSON array", async () => {
		const r = await h.run("surfaces", "list", "mixed-lang", "--json");
		expect(r.exitCode).toBe(0);
		const json = JSON.parse(r.stdout) as unknown[];
		expect(Array.isArray(json)).toBe(true);
		expect(json.length).toBeGreaterThanOrEqual(1);
	});

	it("JSON rows have required fields", async () => {
		const r = await h.run("surfaces", "list", "mixed-lang", "--json");
		const json = JSON.parse(r.stdout) as Array<Record<string, unknown>>;
		for (const row of json) {
			expect(row.projectSurfaceUid).toBeTruthy();
			expect(row.moduleCandidateUid).toBeTruthy();
			expect(row.surfaceKind).toBeTruthy();
			expect(row.buildSystem).toBeTruthy();
			expect(row.runtimeKind).toBeTruthy();
			expect(typeof row.confidence).toBe("number");
			expect(typeof row.evidenceCount).toBe("number");
			expect(typeof row.configRootCount).toBe("number");
		}
	});

	it("detects backend_service from express dependency", async () => {
		const r = await h.run("surfaces", "list", "mixed-lang", "--json");
		const json = JSON.parse(r.stdout) as Array<Record<string, unknown>>;
		const backend = json.find((s) => s.surfaceKind === "backend_service");
		expect(backend).toBeDefined();
		expect(backend!.runtimeKind).toBe("node");
	});

	it("human output includes header row", async () => {
		const r = await h.run("surfaces", "list", "mixed-lang");
		expect(r.exitCode).toBe(0);
		expect(r.stdout).toContain("MODULE");
		expect(r.stdout).toContain("KIND");
		expect(r.stdout).toContain("BUILD");
	});
});

// ── surfaces show ──────────────────────────────────────────────────

describe("surfaces show", () => {
	it("shows full detail for a surface by kind", async () => {
		const r = await h.run("surfaces", "show", "mixed-lang", "backend_service", "--json");
		expect(r.exitCode).toBe(0);
		const json = JSON.parse(r.stdout) as Record<string, unknown>;
		expect(json.surface).toBeDefined();
		const surface = json.surface as Record<string, unknown>;
		expect(surface.surfaceKind).toBe("backend_service");
		expect(json.module).toBeDefined();
		expect(json.evidence).toBeDefined();
		expect(Array.isArray(json.configRoots)).toBe(true);
		expect(Array.isArray(json.entrypoints)).toBe(true);
	});

	it("exits 1 for nonexistent surface", async () => {
		const r = await h.run("surfaces", "show", "mixed-lang", "nonexistent");
		expect(r.exitCode).toBe(1);
	});

	it("human output includes module and build info", async () => {
		const r = await h.run("surfaces", "show", "mixed-lang", "backend_service");
		expect(r.exitCode).toBe(0);
		expect(r.stdout).toContain("Surface:");
		expect(r.stdout).toContain("Module:");
		expect(r.stdout).toContain("Runtime:");
	});
});

// ── surfaces evidence ──────────────────────────────────────────────

describe("surfaces evidence", () => {
	it("returns evidence array for a surface", async () => {
		const r = await h.run("surfaces", "evidence", "mixed-lang", "backend_service", "--json");
		expect(r.exitCode).toBe(0);
		const json = JSON.parse(r.stdout) as unknown[];
		expect(Array.isArray(json)).toBe(true);
		expect(json.length).toBeGreaterThanOrEqual(1);
	});

	it("exits 1 for nonexistent surface", async () => {
		const r = await h.run("surfaces", "evidence", "mixed-lang", "nonexistent");
		expect(r.exitCode).toBe(1);
	});
});

// ── modules show ───────────────────────────────────────────────────

describe("modules show", () => {
	it("shows module detail with surfaces in JSON", async () => {
		const r = await h.run("modules", "show", "mixed-lang", ".", "--json");
		expect(r.exitCode).toBe(0);
		const json = JSON.parse(r.stdout) as Record<string, unknown>;
		expect(json.module).toBeDefined();
		const mod = json.module as Record<string, unknown>;
		expect(mod.canonicalRootPath).toBe(".");
		expect(mod.displayName).toBe("mixed-lang-fixture");
		expect(typeof json.fileCount).toBe("number");
		expect(Array.isArray(json.surfaces)).toBe(true);
		expect(Array.isArray(json.evidence)).toBe(true);
	});

	it("surfaces include build and runtime info", async () => {
		const r = await h.run("modules", "show", "mixed-lang", ".", "--json");
		const json = JSON.parse(r.stdout) as Record<string, unknown>;
		const surfaces = json.surfaces as Array<Record<string, unknown>>;
		expect(surfaces.length).toBeGreaterThanOrEqual(1);
		for (const s of surfaces) {
			expect(s.surfaceKind).toBeTruthy();
			expect(s.buildSystem).toBeTruthy();
			expect(s.runtimeKind).toBeTruthy();
		}
	});

	it("exits 1 for nonexistent module", async () => {
		const r = await h.run("modules", "show", "mixed-lang", "nonexistent");
		expect(r.exitCode).toBe(1);
	});

	it("human output includes files and surfaces", async () => {
		const r = await h.run("modules", "show", "mixed-lang", ".");
		expect(r.exitCode).toBe(0);
		expect(r.stdout).toContain("Module:");
		expect(r.stdout).toContain("Files:");
		expect(r.stdout).toContain("Surfaces");
	});
});
