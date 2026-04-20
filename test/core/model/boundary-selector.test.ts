/**
 * Unit tests for boundary selector model.
 *
 * Tests the typed selector model for boundary declarations:
 *   - Type guards for selector domain discrimination
 *   - Parsing and validation for both domain types
 *   - Backwards compatibility (missing selectorDomain = directory_module)
 */

import { describe, expect, it } from "vitest";
import {
	type BoundaryDeclarationValue,
	type DirectoryModuleBoundaryValue,
	type DiscoveredModuleBoundaryValue,
	isDirectoryModuleBoundary,
	isDiscoveredModuleBoundary,
	parseBoundaryValue,
} from "../../../src/core/model/boundary-selector.js";

// ── Type guards ────────────────────────────────────────────────────

describe("isDiscoveredModuleBoundary", () => {
	it("returns true for discovered_module selectorDomain", () => {
		const value: DiscoveredModuleBoundaryValue = {
			selectorDomain: "discovered_module",
			source: { canonicalRootPath: "packages/app" },
			forbids: { canonicalRootPath: "packages/db" },
		};
		expect(isDiscoveredModuleBoundary(value)).toBe(true);
	});

	it("returns false for directory_module selectorDomain", () => {
		const value: DirectoryModuleBoundaryValue = {
			selectorDomain: "directory_module",
			forbids: "src/db",
		};
		expect(isDiscoveredModuleBoundary(value)).toBe(false);
	});

	it("returns false for missing selectorDomain", () => {
		const value: DirectoryModuleBoundaryValue = {
			forbids: "src/db",
		};
		expect(isDiscoveredModuleBoundary(value)).toBe(false);
	});
});

describe("isDirectoryModuleBoundary", () => {
	it("returns true for directory_module selectorDomain", () => {
		const value: DirectoryModuleBoundaryValue = {
			selectorDomain: "directory_module",
			forbids: "src/db",
		};
		expect(isDirectoryModuleBoundary(value)).toBe(true);
	});

	it("returns true for missing selectorDomain (backwards compat)", () => {
		const value: DirectoryModuleBoundaryValue = {
			forbids: "src/db",
		};
		expect(isDirectoryModuleBoundary(value)).toBe(true);
	});

	it("returns false for discovered_module selectorDomain", () => {
		const value: DiscoveredModuleBoundaryValue = {
			selectorDomain: "discovered_module",
			source: { canonicalRootPath: "packages/app" },
			forbids: { canonicalRootPath: "packages/db" },
		};
		expect(isDirectoryModuleBoundary(value)).toBe(false);
	});
});

// ── Parsing — discovered_module ────────────────────────────────────

describe("parseBoundaryValue — discovered_module", () => {
	it("parses valid discovered_module boundary", () => {
		const json = JSON.stringify({
			selectorDomain: "discovered_module",
			source: { canonicalRootPath: "packages/app" },
			forbids: { canonicalRootPath: "packages/db" },
			reason: "ui must not depend on db",
		});

		const result = parseBoundaryValue(json);

		expect(isDiscoveredModuleBoundary(result)).toBe(true);
		if (isDiscoveredModuleBoundary(result)) {
			expect(result.source.canonicalRootPath).toBe("packages/app");
			expect(result.forbids.canonicalRootPath).toBe("packages/db");
			expect(result.reason).toBe("ui must not depend on db");
		}
	});

	it("parses discovered_module boundary without reason", () => {
		const json = JSON.stringify({
			selectorDomain: "discovered_module",
			source: { canonicalRootPath: "packages/app" },
			forbids: { canonicalRootPath: "packages/db" },
		});

		const result = parseBoundaryValue(json);

		expect(isDiscoveredModuleBoundary(result)).toBe(true);
		if (isDiscoveredModuleBoundary(result)) {
			expect(result.reason).toBeUndefined();
		}
	});

	it("throws when source is missing", () => {
		const json = JSON.stringify({
			selectorDomain: "discovered_module",
			forbids: { canonicalRootPath: "packages/db" },
		});

		expect(() => parseBoundaryValue(json)).toThrow(
			"discovered_module boundary requires source object",
		);
	});

	it("throws when forbids is missing", () => {
		const json = JSON.stringify({
			selectorDomain: "discovered_module",
			source: { canonicalRootPath: "packages/app" },
		});

		expect(() => parseBoundaryValue(json)).toThrow(
			"discovered_module boundary requires forbids object",
		);
	});

	it("throws when source.canonicalRootPath is missing", () => {
		const json = JSON.stringify({
			selectorDomain: "discovered_module",
			source: {},
			forbids: { canonicalRootPath: "packages/db" },
		});

		expect(() => parseBoundaryValue(json)).toThrow(
			"discovered_module boundary source requires canonicalRootPath",
		);
	});

	it("throws when forbids.canonicalRootPath is missing", () => {
		const json = JSON.stringify({
			selectorDomain: "discovered_module",
			source: { canonicalRootPath: "packages/app" },
			forbids: {},
		});

		expect(() => parseBoundaryValue(json)).toThrow(
			"discovered_module boundary forbids requires canonicalRootPath",
		);
	});
});

