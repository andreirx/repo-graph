/**
 * Trust command CLI integration tests.
 *
 * Exercises `rgr trust <repo>` against the simple-imports fixture,
 * verifying the full JSON envelope shape, reliability axes, and
 * downgrade trigger fields are emitted correctly.
 */

import { join } from "node:path";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { createTestHarness, type TestHarness } from "./harness.js";

const FIXTURES_PATH = join(
	import.meta.dirname,
	"../fixtures/typescript/simple-imports",
);
const REPO_NAME = "trust-test-repo";

let h: TestHarness;

beforeAll(async () => {
	h = await createTestHarness();
	await h.run("repo", "add", FIXTURES_PATH, "--name", REPO_NAME);
	await h.run("repo", "index", REPO_NAME);
}, 30000);

afterAll(() => {
	h.cleanup();
});

describe("trust command — JSON envelope", () => {
	it("emits the full envelope with reliability + downgrade fields", async () => {
		const r = await h.run("trust", REPO_NAME, "--json");
		expect(r.exitCode).toBe(0);
		const json = JSON.parse(r.stdout);

		expect(json.command).toBe("trust");
		expect(json.repo).toBe(REPO_NAME);
		expect(json.snapshot_uid).toBeDefined();
		expect(json.diagnostics_available).toBe(true);

		// Provenance fields for cross-snapshot comparability
		expect(json.toolchain).toBeDefined();
		expect(json.toolchain).not.toBeNull();
		expect(json.toolchain.extraction_semantics).toBeDefined();
		expect(json.toolchain.stable_key_format).toBeDefined();
		expect(json.diagnostics_version).toBe(2);

		// Summary
		expect(json.summary).toBeDefined();
		expect(json.summary.edges_total).toBeGreaterThanOrEqual(0);
		expect(json.summary.unresolved_total).toBeGreaterThanOrEqual(0);
		expect(json.summary.call_resolution_rate).toBeGreaterThanOrEqual(0);
		expect(json.summary.call_resolution_rate).toBeLessThanOrEqual(1);

		// Reliability has all four axes with level + reasons
		const axes = ["import_graph", "call_graph", "dead_code", "change_impact"];
		for (const axis of axes) {
			expect(json.summary.reliability[axis]).toBeDefined();
			expect(["HIGH", "MEDIUM", "LOW"]).toContain(
				json.summary.reliability[axis].level,
			);
			expect(Array.isArray(json.summary.reliability[axis].reasons)).toBe(true);
		}

		// Downgrade triggers have all four flags
		const flags = [
			"framework_heavy_suspicion",
			"registry_pattern_suspicion",
			"missing_entrypoint_declarations",
			"alias_resolution_suspicion",
		];
		for (const flag of flags) {
			expect(json.summary.triggered_downgrades[flag]).toBeDefined();
			expect(typeof json.summary.triggered_downgrades[flag].triggered).toBe(
				"boolean",
			);
			expect(
				Array.isArray(json.summary.triggered_downgrades[flag].reasons),
			).toBe(true);
		}

		// Categories + classifications + modules + caveats are arrays
		expect(Array.isArray(json.categories)).toBe(true);
		expect(Array.isArray(json.classifications)).toBe(true);
		expect(Array.isArray(json.modules)).toBe(true);
		expect(Array.isArray(json.caveats)).toBe(true);
	});

	it("emits classification_breakdown rows with machine keys + counts", async () => {
		const r = await h.run("trust", REPO_NAME, "--json");
		const json = JSON.parse(r.stdout);
		// The simple-imports fixture has unresolved edges from this.repo.*
		// calls, so classifications should be non-empty post-migration-007.
		if (json.classifications.length > 0) {
			for (const row of json.classifications) {
				expect(typeof row.classification).toBe("string");
				expect(typeof row.count).toBe("number");
				expect(row.count).toBeGreaterThan(0);
			}
			// Sorted by count desc (non-strictly)
			for (let i = 1; i < json.classifications.length; i++) {
				expect(json.classifications[i - 1].count).toBeGreaterThanOrEqual(
					json.classifications[i].count,
				);
			}
		}
	});

	it("missing_entrypoint_declarations triggers on fixture with no entrypoints", async () => {
		const r = await h.run("trust", REPO_NAME, "--json");
		const json = JSON.parse(r.stdout);
		expect(
			json.summary.triggered_downgrades.missing_entrypoint_declarations
				.triggered,
		).toBe(true);
	});

	it("category rows include machine key + human label", async () => {
		const r = await h.run("trust", REPO_NAME, "--json");
		const json = JSON.parse(r.stdout);
		if (json.categories.length > 0) {
			const c = json.categories[0];
			expect(typeof c.category).toBe("string");
			expect(typeof c.label).toBe("string");
			expect(typeof c.unresolved).toBe("number");
			// Machine key uses snake_case; label is human-readable
			expect(c.label).not.toBe(c.category);
		}
	});

	it("module rows include stable_key + suspicious flag + trust notes", async () => {
		const r = await h.run("trust", REPO_NAME, "--json");
		const json = JSON.parse(r.stdout);
		if (json.modules.length > 0) {
			const m = json.modules[0];
			expect(typeof m.module_stable_key).toBe("string");
			expect(typeof m.qualified_name).toBe("string");
			expect(typeof m.fan_in).toBe("number");
			expect(typeof m.fan_out).toBe("number");
			expect(typeof m.file_count).toBe("number");
			expect(typeof m.suspicious_zero_connectivity).toBe("boolean");
			expect(Array.isArray(m.trust_notes)).toBe(true);
		}
	});

	it("emits caveats describing trust posture", async () => {
		const r = await h.run("trust", REPO_NAME, "--json");
		const json = JSON.parse(r.stdout);
		expect(json.caveats.length).toBeGreaterThan(0);
		// At least one caveat should mention the cycle payload limitation
		const combined = json.caveats.join(" ").toLowerCase();
		expect(combined).toContain("cycle");
	});
});

