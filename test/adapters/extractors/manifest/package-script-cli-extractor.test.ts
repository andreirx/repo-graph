/**
 * package.json script CLI consumer extractor — unit tests.
 *
 * Tests extraction of direct tool invocations from package.json scripts
 * as cli_command boundary consumer facts.
 */

import { describe, expect, it } from "vitest";
import { extractPackageScriptConsumers } from "../../../../src/adapters/extractors/manifest/package-script-cli-extractor.js";

function extract(scripts: Record<string, string>) {
	return extractPackageScriptConsumers(scripts, "package.json", "test-repo");
}

// ── Direct binary invocation ────────────────────────────────────────

describe("extractPackageScriptConsumers — direct invocation", () => {
	it("extracts simple binary invocation", () => {
		const facts = extract({ build: "tsc" });
		expect(facts.length).toBe(1);
		expect(facts[0].address).toBe("tsc");
		expect(facts[0].operation).toBe("CLI tsc");
		expect(facts[0].mechanism).toBe("cli_command");
		expect(facts[0].basis).toBe("literal");
	});

	it("extracts binary with subcommand", () => {
		const facts = extract({ test: "vitest run" });
		expect(facts.length).toBe(1);
		expect(facts[0].address).toBe("vitest run");
		expect(facts[0].metadata.binary).toBe("vitest");
	});

	it("extracts binary with flags (flags become args, not command path)", () => {
		const facts = extract({ dev: "tsc --watch" });
		expect(facts.length).toBe(1);
		expect(facts[0].address).toBe("tsc");
		expect(facts[0].metadata.args).toContain("--watch");
	});

	it("extracts binary with subcommand and path args", () => {
		const facts = extract({ lint: "biome check src/ test/" });
		expect(facts.length).toBe(1);
		expect(facts[0].address).toBe("biome check");
		expect(facts[0].metadata.args).toEqual(["src/", "test/"]);
	});

	it("extracts vite with subcommand", () => {
		const facts = extract({ build: "vite build" });
		expect(facts.length).toBe(1);
		expect(facts[0].address).toBe("vite build");
	});

	it("extracts cdk commands", () => {
		const facts = extract({ deploy: "cdk deploy" });
		expect(facts.length).toBe(1);
		expect(facts[0].address).toBe("cdk deploy");
	});
});

// ── Chained commands ────────────────────────────────────────────────

describe("extractPackageScriptConsumers — chained commands", () => {
	it("splits on && and extracts both commands", () => {
		const facts = extract({ test: "tsc && vitest run" });
		expect(facts.length).toBe(2);
		expect(facts[0].address).toBe("tsc");
		expect(facts[1].address).toBe("vitest run");
	});

	it("splits on ; and extracts both commands", () => {
		const facts = extract({ ci: "tsc; vitest run" });
		expect(facts.length).toBe(2);
	});

	it("splits on || and extracts both commands", () => {
		const facts = extract({ check: "tsc || echo failed" });
		// tsc is extracted, echo is a shell builtin and skipped.
		const tsc = facts.find((f) => f.address === "tsc");
		expect(tsc).toBeDefined();
	});
});

// ── npx/pnpm exec unwrapping ────────────────────────────────────────

describe("extractPackageScriptConsumers — exec wrapper unwrapping", () => {
	it("unwraps npx prefix", () => {
		const facts = extract({ check: "npx vitest run" });
		expect(facts.length).toBe(1);
		expect(facts[0].address).toBe("vitest run");
		expect(facts[0].metadata.binary).toBe("vitest");
	});

	it("unwraps pnpm exec prefix", () => {
		const facts = extract({ check: "pnpm exec rgr boundary summary" });
		expect(facts.length).toBe(1);
		expect(facts[0].address).toBe("rgr boundary summary");
	});

	it("unwraps pnpx prefix", () => {
		const facts = extract({ check: "pnpx vitest" });
		expect(facts.length).toBe(1);
		expect(facts[0].address).toBe("vitest");
	});

	it("unwraps npm exec prefix (not confused with npm run indirection)", () => {
		const facts = extract({ check: "npm exec rgr boundary summary" });
		expect(facts.length).toBe(1);
		expect(facts[0].address).toBe("rgr boundary summary");
	});

	it("unwraps yarn exec prefix", () => {
		const facts = extract({ check: "yarn exec vitest run" });
		expect(facts.length).toBe(1);
		expect(facts[0].address).toBe("vitest run");
	});
});

// ── Script indirection (skipped) ────────────────────────────────────

describe("extractPackageScriptConsumers — script indirection", () => {
	it("skips npm run commands", () => {
		const facts = extract({ prepublish: "npm run build" });
		expect(facts).toEqual([]);
	});

	it("skips pnpm run commands", () => {
		const facts = extract({ ci: "pnpm run test" });
		expect(facts).toEqual([]);
	});

	it("skips npm test shorthand", () => {
		const facts = extract({ ci: "npm test" });
		expect(facts).toEqual([]);
	});

	it("skips yarn run commands", () => {
		const facts = extract({ ci: "yarn run lint" });
		expect(facts).toEqual([]);
	});
});

// ── Shell builtins (skipped) ────────────────────────────────────────

