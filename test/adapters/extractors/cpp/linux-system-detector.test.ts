/**
 * Linux/system framework detector — unit tests.
 */

import { describe, expect, it } from "vitest";
import { detectLinuxSystemPatterns } from "../../../../src/adapters/extractors/cpp/linux-system-detector.js";

function sym(
	name: string,
	subtype: "FUNCTION" | "CLASS" | "VARIABLE",
	lineStart: number,
) {
	return {
		stableKey: `test:src/driver.c#${name}:SYMBOL:${subtype}`,
		name,
		qualifiedName: name,
		subtype,
		lineStart,
	};
}

// ── module_init / module_exit ───────────────────────────────────────

describe("detectLinuxSystemPatterns — kernel modules", () => {
	it("detects module_init", () => {
		const results = detectLinuxSystemPatterns(
			"module_init(omap_hwspinlock_init);",
			"drivers/hwspinlock/omap.c",
			[sym("omap_hwspinlock_init", "FUNCTION", 1)],
		);
		expect(results.length).toBe(1);
		expect(results[0].convention).toBe("linux_module_init");
		expect(results[0].confidence).toBe(0.95);
	});

	it("detects module_exit", () => {
		const results = detectLinuxSystemPatterns(
			"module_exit(omap_hwspinlock_exit);",
			"drivers/hwspinlock/omap.c",
			[sym("omap_hwspinlock_exit", "FUNCTION", 1)],
		);
		expect(results.length).toBe(1);
		expect(results[0].convention).toBe("linux_module_exit");
	});

	it("detects both module_init and module_exit in same file", () => {
		const results = detectLinuxSystemPatterns(
			"module_init(my_init);\nmodule_exit(my_exit);",
			"drivers/foo.c",
			[
				sym("my_init", "FUNCTION", 1),
				sym("my_exit", "FUNCTION", 2),
			],
		);
		expect(results.length).toBe(2);
	});

	it("detects module_platform_driver", () => {
		const results = detectLinuxSystemPatterns(
			"module_platform_driver(omap_hwspinlock_driver);",
			"drivers/hwspinlock/omap.c",
			[sym("omap_hwspinlock_driver", "VARIABLE", 1)],
		);
		expect(results.length).toBe(1);
		expect(results[0].convention).toBe("linux_platform_driver");
	});

	it("detects builtin_platform_driver", () => {
		const results = detectLinuxSystemPatterns(
			"builtin_platform_driver(my_driver);",
			"drivers/foo.c",
			[sym("my_driver", "VARIABLE", 1)],
		);
		expect(results.length).toBe(1);
		expect(results[0].convention).toBe("linux_platform_driver");
	});
});

// ── GCC constructor/destructor ──────────────────────────────────────

describe("detectLinuxSystemPatterns — GCC attributes", () => {
	it("detects __attribute__((constructor))", () => {
		const results = detectLinuxSystemPatterns(
			"__attribute__((constructor))\nstatic void register_uniqueuuid(void) {}",
			"handlers/uniqueuuid.c",
			[sym("register_uniqueuuid", "FUNCTION", 2)],
		);
		expect(results.length).toBe(1);
		expect(results[0].convention).toBe("gcc_constructor");
		expect(results[0].targetStableKey).toContain("register_uniqueuuid");
	});

	it("detects __attribute__((destructor))", () => {
		const results = detectLinuxSystemPatterns(
			"__attribute__((destructor))\nstatic void cleanup(void) {}",
			"src/cleanup.c",
			[sym("cleanup", "FUNCTION", 2)],
		);
		expect(results.length).toBe(1);
		expect(results[0].convention).toBe("gcc_destructor");
	});
});

// ── register_handler (swupdate style) ───────────────────────────────

describe("detectLinuxSystemPatterns — register_handler", () => {
	it("detects register_handler invocation", () => {
		const results = detectLinuxSystemPatterns(
			'register_handler("uniqueuuid", uniqueuuid, SCRIPT_HANDLER, NULL);',
			"handlers/uniqueuuid.c",
			[sym("uniqueuuid", "FUNCTION", 1)],
		);
		expect(results.length).toBe(1);
		expect(results[0].convention).toBe("register_handler");
		expect(results[0].confidence).toBe(0.90);
	});
});

// ── No false positives ──────────────────────────────────────────────

describe("detectLinuxSystemPatterns — no false positives", () => {
	it("returns empty for plain C code", () => {
		const results = detectLinuxSystemPatterns(
			"int main() { return 0; }",
			"src/main.c",
			[sym("main", "FUNCTION", 1)],
		);
		expect(results).toEqual([]);
	});

	it("does not match module_init in single-line comment even with real symbol", () => {
		const results = detectLinuxSystemPatterns(
			"// module_init(my_init); was removed",
			"src/main.c",
			[sym("my_init", "FUNCTION", 1)],
		);
		expect(results).toEqual([]);
	});

	it("does not match register_handler in block comment with real symbol", () => {
		const results = detectLinuxSystemPatterns(
			'/* register_handler("x", my_handler, SCRIPT_HANDLER, NULL); */',
			"handlers/foo.c",
			[sym("my_handler", "FUNCTION", 1)],
		);
		expect(results).toEqual([]);
	});

	it("does not match module_init in Javadoc-style comment", () => {
		const results = detectLinuxSystemPatterns(
			" * module_init(my_init) registers the driver",
			"src/main.c",
			[sym("my_init", "FUNCTION", 1)],
		);
		expect(results).toEqual([]);
	});

	it("does not match function name not in symbol list", () => {
		const results = detectLinuxSystemPatterns(
			"module_init(nonexistent_func);",
			"src/driver.c",
			[], // No symbols provided.
		);
		expect(results).toEqual([]);
	});
});

// ── Output shape ────────────────────────────────────────────────────

describe("detectLinuxSystemPatterns — output shape", () => {
	it("includes targetStableKey", () => {
		const results = detectLinuxSystemPatterns(
			"module_init(init);",
			"d.c",
			[sym("init", "FUNCTION", 1)],
		);
		expect(results[0].targetStableKey).toBeDefined();
	});

	it("includes reason string", () => {
		const results = detectLinuxSystemPatterns(
			"module_init(init);",
			"d.c",
			[sym("init", "FUNCTION", 1)],
		);
		expect(typeof results[0].reason).toBe("string");
		expect(results[0].reason.length).toBeGreaterThan(0);
	});
});
