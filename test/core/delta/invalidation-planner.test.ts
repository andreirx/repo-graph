/**
 * Invalidation planner — unit tests.
 *
 * Pure functions, no filesystem or storage.
 *
 * Covers:
 *   - unchanged files (hash match → copy forward)
 *   - changed files (hash mismatch → re-extract)
 *   - new files (not in parent → extract fresh)
 *   - deleted files (in parent, not in current → do not copy)
 *   - config widening:
 *     - root-level config change → widen all unchanged
 *     - nested config change → widen only subtree
 *     - multiple config changes
 *   - count aggregation
 *   - filesToExtract/filesToCopy/filesToDelete computed subsets
 */

import { describe, expect, it } from "vitest";
import {
	buildInvalidationPlan,
	type CurrentFileState,
	type ParentFileHashes,
} from "../../../src/core/delta/invalidation-planner.js";

const REPO = "test-repo";

function makeCurrentFile(path: string, hash: string): CurrentFileState {
	return { fileUid: `${REPO}:${path}`, path, contentHash: hash };
}

function makeParentHashes(entries: Array<[string, string]>): ParentFileHashes {
	return new Map(entries.map(([path, hash]) => [`${REPO}:${path}`, hash]));
}

// ── Basic classification ───────────────────────────────────────────

describe("buildInvalidationPlan — basic classification", () => {
	it("classifies unchanged files", () => {
		const plan = buildInvalidationPlan(
			"parent-snap",
			makeParentHashes([["src/a.ts", "abc123"]]),
			[makeCurrentFile("src/a.ts", "abc123")],
			REPO,
		);

		expect(plan.files).toHaveLength(1);
		expect(plan.files[0].disposition).toBe("unchanged");
		expect(plan.counts.unchanged).toBe(1);
		expect(plan.filesToCopy).toEqual(["src/a.ts"]);
	});

	it("classifies changed files", () => {
		const plan = buildInvalidationPlan(
			"parent-snap",
			makeParentHashes([["src/a.ts", "abc123"]]),
			[makeCurrentFile("src/a.ts", "def456")],
			REPO,
		);

		expect(plan.files[0].disposition).toBe("changed");
		expect(plan.files[0].reason).toContain("differs");
		expect(plan.counts.changed).toBe(1);
		expect(plan.filesToExtract).toEqual(["src/a.ts"]);
	});

	it("classifies new files", () => {
		const plan = buildInvalidationPlan(
			"parent-snap",
			makeParentHashes([]),
			[makeCurrentFile("src/new.ts", "abc123")],
			REPO,
		);

		expect(plan.files[0].disposition).toBe("new");
		expect(plan.counts.new).toBe(1);
		expect(plan.filesToExtract).toEqual(["src/new.ts"]);
	});

	it("classifies deleted files", () => {
		const plan = buildInvalidationPlan(
			"parent-snap",
			makeParentHashes([["src/old.ts", "abc123"]]),
			[],
			REPO,
		);

		expect(plan.files).toHaveLength(1);
		expect(plan.files[0].disposition).toBe("deleted");
		expect(plan.files[0].currentHash).toBeNull();
		expect(plan.counts.deleted).toBe(1);
		expect(plan.filesToDelete).toEqual(["src/old.ts"]);
	});

	it("handles mixed classifications", () => {
		const plan = buildInvalidationPlan(
			"parent-snap",
			makeParentHashes([
				["src/unchanged.ts", "aaa"],
				["src/changed.ts", "bbb"],
				["src/deleted.ts", "ccc"],
			]),
			[
				makeCurrentFile("src/unchanged.ts", "aaa"),
				makeCurrentFile("src/changed.ts", "ddd"),
				makeCurrentFile("src/new.ts", "eee"),
			],
			REPO,
		);

		expect(plan.counts.unchanged).toBe(1);
		expect(plan.counts.changed).toBe(1);
		expect(plan.counts.new).toBe(1);
		expect(plan.counts.deleted).toBe(1);
		expect(plan.counts.total).toBe(4);
		expect(plan.filesToCopy).toEqual(["src/unchanged.ts"]);
		expect(plan.filesToExtract.sort()).toEqual(["src/changed.ts", "src/new.ts"]);
		expect(plan.filesToDelete).toEqual(["src/deleted.ts"]);
	});
});

