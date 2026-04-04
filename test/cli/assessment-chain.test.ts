/**
 * Assessment chain integration tests.
 *
 * Exercises the full assessment pipeline end-to-end against a real
 * temporary git repository with two commits, real churn data,
 * synthetic coverage, and obligation evaluation.
 *
 * Chain under test:
 *   repo index -> graph churn -> graph hotspots -> graph coverage -> graph risk -> obligations
 *
 * Fixture design (from git-harness.ts):
 *   src/complex.ts  — high CC (sum ~14), modified in both commits (highest churn)
 *   src/simple.ts   — low CC (sum 3), small modification (moderate churn)
 *   src/stable.ts   — no functions (CC 0), untouched after commit 1
 *   src/index.ts    — imports only, no functions
 *
 * Coverage setup:
 *   complex.ts — 1/3 statements covered (~33%)
 *   simple.ts  — 3/3 statements covered (100%)
 *   stable.ts  — no coverage entry (treated as 0% by risk formula)
 *
 * Expected ranking:
 *   Hotspot: complex.ts >> simple.ts (stable.ts excluded, no CC)
 *   Risk:    complex.ts high (hotspot * 0.67), simple.ts zero (hotspot * 0.0)
 *
 * Architecture note — order coupling:
 *   The full pipeline is run in a single beforeAll so that all describe
 *   blocks can assert against the resulting state without depending on
 *   test execution order. Individual tests are pure read-only queries.
 *   The only exception is the obligations describe, which declares
 *   requirements and obligations in its own beforeAll (these are
 *   additive declarations, not mutations of pipeline state).
 */

import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { createGitTestRepo, type GitTestRepo } from "./git-harness.js";
import { createTestHarness, type TestHarness } from "./harness.js";

const REPO_NAME = "assessment-repo";

let h: TestHarness;
let gitRepo: GitTestRepo;

// ── Pipeline setup (runs once) ───────────────────────────────────────
// Establishes all prerequisite state in declared order so individual
// test blocks are self-contained assertions, not implicit order deps.

beforeAll(async () => {
	h = await createTestHarness();
	gitRepo = await createGitTestRepo();

	// 1. Register and index
	await h.run("repo", "add", gitRepo.repoDir, "--name", REPO_NAME);
	await h.run("repo", "index", REPO_NAME);

	// 2. Import churn from real git history
	await h.run("graph", "churn", REPO_NAME, "--since", "365.days.ago");

	// 3. Compute hotspots (requires churn + CC from index)
	await h.run("graph", "hotspots", REPO_NAME);

	// 4. Import synthetic coverage
	const coverageReportPath = gitRepo.writeCoverageReport([
		{
			file: "src/complex.ts",
			statements: { covered: 1, total: 3 },
			functions: { covered: 1, total: 3 },
			branches: { covered: 0, total: 2 },
		},
		{
			file: "src/simple.ts",
			statements: { covered: 3, total: 3 },
			functions: { covered: 3, total: 3 },
			branches: { covered: 0, total: 0 },
		},
	]);
	await h.run("graph", "coverage", REPO_NAME, coverageReportPath);

	// 5. Compute risk (requires hotspots + coverage)
	await h.run("graph", "risk", REPO_NAME);
}, 30000);

afterAll(() => {
	h.cleanup();
	gitRepo.cleanup();
});

// ── graph churn ──────────────────────────────────────────────────────

describe("graph churn (git repo)", () => {
	it("returns non-empty churn data for files modified across commits", async () => {
		const r = await h.run(
			"graph",
			"churn",
			REPO_NAME,
			"--since",
			"365.days.ago",
			"--json",
		);
		expect(r.exitCode).toBe(0);
		const json = r.json();
		expect(json.command).toBe("graph churn");
		expect(json.count).toBeGreaterThan(0);

		const results = json.results as Array<{
			file: string;
			commit_count: number;
			lines_changed: number;
		}>;

		// complex.ts: 2 commits (initial + expansion)
		const complex = results.find((f) => f.file.includes("complex.ts"));
		expect(complex).toBeDefined();
		expect(complex!.commit_count).toBe(2);
		expect(complex!.lines_changed).toBeGreaterThan(0);

		// simple.ts: 2 commits (initial + small addition)
		const simple = results.find((f) => f.file.includes("simple.ts"));
		expect(simple).toBeDefined();
		expect(simple!.commit_count).toBe(2);

		// complex.ts should have more lines changed than simple.ts
		expect(complex!.lines_changed).toBeGreaterThan(simple!.lines_changed);

		// stable.ts: 1 commit only
		const stable = results.find((f) => f.file.includes("stable.ts"));
		expect(stable).toBeDefined();
		expect(stable!.commit_count).toBe(1);
	});
});

// ── graph hotspots ───────────────────────────────────────────────────

