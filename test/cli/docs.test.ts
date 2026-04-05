/**
 * rgr docs <repo> <target> — CLI integration tests.
 *
 * Exercises the full command path: indexer populates annotations,
 * then `rgr docs` reads them with target resolution precedence.
 */

import { join } from "node:path";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { createTestHarness, type TestHarness } from "./harness.js";

const FIXTURES_PATH = join(
	import.meta.dirname,
	"../fixtures/typescript/simple-imports",
);
const REPO_NAME = "docs-test-repo";

let h: TestHarness;

beforeAll(async () => {
	h = await createTestHarness();
	await h.run("repo", "add", FIXTURES_PATH, "--name", REPO_NAME);
	await h.run("repo", "index", REPO_NAME);
}, 30000);

afterAll(() => {
	h.cleanup();
});

describe("rgr docs command", () => {
	it("resolves repo target via exact stable_key and returns annotations", async () => {
		// The repo stable_key format is <repo_uid>:.:MODULE
		// We don't know the repo_uid upfront, but we can find it via
		// another command. Use the fixture name instead, which hits
		// step 2 (exact path) or step 3 (module name).
		// For this first assertion, query by the path "." which should
		// match the repo-root module created by the indexer.
		const r = await h.run("docs", REPO_NAME, ".", "--json");
		expect(r.exitCode).toBe(0);
		const json = r.json();
		expect(json.command).toBe("docs");
		expect(json.repo).toBe(REPO_NAME);
		expect(json.resolved_target).toBeDefined();
		expect(json.count).toBeGreaterThan(0);
		// Should have both MODULE_README and PACKAGE_DESCRIPTION
		const annotations = json.annotations as Array<{
			annotation_kind: string;
			content: string;
			source_file: string;
		}>;
		const kinds = annotations.map((a) => a.annotation_kind);
		expect(kinds).toContain("module_readme");
		expect(kinds).toContain("package_description");
	});

	it("returns annotations in deterministic kind order", async () => {
		const r = await h.run("docs", REPO_NAME, ".", "--json");
		const json = r.json();
		const annotations = json.annotations as Array<{
			annotation_kind: string;
		}>;
		// Contract order: package_description → module_readme → file_header_comment → jsdoc_block
		const order = ["package_description", "module_readme"];
		const actualOrder = annotations
			.map((a) => a.annotation_kind)
			.filter((k) => order.includes(k));
		// Each pair of adjacent kinds must respect the order
		for (let i = 0; i < actualOrder.length - 1; i++) {
			expect(order.indexOf(actualOrder[i])).toBeLessThanOrEqual(
				order.indexOf(actualOrder[i + 1]),
			);
		}
	});

	it("returns package_description content from the fixture", async () => {
		const r = await h.run("docs", REPO_NAME, ".", "--json");
		const json = r.json();
		const annotations = json.annotations as Array<{
			annotation_kind: string;
			content: string;
			language: string;
		}>;
		const pkg = annotations.find(
			(a) => a.annotation_kind === "package_description",
		);
		expect(pkg).toBeDefined();
		expect(pkg!.content).toBe("Test fixture for rgr extractor and CLI tests.");
		expect(pkg!.language).toBe("json");
	});

	it("returns module_readme content from the fixture", async () => {
		const r = await h.run("docs", REPO_NAME, ".", "--json");
		const json = r.json();
		const annotations = json.annotations as Array<{
			annotation_kind: string;
			content: string;
			source_file: string;
			language: string;
		}>;
		const readme = annotations.find(
			(a) => a.annotation_kind === "module_readme",
		);
		expect(readme).toBeDefined();
		expect(readme!.content).toContain("simple-imports fixture");
		expect(readme!.source_file).toBe("README.md");
		expect(readme!.language).toBe("markdown");
	});

	it("contract_class is HINT for all annotations in v1", async () => {
		const r = await h.run("docs", REPO_NAME, ".", "--json");
		const json = r.json();
		const annotations = json.annotations as Array<{
			contract_class: string;
			provisional: boolean;
		}>;
		for (const a of annotations) {
			expect(a.contract_class).toBe("HINT");
			expect(a.provisional).toBe(true);
		}
	});

	it("content_hash is present and sha256-prefixed", async () => {
		const r = await h.run("docs", REPO_NAME, ".", "--json");
		const json = r.json();
		const annotations = json.annotations as Array<{ content_hash: string }>;
		for (const a of annotations) {
			expect(a.content_hash.startsWith("sha256:")).toBe(true);
			// Full sha256 is 64 hex chars + "sha256:" prefix = 71 chars
			expect(a.content_hash.length).toBe(71);
		}
	});

	it("returns empty annotations + resolution_error=not_found for unknown target", async () => {
		const r = await h.run(
			"docs",
			REPO_NAME,
			"totally-unknown-target-xyz",
			"--json",
		);
		expect(r.exitCode).toBe(0);
		const json = r.json();
		expect(json.resolved_target).toBeNull();
		expect(json.resolution_error).toBe("not_found");
		expect(json.count).toBe(0);
	});

	it("exits 1 for unknown repo", async () => {
		const r = await h.run("docs", "does-not-exist", ".", "--json");
		expect(r.exitCode).toBe(1);
	});

	it("human output shows annotation sections + provisional warning", async () => {
		const r = await h.run("docs", REPO_NAME, ".");
		expect(r.exitCode).toBe(0);
		expect(r.stdout).toContain("Target query: .");
		expect(r.stdout).toContain("Resolved:");
		expect(r.stdout).toContain("[module_readme]");
		expect(r.stdout).toContain("[package_description]");
		expect(r.stdout).toContain("PROVISIONAL");
	});

	it("resolves via exact stable_key (step 1) when the key is known", async () => {
		// First resolve via path to learn the stable_key
		const r1 = await h.run("docs", REPO_NAME, ".", "--json");
		const resolvedKey = (r1.json() as { resolved_target: string })
			.resolved_target;
		expect(resolvedKey).toBeDefined();
		// Now query with the exact stable_key — step 1 wins
		const r2 = await h.run("docs", REPO_NAME, resolvedKey, "--json");
		expect(r2.exitCode).toBe(0);
		const json = r2.json();
		expect(json.resolved_target).toBe(resolvedKey);
		expect(json.count).toBeGreaterThan(0);
	});
});
