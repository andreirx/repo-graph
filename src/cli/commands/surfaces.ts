/**
 * rgr surfaces <repo> — project surface query surface.
 *
 * Separate from `modules` (module identity/ownership) and `graph`
 * (structural code graph). Surfaces describe how modules are
 * operationalized: CLI tools, libraries, backend services, etc.
 *
 * Subcommands:
 *   rgr surfaces list <repo>                — catalog with kind/build/runtime
 *   rgr surfaces evidence <repo> <surface>  — evidence for a surface
 */

import type { Command } from "commander";
import type { AppContext } from "../../main.js";

export function registerSurfacesCommands(
	program: Command,
	getCtx: () => AppContext,
): void {
	const surfaces = program
		.command("surfaces")
		.description("Project surface catalog and evidence");

	// ── list ────────────────────────────────────────────────────────

	surfaces
		.command("list <repo>")
		.description("List detected project surfaces with build/runtime context")
		.option("--json", "JSON output")
		.action((repoRef: string, opts: { json?: boolean }) => {
			const ctx = getCtx();
			const resolved = resolveRepoAndSnapshot(ctx, repoRef);
			if (!resolved) {
				outputError(opts.json, `Repository not found or not indexed: ${repoRef}`);
				process.exitCode = 1;
				return;
			}
			const { snapshotUid } = resolved;

			const surfaceRows = ctx.storage.queryProjectSurfaces(snapshotUid);

			if (surfaceRows.length === 0) {
				if (opts.json) {
					process.stdout.write("[]");
				} else {
					process.stdout.write("No project surfaces detected. Index with module discovery enabled.\n");
				}
				return;
			}

			// Enrich with module candidate display names.
			const candidates = ctx.storage.queryModuleCandidates(snapshotUid);
			const candidateMap = new Map(candidates.map((c) => [c.moduleCandidateUid, c]));

			// Count evidence and config roots per surface.
			const allEvidence = ctx.storage.queryAllProjectSurfaceEvidence(snapshotUid);
			const evidenceCountMap = new Map<string, number>();
			for (const e of allEvidence) {
				evidenceCountMap.set(
					e.projectSurfaceUid,
					(evidenceCountMap.get(e.projectSurfaceUid) ?? 0) + 1,
				);
			}
			const allConfigRoots = ctx.storage.queryAllSurfaceConfigRoots(snapshotUid);
			const configCountMap = new Map<string, number>();
			for (const cr of allConfigRoots) {
				configCountMap.set(
					cr.projectSurfaceUid,
					(configCountMap.get(cr.projectSurfaceUid) ?? 0) + 1,
				);
			}
			// Primary entrypoint per surface.
			const allEntrypoints = ctx.storage.queryAllSurfaceEntrypoints(snapshotUid);
			const primaryEntryMap = new Map<string, string>();
			for (const ep of allEntrypoints) {
				if (!primaryEntryMap.has(ep.projectSurfaceUid)) {
					primaryEntryMap.set(
						ep.projectSurfaceUid,
						ep.entrypointPath ?? ep.entrypointTarget ?? ep.displayName ?? "-",
					);
				}
			}

			if (opts.json) {
				const jsonRows = surfaceRows.map((s) => ({
					projectSurfaceUid: s.projectSurfaceUid,
					moduleCandidateUid: s.moduleCandidateUid,
					moduleDisplayName: candidateMap.get(s.moduleCandidateUid)?.displayName ?? null,
					surfaceKind: s.surfaceKind,
					displayName: s.displayName,
					rootPath: s.rootPath,
					entrypointPath: s.entrypointPath,
					primaryEntrypoint: primaryEntryMap.get(s.projectSurfaceUid) ?? null,
					buildSystem: s.buildSystem,
					runtimeKind: s.runtimeKind,
					confidence: s.confidence,
					evidenceCount: evidenceCountMap.get(s.projectSurfaceUid) ?? 0,
					configRootCount: configCountMap.get(s.projectSurfaceUid) ?? 0,
				}));
				process.stdout.write(JSON.stringify(jsonRows, null, 2));
				return;
			}

			// Human-readable table.
			const header = padRow([
				"MODULE", "SURFACE", "KIND", "BUILD", "RUNTIME",
				"ENTRYPOINT", "CONF", "EVID", "CFGS",
			]);
			process.stdout.write(header + "\n");

			for (const s of surfaceRows) {
				const moduleName = candidateMap.get(s.moduleCandidateUid)?.displayName
					?? candidateMap.get(s.moduleCandidateUid)?.canonicalRootPath
					?? "-";
				const entrypoint = primaryEntryMap.get(s.projectSurfaceUid) ?? s.entrypointPath ?? "-";
				const row = padRow([
					moduleName,
					s.displayName ?? s.surfaceKind,
					s.surfaceKind,
					s.buildSystem,
					s.runtimeKind,
					entrypoint,
					s.confidence.toFixed(2),
					String(evidenceCountMap.get(s.projectSurfaceUid) ?? 0),
					String(configCountMap.get(s.projectSurfaceUid) ?? 0),
				]);
				process.stdout.write(row + "\n");
			}
		});

	// ── evidence ────────────────────────────────────────────────────

	surfaces
		.command("evidence <repo> <surface>")
		.description("Show evidence items for a project surface")
		.option("--json", "JSON output")
		.action((repoRef: string, surfaceQuery: string, opts: { json?: boolean }) => {
			const ctx = getCtx();
			const resolved = resolveRepoAndSnapshot(ctx, repoRef);
			if (!resolved) {
				outputError(opts.json, `Repository not found or not indexed: ${repoRef}`);
				process.exitCode = 1;
				return;
			}
			const { snapshotUid } = resolved;

			// Resolve surface by display name, surface kind, or UID prefix.
			// Reject ambiguous matches — force the user to narrow the query.
			const allSurfaces = ctx.storage.queryProjectSurfaces(snapshotUid);
			const matches = allSurfaces.filter(
				(s) =>
					s.displayName === surfaceQuery ||
					s.surfaceKind === surfaceQuery ||
					s.projectSurfaceUid.startsWith(surfaceQuery),
			);

			if (matches.length === 0) {
				outputError(opts.json, `Surface not found: ${surfaceQuery}`);
				process.exitCode = 1;
				return;
			}
			if (matches.length > 1) {
				const names = matches.map((s) =>
					`${s.displayName ?? s.surfaceKind} (${s.rootPath})`,
				).join(", ");
				outputError(opts.json, `Ambiguous surface query "${surfaceQuery}" matches ${matches.length}: ${names}. Use a more specific name or UID prefix.`);
				process.exitCode = 1;
				return;
			}
			const surface = matches[0];

			const evidence = ctx.storage.queryProjectSurfaceEvidence(
				surface.projectSurfaceUid,
			);

			if (opts.json) {
				process.stdout.write(JSON.stringify(evidence, null, 2));
				return;
			}

			process.stdout.write(
				`Surface: ${surface.displayName ?? surface.surfaceKind} (${surface.surfaceKind})\n`,
			);
			process.stdout.write(
				`Module: ${surface.rootPath}\n`,
			);
			process.stdout.write(
				`Build: ${surface.buildSystem} | Runtime: ${surface.runtimeKind}\n\n`,
			);

			if (evidence.length === 0) {
				process.stdout.write("No evidence items.\n");
				return;
			}

			const header = padRow([
				"SOURCE_TYPE", "SOURCE_PATH", "EVIDENCE_KIND", "CONF", "PAYLOAD",
			]);
			process.stdout.write(header + "\n");

			for (const e of evidence) {
				const payload = e.payloadJson
					? truncate(e.payloadJson, 60)
					: "-";
				const row = padRow([
					e.sourceType,
					e.sourcePath,
					e.evidenceKind,
					e.confidence.toFixed(2),
					payload,
				]);
				process.stdout.write(row + "\n");
			}
		});

	// ── show ─────────────────────────────────────────────────────────

	surfaces
		.command("show <repo> <surface>")
		.description("Full detail for one project surface: module, build, runtime, config roots, entrypoints, evidence")
		.option("--json", "JSON output")
		.action((repoRef: string, surfaceQuery: string, opts: { json?: boolean }) => {
			const ctx = getCtx();
			const resolved = resolveRepoAndSnapshot(ctx, repoRef);
			if (!resolved) {
				outputError(opts.json, `Repository not found or not indexed: ${repoRef}`);
				process.exitCode = 1;
				return;
			}
			const { snapshotUid } = resolved;

			const allSurfaces = ctx.storage.queryProjectSurfaces(snapshotUid);
			const matches = allSurfaces.filter(
				(s) =>
					s.displayName === surfaceQuery ||
					s.surfaceKind === surfaceQuery ||
					s.projectSurfaceUid.startsWith(surfaceQuery),
			);

			if (matches.length === 0) {
				outputError(opts.json, `Surface not found: ${surfaceQuery}`);
				process.exitCode = 1;
				return;
			}
			if (matches.length > 1) {
				const names = matches.map((s) =>
					`${s.displayName ?? s.surfaceKind} (${s.rootPath})`,
				).join(", ");
				outputError(opts.json, `Ambiguous surface query "${surfaceQuery}" matches ${matches.length}: ${names}. Use a more specific name or UID prefix.`);
				process.exitCode = 1;
				return;
			}
			const surface = matches[0];

			// Gather all related data.
			const candidates = ctx.storage.queryModuleCandidates(snapshotUid);
			const owningModule = candidates.find(
				(c) => c.moduleCandidateUid === surface.moduleCandidateUid,
			);
			const evidence = ctx.storage.queryProjectSurfaceEvidence(surface.projectSurfaceUid);
			const configRoots = ctx.storage.querySurfaceConfigRoots(surface.projectSurfaceUid);
			const entrypoints = ctx.storage.querySurfaceEntrypoints(surface.projectSurfaceUid);

			if (opts.json) {
				process.stdout.write(JSON.stringify({
					surface: {
						projectSurfaceUid: surface.projectSurfaceUid,
						surfaceKind: surface.surfaceKind,
						displayName: surface.displayName,
						rootPath: surface.rootPath,
						buildSystem: surface.buildSystem,
						runtimeKind: surface.runtimeKind,
						confidence: surface.confidence,
					},
					module: owningModule ? {
						moduleCandidateUid: owningModule.moduleCandidateUid,
						moduleKey: owningModule.moduleKey,
						displayName: owningModule.displayName,
						canonicalRootPath: owningModule.canonicalRootPath,
					} : null,
					configRoots: configRoots.map((cr) => ({
						configPath: cr.configPath,
						configKind: cr.configKind,
						confidence: cr.confidence,
					})),
					entrypoints: entrypoints.map((ep) => ({
						entrypointPath: ep.entrypointPath,
						entrypointTarget: ep.entrypointTarget,
						entrypointKind: ep.entrypointKind,
						displayName: ep.displayName,
						confidence: ep.confidence,
					})),
					evidence: evidence.map((e) => ({
						sourceType: e.sourceType,
						sourcePath: e.sourcePath,
						evidenceKind: e.evidenceKind,
						confidence: e.confidence,
						payload: e.payloadJson ? JSON.parse(e.payloadJson) : null,
					})),
				}, null, 2));
				return;
			}

			// Human-readable detail view.
			process.stdout.write(`Surface: ${surface.displayName ?? surface.surfaceKind}\n`);
			process.stdout.write(`Kind: ${surface.surfaceKind}\n`);
			process.stdout.write(`Root: ${surface.rootPath}\n`);
			process.stdout.write(`Build: ${surface.buildSystem}\n`);
			process.stdout.write(`Runtime: ${surface.runtimeKind}\n`);
			process.stdout.write(`Confidence: ${surface.confidence.toFixed(2)}\n`);
			process.stdout.write(`\n`);

			if (owningModule) {
				process.stdout.write(`Module: ${owningModule.displayName ?? owningModule.canonicalRootPath}\n`);
				process.stdout.write(`Module Key: ${owningModule.moduleKey}\n`);
				process.stdout.write(`\n`);
			}

			if (configRoots.length > 0) {
				process.stdout.write(`Config Roots (${configRoots.length}):\n`);
				for (const cr of configRoots) {
					process.stdout.write(`  ${cr.configKind.padEnd(22)} ${cr.configPath}\n`);
				}
				process.stdout.write(`\n`);
			}

			if (entrypoints.length > 0) {
				process.stdout.write(`Entrypoints (${entrypoints.length}):\n`);
				for (const ep of entrypoints) {
					const target = ep.entrypointPath ?? ep.entrypointTarget ?? "-";
					process.stdout.write(`  ${ep.entrypointKind.padEnd(16)} ${ep.displayName ?? "-"} -> ${target}\n`);
				}
				process.stdout.write(`\n`);
			}

			if (evidence.length > 0) {
				process.stdout.write(`Evidence (${evidence.length}):\n`);
				for (const e of evidence) {
					process.stdout.write(`  ${e.sourceType.padEnd(25)} ${e.evidenceKind.padEnd(20)} ${e.sourcePath}\n`);
				}
			}
		});
}

// ── Helpers ────────────────────────────────────────────────────────

function resolveRepoAndSnapshot(
	ctx: AppContext,
	repoRef: string,
): { repo: ReturnType<typeof ctx.storage.listRepos>[0]; snapshotUid: string } | null {
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
	const widths = [25, 20, 18, 18, 15, 35, 6, 9];
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
