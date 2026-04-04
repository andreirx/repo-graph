/**
 * rgr gate — CI gate command.
 *
 * Evaluates all active obligations in a repo, applies waiver overlays,
 * and reduces the result to a single gate outcome plus process exit
 * code. This is the CI integration surface.
 *
 * Architectural role: orchestration only. The command is a thin layer
 * that composes existing support modules:
 *   1. storage.getActiveDeclarations — load requirements
 *   2. resolveEffectiveVerdict — compute + waiver overlay per obligation
 *   3. reduceToGateOutcome — policy reduction to gate outcome
 *
 * The output JSON preserves BOTH computed_verdict (truth) and
 * effective_verdict (policy-adjusted). The waiver_basis surfaces the
 * full exception audit trail. Exit codes follow the three-state model
 * (see core/gate/reducer.ts).
 */

import { resolve } from "node:path";
import type { Command } from "commander";
import {
	resolveEffectiveVerdict,
	type EffectiveObligationVerdict,
} from "../../core/evaluator/effective-verdict.js";
import {
	type GateMode,
	reduceToGateOutcome,
} from "../../core/gate/reducer.js";
import type { VerificationObligation } from "../../core/model/index.js";
import { DeclarationKind } from "../../core/model/index.js";
import type { AppContext } from "../../main.js";

interface ObligationLine extends EffectiveObligationVerdict {
	req_id: string;
	req_version: number;
}

export function registerGateCommand(
	program: Command,
	getCtx: () => AppContext,
): void {
	program
		.command("gate <repo>")
		.description(
			"Evaluate all obligations and produce a CI gate outcome with exit code",
		)
		.option(
			"--strict",
			"Treat MISSING_EVIDENCE and UNSUPPORTED as fail (exit 1)",
		)
		.option(
			"--advisory",
			"Treat MISSING_EVIDENCE and UNSUPPORTED as informational (exit 0)",
		)
		.option("--json", "JSON output")
		.action(
			(
				repoRef: string,
				opts: { strict?: boolean; advisory?: boolean; json?: boolean },
			) => {
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

				if (opts.strict && opts.advisory) {
					outputError(
						opts.json,
						"--strict and --advisory are mutually exclusive",
					);
					process.exitCode = 1;
					return;
				}

				const mode: GateMode = opts.strict
					? "strict"
					: opts.advisory
						? "advisory"
						: "default";

				const snapshotUid = snapshot.snapshotUid;
				const repoUid = repo.repoUid;

				// Load all active requirements
				const requirements = ctx.storage.getActiveDeclarations({
					repoUid,
					kind: DeclarationKind.REQUIREMENT,
				});

				// Evaluate each obligation with waiver overlay
				const obligations: ObligationLine[] = [];
				for (const reqDecl of requirements) {
					const val = JSON.parse(reqDecl.valueJson) as {
						req_id: string;
						version: number;
						verification?: VerificationObligation[];
					};
					if (!val.verification || val.verification.length === 0) continue;

					for (const obl of val.verification) {
						const effective = resolveEffectiveVerdict(
							obl,
							val.req_id,
							val.version,
							ctx.storage,
							snapshotUid,
							repoUid,
						);
						obligations.push({
							...effective,
							req_id: val.req_id,
							req_version: val.version,
						});
					}
				}

				// Reduce to gate outcome
				const gate = reduceToGateOutcome(
					obligations.map((o) => o.effective_verdict),
					mode,
				);

				// Toolchain metadata for traceability
				const toolchain = snapshot.toolchainJson
					? JSON.parse(snapshot.toolchainJson)
					: null;

				if (opts.json) {
					console.log(
						JSON.stringify(
							{
								command: "gate",
								repo: repo.name,
								snapshot: snapshotUid,
								toolchain,
								obligations: obligations.map((o) => ({
									req_id: o.req_id,
									req_version: o.req_version,
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
								gate,
							},
							null,
							2,
						),
					);
				} else {
					printHuman(obligations, gate, mode);
				}

				// Exit with the gate's exit code
				process.exitCode = gate.exit_code;
			},
		);
}

function printHuman(
	obligations: ObligationLine[],
	gate: ReturnType<typeof reduceToGateOutcome>,
	mode: GateMode,
): void {
	const headerIcon =
		gate.outcome === "pass"
			? "[PASS]"
			: gate.outcome === "fail"
				? "[FAIL]"
				: "[INCOMPLETE]";
	console.log(
		`Gate: ${headerIcon} (${mode} mode, ${gate.counts.total} obligations, exit ${gate.exit_code})`,
	);
	const breakdown = [
		gate.counts.pass > 0 ? `${gate.counts.pass} PASS` : null,
		gate.counts.fail > 0 ? `${gate.counts.fail} FAIL` : null,
		gate.counts.waived > 0 ? `${gate.counts.waived} WAIVED` : null,
		gate.counts.missing_evidence > 0
			? `${gate.counts.missing_evidence} MISSING_EVIDENCE`
			: null,
		gate.counts.unsupported > 0
			? `${gate.counts.unsupported} UNSUPPORTED`
			: null,
	]
		.filter((s): s is string => s !== null)
		.join(", ");
	if (breakdown.length > 0) {
		console.log(`  ${breakdown}`);
	}
	console.log("");

	if (obligations.length === 0) {
		console.log("(no obligations to evaluate)");
		return;
	}

	// Group by requirement
	const byReq = new Map<string, ObligationLine[]>();
	for (const o of obligations) {
		const key = `${o.req_id} v${o.req_version}`;
		const list = byReq.get(key) ?? [];
		list.push(o);
		byReq.set(key, list);
	}

	for (const [reqKey, items] of byReq) {
		console.log(reqKey);
		for (const o of items) {
			const icon = verdictIcon(o.effective_verdict);
			console.log(`  ${icon} ${o.obligation} (${o.method})`);
			if (o.effective_verdict === "WAIVED" && o.waiver_basis !== null) {
				console.log(`       computed: ${o.computed_verdict}`);
				console.log(`       waiver: ${o.waiver_basis.reason}`);
			} else if (
				o.effective_verdict === "FAIL" ||
				o.effective_verdict === "MISSING_EVIDENCE"
			) {
				console.log(`       ${JSON.stringify(o.evidence)}`);
			}
		}
		console.log("");
	}
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

function outputError(json: boolean | undefined, msg: string): void {
	if (json) {
		console.log(JSON.stringify({ error: msg }));
	} else {
		console.error(msg);
	}
}
