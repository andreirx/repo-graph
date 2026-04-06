/**
 * rgr enrich <repo> — compiler-assisted semantic enrichment.
 *
 * Runs AFTER indexing. Uses the TypeScript compiler to resolve
 * receiver types on unresolved obj.method() calls that the
 * syntax-only extractor could not determine.
 *
 * Scope (C1):
 *   - only CALLS_OBJ_METHOD_NEEDS_TYPE_INFO
 *   - only classification = unknown
 *   - enriches metadata_json on unresolved_edges with receiver type
 *   - does NOT resolve edges (observational only)
 *
 * Exit codes:
 *   0 — enrichment completed
 *   1 — repo/snapshot not found
 */

import { resolve as resolvePath } from "node:path";
import type { Command } from "commander";
import { resolveReceiverTypes } from "../../adapters/enrichment/typescript-receiver-resolver.js";
import type { AppContext } from "../../main.js";

export function registerEnrichCommand(
	program: Command,
	getCtx: () => AppContext,
): void {
	program
		.command("enrich <repo>")
		.description(
			"Compiler-assisted semantic enrichment: resolve receiver types on unresolved obj.method() calls",
		)
		.option("--json", "JSON output")
		.option("--limit <n>", "Max edges to enrich", "10000")
		.action(async (repoRef: string, opts: { json?: boolean; limit: string }) => {
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

			const limit = Number.parseInt(opts.limit, 10);
			if (!Number.isFinite(limit) || limit <= 0) {
				outputError(opts.json, `Invalid --limit: ${opts.limit}`);
				process.exitCode = 1;
				return;
			}

			// Query unknown CALLS_OBJ_METHOD edges (the dominant residual).
			const edges = ctx.storage.queryUnresolvedEdges({
				snapshotUid: snapshot.snapshotUid,
				classification: "unknown" as any,
				category: "calls_obj_method_needs_type_info" as any,
				limit,
			});

			if (edges.length === 0) {
				if (opts.json) {
					console.log(JSON.stringify({
						command: "enrich",
						repo: repo.name,
						enriched: 0,
						failed: 0,
						message: "no eligible edges to enrich",
					}));
				} else {
					console.log("No eligible unresolved edges to enrich.");
				}
				return;
			}

			if (!opts.json) {
				console.log(`Enriching ${edges.length} unresolved obj.method() calls...`);
			}

			// Build sites for the resolver.
			const sites = edges
				.filter((e) => e.sourceFilePath !== null && e.lineStart !== null)
				.map((e) => ({
					edgeUid: e.edgeUid,
					sourceFilePath: e.sourceFilePath!,
					lineStart: e.lineStart!,
					colStart: e.colStart ?? 0,
					targetKey: e.targetKey,
				}));

			const results = await resolveReceiverTypes(
				repo.rootPath,
				sites,
				!opts.json
					? (p) => {
							if (p.phase === "building_program") {
								process.stdout.write(
									`\r  Building program... ${p.current}/${p.total}`,
								);
							} else if (p.phase === "resolving_types") {
								process.stdout.write(
									`\r  Resolving types... ${p.current}/${p.total}`,
								);
							} else if (p.phase === "done") {
								process.stdout.write("\r  Done.                              \n");
							}
						}
					: undefined,
			);

			// Persist enrichments via merge into existing metadata_json.
			// Merge semantics preserve extractor-provided context
			// (rawCalleeName, rawPath, etc.) while adding the enrichment key.
			const updates: Array<{ edgeUid: string; metadataJsonPatch: string }> = [];
			for (const r of results) {
				if (r.receiverType === null) continue;
				updates.push({
					edgeUid: r.edgeUid,
					metadataJsonPatch: JSON.stringify({
						enrichment: {
							receiverType: r.receiverType,
							typeDisplayName: r.typeDisplayName,
							isExternalType: r.isExternalType,
							origin: r.receiverTypeOrigin,
						},
					}),
				});
			}
			if (updates.length > 0) {
				ctx.storage.mergeUnresolvedEdgesMetadata(updates);
			}

			const resolved = results.filter((r) => r.receiverType !== null);
			const failedResults = results.filter((r) => r.receiverType === null);

			if (opts.json) {
				// Summarize failure reasons.
				const failureReasons: Record<string, number> = {};
				for (const r of failedResults) {
					const reason = r.failureReason ?? "unknown";
					failureReasons[reason] = (failureReasons[reason] ?? 0) + 1;
				}
				// Summarize resolved type distribution.
				const typeDistribution: Record<string, number> = {};
				for (const r of resolved) {
					const key = r.isExternalType
						? `external:${r.typeDisplayName ?? r.receiverType}`
						: `internal:${r.typeDisplayName ?? r.receiverType}`;
					typeDistribution[key] = (typeDistribution[key] ?? 0) + 1;
				}

				console.log(JSON.stringify({
					command: "enrich",
					repo: repo.name,
					snapshot_uid: snapshot.snapshotUid,
					eligible: edges.length,
					enriched: resolved.length,
					failed: failedResults.length,
					enrichment_rate: edges.length > 0
						? `${((resolved.length / edges.length) * 100).toFixed(1)}%`
						: "n/a",
					top_types: Object.entries(typeDistribution)
						.sort((a, b) => b[1] - a[1])
						.slice(0, 20)
						.map(([type, count]) => ({ type, count })),
					failure_reasons: Object.entries(failureReasons)
						.sort((a, b) => b[1] - a[1])
						.map(([reason, count]) => ({ reason, count })),
				}, null, 2));
			} else {
				console.log(`Enriched: ${resolved.length}/${edges.length} (${((resolved.length / edges.length) * 100).toFixed(1)}%)`);
				console.log(`Failed:   ${failedResults.length}`);
				if (resolved.length > 0) {
					// Show top types.
					const typeCounts: Record<string, number> = {};
					for (const r of resolved) {
						const key = r.typeDisplayName ?? r.receiverType ?? "?";
						typeCounts[key] = (typeCounts[key] ?? 0) + 1;
					}
					const top = Object.entries(typeCounts).sort((a, b) => b[1] - a[1]).slice(0, 15);
					console.log("\nTop resolved receiver types:");
					for (const [type, count] of top) {
						console.log(`  ${String(count).padStart(5)}  ${type}`);
					}
				}
			}
		});
}

function outputError(json: boolean | undefined, message: string): void {
	if (json) {
		console.log(JSON.stringify({ error: message }));
	} else {
		console.error(`Error: ${message}`);
	}
}
