/**
 * Core classifier-signals — pure predicate tests.
 *
 * Every function under test is pure. No I/O, no fixtures.
 */

import { describe, expect, it } from "vitest";
import {
	hasPackageDependency,
	matchesAnyAlias,
	type PackageDependencySet,
	type TsconfigAliases,
} from "../../../src/core/classification/signals.js";

// ── matchesAnyAlias ─────────────────────────────────────────────────

function aliasesOf(
	...patterns: Array<{ pattern: string; substitutions?: string[] }>
): TsconfigAliases {
	return {
		entries: patterns.map((p) => ({
			pattern: p.pattern,
			substitutions: p.substitutions ?? [],
		})),
	};
}

describe("matchesAnyAlias", () => {
	it("wildcard pattern '@/*' matches any specifier starting with '@/'", () => {
		const aliases = aliasesOf({ pattern: "@/*" });
		expect(matchesAnyAlias("@/lib/foo", aliases)).toBe(true);
		expect(matchesAnyAlias("@/", aliases)).toBe(true);
		expect(matchesAnyAlias("@/x/y/z", aliases)).toBe(true);
	});

	it("wildcard pattern does NOT match when prefix does not match", () => {
		const aliases = aliasesOf({ pattern: "@/*" });
		expect(matchesAnyAlias("react", aliases)).toBe(false);
		expect(matchesAnyAlias("./local", aliases)).toBe(false);
		expect(matchesAnyAlias("@app/lib", aliases)).toBe(false);
	});

	it("exact pattern '@types' matches only the exact specifier", () => {
		const aliases = aliasesOf({ pattern: "@types" });
		expect(matchesAnyAlias("@types", aliases)).toBe(true);
		expect(matchesAnyAlias("@types/foo", aliases)).toBe(false);
		expect(matchesAnyAlias("@typescript", aliases)).toBe(false);
	});

	it("skips pattern that is only '*'", () => {
		const aliases = aliasesOf({ pattern: "*" });
		expect(matchesAnyAlias("any-specifier", aliases)).toBe(false);
		expect(matchesAnyAlias("foo", aliases)).toBe(false);
	});

	it("returns false for empty alias set", () => {
		const aliases = aliasesOf();
		expect(matchesAnyAlias("@/anything", aliases)).toBe(false);
	});

	it("evaluates multiple patterns (OR semantics)", () => {
		const aliases = aliasesOf(
			{ pattern: "@app/*" },
			{ pattern: "~components/*" },
			{ pattern: "@types" },
		);
		expect(matchesAnyAlias("@app/user", aliases)).toBe(true);
		expect(matchesAnyAlias("~components/Button", aliases)).toBe(true);
		expect(matchesAnyAlias("@types", aliases)).toBe(true);
		expect(matchesAnyAlias("lodash", aliases)).toBe(false);
	});

	it("ignores entries whose pattern ends in '*' with empty prefix", () => {
		// This is the pattern `*` variant after slice(0, -1).
		// Already covered by the '*' skip, but double-check.
		const aliases = aliasesOf({ pattern: "*" });
		expect(matchesAnyAlias("x", aliases)).toBe(false);
	});
});

// ── hasPackageDependency ────────────────────────────────────────────

describe("hasPackageDependency", () => {
	const deps: PackageDependencySet = {
		names: Object.freeze(["lodash", "react", "tsx"]),
	};

	it("returns true for a name present in the set", () => {
		expect(hasPackageDependency(deps, "react")).toBe(true);
		expect(hasPackageDependency(deps, "lodash")).toBe(true);
	});

	it("returns false for a name not present", () => {
		expect(hasPackageDependency(deps, "missing")).toBe(false);
		expect(hasPackageDependency(deps, "rea")).toBe(false); // no prefix match
	});

	it("returns false for empty dep set", () => {
		const empty: PackageDependencySet = { names: Object.freeze([]) };
		expect(hasPackageDependency(empty, "anything")).toBe(false);
	});
});
