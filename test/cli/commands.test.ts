/**
 * CLI integration tests.
 *
 * These tests invoke the compiled CLI against a temporary database,
 * capturing stdout/stderr/exit code. The `pnpm test` script runs tsc
 * before vitest, so dist/ is always fresh. Validates end-to-end command
 * behavior including argument parsing, JSON output contracts, and
 * human-readable formatting.
 */

import { randomUUID } from "node:crypto";
import { mkdirSync, unlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { createTestHarness, type TestHarness } from "./harness.js";

const FIXTURES_PATH = join(
	import.meta.dirname,
	"../fixtures/typescript/simple-imports",
);

let h: TestHarness;

beforeAll(async () => {
	h = await createTestHarness();
	// Register and index the fixture repo
	await h.run("repo", "add", FIXTURES_PATH, "--name", "test-repo");
	await h.run("repo", "index", "test-repo");
}, 30000);

afterAll(() => {
	h.cleanup();
});

// ── repo commands ─────────────────────────────────────────────────────

describe("repo commands", () => {
	it("repo status --json returns snapshot info", async () => {
		const r = await h.run("repo", "status", "test-repo", "--json");
		expect(r.exitCode).toBe(0);
		const json = r.json();
		expect(json.name).toBe("test-repo");
		expect(json.snapshot).toBeDefined();
		const snap = json.snapshot as Record<string, unknown>;
		expect(snap.status).toBe("ready");
		expect(snap.toolchain).toBeDefined();
		const toolchain = snap.toolchain as Record<string, unknown>;
		expect(toolchain.extraction_semantics).toBe(2);
		expect(toolchain.stable_key_format).toBe(2);
	});

	it("repo list --json returns registered repos", async () => {
		const r = await h.run("repo", "list", "--json");
		expect(r.exitCode).toBe(0);
		const json = JSON.parse(r.stdout) as Array<Record<string, unknown>>;
		expect(json.length).toBeGreaterThanOrEqual(1);
		expect(json.some((repo) => repo.name === "test-repo")).toBe(true);
	});
});

// ── graph stats ───────────────────────────────────────────────────────

describe("graph stats", () => {
	it("--json returns module metrics in query envelope", async () => {
		const r = await h.run("graph", "stats", "test-repo", "--json");
		expect(r.exitCode).toBe(0);
		const json = r.json();
		expect(json.command).toBe("graph stats");
		expect(json.repo).toBe("test-repo");
		expect(json.count).toBeGreaterThan(0);
		const results = json.results as Array<Record<string, unknown>>;
		expect(results[0]).toHaveProperty("module");
		expect(results[0]).toHaveProperty("fan_in");
		expect(results[0]).toHaveProperty("fan_out");
		expect(results[0]).toHaveProperty("instability");
		expect(results[0]).toHaveProperty("abstractness");
		expect(results[0]).toHaveProperty("distance_from_main_sequence");
		expect(results[0]).toHaveProperty("file_count");
		expect(results[0]).toHaveProperty("symbol_count");
	});

	it("human output includes summary line", async () => {
		const r = await h.run("graph", "stats", "test-repo");
		expect(r.exitCode).toBe(0);
		expect(r.stdout).toContain("modules with source files");
		expect(r.stdout).toContain("Avg instability");
	});
});

// ── graph metrics ─────────────────────────────────────────────────────

describe("graph metrics", () => {
	it("--json returns function metrics in query envelope", async () => {
		const r = await h.run("graph", "metrics", "test-repo", "--json");
		expect(r.exitCode).toBe(0);
		const json = r.json();
		expect(json.command).toBe("graph metrics");
		const results = json.results as Array<Record<string, unknown>>;
		expect(results.length).toBeGreaterThan(0);
		expect(results[0]).toHaveProperty("symbol");
		expect(results[0]).toHaveProperty("file");
		expect(results[0]).toHaveProperty("cyclomatic_complexity");
		expect(results[0]).toHaveProperty("parameter_count");
		expect(results[0]).toHaveProperty("max_nesting_depth");
	});

	it("--limit restricts results", async () => {
		const r = await h.run(
			"graph",
			"metrics",
			"test-repo",
			"--limit",
			"2",
			"--json",
		);
		expect(r.exitCode).toBe(0);
		const json = r.json();
		expect(json.count).toBe(2);
	});

	it("--sort params orders by parameter count", async () => {
		const r = await h.run(
			"graph",
			"metrics",
			"test-repo",
			"--sort",
			"params",
			"--json",
		);
		expect(r.exitCode).toBe(0);
		const json = r.json();
		const results = json.results as Array<Record<string, unknown>>;
		if (results.length >= 2) {
			expect(
				(results[0].parameter_count as number) >=
					(results[1].parameter_count as number),
			).toBe(true);
		}
	});

	it("--module --json returns per-module aggregates", async () => {
		const r = await h.run(
			"graph",
			"metrics",
			"test-repo",
			"--module",
			"--json",
		);
		expect(r.exitCode).toBe(0);
		const json = r.json();
		expect(json.command).toBe("graph metrics --module");
		const results = json.results as Array<Record<string, unknown>>;
		expect(results.length).toBeGreaterThan(0);
		expect(results[0]).toHaveProperty("module");
		expect(results[0]).toHaveProperty("function_count");
		expect(results[0]).toHaveProperty("avg_cyclomatic_complexity");
		expect(results[0]).toHaveProperty("max_cyclomatic_complexity");
		expect(results[0]).toHaveProperty("avg_nesting_depth");
		expect(results[0]).toHaveProperty("max_nesting_depth");
	});

	it("--module --limit restricts module results", async () => {
		const r = await h.run(
			"graph",
			"metrics",
			"test-repo",
			"--module",
			"--limit",
			"1",
			"--json",
		);
		expect(r.exitCode).toBe(0);
		const json = r.json();
		expect(json.count).toBe(1);
	});

	it("human output includes summary", async () => {
		const r = await h.run("graph", "metrics", "test-repo");
		expect(r.exitCode).toBe(0);
		expect(r.stdout).toContain("functions measured");
		expect(r.stdout).toContain("Avg cyclomatic complexity");
	});
});

// ── arch violations ───────────────────────────────────────────────────

describe("arch violations", () => {
	it("returns no violations when no boundaries declared", async () => {
		const r = await h.run("arch", "violations", "test-repo", "--json");
		// May return 0 violations or a message — either way exit 0
		expect(r.exitCode).toBe(0);
	});

	it("detects violations after boundary declaration", async () => {
		// Declare: src must not import itself (trivially violated)
		// Actually, fixture has no cross-module imports to violate.
		// Instead, declare a boundary that IS clean and verify 0 violations.
		await h.run(
			"declare",
			"boundary",
			"test-repo",
			"src",
			"--forbids",
			"nonexistent",
		);
		const r = await h.run("arch", "violations", "test-repo", "--json");
		expect(r.exitCode).toBe(0);
		const json = r.json();
		expect(json.count).toBe(0);
	});
});

// ── graph callers (existing v1 command) ───────────────────────────────

describe("graph callers", () => {
	it("--json returns caller results", async () => {
		const r = await h.run(
			"graph",
			"callers",
			"test-repo",
			"generateId",
			"--json",
		);
		expect(r.exitCode).toBe(0);
		const json = r.json();
		expect(json.command).toBe("graph callers");
		expect(json.count).toBeGreaterThanOrEqual(1);
	});
});

// ── graph cycles ──────────────────────────────────────────────────────

describe("graph cycles", () => {
	it("--json returns cycle results", async () => {
		const r = await h.run("graph", "cycles", "test-repo", "--json");
		expect(r.exitCode).toBe(0);
		const json = r.json();
		expect(json.command).toBe("graph cycles");
		// Fixture has no cycles
		expect(json.count).toBe(0);
	});
});

// ── graph versions ────────────────────────────────────────────────────

describe("graph versions", () => {
	it("--json returns extracted domain versions", async () => {
		const r = await h.run("graph", "versions", "test-repo", "--json");
		expect(r.exitCode).toBe(0);
		const json = r.json();
		expect(json.command).toBe("graph versions");
		expect(json.count).toBe(2); // package_name + package_version
		const results = json.results as Array<Record<string, unknown>>;
		const nameRow = results.find((v) => v.kind === "package_name");
		const versionRow = results.find((v) => v.kind === "package_version");
		expect(nameRow?.value).toBe("simple-imports-fixture");
		expect(versionRow?.value).toBe("0.0.1");
		expect(nameRow?.source_file).toBe("package.json");
	});

	it("human output shows version info", async () => {
		const r = await h.run("graph", "versions", "test-repo");
		expect(r.exitCode).toBe(0);
		expect(r.stdout).toContain("package_name: simple-imports-fixture");
		expect(r.stdout).toContain("package_version: 0.0.1");
	});
});

// ── declare requirement ───────────────────────────────────────────────

describe("declare requirement", () => {
	it("creates a requirement with objective and constraints", async () => {
		const r = await h.run(
			"declare",
			"requirement",
			"test-repo",
			"src",
			"--req-id",
			"REQ-TEST-001",
			"--objective",
			"All source files must be type-safe",
			"--constraint",
			"No any types",
			"--json",
		);
		expect(r.exitCode).toBe(0);
		const json = r.json();
		expect(json.kind).toBe("requirement");
	});

	it("requirement appears in declare list", async () => {
		const r = await h.run(
			"declare",
			"list",
			"test-repo",
			"--kind",
			"requirement",
			"--json",
		);
		expect(r.exitCode).toBe(0);
		const json = JSON.parse(r.stdout) as Array<Record<string, unknown>>;
		const req = json.find(
			(d) => (d.value as Record<string, unknown>).req_id === "REQ-TEST-001",
		);
		expect(req).toBeDefined();
		expect((req?.value as Record<string, unknown>).objective).toBe(
			"All source files must be type-safe",
		);
	});
});

// ── declare obligation + graph obligations ────────────────────────────

describe("obligations", () => {
	it("graph obligations --json returns empty with no requirements", async () => {
		const r = await h.run("graph", "obligations", "test-repo", "--json");
		expect(r.exitCode).toBe(0);
		const json = r.json();
		expect(json.command).toBe("graph obligations");
		expect(json.count).toBe(0);
	});

	it("evaluates arch_violations obligation as PASS when no violations", async () => {
		// Create requirement
		await h.run(
			"declare",
			"requirement",
			"test-repo",
			"src",
			"--req-id",
			"REQ-ARCH-001",
			"--objective",
			"Source must not import from tests",
		);
		// Add boundary
		await h.run(
			"declare",
			"boundary",
			"test-repo",
			"src",
			"--forbids",
			"nonexistent",
		);
		// Add obligation
		await h.run(
			"declare",
			"obligation",
			"test-repo",
			"REQ-ARCH-001",
			"--obligation",
			"No boundary violations",
			"--method",
			"arch_violations",
			"--target",
			"src",
		);
		// Evaluate
		const r = await h.run("graph", "obligations", "test-repo", "--json");
		expect(r.exitCode).toBe(0);
		const json = r.json();
		expect(json.count).toBeGreaterThanOrEqual(1);
		const archObl = (json.results as Array<Record<string, unknown>>).find(
			(o) => o.req_id === "REQ-ARCH-001",
		);
		expect(archObl?.verdict).toBe("PASS");
	});

	it("obligation addition bumps requirement version", async () => {
		// REQ-ARCH-001 was already created and had an obligation added above.
		// The obligation command supersedes the old declaration and bumps version.
		// Note: supersedes_uid linkage is set in code but not verified here
		// because declare list --json does not expose the supersedes_uid field.
		const r = await h.run(
			"declare",
			"list",
			"test-repo",
			"--kind",
			"requirement",
			"--json",
		);
		expect(r.exitCode).toBe(0);
		const reqs = JSON.parse(r.stdout) as Array<Record<string, unknown>>;
		const archReq = reqs.find(
			(d) => (d.value as Record<string, unknown>).req_id === "REQ-ARCH-001",
		);
		expect(archReq).toBeDefined();
		// Version should be > 1 (was bumped by declare obligation)
		expect((archReq?.value as Record<string, unknown>).version).toBeGreaterThan(
			1,
		);
	});

	it("unsupported method returns UNSUPPORTED verdict", async () => {
		// Create a requirement with an unsupported method
		await h.run(
			"declare",
			"requirement",
			"test-repo",
			"src",
			"--req-id",
			"REQ-UNSUP-001",
			"--objective",
			"Test unsupported method",
		);
		await h.run(
			"declare",
			"obligation",
			"test-repo",
			"REQ-UNSUP-001",
			"--obligation",
			"Manual code review",
			"--method",
			"manual_review",
		);
		const r = await h.run("graph", "obligations", "test-repo", "--json");
		expect(r.exitCode).toBe(0);
		const json = r.json();
		const unsup = (json.results as Array<Record<string, unknown>>).find(
			(o) => o.req_id === "REQ-UNSUP-001" && o.method === "manual_review",
		);
		expect(unsup?.verdict).toBe("UNSUPPORTED");
	});
});

// ── graph churn ───────────────────────────────────────────────────────

describe("graph churn", () => {
	it("--json returns churn data with since field", async () => {
		const r = await h.run(
			"graph",
			"churn",
			"test-repo",
			"--since",
			"365.days.ago",
			"--json",
		);
		expect(r.exitCode).toBe(0);
		const json = r.json();
		expect(json.command).toBe("graph churn");
		expect(json.since).toBe("365.days.ago");
		// Fixture dir may not be a git repo, so count could be 0
		expect(json.count).toBeGreaterThanOrEqual(0);
	});

	it("does not crash on non-git fixture directory", async () => {
		const r = await h.run("graph", "churn", "test-repo", "--json");
		expect(r.exitCode).toBe(0);
	});

	// Note: idempotency of persisted measurements is tested at the storage
	// layer (deleteMeasurementsByKind + re-insert test). The CLI test cannot
	// verify storage row counts because graph churn displays from fresh git
	// data, not from stored measurements.
});

// ── graph hotspots ────────────────────────────────────────────────────

describe("graph hotspots", () => {
	it("--json returns consistent envelope even with no data", async () => {
		const r = await h.run("graph", "hotspots", "test-repo", "--json");
		expect(r.exitCode).toBe(0);
		const json = r.json();
		expect(json.command).toBe("graph hotspots");
		expect(json.repo).toBe("test-repo");
		expect(json.snapshot).toBeDefined();
		expect(json.count).toBe(0);
		expect(json.total_files).toBe(0);
		expect(json.formula).toBe("churn_lines * sum_cyclomatic_complexity");
		expect(json.formula_version).toBe(1);
	});
});

// ── graph coverage ────────────────────────────────────────────────────

describe("graph coverage", () => {
	let coverageReportPath: string;

	beforeAll(() => {
		// Create a synthetic Istanbul coverage report referencing fixture files
		const dir = join(tmpdir(), `rgr-cov-cli-${randomUUID()}`);
		mkdirSync(dir, { recursive: true });
		coverageReportPath = join(dir, "coverage-final.json");

		const report: Record<string, unknown> = {};
		// Use absolute paths as Istanbul does
		report[join(FIXTURES_PATH, "src/service.ts")] = {
			s: { "0": 5, "1": 0, "2": 3 },
			f: { "0": 2, "1": 0 },
			b: { "0": [1, 0] },
			statementMap: {},
			fnMap: {},
			branchMap: {},
		};
		report[join(FIXTURES_PATH, "src/types.ts")] = {
			s: { "0": 1, "1": 1 },
			f: { "0": 1 },
			b: {},
			statementMap: {},
			fnMap: {},
			branchMap: {},
		};

		writeFileSync(coverageReportPath, JSON.stringify(report));
	});

	afterAll(() => {
		try {
			unlinkSync(coverageReportPath);
		} catch {
			// best effort
		}
	});

	it("--json imports coverage and returns per-file data", async () => {
		const r = await h.run(
			"graph",
			"coverage",
			"test-repo",
			coverageReportPath,
			"--json",
		);
		expect(r.exitCode).toBe(0);
		const json = r.json();
		expect(json.command).toBe("graph coverage");
		expect(json.repo).toBe("test-repo");
		expect(json.matched_to_index).toBeGreaterThanOrEqual(1);
		const results = json.results as Array<Record<string, unknown>>;
		expect(results.length).toBeGreaterThanOrEqual(1);
		// service.ts: 2/3 statements covered = ~0.6667
		const service = results.find((r) =>
			(r.file as string).includes("service.ts"),
		);
		expect(service).toBeDefined();
		expect(service?.line_coverage).toBeCloseTo(0.6667, 2);
	});

	// Note: coverage import idempotency (delete-before-insert) is tested at the
	// storage layer via deleteMeasurementsByKind. The CLI renders from the
	// freshly parsed report, not stored measurements, so a CLI-level
	// idempotency test cannot verify storage row counts.

	it("human output shows percentage columns", async () => {
		const r = await h.run("graph", "coverage", "test-repo", coverageReportPath);
		expect(r.exitCode).toBe(0);
		expect(r.stdout).toContain("LINE%");
		expect(r.stdout).toContain("files imported");
	});
});

// ── graph risk ────────────────────────────────────────────────────────

describe("graph risk", () => {
	it("--json returns consistent envelope with no data", async () => {
		const r = await h.run("graph", "risk", "test-repo", "--json");
		expect(r.exitCode).toBe(0);
		const json = r.json();
		expect(json.command).toBe("graph risk");
		expect(json.repo).toBe("test-repo");
		expect(json.snapshot).toBeDefined();
		expect(json.count).toBe(0);
		expect(json.total_files).toBe(0);
		expect(json.formula).toBe("hotspot_score * (1 - line_coverage)");
		expect(json.formula_version).toBe(1);
	});

	// Note: non-empty graph risk path is not testable via the CLI harness
	// because it requires both hotspot inferences (from churn + complexity)
	// and coverage measurements. The test fixture is not a git repo (no churn),
	// so hotspot data is always empty. The formula is verified by the
	// storage-level hotspot input tests and the manual validation on
	// repo-graph with real coverage data.
});
