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

import type { Command } from "commander";
import type { AppContext } from "../../main.js";

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
				outputError(opts.json, `Repository not found or not indexed: ${repoRef}`);
				process.exitCode = 1;
				return;
			}
			const { snapshotUid } = resolved;

			const rollups = ctx.storage.queryModuleCandidateRollups(snapshotUid);

			if (rollups.length === 0) {
				if (opts.json) {
					process.stdout.write(JSON.stringify([]));
				} else {
					process.stdout.write("No discovered modules. Index with manifest/workspace detection enabled.\n");
				}
				return;
			}

			if (opts.json) {
				process.stdout.write(JSON.stringify(rollups, null, 2));
				return;
			}

			// Human-readable table.
			const header = padRow([
				"MODULE", "NAME", "KIND", "CONF", "FILES", "SYMBOLS",
				"TESTS", "EVIDENCE", "LANGS", "DIR_MOD",
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
		.action((repoRef: string, moduleQuery: string, opts: { json?: boolean }) => {
			const ctx = getCtx();
			const resolved = resolveRepoAndSnapshot(ctx, repoRef);
			if (!resolved) {
				outputError(opts.json, `Repository not found or not indexed: ${repoRef}`);
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

			const header = padRow(["SOURCE_TYPE", "SOURCE_PATH", "EVIDENCE_KIND", "CONF", "PAYLOAD"]);
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

	// ── files ───────────────────────────────────────────────────────

	modules
		.command("files <repo> <module>")
		.description("List files owned by a discovered module")
		.option("--json", "JSON output")
		.option("--limit <n>", "Max files to show", "50")
		.action((repoRef: string, moduleQuery: string, opts: { json?: boolean; limit?: string }) => {
			const ctx = getCtx();
			const resolved = resolveRepoAndSnapshot(ctx, repoRef);
			if (!resolved) {
				outputError(opts.json, `Repository not found or not indexed: ${repoRef}`);
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

			const header = padRow(["FILE", "LANGUAGE", "TEST", "ASSIGNMENT", "CONF"]);
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
