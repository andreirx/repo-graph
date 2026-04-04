/**
 * Declaration model tests.
 *
 * Focus: the obligation identity contract and requirement validation.
 * The semantic-match contract is frozen (see SEMANTIC_MATCH_FIELDS).
 * These tests exist to detect accidental changes to that contract.
 */

import { describe, expect, it } from "vitest";
import {
	DeclarationValidationError,
	inheritObligationIds,
	obligationsSemanticallyMatch,
	parseDeclarationValue,
	SEMANTIC_MATCH_FIELDS,
	type VerificationObligation,
} from "../../../src/core/model/declaration.js";
import { DeclarationKind } from "../../../src/core/model/types.js";

// Deterministic id generator for tests
function makeIdGen(): () => string {
	let counter = 0;
	return () => `gen-${++counter}`;
}

describe("SEMANTIC_MATCH_FIELDS", () => {
	it("is the frozen identity contract", () => {
		// This list defines obligation identity. Changes to it are
		// breaking changes to the waiver contract.
		expect([...SEMANTIC_MATCH_FIELDS]).toEqual([
			"method",
			"target",
			"threshold",
			"operator",
		]);
	});

	it("is frozen at runtime", () => {
		expect(Object.isFrozen(SEMANTIC_MATCH_FIELDS)).toBe(true);
	});
});

describe("obligationsSemanticallyMatch", () => {
	const base: VerificationObligation = {
		obligation_id: "id-1",
		obligation: "text",
		method: "coverage_threshold",
		target: "src",
		threshold: 0.8,
		operator: ">=",
	};

	it("returns true for identical semantic tuples", () => {
		expect(obligationsSemanticallyMatch(base, { ...base })).toBe(true);
	});

	it("ignores obligation_id differences", () => {
		expect(
			obligationsSemanticallyMatch(base, { ...base, obligation_id: "id-2" }),
		).toBe(true);
	});

	it("ignores obligation text differences", () => {
		expect(
			obligationsSemanticallyMatch(base, {
				...base,
				obligation: "different text",
			}),
		).toBe(true);
	});

	it("returns false on method change", () => {
		expect(
			obligationsSemanticallyMatch(base, {
				...base,
				method: "complexity_threshold",
			}),
		).toBe(false);
	});

	it("returns false on target change", () => {
		expect(
			obligationsSemanticallyMatch(base, { ...base, target: "src/core" }),
		).toBe(false);
	});

	it("returns false on threshold change", () => {
		expect(
			obligationsSemanticallyMatch(base, { ...base, threshold: 0.85 }),
		).toBe(false);
	});

	it("returns false on operator change", () => {
		expect(obligationsSemanticallyMatch(base, { ...base, operator: ">" })).toBe(
			false,
		);
	});

	it("treats missing undefined vs present undefined as matching", () => {
		const a: Partial<VerificationObligation> = { method: "arch_violations" };
		const b: Partial<VerificationObligation> = {
			method: "arch_violations",
			target: undefined,
		};
		expect(obligationsSemanticallyMatch(a, b)).toBe(true);
	});

	it("threshold reversion (0.8 -> 0.85 -> 0.8) matches original", () => {
		// This is the reversion case. Two obligations with threshold 0.8
		// are semantically identical regardless of intermediate history.
		// Waiver re-resurrection is prevented at the supersession-chain
		// layer, not by the semantic-match function.
		const original = { ...base, threshold: 0.8 };
		const reverted = { ...base, threshold: 0.8 };
		expect(obligationsSemanticallyMatch(original, reverted)).toBe(true);
	});
});

