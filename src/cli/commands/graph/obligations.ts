/**
 * Obligation evaluation command.
 *
 * Evaluates verification obligations against current measurements,
 * then applies active waivers to produce effective verdicts. Both
 * computed_verdict (truth) and effective_verdict (policy-adjusted)
 * are surfaced. This keeps `graph obligations` consistent with the
 * `gate` command's verdict semantics.
 */

import type { Command } from "commander";
import {
	resolveEffectiveVerdict,
	type WaiverBasis,
} from "../../../core/evaluator/effective-verdict.js";
import type { Verdict } from "../../../core/evaluator/obligation-evaluator.js";
import type { VerificationObligation } from "../../../core/model/index.js";
import { DeclarationKind } from "../../../core/model/index.js";
import type { AppContext } from "../../../main.js";
import { outputError, resolveSnapshot } from "./helpers.js";

export function registerObligationCommands(
	graph: Command,
	getCtx: () => AppContext,
): void {
	graph
		.command("obligations <repo>")
		.description(
			"Evaluate verification obligations against current measurements and active waivers",
		)
		.option("--json", "JSON output")
		.action((repoRef: string, opts: { json?: boolean }) => {
			const ctx = getCtx();
			const snap = resolveSnapshot(ctx, repoRef);
			const { snapshotUid, repoUid } = snap;
			if (!snapshotUid || !repoUid) {
				outputError(
					opts.json,
					`Repository not found or not indexed: ${repoRef}`,
				);
				process.exitCode = 1;
				return;
			}

			// Load all active requirements
			const requirements = ctx.storage.getActiveDeclarations({
				repoUid,
				kind: DeclarationKind.REQUIREMENT,
			});

			if (requirements.length === 0) {
				if (opts.json) {
					console.log(
						JSON.stringify(
							{
								command: "graph obligations",
								repo: snap.repoName,
								snapshot: snapshotUid,
								results: [],
								count: 0,
							},
							null,
							2,
						),
					);
				} else {
					console.log(
						"No requirements declared. Use `declare requirement` to create one.",
					);
				}
				return;
			}

			interface ObligationResult {
				reqId: string;
				reqVersion: number;
				obligationId: string;
				obligation: string;
				method: string;
				target: string | null;
				threshold: number | null;
				operator: string | null;
				computedVerdict: Verdict;
				effectiveVerdict: Verdict | "WAIVED";
				evidence: Record<string, unknown>;
				waiverBasis: WaiverBasis | null;
			}

			const results: ObligationResult[] = [];

			for (const reqDecl of requirements) {
				const val = JSON.parse(reqDecl.valueJson) as {
					req_id: string;
					version: number;
					verification?: VerificationObligation[];
				};

				if (!val.verification || val.verification.length === 0) continue;

				for (const obl of val.verification) {
					const ev = resolveEffectiveVerdict(
						obl,
						val.req_id,
						val.version,
						ctx.storage,
						snapshotUid,
						repoUid,
					);
					results.push({
						reqId: val.req_id,
						reqVersion: val.version,
						obligationId: ev.obligation_id,
						obligation: ev.obligation,
						method: ev.method,
						target: ev.target,
						threshold: ev.threshold,
						operator: ev.operator,
						computedVerdict: ev.computed_verdict,
						effectiveVerdict: ev.effective_verdict,
						evidence: ev.evidence,
						waiverBasis: ev.waiver_basis,
					});
				}
			}

			// Counts are against EFFECTIVE verdicts (policy-adjusted)
			const passCount = results.filter(
				(r) => r.effectiveVerdict === "PASS",
			).length;
			const failCount = results.filter(
				(r) => r.effectiveVerdict === "FAIL",
			).length;
			const waivedCount = results.filter(
				(r) => r.effectiveVerdict === "WAIVED",
			).length;
			const missingCount = results.filter(
				(r) => r.effectiveVerdict === "MISSING_EVIDENCE",
			).length;
			const unsupportedCount = results.filter(
				(r) => r.effectiveVerdict === "UNSUPPORTED",
			).length;

			// Output
			if (opts.json) {
				console.log(
					JSON.stringify(
						{
							command: "graph obligations",
							repo: snap.repoName,
							snapshot: snapshotUid,
							results: results.map((r) => ({
								req_id: r.reqId,
								req_version: r.reqVersion,
								obligation_id: r.obligationId,
								obligation: r.obligation,
								method: r.method,
								target: r.target,
								threshold: r.threshold,
								operator: r.operator,
								computed_verdict: r.computedVerdict,
								effective_verdict: r.effectiveVerdict,
								evidence: r.evidence,
								waiver_basis: r.waiverBasis,
							})),
							count: results.length,
							pass: passCount,
							fail: failCount,
							waived: waivedCount,
							missing_evidence: missingCount,
							unsupported: unsupportedCount,
						},
						null,
						2,
					),
				);
			} else {
				if (results.length === 0) {
					console.log("No verification obligations found in any requirement.");
				} else {
					for (const r of results) {
						const icon = verdictIcon(r.effectiveVerdict);
						console.log(
							`${icon} ${r.reqId} v${r.reqVersion}: ${r.obligation}`,
						);
						if (
							r.effectiveVerdict === "FAIL" ||
							r.effectiveVerdict === "MISSING_EVIDENCE"
						) {
							console.log(`       ${JSON.stringify(r.evidence)}`);
						}
						if (r.effectiveVerdict === "WAIVED" && r.waiverBasis !== null) {
							console.log(`       computed: ${r.computedVerdict}`);
							console.log(`       waiver: ${r.waiverBasis.reason}`);
						}
					}
					console.log("");
					console.log(
						`${results.length} obligations: ${passCount} pass, ${failCount} fail, ${waivedCount} waived, ${missingCount} missing evidence, ${unsupportedCount} unsupported`,
					);
				}
			}
		});
}

function verdictIcon(v: string): string {
	switch (v) {
		case "PASS":
			return "[PASS]";
		case "FAIL":
			return "[FAIL]";
		case "WAIVED":
			return "[WAIVED]";
		case "MISSING_EVIDENCE":
			return "[MISS]";
		case "UNSUPPORTED":
			return "[SKIP]";
		default:
			return "[????]";
	}
}
