/**
 * Change-impact CLI integration tests.
 *
 * Uses the git-harness (temp git repo with 2 commits) to exercise
 * the full chain:
 *   rgr repo index → rgr change impact <repo> --since HEAD~1
 *
 * The git-harness fixture's second commit modifies:
 *   src/complex.ts
 *   src/simple.ts
 * so those appear as changed files, and their owning module
 * (src) is the seed module for propagation.
 *
 * All tests use a dedicated harness so they are isolated from other
 * suites.
 */

import { writeFileSync } from "node:fs";
import { join } from "node:path";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { createGitTestRepo, type GitTestRepo } from "./git-harness.js";
import { createTestHarness, type TestHarness } from "./harness.js";

const REPO_NAME = "change-impact-repo";

let h: TestHarness;
let gitRepo: GitTestRepo;

beforeAll(async () => {
	h = await createTestHarness();
	gitRepo = await createGitTestRepo();

	await h.run("repo", "add", gitRepo.repoDir, "--name", REPO_NAME);
	await h.run("repo", "index", REPO_NAME);
}, 30000);

afterAll(() => {
	h.cleanup();
	gitRepo.cleanup();
});

describe("change impact --since HEAD~1", () => {
	it("returns changed files and owning module as seed", async () => {
		const r = await h.run(
			"change",
			"impact",
			REPO_NAME,
			"--since",
			"HEAD~1",
			"--json",
		);
		expect(r.exitCode).toBe(0);
		const json = JSON.parse(r.stdout);

		expect(json.command).toBe("change impact");
		expect(json.repo).toBe(REPO_NAME);
		expect(json.scope).toEqual({ kind: "since_ref", ref: "HEAD~1" });

		// Commit 2 modified src/complex.ts and src/simple.ts
		const paths = (
			json.changed_files as Array<{ path: string; matched_to_index: boolean }>
		).map((f) => f.path);
		expect(paths).toContain("src/complex.ts");
		expect(paths).toContain("src/simple.ts");

		// Both are indexed and owned by the src module
		const matched = (
			json.changed_files as Array<{
				path: string;
				matched_to_index: boolean;
				owning_module: string | null;
			}>
		).filter((f) => f.matched_to_index);
		expect(matched.length).toBeGreaterThanOrEqual(2);
		for (const f of matched) {
			expect(f.owning_module).toContain(":src:MODULE");
		}

		// src module is the only seed
		expect(json.seed_modules).toHaveLength(1);
		expect(json.seed_modules[0]).toContain(":src:MODULE");

		// src is at distance 0 as a seed
		const seedRow = (
			json.impacted_modules as Array<{
				module: string;
				distance: number;
				reason: string;
			}>
		).find((m) => m.reason === "seed");
		expect(seedRow).toBeDefined();
		expect(seedRow!.distance).toBe(0);
	});

	it("surfaces trust metadata with standard caveats", async () => {
		const r = await h.run(
			"change",
			"impact",
			REPO_NAME,
			"--since",
			"HEAD~1",
			"--json",
		);
		const json = JSON.parse(r.stdout);
		expect(json.trust).toBeDefined();
		expect(json.trust.graph_basis).toBe("reverse_module_imports_only");
		expect(json.trust.calls_included).toBe(false);
		expect(Array.isArray(json.trust.caveats)).toBe(true);
		expect(json.trust.caveats.length).toBeGreaterThanOrEqual(4);
		const text = (json.trust.caveats as string[]).join(" ").toLowerCase();
		expect(text).toContain("call");
		expect(text).toContain("registry");
	});

	it("counts reflect the changed and matched set", async () => {
		const r = await h.run(
			"change",
			"impact",
			REPO_NAME,
			"--since",
			"HEAD~1",
			"--json",
		);
		const json = JSON.parse(r.stdout);
		expect(json.counts.changed_files).toBeGreaterThanOrEqual(2);
		expect(json.counts.changed_files_matched).toBeGreaterThanOrEqual(2);
		expect(json.counts.seed_modules).toBeGreaterThanOrEqual(1);
		expect(json.counts.impacted_modules).toBeGreaterThanOrEqual(1);
	});
});

describe("change impact --against-snapshot", () => {
	it("picks up working-tree modifications against basis commit", async () => {
		// Append to an existing file in the working tree.
		// The index snapshot's basis_commit is HEAD, so the diff
		// against_snapshot will show this modification.
		const targetPath = join(gitRepo.repoDir, "src", "stable.ts");
		writeFileSync(targetPath, "// touched by test\n", { flag: "a" });

		const r = await h.run(
			"change",
			"impact",
			REPO_NAME,
			"--against-snapshot",
			"--json",
		);
		expect(r.exitCode).toBe(0);
		const json = JSON.parse(r.stdout);

		expect(json.scope.kind).toBe("against_snapshot");
		expect(json.scope.basis_commit).toBeDefined();

		const paths = (
			json.changed_files as Array<{ path: string }>
		).map((f) => f.path);
		expect(paths).toContain("src/stable.ts");
	});
});

describe("change impact — error paths", () => {
	it("rejects mutually exclusive scope flags", async () => {
		const r = await h.run(
			"change",
			"impact",
			REPO_NAME,
			"--against-snapshot",
			"--staged",
			"--json",
		);
		expect(r.exitCode).toBe(1);
	});

	it("rejects invalid max-depth", async () => {
		const r = await h.run(
			"change",
			"impact",
			REPO_NAME,
			"--since",
			"HEAD~1",
			"--max-depth",
			"0",
			"--json",
		);
		expect(r.exitCode).toBe(1);
	});

	it("returns error for non-existent repo", async () => {
		const r = await h.run("change", "impact", "does-not-exist", "--json");
		expect(r.exitCode).toBe(1);
	});
});

describe("change impact --max-depth", () => {
	it("caps traversal depth", async () => {
		const r = await h.run(
			"change",
			"impact",
			REPO_NAME,
			"--since",
			"HEAD~1",
			"--max-depth",
			"1",
			"--json",
		);
		expect(r.exitCode).toBe(0);
		const json = JSON.parse(r.stdout);
		const maxDist = (
			json.impacted_modules as Array<{ distance: number }>
		).reduce((max, m) => (m.distance > max ? m.distance : max), 0);
		expect(maxDist).toBeLessThanOrEqual(1);
	});
});

describe("change impact human output", () => {
	it("shows changed files, impacted modules, and trust section", async () => {
		const r = await h.run("change", "impact", REPO_NAME, "--since", "HEAD~1");
		expect(r.exitCode).toBe(0);
		expect(r.stdout).toContain("Change Impact");
		expect(r.stdout).toContain("Changed files:");
		expect(r.stdout).toContain("Impacted modules");
		expect(r.stdout).toContain("Trust:");
		expect(r.stdout).toContain("reverse_module_imports_only");
	});
});
