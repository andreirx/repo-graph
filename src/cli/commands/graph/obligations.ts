/**
 * Obligation evaluation command.
 * Evaluates verification obligations against current measurements.
 */

import type { Command } from "commander";
import { evaluateObligation } from "../../../core/evaluator/obligation-evaluator.js";
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
			"Evaluate verification obligations against current measurements",
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

			type Verdict = "PASS" | "FAIL" | "MISSING_EVIDENCE" | "UNSUPPORTED";

			interface ObligationResult {
				reqId: string;
				reqVersion: number;
				obligation: string;
				method: string;
				target: string | null;
				threshold: number | null;
				operator: string | null;
				verdict: Verdict;
				evidence: Record<string, unknown>;
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
					const ev = evaluateObligation(obl, ctx.storage, snapshotUid, repoUid);
					results.push({
						reqId: val.req_id,
						reqVersion: val.version,
						obligation: ev.obligation,
						method: ev.method,
						target: ev.target,
						threshold: ev.threshold,
						operator: ev.operator,
						verdict: ev.verdict,
						evidence: ev.evidence,
					});
				}
			}

			// Output
			if (opts.json) {
				const passCount = results.filter((r) => r.verdict === "PASS").length;
				const failCount = results.filter((r) => r.verdict === "FAIL").length;
				console.log(
					JSON.stringify(
						{
							command: "graph obligations",
							repo: snap.repoName,
							snapshot: snapshotUid,
							results: results.map((r) => ({
								req_id: r.reqId,
								req_version: r.reqVersion,
								obligation: r.obligation,
								method: r.method,
								target: r.target,
								threshold: r.threshold,
								verdict: r.verdict,
								evidence: r.evidence,
							})),
							count: results.length,
							pass: passCount,
							fail: failCount,
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
						const icon =
							r.verdict === "PASS"
								? "[PASS]"
								: r.verdict === "FAIL"
									? "[FAIL]"
									: r.verdict === "MISSING_EVIDENCE"
										? "[MISS]"
										: "[SKIP]";
						console.log(`${icon} ${r.reqId} v${r.reqVersion}: ${r.obligation}`);
						if (r.verdict === "FAIL" || r.verdict === "MISSING_EVIDENCE") {
							console.log(`       ${JSON.stringify(r.evidence)}`);
						}
					}
					const passCount = results.filter((r) => r.verdict === "PASS").length;
					const failCount = results.filter((r) => r.verdict === "FAIL").length;
					const missCount = results.filter(
						(r) => r.verdict === "MISSING_EVIDENCE",
					).length;
					console.log("");
					console.log(
						`${results.length} obligations: ${passCount} pass, ${failCount} fail, ${missCount} missing evidence`,
					);
				}
			}
		});
}
