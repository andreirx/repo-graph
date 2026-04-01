import { resolve } from "node:path";
import type { Command } from "commander";
import { v4 as uuidv4 } from "uuid";
import type { AppContext } from "../../main.js";
import {
	formatIndexResult,
	formatRepoStatus,
	formatRepoTable,
} from "../formatters/table.js";

export function registerRepoCommands(
	program: Command,
	getCtx: () => AppContext,
): void {
	const repo = program.command("repo").description("Repository management");

	repo
		.command("add <path>")
		.description("Register a local repository for indexing")
		.option("--name <name>", "Alias for the repo")
		.option("--json", "JSON output")
		.action((path: string, opts: { name?: string; json?: boolean }) => {
			const ctx = getCtx();
			const absPath = resolve(path);
			const name = opts.name ?? absPath.split("/").pop() ?? "repo";

			// Check if already registered
			const existing = ctx.storage.getRepo({ rootPath: absPath });
			if (existing) {
				if (opts.json) {
					console.log(
						JSON.stringify({
							error: "Repository already registered",
							repo_uid: existing.repoUid,
						}),
					);
				} else {
					console.log(
						`Repository already registered as "${existing.name}" (${existing.repoUid})`,
					);
				}
				return;
			}

			const repoUid = uuidv4().slice(0, 8);
			ctx.storage.addRepo({
				repoUid,
				name,
				rootPath: absPath,
				defaultBranch: null,
				createdAt: new Date().toISOString(),
				metadataJson: null,
			});

			if (opts.json) {
				console.log(JSON.stringify({ repo_uid: repoUid, name, path: absPath }));
			} else {
				console.log(`Registered "${name}" (${repoUid}) at ${absPath}`);
			}
		});

	repo
		.command("index <repo>")
		.description("Full parse of a repository")
		.option("--exclude <patterns...>", "Exclusion glob patterns")
		.option("--json", "JSON output")
		.option("--verbose", "Show progress")
		.action(
			async (
				repoRef: string,
				opts: { exclude?: string[]; json?: boolean; verbose?: boolean },
			) => {
				const ctx = getCtx();
				const repoEntity = resolveRepo(ctx, repoRef);
				if (!repoEntity) {
					outputError(opts.json, `Repository not found: ${repoRef}`);
					process.exitCode = 1;
					return;
				}

				const result = await ctx.indexer.indexRepo(repoEntity.repoUid, {
					exclude: opts.exclude,
					onProgress: opts.verbose
						? (e) => {
								if (e.file) {
									process.stderr.write(
										`\r[${e.phase}] ${e.current}/${e.total} ${e.file}`,
									);
								} else {
									process.stderr.write(
										`\r[${e.phase}] ${e.current}/${e.total}`,
									);
								}
							}
						: undefined,
				});

				if (opts.verbose) {
					process.stderr.write("\n");
				}

				if (opts.json) {
					console.log(
						JSON.stringify({
							snapshot_uid: result.snapshotUid,
							files_total: result.filesTotal,
							nodes_total: result.nodesTotal,
							edges_total: result.edgesTotal,
							edges_unresolved: result.edgesUnresolved,
							duration_ms: result.durationMs,
						}),
					);
				} else {
					console.log(formatIndexResult(result));
				}
			},
		);

	repo
		.command("status <repo>")
		.description("Show current state of a repository")
		.option("--json", "JSON output")
		.action((repoRef: string, opts: { json?: boolean }) => {
			const ctx = getCtx();
			const repoEntity = resolveRepo(ctx, repoRef);
			if (!repoEntity) {
				outputError(opts.json, `Repository not found: ${repoRef}`);
				process.exitCode = 1;
				return;
			}

			const snapshot = ctx.storage.getLatestSnapshot(repoEntity.repoUid);

			if (opts.json) {
				console.log(
					JSON.stringify({
						repo_uid: repoEntity.repoUid,
						name: repoEntity.name,
						path: repoEntity.rootPath,
						snapshot: snapshot
							? {
									snapshot_uid: snapshot.snapshotUid,
									kind: snapshot.kind,
									status: snapshot.status,
									basis_commit: snapshot.basisCommit,
									files_total: snapshot.filesTotal,
									nodes_total: snapshot.nodesTotal,
									edges_total: snapshot.edgesTotal,
									created_at: snapshot.createdAt,
								}
							: null,
					}),
				);
			} else {
				console.log(formatRepoStatus(repoEntity, snapshot));
			}
		});

	repo
		.command("refresh <repo>")
		.description(
			"Re-index repository (creates a refresh snapshot linked to the previous one)",
		)
		.option("--json", "JSON output")
		.action(async (repoRef: string, opts: { json?: boolean }) => {
			const ctx = getCtx();
			const repoEntity = resolveRepo(ctx, repoRef);
			if (!repoEntity) {
				outputError(opts.json, `Repository not found: ${repoRef}`);
				process.exitCode = 1;
				return;
			}

			const result = await ctx.indexer.refreshRepo(repoEntity.repoUid);

			if (opts.json) {
				console.log(
					JSON.stringify({
						snapshot_uid: result.snapshotUid,
						files_total: result.filesTotal,
						nodes_total: result.nodesTotal,
						edges_total: result.edgesTotal,
						edges_unresolved: result.edgesUnresolved,
						duration_ms: result.durationMs,
					}),
				);
			} else {
				console.log(formatIndexResult(result));
			}
		});

	repo
		.command("list")
		.description("List all registered repositories")
		.option("--json", "JSON output")
		.action((opts: { json?: boolean }) => {
			const ctx = getCtx();
			const repos = ctx.storage.listRepos();

			if (opts.json) {
				console.log(
					JSON.stringify(
						repos.map((r) => ({
							repo_uid: r.repoUid,
							name: r.name,
							path: r.rootPath,
						})),
					),
				);
			} else {
				console.log(formatRepoTable(repos));
			}
		});

	repo
		.command("remove <repo>")
		.description("Unregister a repository")
		.option("--json", "JSON output")
		.action((repoRef: string, opts: { json?: boolean }) => {
			const ctx = getCtx();
			const repoEntity = resolveRepo(ctx, repoRef);
			if (!repoEntity) {
				outputError(opts.json, `Repository not found: ${repoRef}`);
				process.exitCode = 1;
				return;
			}

			ctx.storage.removeRepo(repoEntity.repoUid);

			if (opts.json) {
				console.log(JSON.stringify({ removed: repoEntity.repoUid }));
			} else {
				console.log(`Removed "${repoEntity.name}" (${repoEntity.repoUid})`);
			}
		});
}

// ── Helpers ────────────────────────────────────────────────────────────

function resolveRepo(ctx: AppContext, ref: string) {
	return (
		ctx.storage.getRepo({ uid: ref }) ??
		ctx.storage.getRepo({ name: ref }) ??
		ctx.storage.getRepo({ rootPath: resolve(ref) })
	);
}

function outputError(json: boolean | undefined, message: string): void {
	if (json) {
		console.log(JSON.stringify({ error: message }));
	} else {
		console.error(`Error: ${message}`);
	}
}
