/**
 * rgr modules <repo> — discovered module query surface.
 *
 * Separate from `graph stats` (which uses directory MODULE nodes).
 * Discovered modules are manifest/workspace-detected candidates
 * with evidence trails and file ownership.
 *
 * Subcommands:
 *   rgr modules list <repo>              — catalog with rollups
 *   rgr modules evidence <repo> <module> — evidence for a module
 *   rgr modules files <repo> <module>    — files owned by a module
 */

import { randomUUID } from "node:crypto";
import type { Command } from "commander";
import {
	type EnrichedModuleDependencyEdge,
	getModuleDependencyGraph,
	type ModuleDependencyStorageQueries,
} from "../../adapters/query/module-dependency-query.js";
import {
	DeclarationKind,
	type DiscoveredModuleBoundaryValue,
} from "../../core/model/index.js";
import type { SurfaceEnvEvidence } from "../../core/seams/env-dependency.js";
import type { SurfaceFsMutationEvidence } from "../../core/seams/fs-mutation.js";
import {
	aggregateEnvAcrossSurfaces,
	aggregateFsAcrossSurfaces,
	type EnvAggregationInput,
	type FsAggregationInput,
	summarizeFsDynamicEvidence,
} from "../../core/seams/module-seam-rollup.js";
import type { AppContext } from "../../main.js";
import { buildEnvRows, buildFsLiteralRows } from "./_seam-display.js";