describe("inheritObligationIds", () => {
	const baseNext = {
		obligation: "text",
		method: "coverage_threshold" as const,
		target: "src",
		threshold: 0.8,
		operator: ">=" as const,
	};

	it("inherits obligation_id when semantic tuple matches prior", () => {
		const prior: VerificationObligation[] = [
			{ ...baseNext, obligation_id: "prior-A" },
		];
		const result = inheritObligationIds(prior, [baseNext], makeIdGen());
		expect(result[0].obligation_id).toBe("prior-A");
	});

	it("ignores obligation text changes when inheriting", () => {
		const prior: VerificationObligation[] = [
			{ ...baseNext, obligation_id: "prior-A", obligation: "old text" },
		];
		const result = inheritObligationIds(
			prior,
			[{ ...baseNext, obligation: "new text" }],
			makeIdGen(),
		);
		expect(result[0].obligation_id).toBe("prior-A");
	});

	it("generates new id when threshold changes", () => {
		const prior: VerificationObligation[] = [
			{ ...baseNext, obligation_id: "prior-A", threshold: 0.8 },
		];
		const result = inheritObligationIds(
			prior,
			[{ ...baseNext, threshold: 0.85 }],
			makeIdGen(),
		);
		expect(result[0].obligation_id).toBe("gen-1");
		expect(result[0].obligation_id).not.toBe("prior-A");
	});

	it("generates new id when method changes", () => {
		const prior: VerificationObligation[] = [
			{ ...baseNext, obligation_id: "prior-A", method: "coverage_threshold" },
		];
		const result = inheritObligationIds(
			prior,
			[{ ...baseNext, method: "complexity_threshold" }],
			makeIdGen(),
		);
		expect(result[0].obligation_id).toBe("gen-1");
	});

	it("generates new id when target changes", () => {
		const prior: VerificationObligation[] = [
			{ ...baseNext, obligation_id: "prior-A", target: "src" },
		];
		const result = inheritObligationIds(
			prior,
			[{ ...baseNext, target: "src/core" }],
			makeIdGen(),
		);
		expect(result[0].obligation_id).toBe("gen-1");
	});

	it("generates new id when operator changes", () => {
		const prior: VerificationObligation[] = [
			{ ...baseNext, obligation_id: "prior-A", operator: ">=" },
		];
		const result = inheritObligationIds(
			prior,
			[{ ...baseNext, operator: ">" }],
			makeIdGen(),
		);
		expect(result[0].obligation_id).toBe("gen-1");
	});

	it("generates new id when no prior obligations exist", () => {
		const result = inheritObligationIds([], [baseNext], makeIdGen());
		expect(result[0].obligation_id).toBe("gen-1");
	});

	it("handles appending a new obligation to existing set", () => {
		const prior: VerificationObligation[] = [
			{ ...baseNext, obligation_id: "prior-A" },
		];
		const result = inheritObligationIds(
			prior,
			[
				baseNext, // unchanged, inherits prior-A
				{ ...baseNext, threshold: 0.9 }, // new, gets fresh id
			],
			makeIdGen(),
		);
		expect(result).toHaveLength(2);
		expect(result[0].obligation_id).toBe("prior-A");
		expect(result[1].obligation_id).toBe("gen-1");
	});

	it("greedy one-to-one matching: duplicate semantic tuples get distinct ids", () => {
		// If prior has only one obligation but next has two with the same
		// semantic tuple, only the first inherits; the second gets fresh.
		const prior: VerificationObligation[] = [
			{ ...baseNext, obligation_id: "prior-A" },
		];
		const result = inheritObligationIds(
			prior,
			[baseNext, baseNext],
			makeIdGen(),
		);
		expect(result[0].obligation_id).toBe("prior-A");
		expect(result[1].obligation_id).toBe("gen-1");
		expect(result[0].obligation_id).not.toBe(result[1].obligation_id);
	});

	it("reversion does not resurrect id from older version", () => {
		// Simulate a three-step history:
		//   v1: threshold 0.8 (id=A), via first supersession
		//   v2: threshold 0.85, the v1 id does NOT match on tuple mismatch
		//   v3: threshold 0.8 again — matches v2's obligations, not v1's
		// The function only sees the IMMEDIATE prior version, so the
		// v1 id cannot resurrect.
		const v1: VerificationObligation[] = [
			{ ...baseNext, obligation_id: "id-v1", threshold: 0.8 },
		];
		const v2 = inheritObligationIds(
			v1,
			[{ ...baseNext, threshold: 0.85 }],
			makeIdGen(),
		);
		expect(v2[0].obligation_id).toBe("gen-1"); // fresh, threshold differs from v1

		const v3 = inheritObligationIds(
			v2,
			[{ ...baseNext, threshold: 0.8 }],
			() => "gen-v3",
		);
		expect(v3[0].obligation_id).toBe("gen-v3"); // fresh, threshold differs from v2
		expect(v3[0].obligation_id).not.toBe("id-v1"); // did NOT resurrect
	});

	it("does not mutate inputs", () => {
		const prior: VerificationObligation[] = [
			{ ...baseNext, obligation_id: "prior-A" },
		];
		const next = [baseNext];
		const priorCopy = JSON.parse(JSON.stringify(prior));
		const nextCopy = JSON.parse(JSON.stringify(next));
		inheritObligationIds(prior, next, makeIdGen());
		expect(prior).toEqual(priorCopy);
		expect(next).toEqual(nextCopy);
	});
});

