/**
 * graph evidence — verification status report.
 *
 * Produces a snapshot-level view of all requirements, their obligations,
 * verdicts, and the evidence basis for each verdict. This is the first
 * Tier 1 read surface: a coherent verification report that makes the
 * requirement-to-evidence closure loop visible.
 *
 * Recomputes verdicts on demand (does not read stored inferences).
 * Includes snapshot and toolchain metadata for traceability.
 */

import { resolve } from "node:path";
import type { Command } from "commander";
import {
	evaluateObligation,
	type ObligationVerdict,
} from "../../core/evaluator/obligation-evaluator.js";
import type { VerificationObligation } from "../../core/model/index.js";
import { DeclarationKind } from "../../core/model/index.js";
import type { AppContext } from "../../main.js";

interface RequirementEvidence {
	reqId: string;
	version: number;
	objective: string;
	target: string;
	status: string;
	obligations: ObligationVerdict[];
	summary: {
		total: number;
		pass: number;
		fail: number;
		missing: number;
		unsupported: number;
	};
}

export function registerEvidenceCommand(
	program: Command,
	getCtx: () => AppContext,
): void {
	program
		.command("evidence <repo>")
		.description(
			"Verification status report: requirements, obligations, verdicts, evidence",
		)
		.option("--req-id <id>", "Filter to a specific requirement")
		.option("--json", "JSON output")
		.action((repoRef: string, opts: { reqId?: string; json?: boolean }) => {
			const ctx = getCtx();
			const repo =
				ctx.storage.getRepo({ uid: repoRef }) ??
				ctx.storage.getRepo({ name: repoRef }) ??
				ctx.storage.getRepo({ rootPath: resolve(repoRef) });

			if (!repo) {
				outputError(opts.json, `Repository not found: ${repoRef}`);
				process.exitCode = 1;
				return;
			}

			const snapshot = ctx.storage.getLatestSnapshot(repo.repoUid);
			if (!snapshot) {
				outputError(opts.json, `Repository not indexed: ${repoRef}`);
				process.exitCode = 1;
				return;
			}

			const snapshotUid = snapshot.snapshotUid;
			const repoUid = repo.repoUid;

			// Load requirements
			let requirements = ctx.storage
				.getActiveDeclarations({
					repoUid,
					kind: DeclarationKind.REQUIREMENT,
				})
				.map((d) => ({
					decl: d,
					value: JSON.parse(d.valueJson) as {
						req_id: string;
						version: number;
						objective: string;
						status?: string;
						verification?: VerificationObligation[];
					},
				}));

			if (opts.reqId) {
				requirements = requirements.filter(
					(r) => r.value.req_id === opts.reqId,
				);
			}

			// Evaluate each requirement's obligations
			const report: RequirementEvidence[] = [];

			for (const req of requirements) {
				const target = req.decl.targetStableKey
					.replace(`${repoUid}:`, "")
					.replace(/:MODULE$/, "");

				const obligations: ObligationVerdict[] = [];

				for (const obl of req.value.verification ?? []) {
					const verdict = evaluateObligation(
						obl,
						ctx.storage,
						snapshotUid,
						repoUid,
					);
					obligations.push(verdict);
				}

				const pass = obligations.filter((o) => o.verdict === "PASS").length;
				const fail = obligations.filter((o) => o.verdict === "FAIL").length;
				const missing = obligations.filter(
					(o) => o.verdict === "MISSING_EVIDENCE",
				).length;
				const unsupported = obligations.filter(
					(o) => o.verdict === "UNSUPPORTED",
				).length;

				report.push({
					reqId: req.value.req_id,
					version: req.value.version,
					objective: req.value.objective,
					target,
					status: req.value.status ?? "active",
					obligations,
					summary: {
						total: obligations.length,
						pass,
						fail,
						missing,
						unsupported,
					},
				});
			}

			// Toolchain metadata
			const toolchain = snapshot.toolchainJson
				? JSON.parse(snapshot.toolchainJson)
				: null;

			if (opts.json) {
				console.log(
					JSON.stringify(
						{
							command: "graph evidence",
							repo: repo.name,
							snapshot: snapshotUid,
							toolchain,
							requirements: report.map((r) => ({
								req_id: r.reqId,
								version: r.version,
								objective: r.objective,
								target: r.target,
								status: r.status,
								verification_status: computeRequirementStatus(r.summary),
								obligations: r.obligations.map((o) => ({
									obligation: o.obligation,
									method: o.method,
									target: o.target,
									threshold: o.threshold,
									verdict: o.verdict,
									evidence: o.evidence,
								})),
								summary: r.summary,
							})),
							summary: {
								requirements: report.length,
								total_obligations: report.reduce(
									(s, r) => s + r.summary.total,
									0,
								),
								pass: report.reduce((s, r) => s + r.summary.pass, 0),
								fail: report.reduce((s, r) => s + r.summary.fail, 0),
								missing_evidence: report.reduce(
									(s, r) => s + r.summary.missing,
									0,
								),
								unsupported: report.reduce(
									(s, r) => s + r.summary.unsupported,
									0,
								),
							},
						},
						null,
						2,
					),
				);
			} else {
				if (report.length === 0) {
					console.log(
						"No requirements found. Use `rgr declare requirement` to create one.",
					);
					return;
				}

				console.log(
					`Verification Report — ${repo.name} (${snapshotUid.split("/")[0]})`,
				);
				console.log(`Snapshot: ${snapshotUid}`);
				console.log("");

				for (const req of report) {
					const reqStatus = computeRequirementStatus(req.summary);

					console.log(
						`${req.reqId} v${req.version} [${reqStatus}]: ${req.objective}`,
					);
					console.log(`  Target: ${req.target}`);

					if (req.obligations.length === 0) {
						console.log("  (no verification obligations defined)");
					}

					for (const obl of req.obligations) {
						const icon =
							obl.verdict === "PASS"
								? "[PASS]"
								: obl.verdict === "FAIL"
									? "[FAIL]"
									: obl.verdict === "MISSING_EVIDENCE"
										? "[MISS]"
										: "[SKIP]";
						console.log(`  ${icon} ${obl.obligation}`);
						if (obl.verdict === "FAIL" || obl.verdict === "MISSING_EVIDENCE") {
							console.log(`         ${JSON.stringify(obl.evidence)}`);
						}
					}
					console.log("");
				}

				const totalObl = report.reduce((s, r) => s + r.summary.total, 0);
				const totalPass = report.reduce((s, r) => s + r.summary.pass, 0);
				const totalFail = report.reduce((s, r) => s + r.summary.fail, 0);
				const totalMiss = report.reduce((s, r) => s + r.summary.missing, 0);
				console.log(
					`${report.length} requirements, ${totalObl} obligations: ${totalPass} pass, ${totalFail} fail, ${totalMiss} missing evidence`,
				);
			}
		});
}

function computeRequirementStatus(
	summary: RequirementEvidence["summary"],
): string {
	if (summary.fail > 0) return "INCOMPLETE";
	if (summary.missing > 0) return "PARTIAL";
	if (summary.unsupported > 0) return "UNVERIFIABLE";
	if (summary.total === 0) return "NO OBLIGATIONS";
	return "SATISFIED";
}

function outputError(json: boolean | undefined, message: string): void {
	if (json) {
		console.log(JSON.stringify({ error: message }));
	} else {
		console.error(`Error: ${message}`);
	}
}
