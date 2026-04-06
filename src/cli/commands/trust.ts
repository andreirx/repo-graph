/**
 * rgr trust — extraction trust reporting surface.
 *
 * Two subcommands:
 *
 *   rgr trust <repo>
 *       Default subcommand. Full reliability report (Tier 1a counts
 *       by category + classification).
 *
 *   rgr trust unresolved-samples <repo>
 *       Tier 1b. Per-edge samples joined to source file + line.
 *
 * Both emit JSON with --json, human-readable tables otherwise.
 *
 * Exit codes:
 *   0 — output produced (informational surface, not a gate)
 *   1 — repo/snapshot not found, or invalid filter value
 */

import { resolve as resolvePath } from "node:path";
import type { Command } from "commander";
import { deriveBlastRadius } from "../../core/classification/blast-radius.js";
import { UnresolvedEdgeCategory } from "../../core/diagnostics/unresolved-edge-categories.js";
import {
	UnresolvedEdgeBasisCode,
	UnresolvedEdgeClassification,
} from "../../core/diagnostics/unresolved-edge-classification.js";
import { computeTrustReport } from "../../core/trust/service.js";
import type { TrustReport } from "../../core/trust/types.js";
import type { AppContext } from "../../main.js";

export function registerTrustCommand(
	program: Command,
	getCtx: () => AppContext,
): void {
	const trust = program
		.command("trust")
		.description(
			"Extraction trust reporting: reliability axes, downgrade triggers, unresolved classification",
		);

	// Default subcommand: `rgr trust <repo>` → full report.
	trust
		.command("report <repo>", { isDefault: true })
		.description(
			"Full trust report: reliability levels + category/classification counts",
		)
		.option("--json", "JSON output")
		.action((repoRef: string, opts: { json?: boolean }) => {
			runReport(repoRef, opts, getCtx());
		});

	// Tier 1b: per-edge unresolved samples with source file + line.
	trust
		.command("unresolved-samples <repo>")
		.description(
			"Tier 1b: per-edge unresolved samples (source file + line + classification)",
		)
		.option("--bucket <classification>", "Filter by classification bucket")
		.option("--category <category>", "Filter by extraction failure category")
		.option("--basis-code <code>", "Filter by classifier basis code")
		.option("--blast-radius <level>", "Filter by blast radius (low, medium, high)")
		.option("--limit <n>", "Max rows to return", "20")
		.option("--json", "JSON output")
		.action(
			(
				repoRef: string,
				opts: {
					bucket?: string;
					category?: string;
					basisCode?: string;
					blastRadius?: string;
					limit: string;
					json?: boolean;
				},
			) => {
				runSamples(repoRef, opts, getCtx());
			},
		);
}

// ── Subcommand implementations ──────────────────────────────────────

function runReport(
	repoRef: string,
	opts: { json?: boolean },
	ctx: AppContext,
): void {
	const repo = resolveRepo(ctx, repoRef);
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
				{ command: "trust", repo: repo.name, ...report },
				null,
				2,
			),
		);
	} else {
		printHuman(repo.name, report);
	}
}

