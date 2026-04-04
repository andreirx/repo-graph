/**
 * Migration 004 tests.
 *
 * backfillObligationIds() is the pure function that parses, validates,
 * and mutates a single declaration's value_json. These tests cover:
 *   - the happy path (backfill assigns UUIDs)
 *   - the no-op path (already has ids, or no verification array)
 *   - the hard-fail contracts on malformed legacy data
 *
 * The runMigration004 integration path is covered indirectly by the
 * existing storage tests (fresh databases run all migrations).
 */

import { describe, expect, it } from "vitest";
import {
	backfillObligationIds,
	MalformedRequirementError,
} from "../../../../src/adapters/storage/sqlite/migrations/004-obligation-ids.js";

describe("backfillObligationIds", () => {
	// ── Happy path ───────────────────────────────────────────────────

	it("returns null when there is no verification array", () => {
		const value = JSON.stringify({
			req_id: "REQ-001",
			version: 1,
			objective: "x",
		});
		expect(backfillObligationIds("decl-1", value)).toBeNull();
	});

	it("returns null when all entries already have obligation_id", () => {
		const value = JSON.stringify({
			req_id: "REQ-001",
			version: 2,
			objective: "x",
			verification: [
				{ obligation_id: "existing-1", method: "arch_violations" },
				{ obligation_id: "existing-2", method: "coverage_threshold" },
			],
		});
		expect(backfillObligationIds("decl-1", value)).toBeNull();
	});

	it("assigns obligation_id to entries that lack one", () => {
		const value = JSON.stringify({
			req_id: "REQ-001",
			version: 2,
			objective: "x",
			verification: [
				{ obligation: "a", method: "arch_violations" },
				{ obligation: "b", method: "coverage_threshold" },
			],
		});
		const updated = backfillObligationIds("decl-1", value);
		expect(updated).not.toBeNull();
		const parsed = JSON.parse(updated!) as { verification: unknown[] };
		const entries = parsed.verification as Array<{ obligation_id: string }>;
		expect(entries[0].obligation_id).toBeDefined();
		expect(entries[0].obligation_id.length).toBeGreaterThan(0);
		expect(entries[1].obligation_id).toBeDefined();
		expect(entries[0].obligation_id).not.toBe(entries[1].obligation_id);
	});

	it("preserves existing obligation_id while filling missing ones", () => {
		const value = JSON.stringify({
			req_id: "REQ-001",
			version: 2,
			objective: "x",
			verification: [
				{ obligation_id: "keep-me", method: "arch_violations" },
				{ obligation: "new", method: "coverage_threshold" },
			],
		});
		const updated = backfillObligationIds("decl-1", value);
		expect(updated).not.toBeNull();
		const parsed = JSON.parse(updated!) as { verification: unknown[] };
		const entries = parsed.verification as Array<{
			obligation_id: string;
		}>;
		expect(entries[0].obligation_id).toBe("keep-me");
		expect(entries[1].obligation_id).toBeDefined();
		expect(entries[1].obligation_id).not.toBe("keep-me");
	});

	it("treats empty-string obligation_id as missing and replaces it", () => {
		const value = JSON.stringify({
			req_id: "REQ-001",
			version: 2,
			objective: "x",
			verification: [{ obligation_id: "", method: "arch_violations" }],
		});
		const updated = backfillObligationIds("decl-1", value);
		expect(updated).not.toBeNull();
		const parsed = JSON.parse(updated!) as { verification: unknown[] };
		const entries = parsed.verification as Array<{ obligation_id: string }>;
		expect(entries[0].obligation_id.length).toBeGreaterThan(0);
	});

	// ── Hard-fail contracts ──────────────────────────────────────────

	it("throws on invalid JSON", () => {
		expect(() => backfillObligationIds("decl-1", "{ not json")).toThrow(
			MalformedRequirementError,
		);
	});

	it("throws when value is not a JSON object", () => {
		expect(() => backfillObligationIds("decl-1", "[]")).toThrow(
			MalformedRequirementError,
		);
		expect(() => backfillObligationIds("decl-1", '"string"')).toThrow(
			MalformedRequirementError,
		);
		expect(() => backfillObligationIds("decl-1", "null")).toThrow(
			MalformedRequirementError,
		);
	});

	it("throws when req_id is missing", () => {
		const value = JSON.stringify({ version: 1, objective: "x" });
		expect(() => backfillObligationIds("decl-1", value)).toThrow(
			MalformedRequirementError,
		);
	});

	it("throws when req_id is empty", () => {
		const value = JSON.stringify({ req_id: "", version: 1, objective: "x" });
		expect(() => backfillObligationIds("decl-1", value)).toThrow(
			MalformedRequirementError,
		);
	});

	it("throws when version is not a number", () => {
		const value = JSON.stringify({
			req_id: "REQ-001",
			version: "1",
			objective: "x",
		});
		expect(() => backfillObligationIds("decl-1", value)).toThrow(
			MalformedRequirementError,
		);
	});

	it("throws when verification is not an array", () => {
		const value = JSON.stringify({
			req_id: "REQ-001",
			version: 1,
			objective: "x",
			verification: "not an array",
		});
		expect(() => backfillObligationIds("decl-1", value)).toThrow(
			MalformedRequirementError,
		);
	});

	it("throws when a verification entry is not an object", () => {
		const value = JSON.stringify({
			req_id: "REQ-001",
			version: 1,
			objective: "x",
			verification: ["not an object"],
		});
		expect(() => backfillObligationIds("decl-1", value)).toThrow(
			MalformedRequirementError,
		);
	});

	it("throws when a verification entry has no method", () => {
		const value = JSON.stringify({
			req_id: "REQ-001",
			version: 1,
			objective: "x",
			verification: [{ obligation: "x" }],
		});
		expect(() => backfillObligationIds("decl-1", value)).toThrow(
			MalformedRequirementError,
		);
	});

	it("throws when a verification entry has empty method", () => {
		const value = JSON.stringify({
			req_id: "REQ-001",
			version: 1,
			objective: "x",
			verification: [{ method: "" }],
		});
		expect(() => backfillObligationIds("decl-1", value)).toThrow(
			MalformedRequirementError,
		);
	});

	it("includes declaration_uid in the error for audit", () => {
		try {
			backfillObligationIds("decl-abc", "{ bad json");
			expect.fail("should have thrown");
		} catch (err) {
			expect(err).toBeInstanceOf(MalformedRequirementError);
			expect((err as MalformedRequirementError).declarationUid).toBe("decl-abc");
			expect((err as Error).message).toContain("decl-abc");
		}
	});
});