describe("graph hotspots (git repo)", () => {
	it("ranks complex.ts first by churn * complexity", async () => {
		const r = await h.run("graph", "hotspots", REPO_NAME, "--json");
		expect(r.exitCode).toBe(0);
		const json = r.json();
		expect(json.command).toBe("graph hotspots");
		expect(json.total_files).toBeGreaterThan(0);
		expect(json.formula_version).toBe(1);

		const results = json.results as Array<{
			file: string;
			normalized_score: number;
			raw_score: number;
			churn_lines: number;
			sum_complexity: number;
		}>;
		expect(results.length).toBeGreaterThanOrEqual(2);

		// complex.ts should be rank 1 (normalized_score = 1.0)
		expect(results[0].file).toContain("complex.ts");
		expect(results[0].normalized_score).toBe(1.0);
		expect(results[0].sum_complexity).toBeGreaterThan(0);
		expect(results[0].churn_lines).toBeGreaterThan(0);

		// simple.ts should be present but ranked lower
		const simple = results.find((f) => f.file.includes("simple.ts"));
		expect(simple).toBeDefined();
		expect(simple!.normalized_score).toBeLessThan(1.0);

		// stable.ts should NOT appear (no functions -> no CC -> excluded from join)
		const stable = results.find((f) => f.file.includes("stable.ts"));
		expect(stable).toBeUndefined();
	});

	it("human output shows column headers and summary", async () => {
		const r = await h.run("graph", "hotspots", REPO_NAME);
		expect(r.exitCode).toBe(0);
		expect(r.stdout).toContain("SCORE");
		expect(r.stdout).toContain("CHURN");
		expect(r.stdout).toContain("SUM_CC");
		expect(r.stdout).toContain("files with hotspot data");
	});
});

// ── graph coverage ───────────────────────────────────────────────────

describe("graph coverage (git repo)", () => {
	it("returns correct coverage ratios for indexed files", async () => {
		// Re-import to get JSON output (pipeline already imported in beforeAll)
		const coverageReportPath = gitRepo.writeCoverageReport([
			{
				file: "src/complex.ts",
				statements: { covered: 1, total: 3 },
				functions: { covered: 1, total: 3 },
				branches: { covered: 0, total: 2 },
			},
			{
				file: "src/simple.ts",
				statements: { covered: 3, total: 3 },
				functions: { covered: 3, total: 3 },
				branches: { covered: 0, total: 0 },
			},
		]);
		const r = await h.run(
			"graph",
			"coverage",
			REPO_NAME,
			coverageReportPath,
			"--json",
		);
		expect(r.exitCode).toBe(0);
		const json = r.json();
		expect(json.command).toBe("graph coverage");
		expect(json.matched_to_index).toBeGreaterThanOrEqual(2);

		const results = json.results as Array<{
			file: string;
			line_coverage: number | null;
		}>;

		// complex.ts: 1/3 statements -> ~0.3333
		const complex = results.find((f) =>
			(f.file as string).includes("complex.ts"),
		);
		expect(complex).toBeDefined();
		expect(complex!.line_coverage).toBeCloseTo(0.3333, 2);

		// simple.ts: 3/3 -> 1.0
		const simple = results.find((f) =>
			(f.file as string).includes("simple.ts"),
		);
		expect(simple).toBeDefined();
		expect(simple!.line_coverage).toBe(1.0);
	});
});

// ── graph risk ───────────────────────────────────────────────────────

describe("graph risk (git repo)", () => {
	it("ranks complex.ts as highest risk (high hotspot, low coverage)", async () => {
		const r = await h.run("graph", "risk", REPO_NAME, "--json");
		expect(r.exitCode).toBe(0);
		const json = r.json();
		expect(json.command).toBe("graph risk");
		expect(json.total_files).toBeGreaterThan(0);
		expect(json.formula_version).toBe(1);

		const results = json.results as Array<{
			file: string;
			risk_score: number;
			hotspot_score: number;
			line_coverage: number | null;
		}>;
		expect(results.length).toBeGreaterThanOrEqual(2);

		// complex.ts: high hotspot * (1 - ~0.33) = high risk -> rank 1
		expect(results[0].file).toContain("complex.ts");
		expect(results[0].risk_score).toBeGreaterThan(0.5);
		expect(results[0].hotspot_score).toBe(1.0);
		expect(results[0].line_coverage).toBeCloseTo(0.3333, 2);

		// simple.ts: some hotspot * (1 - 1.0) = 0 risk
		const simple = results.find((f) => f.file.includes("simple.ts"));
		expect(simple).toBeDefined();
		expect(simple!.risk_score).toBe(0);
		expect(simple!.line_coverage).toBe(1.0);
	});

	it("human output shows coverage column and formula note", async () => {
		const r = await h.run("graph", "risk", REPO_NAME);
		expect(r.exitCode).toBe(0);
		expect(r.stdout).toContain("RISK");
		expect(r.stdout).toContain("COVERAGE");
		expect(r.stdout).toContain("files assessed");
		expect(r.stdout).toContain("hotspot_score * (1 - line_coverage)");
	});
});

// ── obligation evaluation with real data ─────────────────────────────