export function registerModulesCommands(
	program: Command,
	getCtx: () => AppContext,
): void {
	const modules = program
		.command("modules")
		.description("Discovered module catalog, evidence, and ownership");

	// ── list ────────────────────────────────────────────────────────

	modules
		.command("list <repo>")
		.description("List discovered module candidates with rollup statistics")
		.option("--json", "JSON output")
		.action((repoRef: string, opts: { json?: boolean }) => {
			const ctx = getCtx();
			const resolved = resolveRepoAndSnapshot(ctx, repoRef);
			if (!resolved) {
				outputError(
					opts.json,
					`Repository not found or not indexed: ${repoRef}`,
				);
				process.exitCode = 1;
				return;
			}
			const { snapshotUid } = resolved;

			const rollups = ctx.storage.queryModuleCandidateRollups(snapshotUid);

			if (rollups.length === 0) {
				if (opts.json) {
					process.stdout.write(JSON.stringify([]));
				} else {
					process.stdout.write(
						"No discovered modules. Index with manifest/workspace detection enabled.\n",
					);
				}
				return;
			}

			if (opts.json) {
				process.stdout.write(JSON.stringify(rollups, null, 2));
				return;
			}

			// Human-readable table.
			const header = padRow([
				"MODULE",
				"NAME",
				"KIND",
				"CONF",
				"FILES",
				"SYMBOLS",
				"TESTS",
				"EVIDENCE",
				"LANGS",
				"DIR_MOD",
			]);
			process.stdout.write(header + "\n");

			for (const r of rollups) {
				const row = padRow([
					r.canonicalRootPath,
					r.displayName ?? "-",
					r.moduleKind,
					r.confidence.toFixed(2),
					String(r.fileCount),
					String(r.symbolCount),
					String(r.testFileCount),
					String(r.evidenceCount),
					r.languages || "-",
					r.hasDirectoryModule ? "yes" : "no",
				]);
				process.stdout.write(row + "\n");
			}
		});

	// ── evidence ────────────────────────────────────────────────────

	modules
		.command("evidence <repo> <module>")
		.description("Show evidence items for a discovered module")
		.option("--json", "JSON output")
		.action(
			(repoRef: string, moduleQuery: string, opts: { json?: boolean }) => {
				const ctx = getCtx();
				const resolved = resolveRepoAndSnapshot(ctx, repoRef);
				if (!resolved) {
					outputError(
						opts.json,
						`Repository not found or not indexed: ${repoRef}`,
					);
					process.exitCode = 1;
					return;
				}
				const { snapshotUid } = resolved;

				// Resolve module by root path or display name.
				const candidates = ctx.storage.queryModuleCandidates(snapshotUid);
				const candidate = candidates.find(
					(c) =>
						c.canonicalRootPath === moduleQuery ||
						c.displayName === moduleQuery ||
						c.moduleKey.endsWith(`:${moduleQuery}:DISCOVERED_MODULE`),
				);

				if (!candidate) {
					outputError(opts.json, `Module not found: ${moduleQuery}`);
					process.exitCode = 1;
					return;
				}

				const evidence = ctx.storage.queryModuleCandidateEvidence(
					candidate.moduleCandidateUid,
				);

				if (opts.json) {
					process.stdout.write(JSON.stringify(evidence, null, 2));
					return;
				}

				process.stdout.write(
					`Module: ${candidate.canonicalRootPath} (${candidate.displayName ?? "unnamed"})\n\n`,
				);

				if (evidence.length === 0) {
					process.stdout.write("No evidence items.\n");
					return;
				}

				const header = padRow([
					"SOURCE_TYPE",
					"SOURCE_PATH",
					"EVIDENCE_KIND",
					"CONF",
					"PAYLOAD",
				]);
				process.stdout.write(header + "\n");

				for (const e of evidence) {
					const payload = e.payloadJson ? truncate(e.payloadJson, 60) : "-";
					const row = padRow([
						e.sourceType,
						e.sourcePath,
						e.evidenceKind,
						e.confidence.toFixed(2),
						payload,
					]);
					process.stdout.write(row + "\n");
				}
			},
		);

	// ── files ───────────────────────────────────────────────────────

	modules
		.command("files <repo> <module>")
		.description("List files owned by a discovered module")
		.option("--json", "JSON output")
		.option("--limit <n>", "Max files to show", "50")
		.action(
			(
				repoRef: string,
				moduleQuery: string,
				opts: { json?: boolean; limit?: string },
			) => {
				const ctx = getCtx();
				const resolved = resolveRepoAndSnapshot(ctx, repoRef);
				if (!resolved) {
					outputError(
						opts.json,
						`Repository not found or not indexed: ${repoRef}`,
					);
					process.exitCode = 1;
					return;
				}
				const { snapshotUid } = resolved;

				// Resolve module.
				const candidates = ctx.storage.queryModuleCandidates(snapshotUid);
				const candidate = candidates.find(
					(c) =>
						c.canonicalRootPath === moduleQuery ||
						c.displayName === moduleQuery ||
						c.moduleKey.endsWith(`:${moduleQuery}:DISCOVERED_MODULE`),
				);

				if (!candidate) {
					outputError(opts.json, `Module not found: ${moduleQuery}`);
					process.exitCode = 1;
					return;
				}

				const limit = Number(opts.limit) || 50;

				// Targeted query: only files for this module, joined with
				// file metadata. No whole-snapshot materialization.
				const files = ctx.storage.queryModuleOwnedFiles(
					snapshotUid,
					candidate.moduleCandidateUid,
					limit,
				);
				// Total count without limit for display header.
				const totalFiles = ctx.storage.queryModuleOwnedFiles(
					snapshotUid,
					candidate.moduleCandidateUid,
				);
				const totalCount = totalFiles.length;

				if (opts.json) {
					const jsonRows = files.map((f) => ({
						filePath: f.filePath,
						language: f.language,
						isTest: f.isTest,
						assignmentKind: f.assignmentKind,
						confidence: f.confidence,
					}));
					process.stdout.write(JSON.stringify(jsonRows, null, 2));
					return;
				}

				process.stdout.write(
					`Module: ${candidate.canonicalRootPath} (${candidate.displayName ?? "unnamed"})\n`,
				);
				process.stdout.write(
					`Files: ${totalCount} total${totalCount > files.length ? ` (showing ${files.length})` : ""}\n\n`,
				);

				const header = padRow([
					"FILE",
					"LANGUAGE",
					"TEST",
					"ASSIGNMENT",
					"CONF",
				]);
				process.stdout.write(header + "\n");

				for (const f of files) {
					const row = padRow([
						f.filePath,
						f.language ?? "-",
						f.isTest ? "yes" : "no",
						f.assignmentKind,
						f.confidence.toFixed(2),
					]);
					process.stdout.write(row + "\n");
				}
			},
		);

	// ── show ─────────────────────────────────────────────────────────

	modules
		.command("show <repo> <module>")
		.description(
			"Full detail for one module: identity, surfaces, files, topology",
		)
		.option("--json", "JSON output")
		.action(
			(repoRef: string, moduleQuery: string, opts: { json?: boolean }) => {
				const ctx = getCtx();
				const resolved = resolveRepoAndSnapshot(ctx, repoRef);
				if (!resolved) {
					outputError(
						opts.json,
						`Repository not found or not indexed: ${repoRef}`,
					);
					process.exitCode = 1;
					return;
				}
				const { snapshotUid } = resolved;

				const candidates = ctx.storage.queryModuleCandidates(snapshotUid);
				const matches = candidates.filter(
					(c) =>
						c.canonicalRootPath === moduleQuery ||
						c.displayName === moduleQuery ||
						c.moduleKey.endsWith(`:${moduleQuery}:DISCOVERED_MODULE`),
				);

				if (matches.length === 0) {
					outputError(opts.json, `Module not found: ${moduleQuery}`);
					process.exitCode = 1;
					return;
				}
				if (matches.length > 1) {
					const names = matches
						.map((c) => `${c.displayName ?? c.canonicalRootPath}`)
						.join(", ");
					outputError(
						opts.json,
						`Ambiguous module query "${moduleQuery}" matches ${matches.length}: ${names}. Use a more specific root path or display name.`,
					);
					process.exitCode = 1;
					return;
				}
				const candidate = matches[0];

				// Gather related data.
				const evidence = ctx.storage.queryModuleCandidateEvidence(
					candidate.moduleCandidateUid,
				);
				const moduleSurfaces = ctx.storage
					.queryProjectSurfaces(snapshotUid)
					.filter((s) => s.moduleCandidateUid === candidate.moduleCandidateUid);
				const fileCount = ctx.storage.queryModuleOwnedFiles(
					snapshotUid,
					candidate.moduleCandidateUid,
					0,
				).length;

				// Sort surfaces by projectSurfaceUid for deterministic
				// iteration. The pure rollup core's "first non-null default"
				// rule depends on a stable input order — we anchor it here.
				const surfacesSorted = [...moduleSurfaces].sort((a, b) =>
					a.projectSurfaceUid.localeCompare(b.projectSurfaceUid),
				);

				// Per-surface fetch (M1 loop pattern). Each surface gets its
				// own configRoots, entrypoints, env deps + evidence, fs
				// mutations + evidence. Direct surface-local data only —
				// aggregation is performed by the pure rollup core, never
				// re-derived in this CLI layer.
				const surfaceDetails = surfacesSorted.map((s) => {
					const configRoots = ctx.storage.querySurfaceConfigRoots(
						s.projectSurfaceUid,
					);
					const entrypoints = ctx.storage.querySurfaceEntrypoints(
						s.projectSurfaceUid,
					);
					const envDeps = ctx.storage.querySurfaceEnvDependencies(
						s.projectSurfaceUid,
					);
					const envEvidence = ctx.storage.querySurfaceEnvEvidenceBySurface(
						s.projectSurfaceUid,
					);
					const fsMutations = ctx.storage.querySurfaceFsMutations(
						s.projectSurfaceUid,
					);
					const fsEvidence =
						ctx.storage.querySurfaceFsMutationEvidenceBySurface(
							s.projectSurfaceUid,
						);
					return {
						surface: s,
						configRoots,
						entrypoints,
						envDeps,
						envEvidence,
						fsMutations,
						fsEvidence,
					};
				});

				// Build aggregation inputs and call the pure rollup core.
				// The CLI does not invent aggregation rules — it only maps
				// storage rows into the input shape and passes them through.
				const envAggInput: EnvAggregationInput[] = [];
				const fsAggInput: FsAggregationInput[] = [];
				const allFsEvidence: SurfaceFsMutationEvidence[] = [];

				for (const sd of surfaceDetails) {
					const envEvidenceCounts = countEnvEvidencePerDep(sd.envEvidence);
					for (const dep of sd.envDeps) {
						envAggInput.push({
							surfaceUid: sd.surface.projectSurfaceUid,
							dependency: dep,
							evidenceCount:
								envEvidenceCounts.get(dep.surfaceEnvDependencyUid) ?? 0,
						});
					}

					const fsLiteralCounts = countFsLiteralEvidencePerIdentity(
						sd.fsEvidence,
					);
					for (const mut of sd.fsMutations) {
						fsAggInput.push({
							surfaceUid: sd.surface.projectSurfaceUid,
							mutation: mut,
							evidenceCount: fsLiteralCounts.get(mut.surfaceFsMutationUid) ?? 0,
						});
					}

					allFsEvidence.push(...sd.fsEvidence);
				}

				const rollupEnv = aggregateEnvAcrossSurfaces(envAggInput);
				const rollupFs = aggregateFsAcrossSurfaces(fsAggInput);
				const rollupFsDynamic = summarizeFsDynamicEvidence(allFsEvidence);

				if (opts.json) {
					process.stdout.write(
						JSON.stringify(
							{
								module: {
									moduleCandidateUid: candidate.moduleCandidateUid,
									moduleKey: candidate.moduleKey,
									displayName: candidate.displayName,
									canonicalRootPath: candidate.canonicalRootPath,
									moduleKind: candidate.moduleKind,
									confidence: candidate.confidence,
								},
								evidence: evidence.map((e) => ({
									sourceType: e.sourceType,
									sourcePath: e.sourcePath,
									evidenceKind: e.evidenceKind,
									confidence: e.confidence,
								})),
								fileCount,
								rollup: {
									envDependencies: rollupEnv,
									fsMutations: {
										literal: rollupFs,
										dynamic: rollupFsDynamic,
									},
								},
								surfaces: surfaceDetails.map((sd) => ({
									projectSurfaceUid: sd.surface.projectSurfaceUid,
									surfaceKind: sd.surface.surfaceKind,
									displayName: sd.surface.displayName,
									buildSystem: sd.surface.buildSystem,
									runtimeKind: sd.surface.runtimeKind,
									entrypointPath: sd.surface.entrypointPath,
									configRoots: sd.configRoots.map((cr) => ({
										configPath: cr.configPath,
										configKind: cr.configKind,
									})),
									entrypoints: sd.entrypoints.map((ep) => ({
										entrypointPath: ep.entrypointPath,
										entrypointTarget: ep.entrypointTarget,
										entrypointKind: ep.entrypointKind,
										displayName: ep.displayName,
									})),
									envDependencies: buildEnvRows(sd.envDeps, sd.envEvidence),
									fsMutations: {
										literal: buildFsLiteralRows(sd.fsMutations, sd.fsEvidence),
										dynamic: summarizeFsDynamicEvidence(sd.fsEvidence),
									},
								})),
							},
							null,
							2,
						),
					);
					return;
				}

				// Human-readable detail.
				process.stdout.write(
					`Module: ${candidate.displayName ?? candidate.canonicalRootPath}\n`,
				);
				process.stdout.write(`Key: ${candidate.moduleKey}\n`);
				process.stdout.write(`Root: ${candidate.canonicalRootPath}\n`);
				process.stdout.write(`Kind: ${candidate.moduleKind}\n`);
				process.stdout.write(
					`Confidence: ${candidate.confidence.toFixed(2)}\n`,
				);
				process.stdout.write(`Files: ${fileCount}\n`);
				process.stdout.write(`\n`);

				if (evidence.length > 0) {
					process.stdout.write(`Evidence (${evidence.length}):\n`);
					for (const e of evidence) {
						process.stdout.write(
							`  ${e.sourceType.padEnd(28)} ${e.evidenceKind.padEnd(18)} ${e.sourcePath}\n`,
						);
					}
					process.stdout.write(`\n`);
				}

				// Module-level rollup is the primary human-readable artifact.
				// Rendered before the per-surface detail block.
				if (
					rollupEnv.length > 0 ||
					rollupFs.length > 0 ||
					rollupFsDynamic.totalCount > 0
				) {
					process.stdout.write(`Module Rollup:\n`);

					if (rollupEnv.length > 0) {
						process.stdout.write(`  Env Dependencies (${rollupEnv.length}):\n`);
						for (const r of rollupEnv) {
							const def = r.defaultValue !== null ? `=${r.defaultValue}` : "";
							const conflict = r.hasConflictingDefaults
								? " [conflicting defaults]"
								: "";
							process.stdout.write(
								`    ${r.accessKind.padEnd(9)} ${(r.envName + def).padEnd(32)} surfaces=${r.surfaceCount}  evidence=${r.evidenceCount}  conf=${r.maxConfidence.toFixed(2)}${conflict}\n`,
							);
						}
					}

					if (rollupFs.length > 0 || rollupFsDynamic.totalCount > 0) {
						const dynLabel =
							rollupFsDynamic.totalCount > 0
								? ` + ${rollupFsDynamic.totalCount} dynamic`
								: "";
						process.stdout.write(
							`  Filesystem Mutations (${rollupFs.length} literal${dynLabel}):\n`,
						);
						for (const r of rollupFs) {
							const dest =
								r.destinationPaths.length > 0
									? ` -> ${r.destinationPaths.join(", ")}`
									: "";
							process.stdout.write(
								`    ${r.mutationKind.padEnd(12)} ${r.targetPath.padEnd(34)} surfaces=${r.surfaceCount}  evidence=${r.evidenceCount}  conf=${r.maxConfidence.toFixed(2)}${dest}\n`,
							);
						}
						if (rollupFsDynamic.totalCount > 0) {
							process.stdout.write(
								`    Dynamic-path: ${rollupFsDynamic.totalCount} occurrence${rollupFsDynamic.totalCount === 1 ? "" : "s"} in ${rollupFsDynamic.distinctFileCount} file${rollupFsDynamic.distinctFileCount === 1 ? "" : "s"}\n`,
							);
							const kindKeys = Object.keys(rollupFsDynamic.byKind).sort();
							for (const k of kindKeys) {
								const count =
									rollupFsDynamic.byKind[
										k as keyof typeof rollupFsDynamic.byKind
									];
								process.stdout.write(`      ${k}: ${count}\n`);
							}
						}
					}

					process.stdout.write(`\n`);
				}

				// Per-surface detail (compact). Each surface gets a one-line
				// summary of how much it contributes to the rollup. For full
				// per-surface env/fs detail, the user runs `rgr surfaces show`.
				if (surfaceDetails.length > 0) {
					process.stdout.write(`Surfaces (${surfaceDetails.length}):\n`);
					for (const sd of surfaceDetails) {
						const envCount = sd.envDeps.length;
						const fsLiteralCount = sd.fsMutations.length;
						const fsDynamicCount = sd.fsEvidence.filter(
							(e) => e.dynamicPath,
						).length;

						process.stdout.write(
							`  ${sd.surface.surfaceKind.padEnd(18)} ${sd.surface.displayName ?? "-"}\n`,
						);
						process.stdout.write(
							`    Build: ${sd.surface.buildSystem} | Runtime: ${sd.surface.runtimeKind}\n`,
						);
						if (sd.configRoots.length > 0) {
							process.stdout.write(
								`    Config: ${sd.configRoots.map((cr) => cr.configPath).join(", ")}\n`,
							);
						}
						if (sd.entrypoints.length > 0) {
							for (const ep of sd.entrypoints) {
								process.stdout.write(
									`    Entry: ${ep.entrypointKind} -> ${ep.entrypointPath ?? ep.entrypointTarget ?? "-"}\n`,
								);
							}
						}
						if (envCount > 0 || fsLiteralCount > 0 || fsDynamicCount > 0) {
							const fsDynLabel =
								fsDynamicCount > 0 ? ` +${fsDynamicCount} dyn` : "";
							process.stdout.write(
								`    Seams: env=${envCount}  fs=${fsLiteralCount}${fsDynLabel}\n`,
							);
						}
					}
				} else {
					process.stdout.write(`No project surfaces detected.\n`);
				}
			},
		);

	// ── deps ────────────────────────────────────────────────────────

	modules
		.command("deps <repo> [module]")
		.description("Show module dependency edges (JSON only)")
		.option("--outbound", "Show only outbound edges from the module")
		.option("--inbound", "Show only inbound edges to the module")
		.action(
			(
				repoRef: string,
				moduleQuery: string | undefined,
				opts: { outbound?: boolean; inbound?: boolean },
			) => {
				const ctx = getCtx();
				const resolved = resolveRepoAndSnapshot(ctx, repoRef);
				if (!resolved) {
					outputJsonError(`Repository not found or not indexed: ${repoRef}`);
					process.exitCode = 1;
					return;
				}
				const { repo, snapshotUid } = resolved;

				// Validate flag usage.
				if ((opts.outbound || opts.inbound) && !moduleQuery) {
					outputJsonError("--outbound and --inbound require a module argument");
					process.exitCode = 1;
					return;
				}
				if (opts.outbound && opts.inbound) {
					outputJsonError("--outbound and --inbound are mutually exclusive");
					process.exitCode = 1;
					return;
				}

				// Resolve module if specified — exact identity resolution only.
				// Accepts exact moduleKey or exact canonicalRootPath.
				let moduleKey: string | undefined;
				if (moduleQuery) {
					const candidates = ctx.storage.queryModuleCandidates(snapshotUid);
					const match = candidates.find(
						(c) =>
							c.moduleKey === moduleQuery ||
							c.canonicalRootPath === moduleQuery,
					);

					if (!match) {
						outputJsonError(`Module not found: ${moduleQuery}`);
						process.exitCode = 1;
						return;
					}
					moduleKey = match.moduleKey;
				}

				// Query module dependency graph.
				const storage = ctx.storage as typeof ctx.storage &
					ModuleDependencyStorageQueries;
				const result = getModuleDependencyGraph(storage, snapshotUid, {
					moduleKey,
					outboundOnly: opts.outbound,
					inboundOnly: opts.inbound,
				});

				// Get snapshot metadata for envelope.
				const snapshot = ctx.storage.getLatestSnapshot(repo.repoUid);
				const snapshotScope =
					snapshot?.kind === "full" ? "full" : "incremental";

				// Format output as JSON envelope.
				const output = {
					command: moduleQuery ? `modules deps ${moduleQuery}` : "modules deps",
					repo: repo.name,
					snapshot: snapshotUid,
					snapshot_scope: snapshotScope,
					basis_commit: snapshot?.basisCommit ?? null,
					results: result.edges.map(formatModuleDependencyEdge),
					count: result.edges.length,
					diagnostics: {
						imports_edges_total: result.diagnostics.importsEdgesTotal,
						imports_source_no_file: result.diagnostics.importsSourceNoFile,
						imports_target_no_file: result.diagnostics.importsTargetNoFile,
						imports_source_no_module: result.diagnostics.importsSourceNoModule,
						imports_target_no_module: result.diagnostics.importsTargetNoModule,
						imports_intra_module: result.diagnostics.importsIntraModule,
						imports_cross_module: result.diagnostics.importsCrossModule,
					},
				};

				process.stdout.write(JSON.stringify(output, null, 2) + "\n");
			},
		);

	// ── boundary ───────────────────────────────────────────────────

	modules
		.command("boundary <repo> <source-module>")
		.description("Declare a boundary constraint between discovered modules")
		.requiredOption(
			"--forbids <target-module>",
			"Module that source must not depend on",
		)
		.option("--reason <text>", "Why this boundary exists")
		.option("--json", "JSON output")
		.action(
			(
				repoRef: string,
				sourceModuleQuery: string,
				opts: {
					forbids: string;
					reason?: string;
					json?: boolean;
				},
			) => {
				const ctx = getCtx();
				const resolved = resolveRepoAndSnapshot(ctx, repoRef);
				if (!resolved) {
					outputBoundaryError(
						opts.json,
						`Repository not found or not indexed: ${repoRef}`,
					);
					process.exitCode = 1;
					return;
				}
				const { repo, snapshotUid } = resolved;

				// Resolve source module — exact identity resolution only.
				const candidates = ctx.storage.queryModuleCandidates(snapshotUid);

				const sourceMatch = candidates.find(
					(c) =>
						c.moduleKey === sourceModuleQuery ||
						c.canonicalRootPath === sourceModuleQuery,
				);
				if (!sourceMatch) {
					outputBoundaryError(
						opts.json,
						`Source module not found: ${sourceModuleQuery}`,
					);
					process.exitCode = 1;
					return;
				}

				// Resolve target module — exact identity resolution only.
				const targetMatch = candidates.find(
					(c) =>
						c.moduleKey === opts.forbids ||
						c.canonicalRootPath === opts.forbids,
				);
				if (!targetMatch) {
					outputBoundaryError(
						opts.json,
						`Target module not found: ${opts.forbids}`,
					);
					process.exitCode = 1;
					return;
				}

				// Prevent self-referential boundaries.
				if (sourceMatch.moduleCandidateUid === targetMatch.moduleCandidateUid) {
					outputBoundaryError(
						opts.json,
						"Source and target modules must be different",
					);
					process.exitCode = 1;
					return;
				}

				// Build declaration payload — store canonicalRootPath, not moduleKey.
				const value: DiscoveredModuleBoundaryValue = {
					selectorDomain: "discovered_module",
					source: { canonicalRootPath: sourceMatch.canonicalRootPath },
					forbids: { canonicalRootPath: targetMatch.canonicalRootPath },
					...(opts.reason && { reason: opts.reason }),
				};

				// Generate stable key for indexing.
				// Format: {repoUid}:{sourceRootPath}:DISCOVERED_MODULE_BOUNDARY
				const targetStableKey = `${repo.repoUid}:${sourceMatch.canonicalRootPath}:DISCOVERED_MODULE_BOUNDARY`;

				const declarationUid = randomUUID();
				const declaration = {
					declarationUid,
					repoUid: repo.repoUid,
					snapshotUid: null, // repo-wide, not snapshot-scoped
					targetStableKey,
					kind: DeclarationKind.BOUNDARY,
					valueJson: JSON.stringify(value),
					createdAt: new Date().toISOString(),
					createdBy: null,
					supersedesUid: null,
					isActive: true,
					authoredBasisJson: null,
				};

				ctx.storage.insertDeclaration(declaration);

				if (opts.json) {
					process.stdout.write(
						JSON.stringify(
							{
								declaration_uid: declarationUid,
								source_root_path: sourceMatch.canonicalRootPath,
								target_root_path: targetMatch.canonicalRootPath,
							},
							null,
							2,
						) + "\n",
					);
				} else {
					process.stdout.write(
						`Created boundary: ${sourceMatch.canonicalRootPath} forbids ${targetMatch.canonicalRootPath}\n`,
					);
					process.stdout.write(`Declaration UID: ${declarationUid}\n`);
				}
			},
		);
}