describe("extractPackageScriptConsumers — shell builtins", () => {
	it("skips rm", () => {
		const facts = extract({ clean: "rm -rf dist" });
		expect(facts).toEqual([]);
	});

	it("skips echo", () => {
		const facts = extract({ hello: "echo hello" });
		expect(facts).toEqual([]);
	});

	it("skips mkdir", () => {
		const facts = extract({ setup: "mkdir -p dist" });
		expect(facts).toEqual([]);
	});
});

// ── Fact shape ──────────────────────────────────────────────────────

describe("extractPackageScriptConsumers — fact shape", () => {
	it("mechanism is cli_command", () => {
		const facts = extract({ build: "tsc" });
		expect(facts[0].mechanism).toBe("cli_command");
	});

	it("basis is literal", () => {
		const facts = extract({ build: "tsc" });
		expect(facts[0].basis).toBe("literal");
	});

	it("sourceFile is the package.json path", () => {
		const facts = extract({ build: "tsc" });
		expect(facts[0].sourceFile).toBe("package.json");
	});

	it("callerStableKey includes script name", () => {
		const facts = extract({ build: "tsc" });
		expect(facts[0].callerStableKey).toBe("package.json:scripts.build");
	});

	it("metadata.scriptName is present", () => {
		const facts = extract({ build: "tsc" });
		expect(facts[0].metadata.scriptName).toBe("build");
	});

	it("metadata.rawCommand is present", () => {
		const facts = extract({ build: "tsc --watch" });
		expect(facts[0].metadata.rawCommand).toBe("tsc --watch");
	});

	it("confidence is 0.85", () => {
		const facts = extract({ build: "tsc" });
		expect(facts[0].confidence).toBe(0.85);
	});
});

// ── Empty / edge cases ──────────────────────────────────────────────

describe("extractPackageScriptConsumers — edge cases", () => {
	it("returns empty for empty scripts object", () => {
		const facts = extract({});
		expect(facts).toEqual([]);
	});

	it("returns empty for empty script body", () => {
		const facts = extract({ build: "" });
		expect(facts).toEqual([]);
	});

	it("handles multiple scripts", () => {
		const facts = extract({
			build: "tsc",
			test: "vitest run",
			lint: "biome check src/",
		});
		expect(facts.length).toBe(3);
	});
});

// ── repo-graph patterns ─────────────────────────────────────────────

describe("extractPackageScriptConsumers — repo-graph scripts", () => {
	it("extracts repo-graph's own script surface", () => {
		const facts = extract({
			build: "tsc",
			dev: "tsc --watch",
			test: "tsc && vitest run",
			"test:int": "vitest run test/adapters",
			"test:cli": "tsc && vitest run test/cli",
			lint: "biome check src/ test/",
			"lint:fix": "biome check --write src/ test/",
			clean: "rm -rf dist",
			prepublishOnly: "pnpm run build",
		});

		// tsc (from build, dev, test, test:cli) — deduplicated by address.
		// Actually, each script emits separately — no dedup at this level.
		const tscFacts = facts.filter((f) => f.address === "tsc");
		expect(tscFacts.length).toBeGreaterThanOrEqual(3); // build, test(1st), test:cli(1st)

		const vitestFacts = facts.filter((f) => f.address === "vitest run");
		expect(vitestFacts.length).toBeGreaterThanOrEqual(2); // test(2nd), test:int, test:cli(2nd)

		const biomeFacts = facts.filter((f) => f.address === "biome check");
		expect(biomeFacts.length).toBeGreaterThanOrEqual(1);

		// rm is skipped (shell builtin).
		const rmFacts = facts.filter((f) => f.metadata.binary === "rm");
		expect(rmFacts).toEqual([]);

		// pnpm run build is skipped (script indirection).
		const pnpmFacts = facts.filter((f) =>
			f.metadata.rawCommand?.includes("pnpm run"),
		);
		expect(pnpmFacts).toEqual([]);
	});
});

// ── glamCRM patterns ────────────────────────────────────────────────

describe("extractPackageScriptConsumers — glamCRM scripts", () => {
	it("extracts vite and eslint from frontend scripts", () => {
		const facts = extract({
			dev: "vite --port 3000",
			build: "vite build",
			lint: "eslint .",
			preview: "vite preview",
		});

		const viteFacts = facts.filter((f) => f.metadata.binary === "vite");
		expect(viteFacts.length).toBe(3); // dev, build, preview

		const eslintFacts = facts.filter((f) => f.address === "eslint");
		expect(eslintFacts.length).toBe(1);
	});

	it("extracts cdk commands from infra scripts", () => {
		const facts = extract({
			deploy: "cdk deploy",
			diff: "cdk diff",
			synth: "cdk synth",
			destroy: "cdk destroy",
			bootstrap: "cdk bootstrap",
		});

		const cdkFacts = facts.filter((f) => f.metadata.binary === "cdk");
		expect(cdkFacts.length).toBe(5);
		expect(cdkFacts.map((f) => f.address).sort()).toEqual([
			"cdk bootstrap",
			"cdk deploy",
			"cdk destroy",
			"cdk diff",
			"cdk synth",
		]);
	});
});
