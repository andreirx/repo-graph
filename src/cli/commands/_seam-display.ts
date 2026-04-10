/**
 * CLI seam display helpers — presentation-only.
 *
 * Shared by `surfaces show` and `modules show` to produce per-surface
 * direct display rows from raw storage DTOs.
 *
 * Strict scope:
 *  - DTO shaping / display-row preparation
 *  - count grouping
 *  - defensive metadata parsing
 *
 * Out of scope:
 *  - storage access (callers fetch and pass in)
 *  - cross-surface aggregation (lives in core/seams/module-seam-rollup)
 *  - any architectural decisions
 *
 * This file is CLI presentation code. It must not be imported from
 * anywhere outside src/cli/.
 */

import type {
	EnvAccessKind,
	SurfaceEnvDependency,
	SurfaceEnvEvidence,
} from "../../core/seams/env-dependency.js";
import type {
	MutationKind,
	SurfaceFsMutation,
	SurfaceFsMutationEvidence,
} from "../../core/seams/fs-mutation.js";

// ── Env display rows ───────────────────────────────────────────────

export interface EnvDisplayRow {
	envName: string;
	accessKind: EnvAccessKind;
	defaultValue: string | null;
	evidenceCount: number;
	confidence: number;
}

/**
 * Map env dependency identity rows to display rows, attaching the
 * count of evidence rows that back each identity. Output sorted by
 * envName ascending.
 *
 * Caller is responsible for fetching `deps` and `evidence` together
 * (typically via querySurfaceEnvDependencies +
 * querySurfaceEnvEvidenceBySurface).
 */
export function buildEnvRows(
	deps: readonly SurfaceEnvDependency[],
	evidence: readonly SurfaceEnvEvidence[],
): EnvDisplayRow[] {
	const counts = new Map<string, number>();
	for (const e of evidence) {
		counts.set(
			e.surfaceEnvDependencyUid,
			(counts.get(e.surfaceEnvDependencyUid) ?? 0) + 1,
		);
	}
	const rows: EnvDisplayRow[] = deps.map((d) => ({
		envName: d.envName,
		accessKind: d.accessKind,
		defaultValue: d.defaultValue,
		evidenceCount: counts.get(d.surfaceEnvDependencyUid) ?? 0,
		confidence: d.confidence,
	}));
	rows.sort((a, b) => a.envName.localeCompare(b.envName));
	return rows;
}

// ── FS literal display rows ────────────────────────────────────────

export interface FsLiteralDisplayRow {
	targetPath: string;
	mutationKind: MutationKind;
	evidenceCount: number;
	confidence: number;
	destinationPaths: string[];
}

/**
 * Map fs mutation identity rows to display rows, attaching the count
 * of literal-path evidence rows backing each identity. Dynamic-path
 * evidence (null surfaceFsMutationUid) is excluded here — callers
 * summarize it separately via summarizeFsDynamicEvidence.
 *
 * Output sorted by targetPath then mutationKind ascending.
 */
export function buildFsLiteralRows(
	mutations: readonly SurfaceFsMutation[],
	evidence: readonly SurfaceFsMutationEvidence[],
): FsLiteralDisplayRow[] {
	const counts = new Map<string, number>();
	for (const e of evidence) {
		if (e.surfaceFsMutationUid === null) continue;
		counts.set(
			e.surfaceFsMutationUid,
			(counts.get(e.surfaceFsMutationUid) ?? 0) + 1,
		);
	}
	const rows: FsLiteralDisplayRow[] = mutations.map((m) => ({
		targetPath: m.targetPath,
		mutationKind: m.mutationKind,
		evidenceCount: counts.get(m.surfaceFsMutationUid) ?? 0,
		confidence: m.confidence,
		destinationPaths: extractDestinationPaths(m.metadataJson),
	}));
	rows.sort((a, b) => {
		const byPath = a.targetPath.localeCompare(b.targetPath);
		if (byPath !== 0) return byPath;
		return a.mutationKind.localeCompare(b.mutationKind);
	});
	return rows;
}

// ── Metadata parsing ───────────────────────────────────────────────

/**
 * Defensively extract destinationPaths from fs mutation metadata.
 *
 * Malformed JSON or unexpected shapes return an empty array. The
 * pure module-seam-rollup core uses the same defensive policy — the
 * detector layer is responsible for well-formed payloads, and the
 * CLI must not turn malformed metadata into user-facing errors.
 */
export function extractDestinationPaths(metadataJson: string | null): string[] {
	if (!metadataJson) return [];
	try {
		const meta = JSON.parse(metadataJson) as { destinationPaths?: unknown };
		if (!Array.isArray(meta.destinationPaths)) return [];
		return meta.destinationPaths.filter((d): d is string => typeof d === "string");
	} catch {
		return [];
	}
}
