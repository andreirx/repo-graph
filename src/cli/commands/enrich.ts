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
import { promoteEdges, type PromotionCandidate } from "../../adapters/enrichment/edge-promoter.js";
import { resolveReceiverTypes } from "../../adapters/enrichment/typescript-receiver-resolver.js";
import { NodeSubtype } from "../../core/model/index.js";
import type { GraphNode } from "../../core/model/index.js";
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
		.option("--promote", "Promote safe-subset enriched edges to resolved graph edges")
		.action(async (repoRef: string, opts: { json?: boolean; limit: string; promote?: boolean }) => {
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

			// Query enrichment-eligible edges:
			// 1. Unknown obj.method() calls (original C1 scope)
			// 2. Internal this.x.method() calls (this-wildcard expansion)
			// Limit is applied across the combined set, not per-category.
			const objMethodEdges = ctx.storage.queryUnresolvedEdges({
				snapshotUid: snapshot.snapshotUid,
				classification: "unknown" as any,
				category: "calls_obj_method_needs_type_info" as any,
				limit,
			});
			const remaining = Math.max(0, limit - objMethodEdges.length);
			const thisWildcardEdges = remaining > 0
				? ctx.storage.queryUnresolvedEdges({
						snapshotUid: snapshot.snapshotUid,
						category: "calls_this_wildcard_method_needs_type_info" as any,
						limit: remaining,
					})
				: [];
			const edges = [...objMethodEdges, ...thisWildcardEdges];

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
			//
			// EVERY eligible edge gets an enrichment marker — even those
			// where the compiler failed. This allows the trust report to
			// distinguish "enrichment not run" from "enrichment run with
			// zero successes." The marker carries origin="compiler" for
			// successes and origin="failed" for failures.
			const updates: Array<{ edgeUid: string; metadataJsonPatch: string }> = [];
			for (const r of results) {
				updates.push({
					edgeUid: r.edgeUid,
					metadataJsonPatch: JSON.stringify({
						enrichment: {
							receiverType: r.receiverType,
							typeDisplayName: r.typeDisplayName,
							isExternalType: r.isExternalType,
							origin: r.receiverTypeOrigin,
							...(r.failureReason ? { failureReason: r.failureReason } : {}),
						},
					}),
				});
			}
			if (updates.length > 0) {
				ctx.storage.mergeUnresolvedEdgesMetadata(updates);
			}

			const resolved = results.filter((r) => r.receiverType !== null);
			const failedResults = results.filter((r) => r.receiverType === null);

			// D2c: promote safe subset if --promote flag is set.
			let promotedCount = 0;
			let promotionSkipped: Record<string, number> = {};
			if (opts.promote && resolved.length > 0) {
				if (!opts.json) console.log("\nPromoting safe-subset edges...");

				// Build node indexes needed for promotion gates.
				// We need: all SYMBOL nodes for name lookup + class→method index.
				const nodesByName = new Map<string, GraphNode[]>();
				const classMethodIndex = new Map<string, Map<string, GraphNode>>();

				// Read nodes from DB for this snapshot.
				// StoragePort doesn't expose a bulk-read-all-nodes method, so
				// we use the fact that resolved symbol names can be looked up.
				// For first slice: query symbol resolution for each unique type name.
				const uniqueTypes = new Set<string>();
				for (const r of resolved) {
					if (!r.isExternalType && r.typeDisplayName) {
						uniqueTypes.add(r.typeDisplayName);
					}
				}

				for (const typeName of uniqueTypes) {
					const matches = ctx.storage.resolveSymbol({
						snapshotUid: snapshot.snapshotUid,
						query: typeName,
						limit: 50,
					});
					if (!nodesByName.has(typeName)) {
						nodesByName.set(typeName, []);
					}
					for (const node of matches) {
						nodesByName.get(typeName)!.push(node);
						// Build class→method index for CLASS nodes.
						if (node.subtype === NodeSubtype.CLASS) {
							const methodMap = new Map<string, GraphNode>();
							// Query methods that are children of this class.
							const classMethodNodes = ctx.storage.resolveSymbol({
								snapshotUid: snapshot.snapshotUid,
								query: `${typeName}.`,
								limit: 200,
							});
							for (const m of classMethodNodes) {
								if (m.subtype === NodeSubtype.METHOD || m.subtype === NodeSubtype.GETTER || m.subtype === NodeSubtype.SETTER) {
									const qn = m.qualifiedName ?? "";
									const dotIdx = qn.lastIndexOf(".");
									if (dotIdx >= 0) {
										const mName = qn.slice(dotIdx + 1);
										if (!methodMap.has(mName)) {
											methodMap.set(mName, m);
										} else {
											// Multiple methods with same name — ambiguous, remove.
											methodMap.delete(mName);
										}
									}
								}
							}
							classMethodIndex.set(node.stableKey, methodMap);
						}
					}
				}

				// Build promotion candidates from enriched results + original edge data.
				const candidates: PromotionCandidate[] = [];
				for (let i = 0; i < results.length; i++) {
					const r = results[i];
					if (!r.receiverType || r.isExternalType) continue;
					const site = sites[i];
					if (!site) continue;
					// Find the original edge data.
					const originalEdge = edges.find((e) => e.edgeUid === r.edgeUid);
					if (!originalEdge) continue;

					let enrichment: PromotionCandidate["enrichment"];
					try {
						enrichment = {
							receiverType: r.receiverType,
							typeDisplayName: r.typeDisplayName,
							isExternalType: r.isExternalType,
							origin: r.receiverTypeOrigin,
						};
					} catch {
						continue;
					}

					candidates.push({
						edgeUid: r.edgeUid,
						snapshotUid: snapshot.snapshotUid,
						repoUid: repo.repoUid,
						sourceNodeUid: originalEdge.sourceNodeUid,
						targetKey: originalEdge.targetKey,
						lineStart: originalEdge.lineStart,
						colStart: originalEdge.colStart,
						lineEnd: null,
						colEnd: null,
						category: originalEdge.category,
						enrichment,
					});
				}

				const promotionResult = promoteEdges(candidates, nodesByName, classMethodIndex);
				promotedCount = promotionResult.promoted.length;
				promotionSkipped = promotionResult.skippedReasons;

				if (promotionResult.promoted.length > 0) {
					// Idempotent: delete any previously promoted edges for
					// this snapshot before inserting, so rerunning --promote
					// doesn't hit duplicate-key failures.
					const promotedUids = promotionResult.promoted.map((e) => e.edgeUid);
					ctx.storage.deleteEdgesByUids(promotedUids);
					ctx.storage.insertEdges(promotionResult.promoted);
				}

				if (!opts.json) {
					console.log(`Promoted: ${promotedCount} edges to resolved graph`);
					if (Object.keys(promotionSkipped).length > 0) {
						const topSkips = Object.entries(promotionSkipped)
							.sort((a, b) => b[1] - a[1])
							.slice(0, 5);
						for (const [reason, count] of topSkips) {
							console.log(`  skipped: ${count} (${reason})`);
						}
					}
				}
			}

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
					promoted: promotedCount,
					promotion_skipped: promotionSkipped,
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