function runSamples(
	repoRef: string,
	opts: {
		bucket?: string;
		category?: string;
		basisCode?: string;
		blastRadius?: string;
		limit: string;
		json?: boolean;
	},
	ctx: AppContext,
): void {
	const repo = resolveRepo(ctx, repoRef);
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

	// Validate typed filters.
	if (opts.bucket && !isKnownClassification(opts.bucket)) {
		outputError(
			opts.json,
			`Unknown --bucket value: ${opts.bucket}. Valid: ${validClassifications().join(", ")}`,
		);
		process.exitCode = 1;
		return;
	}
	if (opts.category && !isKnownCategory(opts.category)) {
		outputError(
			opts.json,
			`Unknown --category value: ${opts.category}. Valid: ${validCategories().join(", ")}`,
		);
		process.exitCode = 1;
		return;
	}
	if (opts.basisCode && !isKnownBasisCode(opts.basisCode)) {
		outputError(
			opts.json,
			`Unknown --basis-code value: ${opts.basisCode}. Valid: ${validBasisCodes().join(", ")}`,
		);
		process.exitCode = 1;
		return;
	}
	const validRadii = ["low", "medium", "high"];
	if (opts.blastRadius && !validRadii.includes(opts.blastRadius)) {
		outputError(
			opts.json,
			`Unknown --blast-radius value: ${opts.blastRadius}. Valid: ${validRadii.join(", ")}`,
		);
		process.exitCode = 1;
		return;
	}

	const limit = Number.parseInt(opts.limit, 10);
	if (!Number.isFinite(limit) || limit <= 0) {
		outputError(opts.json, `Invalid --limit value: ${opts.limit}`);
		process.exitCode = 1;
		return;
	}

	// Fetch more rows than requested if blast-radius filter is active,
	// because the filter is applied post-query (blast-radius is derived,
	// not stored). Overfetch then truncate.
	const queryLimit = opts.blastRadius ? limit * 10 : limit;

	let rows = ctx.storage.queryUnresolvedEdges({
		snapshotUid: snapshot.snapshotUid,
		// biome-ignore lint/suspicious/noExplicitAny: validated above
		classification: opts.bucket as any,
		// biome-ignore lint/suspicious/noExplicitAny: validated above
		category: opts.category as any,
		// biome-ignore lint/suspicious/noExplicitAny: validated above
		basisCode: opts.basisCode as any,
		limit: queryLimit,
	});

	// Enrich rows with derived blast-radius + compiler receiver type.
	const enriched = rows.map((r) => {
		const assessment = deriveBlastRadius({
			category: r.category,
			basisCode: r.basisCode,
			sourceNodeVisibility: r.sourceNodeVisibility,
		});
		// Extract compiler-resolved receiver type from metadata if present.
		let receiverType: string | null = null;
		if (r.metadataJson) {
			try {
				const meta = JSON.parse(r.metadataJson) as Record<string, unknown>;
				const enrichment = meta.enrichment as Record<string, unknown> | undefined;
				if (enrichment?.receiverType) {
					receiverType = String(enrichment.typeDisplayName ?? enrichment.receiverType);
				}
			} catch {
				// ignore
			}
		}
		return { ...r, ...assessment, receiverType };
	});

	// Apply blast-radius filter if requested.
	let filtered = enriched;
	if (opts.blastRadius) {
		filtered = enriched.filter((r) => r.blastRadius === opts.blastRadius);
	}
	const output = filtered.slice(0, limit);

	if (opts.json) {
		console.log(
			JSON.stringify(
				{
					command: "trust unresolved-samples",
					repo: repo.name,
					snapshot_uid: snapshot.snapshotUid,
					filters: {
						bucket: opts.bucket ?? null,
						category: opts.category ?? null,
						basis_code: opts.basisCode ?? null,
						blast_radius: opts.blastRadius ?? null,
						limit,
					},
					count: output.length,
					samples: output,
				},
				null,
				2,
			),
		);
	} else {
		printSamplesHuman(repo.name, output);
	}
}

// ── Validators ──────────────────────────────────────────────────────

function validClassifications(): string[] {
	return Object.values(UnresolvedEdgeClassification);
}
function validCategories(): string[] {
	return Object.values(UnresolvedEdgeCategory);
}
function validBasisCodes(): string[] {
	return Object.values(UnresolvedEdgeBasisCode);
}

function isKnownClassification(v: string): boolean {
	return validClassifications().includes(v);
}
function isKnownCategory(v: string): boolean {
	return validCategories().includes(v);
}
function isKnownBasisCode(v: string): boolean {
	return validBasisCodes().includes(v);
}

// ── Helpers ─────────────────────────────────────────────────────────

function resolveRepo(ctx: AppContext, ref: string) {
	return (
		ctx.storage.getRepo({ uid: ref }) ??
		ctx.storage.getRepo({ name: ref }) ??
		ctx.storage.getRepo({ rootPath: resolvePath(ref) })
	);
}

