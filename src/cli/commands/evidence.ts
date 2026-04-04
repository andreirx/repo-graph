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
import type { VerificationObligation } from "../../core/model/index.js";
import { DeclarationKind } from "../../core/model/index.js";
import type { AppContext } from "../../main.js";

type Verdict = "PASS" | "FAIL" | "MISSING_EVIDENCE" | "UNSUPPORTED";

interface ObligationVerdict {
	obligation: string;
	method: string;
	target: string | null;
	threshold: number | null;
	operator: string | null;
	verdict: Verdict;
	evidence: Record<string, unknown>;
}

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
					const verdict = evaluateObligation(obl, ctx, snapshotUid, repoUid);
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

// ── Obligation evaluator ──────────────────────────────────────────────

function evaluateObligation(
	obl: VerificationObligation,
	ctx: AppContext,
	snapshotUid: string,
	repoUid: string,
): ObligationVerdict {
	const base: ObligationVerdict = {
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
			const boundaries = ctx.storage.getActiveDeclarations({
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
				const violations = ctx.storage.findImportsBetweenPaths({
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
			const coverageRows = ctx.storage.queryMeasurementsByKind(
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
			const pass = compare(avg, op, obl.threshold);
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
			const ccRows = ctx.storage.queryMeasurementsByKind(
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
			base.verdict = compare(maxCC, ccOp, obl.threshold) ? "PASS" : "FAIL";
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
			let hotspots = ctx.storage.queryInferences(snapshotUid, "hotspot_score");
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
			base.verdict = compare(maxHS, hsOp, obl.threshold) ? "PASS" : "FAIL";
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

function compare(value: number, op: string, threshold: number): boolean {
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

function outputError(json: boolean | undefined, message: string): void {
	if (json) {
		console.log(JSON.stringify({ error: message }));
	} else {
		console.error(`Error: ${message}`);
	}
}