describe("trust command — error paths", () => {
	it("exits 1 for nonexistent repo", async () => {
		const r = await h.run("trust", "does-not-exist", "--json");
		expect(r.exitCode).toBe(1);
	});
});

describe("trust command — human output", () => {
	it("shows reliability, triggers, and caveats sections", async () => {
		const r = await h.run("trust", REPO_NAME);
		expect(r.exitCode).toBe(0);
		expect(r.stdout).toContain("Trust Report");
		expect(r.stdout).toContain("Reliability:");
		expect(r.stdout).toContain("Downgrade triggers:");
		expect(r.stdout).toContain("Caveats:");
	});
});

// ── unresolved-samples subcommand ───────────────────────────────────

describe("trust unresolved-samples — JSON envelope", () => {
	it("emits samples with the full envelope", async () => {
		const r = await h.run(
			"trust",
			"unresolved-samples",
			REPO_NAME,
			"--json",
			"--limit",
			"5",
		);
		expect(r.exitCode).toBe(0);
		const json = JSON.parse(r.stdout);
		expect(json.command).toBe("trust unresolved-samples");
		expect(json.repo).toBe(REPO_NAME);
		expect(json.snapshot_uid).toBeDefined();
		expect(json.filters).toBeDefined();
		expect(json.filters.limit).toBe(5);
		expect(typeof json.count).toBe("number");
		expect(Array.isArray(json.samples)).toBe(true);
		expect(json.count).toBe(json.samples.length);
		expect(json.samples.length).toBeLessThanOrEqual(5);
		// Each sample row must have the shape we committed to
		for (const s of json.samples) {
			expect(typeof s.edgeUid).toBe("string");
			expect(typeof s.classification).toBe("string");
			expect(typeof s.category).toBe("string");
			expect(typeof s.basisCode).toBe("string");
			expect(typeof s.targetKey).toBe("string");
			expect(typeof s.sourceNodeUid).toBe("string");
		}
	});

	it("applies --bucket filter", async () => {
		const r = await h.run(
			"trust",
			"unresolved-samples",
			REPO_NAME,
			"--bucket",
			"internal_candidate",
			"--json",
			"--limit",
			"50",
		);
		expect(r.exitCode).toBe(0);
		const json = JSON.parse(r.stdout);
		for (const s of json.samples) {
			expect(s.classification).toBe("internal_candidate");
		}
	});

	it("rejects unknown --bucket value with exit 1", async () => {
		const r = await h.run(
			"trust",
			"unresolved-samples",
			REPO_NAME,
			"--bucket",
			"nonsense_bucket",
			"--json",
		);
		expect(r.exitCode).toBe(1);
		const json = JSON.parse(r.stdout);
		expect(json.error).toContain("Unknown --bucket");
	});

	it("rejects unknown --category value with exit 1", async () => {
		const r = await h.run(
			"trust",
			"unresolved-samples",
			REPO_NAME,
			"--category",
			"nonsense_cat",
			"--json",
		);
		expect(r.exitCode).toBe(1);
	});

	it("rejects invalid --limit value with exit 1", async () => {
		const r = await h.run(
			"trust",
			"unresolved-samples",
			REPO_NAME,
			"--limit",
			"not-a-number",
			"--json",
		);
		expect(r.exitCode).toBe(1);
	});
});
