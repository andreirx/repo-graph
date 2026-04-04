/**
 * rgr trust — extraction trust reporting.
 *
 * Thin CLI layer over computeTrustReport(). Emits JSON with the full
 * report envelope, or a human-readable summary.
 *
 * Exit codes:
 *   0 — report produced (regardless of trust levels; this is an
 *       INFORMATIONAL surface, not a gate)
 *   1 — repo/snapshot not found
 */

import { resolve as resolvePath } from "node:path";
import type { Command } from "commander";
import { computeTrustReport } from "../../core/trust/service.js";
import type { TrustReport } from "../../core/trust/types.js";
import type { AppContext } from "../../main.js";

export function registerTrustCommand(
	program: Command,
	getCtx: () => AppContext,
): void {
	program
		.command("trust <repo>")
		.description(
			"Extraction trust report: reliability levels per axis + downgrade triggers",
		)
		.option("--json", "JSON output")
		.action((repoRef: string, opts: { json?: boolean }) => {
			const ctx = getCtx();
			const repo =
				ctx.storage.getRepo({ uid: repoRef }) ??
				ctx.storage.getRepo({ name: repoRef }) ??
				ctx.storage.getRepo({ rootPath: resolvePath(repoRef) });
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

			const report = computeTrustReport({
				storage: ctx.storage,
				repoUid: repo.repoUid,
				snapshotUid: snapshot.snapshotUid,
				snapshotBasisCommit: snapshot.basisCommit ?? null,
				snapshotToolchainJson: snapshot.toolchainJson ?? null,
			});

			if (opts.json) {
				console.log(
					JSON.stringify(
						{
							command: "trust",
							repo: repo.name,
							...report,
						},
						null,
						2,
					),
				);
			} else {
				printHuman(repo.name, report);
			}
		});
}

function printHuman(repoName: string, report: TrustReport): void {
	console.log(`Trust Report — ${repoName}`);
	console.log(`Snapshot: ${report.snapshot_uid}`);
	if (report.basis_commit) {
		console.log(`Basis commit: ${report.basis_commit}`);
	}
	if (!report.diagnostics_available) {
		console.log("");
		console.log(
			"WARNING: diagnostics unavailable (snapshot indexed before migration 005). Re-index to populate.",
		);
	}
	console.log("");

	// Summary counts
	const s = report.summary;
	console.log("Counts:");
	console.log(`  edges_total:        ${s.edges_total}`);
	console.log(`  unresolved_total:   ${s.unresolved_total}`);
	console.log(`  resolved_calls:     ${s.resolved_calls}`);
	console.log(`  unresolved_calls:   ${s.unresolved_calls}`);
	console.log(
		`  call_resolution_rate: ${(s.call_resolution_rate * 100).toFixed(1)}%`,
	);
	console.log("");

	// Reliability axes
	console.log("Reliability:");
	const axes = [
		["import_graph", s.reliability.import_graph],
		["call_graph", s.reliability.call_graph],
		["dead_code", s.reliability.dead_code],
		["change_impact", s.reliability.change_impact],
	] as const;
	for (const [name, axis] of axes) {
		const tag = `[${axis.level}]`.padEnd(10);
		console.log(`  ${tag} ${name}`);
		for (const reason of axis.reasons) {
			console.log(`             - ${reason}`);
		}
	}
	console.log("");

	// Downgrade triggers
	console.log("Downgrade triggers:");
	const triggers = [
		["framework_heavy_suspicion", s.triggered_downgrades.framework_heavy_suspicion],
		[
			"registry_pattern_suspicion",
			s.triggered_downgrades.registry_pattern_suspicion,
		],
		[
			"missing_entrypoint_declarations",
			s.triggered_downgrades.missing_entrypoint_declarations,
		],
		[
			"alias_resolution_suspicion",
			s.triggered_downgrades.alias_resolution_suspicion,
		],
	] as const;
	for (const [name, trig] of triggers) {
		const tag = trig.triggered ? "[TRIGGERED]" : "[-]        ";
		console.log(`  ${tag} ${name}`);
		for (const reason of trig.reasons) {
			console.log(`             - ${reason}`);
		}
	}
	console.log("");

	// Category breakdown
	if (report.categories.length > 0) {
		console.log("Unresolved edges by category:");
		for (const c of report.categories) {
			const count = String(c.unresolved).padStart(5);
			console.log(`  ${count}  ${c.label}`);
		}
		console.log("");
	}

	// Suspicious modules (truncated)
	const suspicious = report.modules.filter(
		(m) => m.suspicious_zero_connectivity,
	);
	if (suspicious.length > 0) {
		console.log(
			`Suspicious zero-connectivity modules (${suspicious.length} total, showing up to 10):`,
		);
		for (const m of suspicious.slice(0, 10)) {
			console.log(
				`  ${m.qualified_name} (fan_in=${m.fan_in}, fan_out=${m.fan_out}, files=${m.file_count})`,
			);
		}
		console.log("");
	}

	// Caveats
	console.log("Caveats:");
	for (const c of report.caveats) {
		console.log(`  - ${c}`);
	}
}

function outputError(json: boolean | undefined, message: string): void {
	if (json) {
		console.log(JSON.stringify({ error: message }));
	} else {
		console.error(`Error: ${message}`);
	}
}