// ── Human-readable output ───────────────────────────────────────────

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

	const s = report.summary;
	console.log("Counts:");
	console.log(`  edges_total:                  ${s.edges_total}`);
	console.log(`  unresolved_total:             ${s.unresolved_total}`);
	console.log(`  resolved_calls:               ${s.resolved_calls}`);
	console.log(`  unresolved_calls:             ${s.unresolved_calls}`);
	console.log(`  unresolved_calls_external:    ${s.unresolved_calls_external}`);
	console.log(`  unresolved_calls_internal_like: ${s.unresolved_calls_internal_like}`);
	console.log(
		`  call_resolution_rate:         ${(s.call_resolution_rate * 100).toFixed(1)}% (excludes external)`,
	);
	console.log("");

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

	if (report.categories.length > 0) {
		console.log("Unresolved edges by category:");
		for (const c of report.categories) {
			const count = String(c.unresolved).padStart(5);
			console.log(`  ${count}  ${c.label}`);
		}
		console.log("");
	}

	// New: classification breakdown (Tier 1a).
	if (report.classifications.length > 0) {
		console.log("Unresolved edges by classification:");
		for (const c of report.classifications) {
			const count = String(c.count).padStart(5);
			console.log(`  ${count}  ${c.classification}`);
		}
		console.log("");
	}

	// Blast-radius breakdown (scoped to unknown CALLS).
	if (report.unknown_calls_blast_radius) {
		const br = report.unknown_calls_blast_radius;
		const total = br.low + br.medium + br.high;
		if (total > 0) {
			console.log(`Unknown CALLS blast-radius breakdown (${total} edges):`);
			if (br.low > 0) console.log(`  ${String(br.low).padStart(5)}  low    (function-local, private scope)`);
			if (br.medium > 0) console.log(`  ${String(br.medium).padStart(5)}  medium (exported scope or import-bound)`);
			if (br.high > 0) console.log(`  ${String(br.high).padStart(5)}  high   (entrypoint path)`);
			console.log("");
		}
	}

	// Enrichment status (from rgr enrich).
	if (report.enrichment_status) {
		const es = report.enrichment_status;
		const rate = es.eligible > 0
			? ((es.enriched / es.eligible) * 100).toFixed(1)
			: "0";
		console.log(`Compiler enrichment (unknown obj.method() calls):`);
		console.log(`  enriched: ${es.enriched}/${es.eligible} (${rate}%)`);
		if (es.top_types.length > 0) {
			console.log("  Top resolved receiver types:");
			for (const t of es.top_types) {
				const ext = t.isExternal ? " [ext]" : "";
				console.log(`    ${String(t.count).padStart(5)}  ${t.type}${ext}`);
			}
		}
		console.log("");
	} else if (report.classifications.length > 0) {
		console.log("Compiler enrichment: not run (use `rgr enrich` to resolve receiver types)");
		console.log("");
	}

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

	console.log("Caveats:");
	for (const c of report.caveats) {
		console.log(`  - ${c}`);
	}
}

interface SampleRow {
	classification: string;
	category: string;
	basisCode: string;
	targetKey: string;
	sourceFilePath: string | null;
	lineStart: number | null;
	blastRadius?: string;
	receiverType?: string | null;
}

function printSamplesHuman(repoName: string, rows: SampleRow[]): void {
	console.log(`Unresolved samples — ${repoName}`);
	console.log(`Rows: ${rows.length}`);
	if (rows.length === 0) {
		console.log("(no rows match the given filters)");
		return;
	}
	// RECV_TYPE is additive — shown alongside all existing columns
	// when any row has enrichment. Columns are never removed.
	const hasEnrichment = rows.some((r) => r.receiverType);
	console.log("");
	const rtCol = hasEnrichment ? "RECV_TYPE            " : "";
	const header = `BLAST  CLASSIFICATION               BASIS                                 CATEGORY                                     ${rtCol}FILE:LINE                         TARGET`;
	console.log(header);
	console.log("-".repeat(header.length));
	for (const r of rows) {
		const br = pad(r.blastRadius ?? "-", 6);
		const cls = pad(r.classification, 28);
		const basis = pad(r.basisCode, 38);
		const cat = pad(r.category, 44);
		const rt = hasEnrichment ? pad(r.receiverType ?? "-", 21) : "";
		const loc = pad(`${r.sourceFilePath ?? "?"}:${r.lineStart ?? "?"}`, 33);
		console.log(`${br} ${cls} ${basis} ${cat} ${rt} ${loc} ${r.targetKey}`);
	}
}

function pad(s: string, width: number): string {
	if (s.length >= width) return s.slice(0, width - 1) + "…";
	return s.padEnd(width, " ");
}

function outputError(json: boolean | undefined, message: string): void {
	if (json) {
		console.log(JSON.stringify({ error: message }));
	} else {
		console.error(`Error: ${message}`);
	}
}
