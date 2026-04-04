/**
 * graph evidence — verification status report.
 *
 * Produces a snapshot-level view of all requirements, their obligations,
 * computed verdicts, waiver overlays, and effective verdicts. This is
 * a Tier 1 read surface: a coherent verification report that makes the
 * requirement-to-evidence closure loop visible, consistent with `rgr gate`.
 *
 * Recomputes verdicts on demand (does not read stored inferences) and
 * applies active waivers via resolveEffectiveVerdict(). The JSON output
 * surfaces both computed_verdict (truth) and effective_verdict (policy-
 * adjusted) for every obligation, plus waiver_basis when applicable.
 *
 * Requirement status is derived from EFFECTIVE verdicts:
 *   SATISFIED     — all obligations PASS or WAIVED
 *   INCOMPLETE    — at least one effective FAIL
 *   PARTIAL       — no FAIL, but at least one MISSING_EVIDENCE
 *   UNVERIFIABLE  — no FAIL/MISSING, but at least one UNSUPPORTED
 *   NO OBLIGATIONS — no verification entries
 *
 * Includes snapshot and toolchain metadata for traceability.
 */

import { resolve } from "node:path";
import type { Command } from "commander";
import {
	type EffectiveObligationVerdict,
	resolveEffectiveVerdict,
} from "../../core/evaluator/effective-verdict.js";
import type { VerificationObligation } from "../../core/model/index.js";
import { DeclarationKind } from "../../core/model/index.js";
import type { AppContext } from "../../main.js";

interface RequirementEvidence {
	reqId: string;
	version: number;
	objective: string;
	target: string;
	status: string;
	obligations: EffectiveObligationVerdict[];
	summary: {
		total: number;
		pass: number;
		fail: number;
		waived: number;
		missing_evidence: number;
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
			"Verification status report: requirements, obligations, verdicts, waivers, evidence",
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

			// Evaluate each requirement's obligations with waiver overlay
			const report: RequirementEvidence[] = [];

			for (const req of requirements) {
				const target = req.decl.targetStableKey
					.replace(`${repoUid}:`, "")
					.replace(/:MODULE$/, "");

				const obligations: EffectiveObligationVerdict[] = [];

				for (const obl of req.value.verification ?? []) {
					const effective = resolveEffectiveVerdict(
						obl,
						req.value.req_id,
						req.value.version,
						ctx.storage,
						snapshotUid,
						repoUid,
					);
					obligations.push(effective);
				}

				// Summary counts: ALL against EFFECTIVE verdicts
				const pass = obligations.filter(
					(o) => o.effective_verdict === "PASS",
				).length;
				const fail = obligations.filter(
					(o) => o.effective_verdict === "FAIL",
				).length;
				const waived = obligations.filter(
					(o) => o.effective_verdict === "WAIVED",
				).length;
				const missing_evidence = obligations.filter(
					(o) => o.effective_verdict === "MISSING_EVIDENCE",
				).length;
				const unsupported = obligations.filter(
					(o) => o.effective_verdict === "UNSUPPORTED",
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
						waived,
						missing_evidence,
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
									obligation_id: o.obligation_id,
									obligation: o.obligation,
									method: o.method,
									target: o.target,
									threshold: o.threshold,
									operator: o.operator,
									computed_verdict: o.computed_verdict,
									effective_verdict: o.effective_verdict,
									evidence: o.evidence,
									waiver_basis: o.waiver_basis,
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
								waived: report.reduce((s, r) => s + r.summary.waived, 0),
								missing_evidence: report.reduce(
									(s, r) => s + r.summary.missing_evidence,
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
						const icon = verdictIcon(obl.effective_verdict);
						console.log(`  ${icon} ${obl.obligation}`);
						if (
							obl.effective_verdict === "FAIL" ||
							obl.effective_verdict === "MISSING_EVIDENCE"
						) {
							console.log(`         ${JSON.stringify(obl.evidence)}`);
						}
						if (
							obl.effective_verdict === "WAIVED" &&
							obl.waiver_basis !== null
						) {
							console.log(`         computed: ${obl.computed_verdict}`);
							console.log(`         waiver: ${obl.waiver_basis.reason}`);
						}
					}
					console.log("");
				}

				const totalObl = report.reduce((s, r) => s + r.summary.total, 0);
				const totalPass = report.reduce((s, r) => s + r.summary.pass, 0);
				const totalFail = report.reduce((s, r) => s + r.summary.fail, 0);
				const totalWaived = report.reduce((s, r) => s + r.summary.waived, 0);
				const totalMiss = report.reduce(
					(s, r) => s + r.summary.missing_evidence,
					0,
				);
				console.log(
					`${report.length} requirements, ${totalObl} obligations: ${totalPass} pass, ${totalFail} fail, ${totalWaived} waived, ${totalMiss} missing evidence`,
				);
			}
		});
}

function computeRequirementStatus(
	summary: RequirementEvidence["summary"],
): string {
	// Status derived from EFFECTIVE verdicts. Waived counts as satisfied
	// for status purposes (the organization has accepted the exception);
	// the waived count is visible separately in the summary.
	if (summary.fail > 0) return "INCOMPLETE";
	if (summary.missing_evidence > 0) return "PARTIAL";
	if (summary.unsupported > 0) return "UNVERIFIABLE";
	if (summary.total === 0) return "NO OBLIGATIONS";
	return "SATISFIED";
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

function outputError(json: boolean | undefined, message: string): void {
	if (json) {
		console.log(JSON.stringify({ error: message }));
	} else {
		console.error(`Error: ${message}`);
	}
}