describe("obligations (with assessment data)", () => {
	// Declare requirements and obligations. These are additive
	// declarations, not mutations of pipeline state from other tests.
	beforeAll(async () => {
		// Requirement 1: coverage must be >= 80%
		await h.run(
			"declare",
			"requirement",
			REPO_NAME,
			"src",
			"--req-id",
			"REQ-COV-001",
			"--objective",
			"Source code must have adequate test coverage",
		);
		await h.run(
			"declare",
			"obligation",
			REPO_NAME,
			"REQ-COV-001",
			"--obligation",
			"Average line coverage >= 80%",
			"--method",
			"coverage_threshold",
			"--target",
			"src",
			"--threshold",
			"0.8",
			"--operator",
			">=",
		);

		// Requirement 2: complexity must be <= 20
		await h.run(
			"declare",
			"requirement",
			REPO_NAME,
			"src",
			"--req-id",
			"REQ-CC-001",
			"--objective",
			"No excessively complex functions",
		);
		await h.run(
			"declare",
			"obligation",
			REPO_NAME,
			"REQ-CC-001",
			"--obligation",
			"Max cyclomatic complexity <= 20",
			"--method",
			"complexity_threshold",
			"--target",
			"src",
			"--threshold",
			"20",
			"--operator",
			"<=",
		);

		// Requirement 3: hotspot score must be <= 0.5 (will FAIL — max is 1.0)
		await h.run(
			"declare",
			"requirement",
			REPO_NAME,
			"src",
			"--req-id",
			"REQ-HS-001",
			"--objective",
			"No extreme hotspots",
		);
		await h.run(
			"declare",
			"obligation",
			REPO_NAME,
			"REQ-HS-001",
			"--obligation",
			"Max hotspot score <= 0.5",
			"--method",
			"hotspot_threshold",
			"--target",
			"src",
			"--threshold",
			"0.5",
			"--operator",
			"<=",
		);
	}, 30000);

	it("coverage_threshold FAIL when avg coverage below threshold", async () => {
		const r = await h.run("graph", "obligations", REPO_NAME, "--json");
		expect(r.exitCode).toBe(0);
		const json = r.json();
		const results = json.results as Array<{
			req_id: string;
			method: string;
			effective_verdict: string;
			computed_verdict: string;
			evidence: Record<string, unknown>;
		}>;

		const covObl = results.find(
			(o) => o.req_id === "REQ-COV-001" && o.method === "coverage_threshold",
		);
		expect(covObl).toBeDefined();
		expect(covObl!.effective_verdict).toBe("FAIL");
		// avg of ~0.33 and 1.0 = ~0.67, below 0.8
		expect(covObl!.evidence.avg_coverage).toBeDefined();
		expect(covObl!.evidence.avg_coverage as number).toBeLessThan(0.8);
		expect(covObl!.evidence.files_measured).toBeGreaterThanOrEqual(2);
	});

	it("complexity_threshold PASS when max CC below threshold", async () => {
		const r = await h.run("graph", "obligations", REPO_NAME, "--json");
		expect(r.exitCode).toBe(0);
		const json = r.json();
		const results = json.results as Array<{
			req_id: string;
			method: string;
			effective_verdict: string;
			computed_verdict: string;
			evidence: Record<string, unknown>;
		}>;

		const ccObl = results.find(
			(o) => o.req_id === "REQ-CC-001" && o.method === "complexity_threshold",
		);
		expect(ccObl).toBeDefined();
		expect(ccObl!.effective_verdict).toBe("PASS");
		// max CC should be ~5, well below 20
		expect(ccObl!.evidence.max_complexity).toBeDefined();
		expect(ccObl!.evidence.max_complexity as number).toBeLessThanOrEqual(20);
		expect(ccObl!.evidence.functions_measured).toBeGreaterThan(0);
	});

	it("hotspot_threshold FAIL when max score exceeds threshold", async () => {
		const r = await h.run("graph", "obligations", REPO_NAME, "--json");
		expect(r.exitCode).toBe(0);
		const json = r.json();
		const results = json.results as Array<{
			req_id: string;
			method: string;
			effective_verdict: string;
			computed_verdict: string;
			evidence: Record<string, unknown>;
		}>;

		const hsObl = results.find(
			(o) => o.req_id === "REQ-HS-001" && o.method === "hotspot_threshold",
		);
		expect(hsObl).toBeDefined();
		expect(hsObl!.effective_verdict).toBe("FAIL");
		// max hotspot normalized score is 1.0, exceeds 0.5
		expect(hsObl!.evidence.max_hotspot_score).toBeDefined();
		expect(hsObl!.evidence.max_hotspot_score as number).toBeGreaterThan(0.5);
	});

	it("summary counts reflect PASS and FAIL verdicts", async () => {
		const r = await h.run("graph", "obligations", REPO_NAME, "--json");
		expect(r.exitCode).toBe(0);
		const json = r.json();
		// At least 3 obligations: 1 PASS (complexity), 2 FAIL (coverage, hotspot)
		expect(json.count).toBeGreaterThanOrEqual(3);
		expect(json.pass).toBeGreaterThanOrEqual(1);
		expect(json.fail).toBeGreaterThanOrEqual(2);
	});
});
