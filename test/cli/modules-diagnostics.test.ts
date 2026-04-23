/**
 * CLI integration tests for `rgr modules diagnostics` command.
 *
 * Tests that module discovery diagnostics are:
 *   - Persisted during indexing
 *   - Queryable via CLI
 *   - Filterable by source type and diagnostic kind
 *
 * Uses the kbuild-sample fixture which contains Kbuild files
 * with conditional directives that generate diagnostics.
 */

import { writeFileSync, mkdirSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { randomUUID } from "node:crypto";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { createTestHarness, type TestHarness } from "./harness.js";

// Create a temporary fixture with Kbuild content that generates diagnostics
const FIXTURE_PATH = join(tmpdir(), `kbuild-diag-test-${randomUUID()}`);

function createKbuildDiagnosticFixture(): void {
	mkdirSync(FIXTURE_PATH, { recursive: true });
	mkdirSync(join(FIXTURE_PATH, "drivers"), { recursive: true });
	mkdirSync(join(FIXTURE_PATH, "drivers/net"), { recursive: true });

	// Makefile with obj-y (unconditional) and obj-$(CONFIG_...) (conditional)
	writeFileSync(
		join(FIXTURE_PATH, "drivers/Makefile"),
		`# Driver Makefile
obj-y += net/
obj-$(CONFIG_USB) += usb/
ifeq ($(CONFIG_NET),y)
obj-y += netfilter/
endif
`,
	);

	// Some C files to make the directory valid
	writeFileSync(
		join(FIXTURE_PATH, "drivers/net/driver.c"),
		"int main() { return 0; }",
	);
}

let h: TestHarness;

beforeAll(async () => {
	createKbuildDiagnosticFixture();
	h = await createTestHarness();
	await h.run("repo", "add", FIXTURE_PATH, "--name", "kbuild-diag-test");
	await h.run("repo", "index", "kbuild-diag-test");
}, 30000);

afterAll(() => {
	h.cleanup();
	rmSync(FIXTURE_PATH, { recursive: true, force: true });
});

// ── Basic diagnostics query ────────────────────────────────────────

describe("modules diagnostics — basic queries", () => {
	it("returns diagnostics in JSON format", async () => {
		const r = await h.run("modules", "diagnostics", "kbuild-diag-test", "--json");
		expect(r.exitCode).toBe(0);

		const output = r.json() as {
			repo: string;
			count: number;
			diagnostics: Array<Record<string, unknown>>;
		};

		expect(output.repo).toBe("kbuild-diag-test");
		expect(output.count).toBeGreaterThan(0);
		expect(Array.isArray(output.diagnostics)).toBe(true);
	});

	it("includes all diagnostic fields", async () => {
		const r = await h.run("modules", "diagnostics", "kbuild-diag-test", "--json");
		expect(r.exitCode).toBe(0);

		const output = r.json() as {
			diagnostics: Array<Record<string, unknown>>;
		};

		// Check the first diagnostic has all required fields
		const first = output.diagnostics[0];
		expect(first).toHaveProperty("uid");
		expect(first).toHaveProperty("source_type");
		expect(first).toHaveProperty("kind");
		expect(first).toHaveProperty("file");
		expect(first).toHaveProperty("message");
		expect(first).toHaveProperty("severity");
	});

	it("includes skipped_config_gated diagnostics", async () => {
		const r = await h.run("modules", "diagnostics", "kbuild-diag-test", "--json");
		expect(r.exitCode).toBe(0);

		const output = r.json() as {
			diagnostics: Array<{ kind: string }>;
		};

		// The obj-$(CONFIG_USB) line should generate skipped_config_gated
		const configGated = output.diagnostics.filter(
			(d) => d.kind === "skipped_config_gated",
		);
		expect(configGated.length).toBeGreaterThan(0);
	});

	it("includes conditional block diagnostics", async () => {
		const r = await h.run("modules", "diagnostics", "kbuild-diag-test", "--json");
		expect(r.exitCode).toBe(0);

		const output = r.json() as {
			diagnostics: Array<{ kind: string }>;
		};

		// The ifeq block should generate skipped_conditional
		const conditionalDiags = output.diagnostics.filter(
			(d) =>
				d.kind === "skipped_conditional" ||
				d.kind === "skipped_inside_conditional",
		);
		expect(conditionalDiags.length).toBeGreaterThan(0);
	});
});

// ── Filter functionality ───────────────────────────────────────────

describe("modules diagnostics — filters", () => {
	it("filters by source type", async () => {
		const r = await h.run(
			"modules",
			"diagnostics",
			"kbuild-diag-test",
			"--source",
			"kbuild",
			"--json",
		);
		expect(r.exitCode).toBe(0);

		const output = r.json() as {
			filters: { sourceType: string };
			diagnostics: Array<{ source_type: string }>;
		};

		expect(output.filters.sourceType).toBe("kbuild");
		for (const d of output.diagnostics) {
			expect(d.source_type).toBe("kbuild");
		}
	});

	it("filters by diagnostic kind", async () => {
		const r = await h.run(
			"modules",
			"diagnostics",
			"kbuild-diag-test",
			"--kind",
			"skipped_config_gated",
			"--json",
		);
		expect(r.exitCode).toBe(0);

		const output = r.json() as {
			filters: { diagnosticKind: string };
			diagnostics: Array<{ kind: string }>;
		};

		expect(output.filters.diagnosticKind).toBe("skipped_config_gated");
		for (const d of output.diagnostics) {
			expect(d.kind).toBe("skipped_config_gated");
		}
	});

	it("returns empty array when no matches", async () => {
		const r = await h.run(
			"modules",
			"diagnostics",
			"kbuild-diag-test",
			"--kind",
			"malformed_assignment",
			"--json",
		);
		expect(r.exitCode).toBe(0);

		const output = r.json() as {
			count: number;
			diagnostics: Array<unknown>;
		};

		// No malformed assignments in our fixture
		expect(output.count).toBe(0);
		expect(output.diagnostics).toEqual([]);
	});
});

// ── Table output ───────────────────────────────────────────────────

describe("modules diagnostics — table output", () => {
	it("shows header and diagnostic rows", async () => {
		const r = await h.run("modules", "diagnostics", "kbuild-diag-test");
		expect(r.exitCode).toBe(0);

		const output = r.stdout;
		expect(output).toContain("SOURCE");
		expect(output).toContain("FILE");
		expect(output).toContain("LINE");
		expect(output).toContain("KIND");
		expect(output).toContain("SEVERITY");
		expect(output).toContain("MESSAGE");
		expect(output).toContain("Total:");
	});

	it("shows source type in table rows", async () => {
		const r = await h.run("modules", "diagnostics", "kbuild-diag-test");
		expect(r.exitCode).toBe(0);

		// kbuild source should appear in the table
		expect(r.stdout).toContain("kbuild");
	});

	it("shows file path in diagnostics", async () => {
		const r = await h.run("modules", "diagnostics", "kbuild-diag-test");
		expect(r.exitCode).toBe(0);

		expect(r.stdout).toContain("drivers/Makefile");
	});
});

// ── Error handling ─────────────────────────────────────────────────

describe("modules diagnostics — errors", () => {
	it("fails for unknown repository", async () => {
		const r = await h.run("modules", "diagnostics", "nonexistent-repo", "--json");
		expect(r.exitCode).toBe(1);

		const output = r.json() as { error: string };
		expect(output.error).toContain("not found");
	});
});
