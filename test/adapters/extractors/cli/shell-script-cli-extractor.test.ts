/**
 * Shell script CLI consumer extractor — unit tests.
 */

import { describe, expect, it } from "vitest";
import { extractShellScriptConsumers } from "../../../../src/adapters/extractors/cli/shell-script-cli-extractor.js";

function extract(source: string) {
	return extractShellScriptConsumers(source, "scripts/deploy.sh", "test-repo");
}

// ── Direct invocations ──────────────────────────────────────────────

describe("extractShellScriptConsumers — direct invocations", () => {
	it("extracts a simple command", () => {
		// "myrepo" is lowercase and looks like a subcommand to the heuristic parser.
		// This is a known limitation — the parser has no command schema awareness.
		const facts = extract("rgr repo index myrepo");
		expect(facts.length).toBe(1);
		expect(facts[0].address).toBe("rgr repo index myrepo");
		expect(facts[0].mechanism).toBe("cli_command");
		expect(facts[0].metadata.binary).toBe("rgr");
	});

	it("extracts command with flags as args", () => {
		const facts = extract("tsc --watch");
		expect(facts.length).toBe(1);
		expect(facts[0].address).toBe("tsc");
		expect(facts[0].metadata.args).toContain("--watch");
	});

	it("extracts command with subcommand", () => {
		const facts = extract("docker compose up -d");
		expect(facts.length).toBe(1);
		expect(facts[0].address).toBe("docker compose up");
	});

	it("has lower confidence than package.json scripts", () => {
		const facts = extract("vitest run");
		expect(facts[0].confidence).toBeLessThan(0.85);
	});
});

// ── Chained commands ────────────────────────────────────────────────

describe("extractShellScriptConsumers — chained commands", () => {
	it("splits on && and extracts both", () => {
		const facts = extract("tsc && vitest run");
		expect(facts.length).toBe(2);
		expect(facts[0].address).toBe("tsc");
		expect(facts[1].address).toBe("vitest run");
	});

	it("splits on ;", () => {
		const facts = extract("tsc; vitest run");
		expect(facts.length).toBe(2);
	});
});

// ── Environment prefixes ────────────────────────────────────────────

describe("extractShellScriptConsumers — env prefixes", () => {
	it("strips NODE_ENV=test prefix", () => {
		const facts = extract("NODE_ENV=test vitest run");
		expect(facts.length).toBe(1);
		expect(facts[0].address).toBe("vitest run");
	});

	it("strips multiple env prefixes", () => {
		const facts = extract("FOO=bar BAZ=qux vitest");
		expect(facts.length).toBe(1);
		expect(facts[0].address).toBe("vitest");
	});
});

// ── Wrapper unwrapping ──────────────────────────────────────────────

describe("extractShellScriptConsumers — wrapper unwrapping", () => {
	it("unwraps npx", () => {
		const facts = extract("npx vitest run");
		expect(facts.length).toBe(1);
		expect(facts[0].address).toBe("vitest run");
	});
});

// ── Skipped patterns ────────────────────────────────────────────────

