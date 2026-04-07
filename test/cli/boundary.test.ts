/**
 * Boundary command CLI integration tests.
 *
 * Exercises `rgr boundary summary|providers|consumers|links|unmatched`
 * against the boundary-test fixture, verifying JSON envelope shape,
 * fact counts, link materialization, and unmatched reporting.
 *
 * Fixture:
 *   - ProductController.java: 4 Spring routes (GET /{id}, POST, GET list, DELETE /{id})
 *   - client.ts: 3 axios calls (GET /products/{param}, POST /products, GET /orders)
 *   - Expected links: 3 (GET {_}, POST, DELETE has no consumer; GET /orders has no provider)
 */

import { join } from "node:path";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { createTestHarness, type TestHarness } from "./harness.js";

const FIXTURES_PATH = join(
	import.meta.dirname,
	"../fixtures/boundary-test",
);
const REPO_NAME = "boundary-cli-test";

let h: TestHarness;

beforeAll(async () => {
	h = await createTestHarness();
	await h.run("repo", "add", FIXTURES_PATH, "--name", REPO_NAME);
	const indexResult = await h.run("repo", "index", REPO_NAME);
	if (indexResult.exitCode !== 0) {
		throw new Error(`Index failed: ${indexResult.stderr}`);
	}
}, 30000);

afterAll(() => {
	h.cleanup();
});

// ── boundary summary ────────────────────────────────────────────────

describe("boundary summary", () => {
	it("emits correct counts in JSON mode", async () => {
		const r = await h.run("boundary", "summary", REPO_NAME, "--json");
		expect(r.exitCode).toBe(0);
		const json = r.json();

		expect(json.command).toBe("boundary summary");
		expect(json.repo).toBe(REPO_NAME);
		expect(json.snapshot_uid).toBeDefined();
		expect(json.providers).toBe(4);
		expect(json.consumers).toBe(3);
		// Links: GET {_} matches, POST matches, GET list has no consumer match
		// (consumer has GET /api/v2/products which matches provider GET /api/v2/products)
		// So: GET /{id} ↔ GET /{param}, POST ↔ POST, GET list ↔ (no consumer for bare list)
		// Actually: provider GET /api/v2/products matches consumer... let me think.
		// Provider GET /api/v2/products/{id} → key GET /api/v2/products/{_}
		// Provider POST /api/v2/products → key POST /api/v2/products
		// Provider GET /api/v2/products → key GET /api/v2/products
		// Provider DELETE /api/v2/products/{id} → key DELETE /api/v2/products/{_}
		// Consumer GET /api/v2/products/{param} → key GET /api/v2/products/{_} ✓ matches provider GET /{id}
		// Consumer POST /api/v2/products → key POST /api/v2/products ✓ matches provider POST
		// Consumer GET /api/v2/orders → key GET /api/v2/orders ✗ no provider
		// Links: 2 (GET {_} and POST). GET list provider has no matching consumer.
		expect(json.links).toBe(2);
	});

	it("emits human-readable output without --json", async () => {
		const r = await h.run("boundary", "summary", REPO_NAME);
		expect(r.exitCode).toBe(0);
		expect(r.stdout).toContain("Providers: 4");
		expect(r.stdout).toContain("Consumers: 3");
		expect(r.stdout).toContain("Links:");
	});

	it("exits 1 for unknown repo", async () => {
		const r = await h.run("boundary", "summary", "nonexistent", "--json");
		expect(r.exitCode).toBe(1);
		const json = r.json();
		expect(json.error).toBeDefined();
	});
});

// ── boundary providers ──────────────────────────────────────────────

describe("boundary providers", () => {
	it("lists all provider facts in JSON mode", async () => {
		const r = await h.run("boundary", "providers", REPO_NAME, "--json");
		expect(r.exitCode).toBe(0);
		const json = r.json();
		expect(json.command).toBe("boundary providers");
		expect(json.count).toBe(4);
		const results = json.results as Array<Record<string, unknown>>;
		expect(results.length).toBe(4);

		// Every result has the expected shape.
		for (const p of results) {
			expect(p.mechanism).toBe("http");
			expect(typeof p.operation).toBe("string");
			expect(typeof p.address).toBe("string");
			expect(typeof p.matcher_key).toBe("string");
			expect(typeof p.source_file).toBe("string");
			expect(typeof p.line).toBe("number");
			expect(p.framework).toBe("spring-mvc");
			expect(p.basis).toBe("annotation");
		}
	});

	it("filters by --mechanism", async () => {
		const r = await h.run("boundary", "providers", REPO_NAME, "--json", "--mechanism", "grpc");
		expect(r.exitCode).toBe(0);
		const json = r.json();
		expect(json.count).toBe(0);
	});

	it("applies --limit", async () => {
		const r = await h.run("boundary", "providers", REPO_NAME, "--json", "--limit", "2");
		expect(r.exitCode).toBe(0);
		const json = r.json();
		expect((json.results as unknown[]).length).toBe(2);
	});

	it("emits human-readable table without --json", async () => {
		const r = await h.run("boundary", "providers", REPO_NAME);
		expect(r.exitCode).toBe(0);
		expect(r.stdout).toContain("provider fact(s)");
		expect(r.stdout).toContain("spring-mvc");
	});
});