// ── Parsing — directory_module ─────────────────────────────────────

describe("parseBoundaryValue — directory_module", () => {
	it("parses explicit directory_module boundary", () => {
		const json = JSON.stringify({
			selectorDomain: "directory_module",
			forbids: "src/db",
			reason: "api must not depend on db",
		});

		const result = parseBoundaryValue(json);

		expect(isDirectoryModuleBoundary(result)).toBe(true);
		if (isDirectoryModuleBoundary(result)) {
			expect(result.selectorDomain).toBe("directory_module");
			expect(result.forbids).toBe("src/db");
			expect(result.reason).toBe("api must not depend on db");
		}
	});

	it("parses boundary without selectorDomain (backwards compat)", () => {
		const json = JSON.stringify({
			forbids: "src/db",
		});

		const result = parseBoundaryValue(json);

		expect(isDirectoryModuleBoundary(result)).toBe(true);
		if (isDirectoryModuleBoundary(result)) {
			expect(result.selectorDomain).toBeUndefined();
			expect(result.forbids).toBe("src/db");
		}
	});

	it("throws when forbids is not a string", () => {
		const json = JSON.stringify({
			forbids: 123,
		});

		expect(() => parseBoundaryValue(json)).toThrow(
			"directory_module boundary requires forbids string",
		);
	});

	it("throws when forbids is missing", () => {
		const json = JSON.stringify({
			reason: "some reason",
		});

		expect(() => parseBoundaryValue(json)).toThrow(
			"directory_module boundary requires forbids string",
		);
	});
});

// ── Unknown selectorDomain rejection ───────────────────────────────

describe("parseBoundaryValue — unknown selectorDomain", () => {
	it("throws for explicit but unrecognized selectorDomain", () => {
		const json = JSON.stringify({
			selectorDomain: "bogus",
			forbids: "packages/db",
		});

		expect(() => parseBoundaryValue(json)).toThrow(
			'unknown boundary selectorDomain: bogus (expected "directory_module" or "discovered_module")',
		);
	});

	it("throws for typo in selectorDomain", () => {
		const json = JSON.stringify({
			selectorDomain: "discovered_modules", // plural typo
			source: { canonicalRootPath: "packages/app" },
			forbids: { canonicalRootPath: "packages/db" },
		});

		expect(() => parseBoundaryValue(json)).toThrow(
			'unknown boundary selectorDomain: discovered_modules (expected "directory_module" or "discovered_module")',
		);
	});

	it("accepts missing selectorDomain as legacy", () => {
		const json = JSON.stringify({
			forbids: "src/db",
		});

		// Should NOT throw — missing is backwards-compatible
		const result = parseBoundaryValue(json);
		expect(isDirectoryModuleBoundary(result)).toBe(true);
		expect(result.selectorDomain).toBeUndefined();
	});
});

// ── Discrimination in practice ─────────────────────────────────────

describe("boundary value discrimination", () => {
	it("can process mixed array of boundary values", () => {
		const values: BoundaryDeclarationValue[] = [
			{
				selectorDomain: "discovered_module",
				source: { canonicalRootPath: "packages/app" },
				forbids: { canonicalRootPath: "packages/db" },
			},
			{
				selectorDomain: "directory_module",
				forbids: "src/db",
			},
			{
				forbids: "src/legacy", // backwards compat
			},
		];

		const discovered = values.filter(isDiscoveredModuleBoundary);
		const directory = values.filter(isDirectoryModuleBoundary);

		expect(discovered).toHaveLength(1);
		expect(directory).toHaveLength(2);
	});
});
