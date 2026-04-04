import { randomUUID } from "node:crypto";
import { mkdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterAll, describe, expect, it } from "vitest";
import { IstanbulCoverageImporter } from "../../../src/adapters/importers/istanbul-coverage.js";

const importer = new IstanbulCoverageImporter();
const importCoverageReport = (path: string, root: string) =>
	importer.importReport(path, root);

// Create a synthetic Istanbul coverage-final.json fixture
const FIXTURE_DIR = join(tmpdir(), `rgr-coverage-test-${randomUUID()}`);
const REPO_ROOT = join(FIXTURE_DIR, "repo");
const REPORT_PATH = join(FIXTURE_DIR, "coverage-final.json");

mkdirSync(REPO_ROOT, { recursive: true });

const istanbulReport = {
	[join(REPO_ROOT, "src/a.ts")]: {
		// 5 statements, 3 covered
		s: { "0": 1, "1": 1, "2": 0, "3": 1, "4": 0 },
		// 2 functions, 1 covered
		f: { "0": 1, "1": 0 },
		// 2 branches (one if with 2 paths), first taken, second not
		b: { "0": [1, 0] },
		statementMap: {},
		fnMap: {},
		branchMap: {},
	},
	[join(REPO_ROOT, "src/b.ts")]: {
		// 3 statements, all covered
		s: { "0": 5, "1": 3, "2": 1 },
		// 1 function, covered
		f: { "0": 2 },
		// no branches
		b: {},
		statementMap: {},
		fnMap: {},
		branchMap: {},
	},
	// File outside repo root — should be skipped
	["/other/project/c.ts"]: {
		s: { "0": 1 },
		f: { "0": 1 },
		b: {},
		statementMap: {},
		fnMap: {},
		branchMap: {},
	},
};

writeFileSync(REPORT_PATH, JSON.stringify(istanbulReport));

afterAll(() => {
	try {
		const { rmSync } = require("node:fs");
		rmSync(FIXTURE_DIR, { recursive: true });
	} catch {
		// cleanup best effort
	}
});

describe("importCoverageReport", () => {
	it("parses Istanbul JSON and returns per-file coverage", async () => {
		const result = await importCoverageReport(REPORT_PATH, REPO_ROOT);
		expect(result.files.length).toBe(2);
		expect(result.reportPath).toContain("coverage-final.json");
	});

	it("computes correct line coverage ratios", async () => {
		const result = await importCoverageReport(REPORT_PATH, REPO_ROOT);
		const a = result.files.find((f) => f.filePath === "src/a.ts");
		expect(a).toBeDefined();
		// 3 of 5 statements covered
		expect(a?.lineCoverage).toBeCloseTo(0.6, 2);
		expect(a?.totalLines).toBe(5);
	});

	it("computes correct function coverage ratios", async () => {
		const result = await importCoverageReport(REPORT_PATH, REPO_ROOT);
		const a = result.files.find((f) => f.filePath === "src/a.ts");
		// 1 of 2 functions covered
		expect(a?.functionCoverage).toBeCloseTo(0.5, 2);
		expect(a?.totalFunctions).toBe(2);
	});

	it("computes correct branch coverage ratios", async () => {
		const result = await importCoverageReport(REPORT_PATH, REPO_ROOT);
		const a = result.files.find((f) => f.filePath === "src/a.ts");
		// 1 of 2 branch paths taken
		expect(a?.branchCoverage).toBeCloseTo(0.5, 2);
		expect(a?.totalBranches).toBe(2);
	});

	it("handles files with 100% coverage", async () => {
		const result = await importCoverageReport(REPORT_PATH, REPO_ROOT);
		const b = result.files.find((f) => f.filePath === "src/b.ts");
		expect(b?.lineCoverage).toBe(1.0);
		expect(b?.functionCoverage).toBe(1.0);
	});

	it("returns null branch coverage when no branches exist", async () => {
		const result = await importCoverageReport(REPORT_PATH, REPO_ROOT);
		const b = result.files.find((f) => f.filePath === "src/b.ts");
		expect(b?.branchCoverage).toBeNull();
		expect(b?.totalBranches).toBe(0);
	});

	it("excludes files outside the repo root", async () => {
		const result = await importCoverageReport(REPORT_PATH, REPO_ROOT);
		const external = result.files.find((f) => f.filePath.includes("other"));
		expect(external).toBeUndefined();
	});

	it("uses forward slashes in file paths", async () => {
		const result = await importCoverageReport(REPORT_PATH, REPO_ROOT);
		for (const f of result.files) {
			expect(f.filePath).not.toContain("\\");
		}
	});
});
