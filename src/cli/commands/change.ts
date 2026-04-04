/**
 * rgr change impact — change impact analysis.
 *
 * Thin orchestration layer over computeChangeImpact(). Handles:
 *   - scope flag parsing (mutually exclusive)
 *   - repo/snapshot resolution
 *   - JSON / human output formatting
 *   - error presentation
 *
 * No business logic lives here. All propagation rules, trust note
 * construction, and result shape are defined in core/impact/.
 */

import { resolve as resolvePath } from "node:path";
import type { Command } from "commander";
import {
	computeChangeImpact,
	ImpactScopeError,
	type ScopeRequest,
} from "../../core/impact/service.js";
import type { ImpactResult } from "../../core/impact/types.js";
import type { AppContext } from "../../main.js";

export function registerChangeCommands(
	program: Command,
	getCtx: () => AppContext,
): void {
	const change = program
		.command("change")
		.description("Change-impact analysis against the indexed snapshot");

	change
		.command("impact <repo>")
		.description(
			"Compute modules affected by a change scope (reverse module IMPORTS only)",
		)
		.option(
			"--against-snapshot",
			"Diff working tree vs the snapshot's basis commit (default)",
		)
		.option("--staged", "Diff staged changes (git diff --cached)")
		.option(
			"--since <ref>",
			"Diff working tree vs the given git ref (commit, branch, tag)",
		)
		.option(
			"--max-depth <n>",
			"Cap reverse-IMPORTS traversal depth (default: unbounded)",
		)
		.option("--json", "JSON output")
		.action(
			async (
				repoRef: string,
				opts: {
					againstSnapshot?: boolean;
					staged?: boolean;
					since?: string;
					maxDepth?: string;
					json?: boolean;
				},
			) => {
				const ctx = getCtx();

				// Resolve repo
				const repo =
					ctx.storage.getRepo({ uid: repoRef }) ??
					ctx.storage.getRepo({ name: repoRef }) ??
					ctx.storage.getRepo({ rootPath: resolvePath(repoRef) });
				if (!repo) {
					outputError(opts.json, `Repository not found: ${repoRef}`);
					process.exitCode = 1;
					return;
				}

				// Resolve snapshot
				const snapshot = ctx.storage.getLatestSnapshot(repo.repoUid);
				if (!snapshot) {
					outputError(opts.json, `Repository not indexed: ${repoRef}`);
					process.exitCode = 1;
					return;
				}

				// Parse scope flags — mutually exclusive
				const scopeFlags = [
					opts.againstSnapshot ? "against-snapshot" : null,
					opts.staged ? "staged" : null,
					opts.since !== undefined ? "since" : null,
				].filter((s): s is string => s !== null);

				if (scopeFlags.length > 1) {
					outputError(
						opts.json,
						`--against-snapshot, --staged, and --since are mutually exclusive (got: ${scopeFlags.join(", ")})`,
					);
					process.exitCode = 1;
					return;
				}

				// Default scope: against_snapshot
				let scopeRequest: ScopeRequest;
				if (opts.staged) {
					scopeRequest = { kind: "staged" };
				} else if (opts.since !== undefined) {
					scopeRequest = { kind: "since_ref", ref: opts.since };
				} else {
					scopeRequest = { kind: "against_snapshot" };
				}

				// Parse max-depth
				let maxDepth: number | undefined;
				if (opts.maxDepth !== undefined) {
					const parsed = Number.parseInt(opts.maxDepth, 10);
					if (Number.isNaN(parsed) || parsed < 1) {
						outputError(
							opts.json,
							"--max-depth must be a positive integer",
						);
						process.exitCode = 1;
						return;
					}
					maxDepth = parsed;
				}

				// Execute
				let result: ImpactResult;
				try {
					result = await computeChangeImpact({
						git: ctx.git,
						storage: ctx.storage,
						repoUid: repo.repoUid,
						repoPath: repo.rootPath,
						snapshotUid: snapshot.snapshotUid,
						snapshotBasisCommit: snapshot.basisCommit ?? null,
						scopeRequest,
						maxDepth,
					});
				} catch (err) {
					if (err instanceof ImpactScopeError) {
						outputError(opts.json, err.message);
						process.exitCode = 1;
						return;
					}
					outputError(
						opts.json,
						`Change impact failed: ${err instanceof Error ? err.message : String(err)}`,
					);
					process.exitCode = 1;
					return;
				}

				// Output
				if (opts.json) {
					console.log(
						JSON.stringify(
							{
								command: "change impact",
								repo: repo.name,
								...result,
							},
							null,
							2,
						),
					);
				} else {
					printHuman(repo.name, result);
				}
			},
		);
}

function printHuman(repoName: string, result: ImpactResult): void {
	const scopeStr = formatScope(result.scope);
	console.log(`Change Impact — ${repoName} (${scopeStr})`);
	console.log(`Snapshot: ${result.snapshot_uid}`);
	if (result.snapshot_basis_commit) {
		console.log(`Basis commit: ${result.snapshot_basis_commit}`);
	}
	console.log("");

	console.log("Changed files:");
	if (result.changed_files.length === 0) {
		console.log("  (none)");
	} else {
		for (const f of result.changed_files) {
			const marker = f.matched_to_index ? "[indexed]" : "[unmatched]";
			const reason = f.unmatched_reason ? ` (${f.unmatched_reason})` : "";
			console.log(`  ${marker} ${f.path}${reason}`);
		}
	}
	console.log("");

	console.log(
		`Impacted modules (${result.counts.impacted_modules}, max distance ${result.counts.max_distance}):`,
	);
	if (result.impacted_modules.length === 0) {
		console.log("  (none)");
	} else {
		for (const m of result.impacted_modules) {
			const tag = m.reason === "seed" ? "seed" : `d=${m.distance}`;
			console.log(`  [${tag}] ${m.module}`);
		}
	}
	console.log("");

	console.log("Trust:");
	console.log(`  graph_basis: ${result.trust.graph_basis}`);
	console.log(`  calls_included: ${result.trust.calls_included}`);
	for (const caveat of result.trust.caveats) {
		console.log(`  - ${caveat}`);
	}
}

function formatScope(scope: ImpactResult["scope"]): string {
	switch (scope.kind) {
		case "against_snapshot":
			return `against_snapshot @ ${scope.basis_commit.slice(0, 8)}`;
		case "staged":
			return "staged";
		case "since_ref":
			return `since ${scope.ref}`;
	}
}

function outputError(json: boolean | undefined, message: string): void {
	if (json) {
		console.log(JSON.stringify({ error: message }));
	} else {
		console.error(`Error: ${message}`);
	}
}