describe("parseDeclarationValue REQUIREMENT validation", () => {
	const validRequirement = {
		req_id: "REQ-001",
		version: 1,
		objective: "Test objective",
	};

	it("accepts a requirement with no verification array", () => {
		expect(() =>
			parseDeclarationValue(
				DeclarationKind.REQUIREMENT,
				JSON.stringify(validRequirement),
			),
		).not.toThrow();
	});

	it("accepts a requirement with well-formed verification entries", () => {
		const withVerification = {
			...validRequirement,
			verification: [
				{
					obligation_id: "obl-1",
					obligation: "No violations",
					method: "arch_violations",
					target: "src",
				},
			],
		};
		expect(() =>
			parseDeclarationValue(
				DeclarationKind.REQUIREMENT,
				JSON.stringify(withVerification),
			),
		).not.toThrow();
	});

	it("rejects a verification entry missing obligation_id", () => {
		const missing = {
			...validRequirement,
			verification: [
				{ obligation: "x", method: "arch_violations" },
			],
		};
		expect(() =>
			parseDeclarationValue(
				DeclarationKind.REQUIREMENT,
				JSON.stringify(missing),
			),
		).toThrow(DeclarationValidationError);
	});

	it("rejects a verification entry with empty obligation_id", () => {
		const empty = {
			...validRequirement,
			verification: [
				{ obligation_id: "", obligation: "x", method: "arch_violations" },
			],
		};
		expect(() =>
			parseDeclarationValue(DeclarationKind.REQUIREMENT, JSON.stringify(empty)),
		).toThrow(DeclarationValidationError);
	});

	it("rejects a verification entry missing method", () => {
		const noMethod = {
			...validRequirement,
			verification: [{ obligation_id: "obl-1", obligation: "x" }],
		};
		expect(() =>
			parseDeclarationValue(
				DeclarationKind.REQUIREMENT,
				JSON.stringify(noMethod),
			),
		).toThrow(DeclarationValidationError);
	});

	it("rejects a non-array verification field", () => {
		const notArray = { ...validRequirement, verification: "not an array" };
		expect(() =>
			parseDeclarationValue(
				DeclarationKind.REQUIREMENT,
				JSON.stringify(notArray),
			),
		).toThrow(DeclarationValidationError);
	});
});

describe("parseDeclarationValue WAIVER validation", () => {
	const validWaiver = {
		req_id: "REQ-001",
		requirement_version: 2,
		obligation_id: "obl-uuid",
		reason: "Known limitation, tracked in ADR-003",
		created_at: "2026-04-04T12:00:00Z",
	};

	it("accepts a minimal waiver with required fields only", () => {
		expect(() =>
			parseDeclarationValue(
				DeclarationKind.WAIVER,
				JSON.stringify(validWaiver),
			),
		).not.toThrow();
	});

	it("accepts a waiver with all optional fields", () => {
		const full = {
			...validWaiver,
			created_by: "ops-team",
			expires_at: "2026-07-04T12:00:00Z",
			review_by: "2026-06-01T00:00:00Z",
			rationale_category: "tech_debt",
			policy_basis: "gate-v1",
		};
		expect(() =>
			parseDeclarationValue(DeclarationKind.WAIVER, JSON.stringify(full)),
		).not.toThrow();
	});

	it("rejects a waiver missing req_id", () => {
		const { req_id: _, ...missing } = validWaiver;
		expect(() =>
			parseDeclarationValue(DeclarationKind.WAIVER, JSON.stringify(missing)),
		).toThrow(DeclarationValidationError);
	});

	it("rejects a waiver missing obligation_id", () => {
		const { obligation_id: _, ...missing } = validWaiver;
		expect(() =>
			parseDeclarationValue(DeclarationKind.WAIVER, JSON.stringify(missing)),
		).toThrow(DeclarationValidationError);
	});

	it("rejects a waiver missing reason", () => {
		const { reason: _, ...missing } = validWaiver;
		expect(() =>
			parseDeclarationValue(DeclarationKind.WAIVER, JSON.stringify(missing)),
		).toThrow(DeclarationValidationError);
	});

	it("rejects a waiver missing created_at", () => {
		const { created_at: _, ...missing } = validWaiver;
		expect(() =>
			parseDeclarationValue(DeclarationKind.WAIVER, JSON.stringify(missing)),
		).toThrow(DeclarationValidationError);
	});

	it("rejects a waiver with non-numeric requirement_version", () => {
		const bad = { ...validWaiver, requirement_version: "2" };
		expect(() =>
			parseDeclarationValue(DeclarationKind.WAIVER, JSON.stringify(bad)),
		).toThrow(DeclarationValidationError);
	});

	it("rejects a waiver with malformed created_at", () => {
		const bad = { ...validWaiver, created_at: "not-a-date" };
		expect(() =>
			parseDeclarationValue(DeclarationKind.WAIVER, JSON.stringify(bad)),
		).toThrow(DeclarationValidationError);
	});

	it("rejects a waiver with malformed expires_at", () => {
		const bad = { ...validWaiver, expires_at: "yesterday" };
		expect(() =>
			parseDeclarationValue(DeclarationKind.WAIVER, JSON.stringify(bad)),
		).toThrow(DeclarationValidationError);
	});

	it("rejects a waiver with malformed review_by", () => {
		const bad = { ...validWaiver, review_by: "soon" };
		expect(() =>
			parseDeclarationValue(DeclarationKind.WAIVER, JSON.stringify(bad)),
		).toThrow(DeclarationValidationError);
	});

	it("rejects a waiver with non-string rationale_category", () => {
		const bad = { ...validWaiver, rationale_category: 42 };
		expect(() =>
			parseDeclarationValue(DeclarationKind.WAIVER, JSON.stringify(bad)),
		).toThrow(DeclarationValidationError);
	});
});
