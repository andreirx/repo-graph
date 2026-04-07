/**
 * Makefile CLI consumer extractor — unit tests.
 */

import { describe, expect, it } from "vitest";
import { extractMakefileConsumers } from "../../../../src/adapters/extractors/cli/makefile-cli-extractor.js";

function extract(source: string) {
	return extractMakefileConsumers(source, "Makefile", "test-repo");
}

// ── Recipe line extraction ──────────────────────────────────────────

describe("extractMakefileConsumers — recipe lines", () => {
	it("extracts a simple recipe command", () => {
		const facts = extract("build:\n\tgcc -o main main.c");
		expect(facts.length).toBe(1);
		expect(facts[0].address).toBe("gcc");
		expect(facts[0].mechanism).toBe("cli_command");
	});

	it("extracts multiple recipe lines", () => {
		const facts = extract("build:\n\tgcc -o main main.c\n\tstrip main");
		expect(facts.length).toBe(2);
		expect(facts[0].address).toBe("gcc");
		// "main" is lowercase → treated as subcommand by the heuristic parser.
		expect(facts[1].address).toBe("strip main");
	});

	it("extracts chained commands in recipe", () => {
		const facts = extract("test:\n\tgcc -o test test.c && ./test");
		// gcc extracted, ./test skipped (starts with ./)
		expect(facts.length).toBe(1);
		expect(facts[0].address).toBe("gcc");
	});

	it("extracts commands with subcommands", () => {
		const facts = extract("deploy:\n\tcdk deploy --require-approval never");
		expect(facts.length).toBe(1);
		expect(facts[0].address).toBe("cdk deploy");
	});
});

// ── Silent/force prefixes ───────────────────────────────────────────

describe("extractMakefileConsumers — prefixes", () => {
	it("strips @ silent prefix", () => {
		const facts = extract("clean:\n\t@rm -rf build");
		// rm is a shell builtin — skipped.
		expect(facts).toEqual([]);
	});

	it("strips - force prefix", () => {
		const facts = extract("clean:\n\t-cmake --build . --target clean");
		expect(facts.length).toBe(1);
		expect(facts[0].address).toBe("cmake");
	});

	it("strips @- combined prefix", () => {
		const facts = extract("quiet:\n\t@-cmake --version");
		expect(facts.length).toBe(1);
		expect(facts[0].address).toBe("cmake");
	});

	it("strips + force prefix", () => {
		const facts = extract("recurse:\n\t+make -C subdir");
		expect(facts.length).toBe(1);
		expect(facts[0].address).toBe("make");
	});

	it("strips +@ combined prefix", () => {
		const facts = extract("recurse:\n\t+@cmake --build .");
		expect(facts.length).toBe(1);
		expect(facts[0].address).toBe("cmake");
	});

	it("strips +$(MAKE) via prefix + variable skip", () => {
		const facts = extract("sub:\n\t+$(MAKE) -C subdir");
		// + stripped, then $(MAKE) starts command → variable skip.
		expect(facts).toEqual([]);
	});
});

// ── Skipped patterns ────────────────────────────────────────────────

describe("extractMakefileConsumers — skipped patterns", () => {
	it("skips non-recipe lines (targets, assignments)", () => {
		const facts = extract("CXX = clang++\nbuild: main.c\n\tgcc main.c");
		expect(facts.length).toBe(1);
		expect(facts[0].address).toBe("gcc");
	});

	it("skips comments in recipes", () => {
		const facts = extract("build:\n\t# This is a comment\n\tgcc main.c");
		expect(facts.length).toBe(1);
	});

	it("skips $(VAR) dominated lines", () => {
		const facts = extract("build:\n\t$(CXX) $(CXXFLAGS) -o main main.c");
		// First word is $(CXX) — a Make variable expansion. Skipped.
		expect(facts).toEqual([]);
	});

	it("strips $(Q) quiet prefix and extracts command", () => {
		const facts = extract("sign:\n\t$(Q)openssl dgst -sha256 -sign key.pem");
		expect(facts.length).toBe(1);
		expect(facts[0].address).toBe("openssl dgst");
	});

	it("skips continuation lines", () => {
		const facts = extract("build:\n\tgcc -o main \\\n\t\tmain.c");
		// First line ends with \ — continuation. Both skipped.
		expect(facts).toEqual([]);
	});

	it("skips empty recipe lines", () => {
		const facts = extract("build:\n\t\n\tgcc main.c");
		expect(facts.length).toBe(1);
	});

	it("skips Make directives in recipes", () => {
		const facts = extract("build:\n\tinclude config.mk\n\tgcc main.c");
		// "include" is a Make directive — skipped.
		expect(facts.length).toBe(1);
		expect(facts[0].address).toBe("gcc");
	});

	it("skips quoted $(VAR) commands", () => {
		const facts = extract('link:\n\t"$(LDLIBS)" foo');
		expect(facts).toEqual([]);
	});

	it("skips single-quoted $(VAR) commands", () => {
		const facts = extract("link:\n\t'$(CC)' main.c");
		expect(facts).toEqual([]);
	});
});

// ── Real-world patterns ─────────────────────────────────────────────

describe("extractMakefileConsumers — real patterns", () => {
	it("extracts from C++11 Deep Dives Makefile", () => {
		const facts = extract(
			"CXX ?= clang++\nCXXFLAGS ?= -std=c++11 -Wall -Wextra -g\nsolution: solution.cpp\n\t$(CXX) $(CXXFLAGS) -o solution solution.cpp\nrun: solution\n\t./solution input1.txt\nclean:\n\trm -f solution",
		);
		// $(CXX) line → skipped (variable expansion)
		// ./solution → skipped (path)
		// rm → skipped (builtin)
		expect(facts).toEqual([]);
	});

	it("extracts openssl commands from swupdate-style Makefile", () => {
		const facts = extract(
			"sign:\n\t$(Q)openssl dgst -sha256 -sign key.pem data > sig\nverify:\n\t$(Q)openssl rsa -in key.pem -out pub.pem -outform PEM -pubout",
		);
		expect(facts.length).toBe(2);
		expect(facts[0].address).toBe("openssl dgst");
		expect(facts[1].address).toBe("openssl rsa");
	});
});

// ── Fact shape ──────────────────────────────────────────────────────

describe("extractMakefileConsumers — fact shape", () => {
	it("mechanism is cli_command", () => {
		const facts = extract("build:\n\tgcc main.c");
		expect(facts[0].mechanism).toBe("cli_command");
	});

	it("sourceFile matches provided path", () => {
		const facts = extract("build:\n\tgcc main.c");
		expect(facts[0].sourceFile).toBe("Makefile");
	});

	it("lineStart is 1-indexed", () => {
		const facts = extract("build:\n\tgcc main.c");
		expect(facts[0].lineStart).toBe(2);
	});

	it("confidence is lower than shell scripts", () => {
		const facts = extract("build:\n\tgcc main.c");
		expect(facts[0].confidence).toBeLessThan(0.85 * 0.9);
	});
});
