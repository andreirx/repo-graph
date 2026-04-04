/**
 * Shared obligation evaluator.
 *
 * Single source of truth for evaluating verification obligations
 * against measurements, declarations, and inferences. Used by both
 * `graph obligations` and `tool evidence`.
 *
 * This module lives in core/ because it operates on domain types
 * (VerificationObligation, StoragePort) without depending on CLI
 * or adapter internals.
 */

import type {
	DeclarationKind,
	VerificationObligation,
} from "../model/index.js";
import type { StoragePort } from "../ports/storage.js";

export type Verdict = "PASS" | "FAIL" | "MISSING_EVIDENCE" | "UNSUPPORTED";

export interface ObligationVerdict {
	/** Stable identity of the evaluated obligation. Used by waivers. */
	obligation_id: string;
	obligation: string;
	method: string;
	target: string | null;
	threshold: number | null;
	operator: string | null;
	verdict: Verdict;
	evidence: Record<string, unknown>;
}

/**
 * Evaluate a single verification obligation against current
 * measurements, declarations, and inferences.
 *
 * Supported methods:
 *   arch_violations       — boundary violations = 0
 *   coverage_threshold    — avg line coverage vs threshold
 *   complexity_threshold  — max CC vs threshold
 *   hotspot_threshold     — max hotspot score vs threshold
 *
 * All other methods return UNSUPPORTED.
 */
export function evaluateObligation(
	obl: VerificationObligation,
	storage: StoragePort,
	snapshotUid: string,
	repoUid: string,
): ObligationVerdict {
	const base: ObligationVerdict = {
		obligation_id: obl.obligation_id,
		obligation: obl.obligation,
		method: obl.method,
		target: obl.target ?? null,
		threshold: obl.threshold ?? null,
		operator: obl.operator ?? null,
		verdict: "UNSUPPORTED",
		evidence: {},
	};

	switch (obl.method) {
		case "arch_violations": {
			if (!obl.target) {
				base.verdict = "MISSING_EVIDENCE";
				base.evidence = { reason: "no target specified" };
				break;
			}
			const boundaries = storage.getActiveDeclarations({
				repoUid,
				kind: "boundary" as DeclarationKind,
				targetStableKey: `${repoUid}:${obl.target}:MODULE`,
			});
			if (boundaries.length === 0) {
				base.verdict = "MISSING_EVIDENCE";
				base.evidence = {
					reason: "no boundary declarations for target",
				};
				break;
			}
			let totalViolations = 0;
			for (const bd of boundaries) {
				const bv = JSON.parse(bd.valueJson) as { forbids: string };
				const violations = storage.findImportsBetweenPaths({
					snapshotUid,
					sourcePrefix: obl.target,
					targetPrefix: bv.forbids,
				});
				totalViolations += violations.length;
			}
			base.verdict = totalViolations === 0 ? "PASS" : "FAIL";
			base.evidence = {
				violation_count: totalViolations,
				snapshot: snapshotUid,
			};
			break;
		}
		case "coverage_threshold": {
			if (!obl.target || obl.threshold === undefined) {
				base.verdict = "MISSING_EVIDENCE";
				base.evidence = {
					reason: "target or threshold not specified",
				};
				break;
			}
			const coverageRows = storage.queryMeasurementsByKind(
				snapshotUid,
				"line_coverage",
			);
			const prefix = `${repoUid}:${obl.target}/`;
			const matching = coverageRows.filter((r) =>
				r.targetStableKey.startsWith(prefix),
			);
			if (matching.length === 0) {
				base.verdict = "MISSING_EVIDENCE";
				base.evidence = {
					reason: "no coverage data for target path",
				};
				break;
			}
			const avg =
				matching.reduce((sum, r) => {
					const v = JSON.parse(r.valueJson) as { value: number };
					return sum + v.value;
				}, 0) / matching.length;
			const op = obl.operator ?? ">=";
			const pass = compareValues(avg, op, obl.threshold);
			base.verdict = pass ? "PASS" : "FAIL";
			base.evidence = {
				avg_coverage: Math.round(avg * 10000) / 10000,
				threshold: obl.threshold,
				operator: op,
				files_measured: matching.length,
			};
			break;
		}
		case "complexity_threshold": {
			if (!obl.target || obl.threshold === undefined) {
				base.verdict = "MISSING_EVIDENCE";
				base.evidence = {
					reason: "target or threshold not specified",
				};
				break;
			}
			const ccRows = storage.queryMeasurementsByKind(
				snapshotUid,
				"cyclomatic_complexity",
			);
			const ccPrefix = `${repoUid}:${obl.target}/`;
			const matchingCC = ccRows.filter((r) =>
				r.targetStableKey.includes(ccPrefix),
			);
			if (matchingCC.length === 0) {
				base.verdict = "MISSING_EVIDENCE";
				base.evidence = {
					reason: "no complexity data for target path",
				};
				break;
			}
			const maxCC = Math.max(
				...matchingCC.map((r) => {
					const v = JSON.parse(r.valueJson) as { value: number };
					return v.value;
				}),
			);
			const ccOp = obl.operator ?? "<=";
			base.verdict = compareValues(maxCC, ccOp, obl.threshold)
				? "PASS"
				: "FAIL";
			base.evidence = {
				max_complexity: maxCC,
				threshold: obl.threshold,
				operator: ccOp,
				functions_measured: matchingCC.length,
			};
			break;
		}
		case "hotspot_threshold": {
			if (obl.threshold === undefined) {
				base.verdict = "MISSING_EVIDENCE";
				base.evidence = { reason: "threshold not specified" };
				break;
			}
			let hotspots = storage.queryInferences(snapshotUid, "hotspot_score");
			if (obl.target) {
				const hsPrefix = `${repoUid}:${obl.target}/`;
				hotspots = hotspots.filter((h) =>
					h.targetStableKey.startsWith(hsPrefix),
				);
			}
			if (hotspots.length === 0) {
				base.verdict = "MISSING_EVIDENCE";
				base.evidence = {
					reason: obl.target
						? "no hotspot data for target path"
						: "no hotspot data",
				};
				break;
			}
			const maxHS = Math.max(
				...hotspots.map((h) => {
					const v = JSON.parse(h.valueJson) as {
						normalized_score: number;
					};
					return v.normalized_score;
				}),
			);
			const hsOp = obl.operator ?? "<=";
			base.verdict = compareValues(maxHS, hsOp, obl.threshold)
				? "PASS"
				: "FAIL";
			base.evidence = {
				max_hotspot_score: maxHS,
				threshold: obl.threshold,
			};
			break;
		}
		default:
			base.verdict = "UNSUPPORTED";
			base.evidence = {
				reason: `method "${obl.method}" not yet supported`,
			};
	}

	return base;
}

/**
 * Compare a value against a threshold using the specified operator.
 */
export function compareValues(
	value: number,
	op: string,
	threshold: number,
): boolean {
	switch (op) {
		case ">=":
			return value >= threshold;
		case ">":
			return value > threshold;
		case "<=":
			return value <= threshold;
		case "<":
			return value < threshold;
		case "==":
			return value === threshold;
		default:
			return false;
	}
}