// ── boundary consumers ──────────────────────────────────────────────

describe("boundary consumers", () => {
	it("lists all consumer facts in JSON mode", async () => {
		const r = await h.run("boundary", "consumers", REPO_NAME, "--json");
		expect(r.exitCode).toBe(0);
		const json = r.json();
		expect(json.command).toBe("boundary consumers");
		expect(json.count).toBe(3);
		const results = json.results as Array<Record<string, unknown>>;

		for (const c of results) {
			expect(c.mechanism).toBe("http");
			expect(typeof c.operation).toBe("string");
			expect(typeof c.address).toBe("string");
			expect(typeof c.matcher_key).toBe("string");
			expect(typeof c.source_file).toBe("string");
			expect(typeof c.line).toBe("number");
			expect(typeof c.confidence).toBe("number");
		}
	});

	it("emits human-readable table without --json", async () => {
		const r = await h.run("boundary", "consumers", REPO_NAME);
		expect(r.exitCode).toBe(0);
		expect(r.stdout).toContain("consumer fact(s)");
	});
});

// ── boundary links ──────────────────────────────────────────────────

describe("boundary links", () => {
	it("lists matched links in JSON mode", async () => {
		const r = await h.run("boundary", "links", REPO_NAME, "--json");
		expect(r.exitCode).toBe(0);
		const json = r.json();
		expect(json.command).toBe("boundary links");
		expect(json.count).toBe(2);
		const results = json.results as Array<Record<string, unknown>>;

		for (const l of results) {
			expect(typeof l.provider_operation).toBe("string");
			expect(typeof l.provider_file).toBe("string");
			expect(typeof l.consumer_operation).toBe("string");
			expect(typeof l.consumer_file).toBe("string");
			expect(l.match_basis).toBe("address_match");
			expect(typeof l.confidence).toBe("number");
			expect(l.confidence).toBeGreaterThan(0);
			expect(l.confidence).toBeLessThanOrEqual(1);
		}
	});

	it("emits human-readable table without --json", async () => {
		const r = await h.run("boundary", "links", REPO_NAME);
		expect(r.exitCode).toBe(0);
		expect(r.stdout).toContain("link(s)");
		expect(r.stdout).toContain("address_match");
	});
});

// ── boundary unmatched ──────────────────────────────────────────────

describe("boundary unmatched", () => {
	it("lists unmatched providers and consumers in JSON mode", async () => {
		const r = await h.run("boundary", "unmatched", REPO_NAME, "--json");
		expect(r.exitCode).toBe(0);
		const json = r.json();
		expect(json.command).toBe("boundary unmatched");

		const up = json.unmatched_providers as { count: number; results: Array<Record<string, unknown>> };
		const uc = json.unmatched_consumers as { count: number; results: Array<Record<string, unknown>> };

		// 4 providers - 2 matched = 2 unmatched (GET list, DELETE /{id})
		expect(up.count).toBe(2);
		// 3 consumers - 2 matched = 1 unmatched (GET /orders)
		expect(uc.count).toBe(1);

		// The unmatched consumer should be the /orders call.
		expect(uc.results[0].operation).toContain("orders");
	});

	it("filters by --side providers", async () => {
		const r = await h.run("boundary", "unmatched", REPO_NAME, "--json", "--side", "providers");
		expect(r.exitCode).toBe(0);
		const json = r.json();
		expect(json.unmatched_providers).toBeDefined();
		expect(json.unmatched_consumers).toBeUndefined();
	});

	it("filters by --side consumers", async () => {
		const r = await h.run("boundary", "unmatched", REPO_NAME, "--json", "--side", "consumers");
		expect(r.exitCode).toBe(0);
		const json = r.json();
		expect(json.unmatched_consumers).toBeDefined();
		expect(json.unmatched_providers).toBeUndefined();
	});

	it("emits human-readable output without --json", async () => {
		const r = await h.run("boundary", "unmatched", REPO_NAME);
		expect(r.exitCode).toBe(0);
		expect(r.stdout).toContain("Unmatched providers");
		expect(r.stdout).toContain("Unmatched consumers");
	});
});