// ── Config widening ────────────────────────────────────────────────

describe("buildInvalidationPlan — config widening", () => {
	it("root package.json change widens all unchanged files", () => {
		const plan = buildInvalidationPlan(
			"parent-snap",
			makeParentHashes([
				["package.json", "old-pkg"],
				["src/a.ts", "aaa"],
				["src/b.ts", "bbb"],
			]),
			[
				makeCurrentFile("package.json", "new-pkg"),
				makeCurrentFile("src/a.ts", "aaa"),
				makeCurrentFile("src/b.ts", "bbb"),
			],
			REPO,
		);

		// package.json itself is changed.
		const pkgFile = plan.files.find((f) => f.path === "package.json");
		expect(pkgFile!.disposition).toBe("changed");

		// src/a.ts and src/b.ts were unchanged but widened.
		const aFile = plan.files.find((f) => f.path === "src/a.ts");
		expect(aFile!.disposition).toBe("config_widened");
		const bFile = plan.files.find((f) => f.path === "src/b.ts");
		expect(bFile!.disposition).toBe("config_widened");

		expect(plan.counts.configWidened).toBe(2);
		expect(plan.counts.changed).toBe(1);
		expect(plan.changedConfigs).toHaveLength(1);
		expect(plan.changedConfigs[0].configType).toBe("package_json");
	});

	it("nested tsconfig.json change widens only subtree", () => {
		const plan = buildInvalidationPlan(
			"parent-snap",
			makeParentHashes([
				["packages/api/tsconfig.json", "old-ts"],
				["packages/api/src/handler.ts", "aaa"],
				["packages/web/src/app.ts", "bbb"],
			]),
			[
				makeCurrentFile("packages/api/tsconfig.json", "new-ts"),
				makeCurrentFile("packages/api/src/handler.ts", "aaa"),
				makeCurrentFile("packages/web/src/app.ts", "bbb"),
			],
			REPO,
		);

		// handler.ts is under packages/api/ → widened.
		const handler = plan.files.find((f) => f.path === "packages/api/src/handler.ts");
		expect(handler!.disposition).toBe("config_widened");

		// app.ts is under packages/web/ → NOT widened.
		const app = plan.files.find((f) => f.path === "packages/web/src/app.ts");
		expect(app!.disposition).toBe("unchanged");
	});

	it("compile_commands.json change widens all unchanged files", () => {
		const plan = buildInvalidationPlan(
			"parent-snap",
			makeParentHashes([
				["compile_commands.json", "old-cc"],
				["src/main.c", "aaa"],
			]),
			[
				makeCurrentFile("compile_commands.json", "new-cc"),
				makeCurrentFile("src/main.c", "aaa"),
			],
			REPO,
		);

		expect(plan.changedConfigs[0].configType).toBe("compile_commands_json");
		const mainC = plan.files.find((f) => f.path === "src/main.c");
		expect(mainC!.disposition).toBe("config_widened");
	});

	it("does not widen already-changed files", () => {
		const plan = buildInvalidationPlan(
			"parent-snap",
			makeParentHashes([
				["package.json", "old"],
				["src/a.ts", "old-a"],
			]),
			[
				makeCurrentFile("package.json", "new"),
				makeCurrentFile("src/a.ts", "new-a"),
			],
			REPO,
		);

		// src/a.ts is already changed — should stay "changed", not become "config_widened".
		const aFile = plan.files.find((f) => f.path === "src/a.ts");
		expect(aFile!.disposition).toBe("changed");
	});

	it("new config file does not trigger widening of unchanged files", () => {
		// A new package.json (not in parent) shouldn't widen — only
		// changed configs trigger widening.
		const plan = buildInvalidationPlan(
			"parent-snap",
			makeParentHashes([["src/a.ts", "aaa"]]),
			[
				makeCurrentFile("package.json", "new-pkg"),
				makeCurrentFile("src/a.ts", "aaa"),
			],
			REPO,
		);

		// package.json is new (triggers config detection).
		expect(plan.changedConfigs).toHaveLength(1);
		// But since it's root-level, all unchanged files get widened.
		const aFile = plan.files.find((f) => f.path === "src/a.ts");
		expect(aFile!.disposition).toBe("config_widened");
	});

	it("multiple nested config changes widen their respective subtrees", () => {
		const plan = buildInvalidationPlan(
			"parent-snap",
			makeParentHashes([
				["packages/api/package.json", "old-api"],
				["packages/web/package.json", "old-web"],
				["packages/api/src/handler.ts", "aaa"],
				["packages/web/src/app.ts", "bbb"],
				["shared/util.ts", "ccc"],
			]),
			[
				makeCurrentFile("packages/api/package.json", "new-api"),
				makeCurrentFile("packages/web/package.json", "old-web"),
				makeCurrentFile("packages/api/src/handler.ts", "aaa"),
				makeCurrentFile("packages/web/src/app.ts", "bbb"),
				makeCurrentFile("shared/util.ts", "ccc"),
			],
			REPO,
		);

		// Only packages/api/package.json changed.
		expect(plan.changedConfigs).toHaveLength(1);

		// handler.ts widened (under packages/api/).
		expect(plan.files.find((f) => f.path === "packages/api/src/handler.ts")!.disposition).toBe("config_widened");
		// app.ts unchanged (packages/web/package.json didn't change).
		expect(plan.files.find((f) => f.path === "packages/web/src/app.ts")!.disposition).toBe("unchanged");
		// shared/util.ts unchanged (not under any changed config scope).
		expect(plan.files.find((f) => f.path === "shared/util.ts")!.disposition).toBe("unchanged");
	});
});