function outputBoundaryError(json: boolean | undefined, message: string): void {
	if (json) {
		process.stdout.write(JSON.stringify({ error: message }, null, 2) + "\n");
	} else {
		process.stderr.write(message + "\n");
	}
}

// ── Module dependency edge formatter ──────────────────────────────

function formatModuleDependencyEdge(
	e: EnrichedModuleDependencyEdge,
): Record<string, unknown> {
	return {
		source_module_uid: e.sourceModuleUid,
		source_module_key: e.sourceModuleKey,
		source_root_path: e.sourceRootPath,
		source_module_kind: e.sourceModuleKind,
		source_display_name: e.sourceDisplayName,
		target_module_uid: e.targetModuleUid,
		target_module_key: e.targetModuleKey,
		target_root_path: e.targetRootPath,
		target_module_kind: e.targetModuleKind,
		target_display_name: e.targetDisplayName,
		import_count: e.importCount,
		source_file_count: e.sourceFileCount,
	};
}

function outputJsonError(message: string): void {
	process.stdout.write(JSON.stringify({ error: message }, null, 2) + "\n");
}

// ── Local helpers (count grouping only — no aggregation rules) ────

function countEnvEvidencePerDep(
	evidence: readonly SurfaceEnvEvidence[],
): Map<string, number> {
	const counts = new Map<string, number>();
	for (const e of evidence) {
		counts.set(
			e.surfaceEnvDependencyUid,
			(counts.get(e.surfaceEnvDependencyUid) ?? 0) + 1,
		);
	}
	return counts;
}

