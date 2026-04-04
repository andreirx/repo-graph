/**
 * Gate reducer tests.
 *
 * The reducer is a pure function. Tests enumerate verdict combinations
 * against each mode to verify the policy matrix is stable.
 *
 * Mode matrix (see reducer.ts docs):
 *   default:  exit 0 = {PASS, WAIVED}; exit 1 on FAIL; exit 2 on incomplete
 *   strict:   exit 0 = {PASS, WAIVED}; exit 1 on FAIL or incomplete
 *   advisory: exit 0 = anything except FAIL; exit 1 on FAIL
 */

import { describe, expect, it } from "vitest";
import type { EffectiveVerdict } from "../../../src/core/evaluator/effective-verdict.js";
import {
	type GateMode,
	reduceToGateOutcome,
} from "../../../src/core/gate/reducer.js";

const ALL_PASS: EffectiveVerdict[] = ["PASS", "PASS"];
const PASS_AND_WAIVED: EffectiveVerdict[] = ["PASS", "WAIVED"];
const WITH_FAIL: EffectiveVerdict[] = ["PASS", "FAIL", "WAIVED"];
const WITH_MISSING: EffectiveVerdict[] = ["PASS", "MISSING_EVIDENCE"];
const WITH_UNSUPPORTED: EffectiveVerdict[] = ["PASS", "UNSUPPORTED"];
const FAIL_AND_MISSING: EffectiveVerdict[] = [
	"FAIL",
	"MISSING_EVIDENCE",
];

describe("reduceToGateOutcome — default mode", () => {
	it("empty verdicts pass vacuously", () => {
		const r = reduceToGateOutcome([], "default");
		expect(r.outcome).toBe("pass");
		expect(r.exit_code).toBe(0);
		expect(r.counts.total).toBe(0);
	});

	it("all PASS: pass, exit 0", () => {
		const r = reduceToGateOutcome(ALL_PASS, "default");
		expect(r.outcome).toBe("pass");
		expect(r.exit_code).toBe(0);
	});

	it("PASS + WAIVED: pass, exit 0", () => {
		const r = reduceToGateOutcome(PASS_AND_WAIVED, "default");
		expect(r.outcome).toBe("pass");
		expect(r.exit_code).toBe(0);
	});

	it("any FAIL: fail, exit 1", () => {
		const r = reduceToGateOutcome(WITH_FAIL, "default");
		expect(r.outcome).toBe("fail");
		expect(r.exit_code).toBe(1);
	});

	it("MISSING_EVIDENCE but no FAIL: incomplete, exit 2", () => {
		const r = reduceToGateOutcome(WITH_MISSING, "default");
		expect(r.outcome).toBe("incomplete");
		expect(r.exit_code).toBe(2);
	});

	it("UNSUPPORTED but no FAIL: incomplete, exit 2", () => {
		const r = reduceToGateOutcome(WITH_UNSUPPORTED, "default");
		expect(r.outcome).toBe("incomplete");
		expect(r.exit_code).toBe(2);
	});

	it("FAIL + MISSING: fail (FAIL takes precedence), exit 1", () => {
		const r = reduceToGateOutcome(FAIL_AND_MISSING, "default");
		expect(r.outcome).toBe("fail");
		expect(r.exit_code).toBe(1);
	});
});

describe("reduceToGateOutcome — strict mode", () => {
	it("empty: pass, exit 0", () => {
		const r = reduceToGateOutcome([], "strict");
		expect(r.outcome).toBe("pass");
		expect(r.exit_code).toBe(0);
	});

	it("all PASS: pass, exit 0", () => {
		const r = reduceToGateOutcome(ALL_PASS, "strict");
		expect(r.outcome).toBe("pass");
		expect(r.exit_code).toBe(0);
	});

	it("PASS + WAIVED: pass, exit 0", () => {
		const r = reduceToGateOutcome(PASS_AND_WAIVED, "strict");
		expect(r.outcome).toBe("pass");
		expect(r.exit_code).toBe(0);
	});

	it("FAIL: fail, exit 1", () => {
		const r = reduceToGateOutcome(WITH_FAIL, "strict");
		expect(r.outcome).toBe("fail");
		expect(r.exit_code).toBe(1);
	});

	it("MISSING_EVIDENCE escalates to fail, exit 1", () => {
		const r = reduceToGateOutcome(WITH_MISSING, "strict");
		expect(r.outcome).toBe("fail");
		expect(r.exit_code).toBe(1);
	});

	it("UNSUPPORTED escalates to fail, exit 1", () => {
		const r = reduceToGateOutcome(WITH_UNSUPPORTED, "strict");
		expect(r.outcome).toBe("fail");
		expect(r.exit_code).toBe(1);
	});
});

describe("reduceToGateOutcome — advisory mode", () => {
	it("empty: pass, exit 0", () => {
		const r = reduceToGateOutcome([], "advisory");
		expect(r.outcome).toBe("pass");
		expect(r.exit_code).toBe(0);
	});

	it("FAIL: fail, exit 1", () => {
		const r = reduceToGateOutcome(WITH_FAIL, "advisory");
		expect(r.outcome).toBe("fail");
		expect(r.exit_code).toBe(1);
	});

	it("MISSING_EVIDENCE is informational: pass, exit 0", () => {
		const r = reduceToGateOutcome(WITH_MISSING, "advisory");
		expect(r.outcome).toBe("pass");
		expect(r.exit_code).toBe(0);
	});

	it("UNSUPPORTED is informational: pass, exit 0", () => {
		const r = reduceToGateOutcome(WITH_UNSUPPORTED, "advisory");
		expect(r.outcome).toBe("pass");
		expect(r.exit_code).toBe(0);
	});

	it("all unfit non-FAIL: pass, exit 0", () => {
		const r = reduceToGateOutcome(
			["WAIVED", "MISSING_EVIDENCE", "UNSUPPORTED"],
			"advisory",
		);
		expect(r.outcome).toBe("pass");
		expect(r.exit_code).toBe(0);
	});
});

describe("reduceToGateOutcome — counts", () => {
	it("counts every verdict kind accurately", () => {
		const verdicts: EffectiveVerdict[] = [
			"PASS",
			"PASS",
			"FAIL",
			"WAIVED",
			"MISSING_EVIDENCE",
			"MISSING_EVIDENCE",
			"UNSUPPORTED",
		];
		const r = reduceToGateOutcome(verdicts, "default");
		expect(r.counts).toEqual({
			total: 7,
			pass: 2,
			fail: 1,
			waived: 1,
			missing_evidence: 2,
			unsupported: 1,
		});
	});

	it("records the mode in the result", () => {
		const modes: GateMode[] = ["default", "strict", "advisory"];
		for (const m of modes) {
			const r = reduceToGateOutcome(["PASS"], m);
			expect(r.mode).toBe(m);
		}
	});
});

describe("reduceToGateOutcome — input immutability", () => {
	it("does not mutate the input array", () => {
		const verdicts: EffectiveVerdict[] = ["PASS", "FAIL", "WAIVED"];
		const snapshot = [...verdicts];
		reduceToGateOutcome(verdicts, "default");
		expect(verdicts).toEqual(snapshot);
	});
});