describe("extractShellScriptConsumers — skipped patterns", () => {
	it("skips comments", () => {
		const facts = extract("# This is a comment\nvitest run");
		expect(facts.length).toBe(1);
		expect(facts[0].address).toBe("vitest run");
	});

	it("skips shebang", () => {
		const facts = extract("#!/bin/bash\nvitest run");
		expect(facts.length).toBe(1);
	});

	it("skips empty lines", () => {
		const facts = extract("\n\nvitest run\n\n");
		expect(facts.length).toBe(1);
	});

	it("skips variable assignments", () => {
		const facts = extract("FOO=bar");
		expect(facts).toEqual([]);
	});

	it("skips shell control flow", () => {
		const facts = extract("if true\nthen\nvitest run\nfi");
		// "if true" → skipped (control flow)
		// "then" → skipped
		// "vitest run" → extracted
		// "fi" → skipped
		expect(facts.length).toBe(1);
		expect(facts[0].address).toBe("vitest run");
	});

	it("skips function definitions", () => {
		const facts = extract("deploy() {\nrgr repo index foo\n}");
		// function def line skipped, command inside extracted, } skipped
		// "foo" looks like a subcommand to the heuristic parser (lowercase word).
		expect(facts.length).toBe(1);
		expect(facts[0].address).toBe("rgr repo index foo");
	});

	it("skips shell builtins", () => {
		const facts = extract("cd /tmp\necho hello\nvitest run");
		expect(facts.length).toBe(1);
		expect(facts[0].address).toBe("vitest run");
	});

	it("skips piped commands entirely", () => {
		const facts = extract("cat input | jq .foo");
		expect(facts).toEqual([]);
	});

	it("skips script indirection", () => {
		const facts = extract("npm run build");
		expect(facts).toEqual([]);
	});

	it("does not extract ./script.sh paths", () => {
		const facts = extract("./scripts/deploy.sh");
		expect(facts).toEqual([]);
	});

	it("skips heredoc content", () => {
		const facts = extract(`cat << EOF
This is heredoc content
Not a command
EOF
vitest run`);
		// Only "vitest run" should be extracted — heredoc content skipped.
		expect(facts.length).toBe(1);
		expect(facts[0].address).toBe("vitest run");
	});

	it("skips quoted heredoc content", () => {
		const facts = extract(`cat << "EOF"
heredoc stuff
EOF
tsc`);
		expect(facts.length).toBe(1);
		expect(facts[0].address).toBe("tsc");
	});

	it("skips continuation lines (backslash) — all fragments", () => {
		const facts = extract(`aws cloudformation describe-stacks \\
  --stack-name foo \\
  --region us-east-1`);
		// All three lines are skipped (start + two continuation fragments).
		expect(facts).toEqual([]);
	});

	it("skips continuation fragments that look like bare words", () => {
		const facts = extract(`mytool repo add \\
  staging`);
		// Both lines skipped — "staging" is a continuation fragment.
		expect(facts).toEqual([]);
	});

	it("resumes normal parsing after continuation ends", () => {
		const facts = extract(`mytool repo add \\
  staging
vitest run`);
		// First two lines are continuation (skipped). Third line is normal.
		expect(facts.length).toBe(1);
		expect(facts[0].address).toBe("vitest run");
	});

	it("skips dot-source (. ./env.sh)", () => {
		const facts = extract(". ./env.sh");
		expect(facts).toEqual([]);
	});

	it("skips source builtin", () => {
		const facts = extract("source ./env.sh");
		expect(facts).toEqual([]);
	});

	it("skips flag-only lines", () => {
		const facts = extract("--stack-name\n--region\nvitest run");
		expect(facts.length).toBe(1);
		expect(facts[0].address).toBe("vitest run");
	});
});

// ── Multi-line scripts ──────────────────────────────────────────────

describe("extractShellScriptConsumers — multi-line", () => {
	it("extracts commands from a real deploy script", () => {
		const facts = extract(`#!/bin/bash
set -e

# Build
tsc
vitest run

# Deploy
rgr repo refresh myrepo
cdk deploy --require-approval never
`);
		// set → builtin, skipped
		// tsc, vitest run, rgr repo refresh, cdk deploy → extracted
		expect(facts.length).toBe(4);
		expect(facts.map((f) => f.address)).toEqual([
			"tsc",
			"vitest run",
			"rgr repo refresh myrepo",
			"cdk deploy",
		]);
	});
});

// ── Fact shape ──────────────────────────────────────────────────────

describe("extractShellScriptConsumers — fact shape", () => {
	it("mechanism is cli_command", () => {
		const facts = extract("vitest run");
		expect(facts[0].mechanism).toBe("cli_command");
	});

	it("basis is literal", () => {
		const facts = extract("vitest run");
		expect(facts[0].basis).toBe("literal");
	});

	it("sourceFile matches provided path", () => {
		const facts = extract("vitest run");
		expect(facts[0].sourceFile).toBe("scripts/deploy.sh");
	});

	it("lineStart is 1-indexed", () => {
		const facts = extract("vitest run");
		expect(facts[0].lineStart).toBe(1);
	});

	it("callerStableKey includes file:line", () => {
		const facts = extract("vitest run");
		expect(facts[0].callerStableKey).toBe("scripts/deploy.sh:1");
	});
});