function countFsLiteralEvidencePerIdentity(
	evidence: readonly SurfaceFsMutationEvidence[],
): Map<string, number> {
	const counts = new Map<string, number>();
	for (const e of evidence) {
		// Dynamic-path evidence has null surfaceFsMutationUid and is
		// summarized via summarizeFsDynamicEvidence, never counted as
		// per-identity literal evidence.
		if (e.surfaceFsMutationUid === null) continue;
		counts.set(
			e.surfaceFsMutationUid,
			(counts.get(e.surfaceFsMutationUid) ?? 0) + 1,
		);
	}
	return counts;
}

// ── Helpers ────────────────────────────────────────────────────────

function resolveRepoAndSnapshot(
	ctx: AppContext,
	repoRef: string,
): {
	repo: ReturnType<typeof ctx.storage.listRepos>[0];
	snapshotUid: string;
} | null {
	const repos = ctx.storage.listRepos();
	const repo = repos.find((r) => r.name === repoRef || r.repoUid === repoRef);
	if (!repo) return null;
	const snapshot = ctx.storage.getLatestSnapshot(repo.repoUid);
	if (!snapshot) return null;
	return { repo, snapshotUid: snapshot.snapshotUid };
}

function outputError(json: boolean | undefined, message: string): void {
	if (json) {
		process.stdout.write(JSON.stringify({ error: message }));
	} else {
		process.stderr.write(message + "\n");
	}
}

function padRow(cols: string[]): string {
	// Fixed widths per column for readable output.
	const widths = [35, 25, 12, 6, 7, 9, 6, 9, 20, 8];
	return cols
		.map((c, i) => {
			const w = widths[i] ?? 20;
			return c.length > w ? c.slice(0, w - 1) + "~" : c.padEnd(w);
		})
		.join("  ");
}

function truncate(s: string, maxLen: number): string {
	return s.length > maxLen ? s.slice(0, maxLen - 3) + "..." : s;
}
