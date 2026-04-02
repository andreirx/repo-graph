import { resolve } from "node:path";
import type { Command } from "commander";
import type { BoundaryViolation, QueryResult } from "../../core/model/index.js";
import { DeclarationKind } from "../../core/model/index.js";
import type { AppContext } from "../../main.js";
import { formatQueryResult } from "../formatters/json.js";
import { formatViolations } from "../formatters/table.js";

export function registerArchCommands(
	program: Command,
	getCtx: () => AppContext,
): void {
	const arch = program
		.command("arch")
		.description("Architecture boundary queries");

	arch
		.command("violations <repo>")
		.description("Find IMPORTS edges that cross declared forbidden boundaries")
		.option("--json", "JSON output")
		.action((repoRef: string, opts: { json?: boolean }) => {
			const ctx = getCtx();
			const { repo, snapshotUid } = resolveRepoAndSnapshot(ctx, repoRef);
			if (!repo || !snapshotUid) {
				outputError(
					opts.json,
					`Repository not found or not indexed: ${repoRef}`,
				);
				process.exitCode = 1;
				return;
			}

			// Load all active boundary declarations
			const boundaries = ctx.storage.getActiveDeclarations({
				repoUid: repo.repoUid,
				kind: DeclarationKind.BOUNDARY,
			});

			if (boundaries.length === 0) {
				if (opts.json) {
					const qr: QueryResult<BoundaryViolation> = {
						command: "arch violations",
						repo: repo.name,
						snapshot: snapshotUid,
						snapshotScope: "full",
						basisCommit: null,
						results: [],
						count: 0,
						stale: false,
					};
					console.log(formatQueryResult(qr, formatViolationJson));
				} else {
					console.log(
						"No boundary declarations found. Use `rgr declare boundary` to define boundaries.",
					);
				}
				return;
			}

			// Deduplicate boundary rules: multiple declarations for the same
			// (module, forbids) pair should produce one set of violations, not
			// one per declaration. Keep the first reason encountered.
			const ruleMap = new Map<
				string,
				{ boundaryModule: string; forbids: string; reason: string | null }
			>();
			for (const decl of boundaries) {
				const value = JSON.parse(decl.valueJson) as {
					forbids: string;
					reason?: string;
				};
				const stableKey = decl.targetStableKey;
				const moduleMatch = stableKey.match(/^[^:]+:(.+):MODULE$/);
				if (!moduleMatch) continue;
				const boundaryModule = moduleMatch[1];
				const ruleKey = `${boundaryModule}|${value.forbids}`;
				if (!ruleMap.has(ruleKey)) {
					ruleMap.set(ruleKey, {
						boundaryModule,
						forbids: value.forbids,
						reason: value.reason ?? null,
					});
				}
			}

			// For each unique rule, find violating IMPORTS edges
			const violations: BoundaryViolation[] = [];

			for (const rule of ruleMap.values()) {
				const importEdges = ctx.storage.findImportsBetweenPaths({
					snapshotUid,
					sourcePrefix: rule.boundaryModule,
					targetPrefix: rule.forbids,
				});

				for (const edge of importEdges) {
					violations.push({
						boundaryModule: rule.boundaryModule,
						forbiddenModule: rule.forbids,
						reason: rule.reason,
						sourceFile: edge.sourceFile,
						targetFile: edge.targetFile,
						line: edge.line,
					});
				}
			}

			if (opts.json) {
				const snapshot = ctx.storage.getLatestSnapshot(repo.repoUid);
				const qr: QueryResult<BoundaryViolation> = {
					command: "arch violations",
					repo: repo.name,
					snapshot: snapshotUid,
					snapshotScope: snapshot?.kind === "full" ? "full" : "incremental",
					basisCommit: snapshot?.basisCommit ?? null,
					results: violations,
					count: violations.length,
					stale: false,
				};
				console.log(formatQueryResult(qr, formatViolationJson));
			} else {
				console.log(formatViolations(violations));
			}
		});
}

// ── Helpers ────────────────────────────────────────────────────────────

function resolveRepoAndSnapshot(
	ctx: AppContext,
	ref: string,
): {
	repo: ReturnType<AppContext["storage"]["getRepo"]>;
	snapshotUid: string | null;
} {
	const repo =
		ctx.storage.getRepo({ uid: ref }) ??
		ctx.storage.getRepo({ name: ref }) ??
		ctx.storage.getRepo({ rootPath: resolve(ref) });

	if (!repo) return { repo: null, snapshotUid: null };

	const snapshot = ctx.storage.getLatestSnapshot(repo.repoUid);
	return { repo, snapshotUid: snapshot?.snapshotUid ?? null };
}

function formatViolationJson(v: BoundaryViolation): Record<string, unknown> {
	return {
		boundary_module: v.boundaryModule,
		forbidden_module: v.forbiddenModule,
		reason: v.reason,
		source_file: v.sourceFile,
		target_file: v.targetFile,
		line: v.line,
	};
}

function outputError(json: boolean | undefined, message: string): void {
	if (json) {
		console.log(JSON.stringify({ error: message }));
	} else {
		console.error(`Error: ${message}`);
	}
}
