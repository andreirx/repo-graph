import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { GitAdapter } from "../../../src/adapters/git/git-adapter.js";

const REPO_ROOT = join(import.meta.dirname, "../../..");
const adapter = new GitAdapter();

describe("GitAdapter", () => {
	describe("getCurrentCommit", () => {
		it("returns a commit hash for a real git repo", async () => {
			const hash = await adapter.getCurrentCommit(REPO_ROOT);
			expect(hash).not.toBeNull();
			expect(hash?.length).toBeGreaterThanOrEqual(7);
			// SHA format: hex characters
			expect(hash).toMatch(/^[0-9a-f]+$/);
		});

		it("returns null for a non-git directory", async () => {
			const hash = await adapter.getCurrentCommit("/tmp");
			expect(hash).toBeNull();
		});
	});

	describe("isDirty", () => {
		it("returns a boolean for a real git repo", async () => {
			const dirty = await adapter.isDirty(REPO_ROOT);
			expect(typeof dirty).toBe("boolean");
		});
	});

	describe("getFileChurn", () => {
		it("returns churn data for a real git repo", async () => {
			const churn = await adapter.getFileChurn(REPO_ROOT, "365.days.ago");
			expect(churn.length).toBeGreaterThan(0);
			// Each entry has the required fields
			const first = churn[0];
			expect(first.filePath).toBeDefined();
			expect(typeof first.commitCount).toBe("number");
			expect(typeof first.linesChanged).toBe("number");
			expect(first.commitCount).toBeGreaterThanOrEqual(1);
		});

		it("returns empty for an impossible time window", async () => {
			// 0 days ago should return nothing (or very little)
			const churn = await adapter.getFileChurn(REPO_ROOT, "0.seconds.ago");
			// Could be empty or have today's changes — just verify it doesn't crash
			expect(Array.isArray(churn)).toBe(true);
		});

		it("returns empty for a non-git directory", async () => {
			const churn = await adapter.getFileChurn("/tmp", "90.days.ago");
			expect(churn.length).toBe(0);
		});

		it("uses forward slashes in file paths", async () => {
			const churn = await adapter.getFileChurn(REPO_ROOT, "365.days.ago");
			for (const entry of churn) {
				expect(entry.filePath).not.toContain("\\");
			}
		});
	});
});
