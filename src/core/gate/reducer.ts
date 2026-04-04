/**
 * Gate reducer.
 *
 * Pure function that reduces a list of effective verdicts into a
 * single gate outcome plus process exit code.
 *
 * Architectural principle: this module is a POLICY LAYER on top of
 * verdict truth. The verdicts are never mutated. The CLI reduction
 * does not leak back into the data model. Callers retain the full
 * per-obligation verdict list and use it for reporting, while the
 * gate result governs only process outcome.
 *
 * Gate mode model (per Decision Q3):
 *
 *   default:
 *     exit 0  — all obligations PASS or WAIVED
 *     exit 1  — at least one FAIL
 *     exit 2  — no FAIL, but at least one MISSING_EVIDENCE or UNSUPPORTED
 *
 *   strict:
 *     exit 0  — all obligations PASS or WAIVED
 *     exit 1  — at least one FAIL, MISSING_EVIDENCE, or UNSUPPORTED
 *
 *   advisory:
 *     exit 0  — no FAIL (MISSING/UNSUPPORTED are informational)
 *     exit 1  — at least one FAIL
 *
 * The three-exit model preserves the distinction between:
 *   "we judged and it failed" (exit 1)
 *   "we could not reach a verdict" (exit 2)
 * Exit 2 is actionable for CI (alert evidence pipeline) but not a
 * hard developer block.
 */

import type { EffectiveVerdict } from "../evaluator/effective-verdict.js";

export type GateMode = "default" | "strict" | "advisory";
export type GateOutcome = "pass" | "fail" | "incomplete";

export interface GateCounts {
	total: number;
	pass: number;
	fail: number;
	waived: number;
	missing_evidence: number;
	unsupported: number;
}

export interface GateResult {
	outcome: GateOutcome;
	exit_code: 0 | 1 | 2;
	mode: GateMode;
	counts: GateCounts;
}

/**
 * Reduce a list of effective verdicts to a gate outcome.
 *
 * Empty input: treated as vacuously passing (outcome "pass", exit 0).
 * This is the correct default because a gate with no obligations
 * has nothing to fail. Callers that require at least one obligation
 * must enforce that outside the reducer.
 *
 * Input verdicts are read-only. This function does not mutate.
 */
export function reduceToGateOutcome(
	verdicts: EffectiveVerdict[],
	mode: GateMode = "default",
): GateResult {
	const counts: GateCounts = {
		total: verdicts.length,
		pass: 0,
		fail: 0,
		waived: 0,
		missing_evidence: 0,
		unsupported: 0,
	};
	for (const v of verdicts) {
		switch (v) {
			case "PASS":
				counts.pass++;
				break;
			case "FAIL":
				counts.fail++;
				break;
			case "WAIVED":
				counts.waived++;
				break;
			case "MISSING_EVIDENCE":
				counts.missing_evidence++;
				break;
			case "UNSUPPORTED":
				counts.unsupported++;
				break;
		}
	}

	const hasFail = counts.fail > 0;
	const hasIncomplete = counts.missing_evidence > 0 || counts.unsupported > 0;

	let outcome: GateOutcome;
	let exitCode: 0 | 1 | 2;

	switch (mode) {
		case "strict":
			if (hasFail || hasIncomplete) {
				outcome = "fail";
				exitCode = 1;
			} else {
				outcome = "pass";
				exitCode = 0;
			}
			break;

		case "advisory":
			if (hasFail) {
				outcome = "fail";
				exitCode = 1;
			} else {
				// MISSING and UNSUPPORTED are informational in advisory mode
				outcome = "pass";
				exitCode = 0;
			}
			break;

		default: // "default" mode
			if (hasFail) {
				outcome = "fail";
				exitCode = 1;
			} else if (hasIncomplete) {
				outcome = "incomplete";
				exitCode = 2;
			} else {
				outcome = "pass";
				exitCode = 0;
			}
			break;
	}

	return { outcome, exit_code: exitCode, mode, counts };
}