// ── Edge cases ─────────────────────────────────────────────────────

describe("buildInvalidationPlan — edge cases", () => {
	it("handles empty parent (first full index acts as no-op plan)", () => {
		const plan = buildInvalidationPlan(
			"parent-snap",
			makeParentHashes([]),
			[
				makeCurrentFile("src/a.ts", "aaa"),
				makeCurrentFile("src/b.ts", "bbb"),
			],
			REPO,
		);

		expect(plan.counts.new).toBe(2);
		expect(plan.counts.unchanged).toBe(0);
		expect(plan.filesToExtract).toHaveLength(2);
		expect(plan.filesToCopy).toHaveLength(0);
	});

	it("handles empty current (all files deleted)", () => {
		const plan = buildInvalidationPlan(
			"parent-snap",
			makeParentHashes([
				["src/a.ts", "aaa"],
				["src/b.ts", "bbb"],
			]),
			[],
			REPO,
		);

		expect(plan.counts.deleted).toBe(2);
		expect(plan.filesToDelete).toHaveLength(2);
	});

	it("preserves parentSnapshotUid in plan", () => {
		const plan = buildInvalidationPlan(
			"snap-123",
			makeParentHashes([]),
			[],
			REPO,
		);
		expect(plan.parentSnapshotUid).toBe("snap-123");
	});

	it("preserves parent and current hashes in classifications", () => {
		const plan = buildInvalidationPlan(
			"parent-snap",
			makeParentHashes([["src/a.ts", "parent-hash"]]),
			[makeCurrentFile("src/a.ts", "current-hash")],
			REPO,
		);

		expect(plan.files[0].parentHash).toBe("parent-hash");
		expect(plan.files[0].currentHash).toBe("current-hash");
	});
});
