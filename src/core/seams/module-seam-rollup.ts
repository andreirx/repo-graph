/**
 * Module-level seam rollup — pure cross-surface aggregation.
 *
 * When a module owns multiple project surfaces, the same env var or
 * fs target may be referenced from more than one surface. This module
 * unions per-surface seam facts into module-level rollup rows.
 *
 * Aggregation is a module-level concern: surfaceCount, hasConflictingDefaults,
 * union semantics are meaningless on a single-surface input. The
 * `surfaces show` command does NOT use these aggregators — it renders
 * its identity rows directly. Only `modules show` consumes them.
 *
 * The fs dynamic-summary helper is the exception — it is shared by both
 * commands because dynamic-evidence summarization is identical regardless
 * of how many surfaces contributed.
 *
 * Pure functions only. No I/O. No storage access.
 */

import type { EnvAccessKind, SurfaceEnvDependency } from "./env-dependency.js";
import type {
	MutationKind,
	SurfaceFsMutation,
	SurfaceFsMutationEvidence,
} from "./fs-mutation.js";

// ── Env aggregation ────────────────────────────────────────────────

/**
 * One env dependency contribution from a single surface, plus the
 * count of evidence rows backing it on that surface.
 *
 * The CLI is expected to compute `evidenceCount` once per (surface,
 * dependency) by grouping the bulk querySurfaceEnvEvidenceBySurface
 * result, so the aggregator never has to touch storage.
 */
export interface EnvAggregationInput {
	readonly surfaceUid: string;
	readonly dependency: SurfaceEnvDependency;
	readonly evidenceCount: number;
}

/**
 * One env var rolled up across all contributing surfaces of a module.
 *
 * Aggregation rules (D5):
 *  - accessKind: required if any contribution is required, otherwise
 *    optional if any is optional, otherwise unknown.
 *  - defaultValue: first non-null default in the input order. Stable
 *    only if the caller passes input in a deterministic order.
 *  - hasConflictingDefaults: true iff two or more distinct non-null
 *    default values were observed.
 *  - surfaceCount: distinct surface UIDs contributing this env var.
 *  - evidenceCount: sum of per-contribution evidence counts.
 *  - maxConfidence: max confidence across contributions.
 */
export interface EnvRollupItem {
	readonly envName: string;
	readonly accessKind: EnvAccessKind;
	readonly defaultValue: string | null;
	readonly hasConflictingDefaults: boolean;
	readonly surfaceCount: number;
	readonly evidenceCount: number;
	readonly maxConfidence: number;
}

/**
 * Aggregate env dependencies across one or more surfaces by env_name.
 *
 * Output is sorted by envName ascending (D6).
 */
export function aggregateEnvAcrossSurfaces(
	input: readonly EnvAggregationInput[],
): EnvRollupItem[] {
	// Group by env name. Map preserves insertion order so the
	// "first non-null default" rule is deterministic given a
	// deterministic input order.
	const groups = new Map<
		string,
		{
			contributions: EnvAggregationInput[];
			surfaces: Set<string>;
			distinctDefaults: Set<string>;
		}
	>();

	for (const item of input) {
		const name = item.dependency.envName;
		let group = groups.get(name);
		if (!group) {
			group = {
				contributions: [],
				surfaces: new Set<string>(),
				distinctDefaults: new Set<string>(),
			};
			groups.set(name, group);
		}
		group.contributions.push(item);
		group.surfaces.add(item.surfaceUid);
		if (item.dependency.defaultValue !== null) {
			group.distinctDefaults.add(item.dependency.defaultValue);
		}
	}

	const out: EnvRollupItem[] = [];
	for (const [envName, group] of groups) {
		const accessKind = aggregateEnvAccessKind(
			group.contributions.map((c) => c.dependency.accessKind),
		);

		// First non-null default in input order.
		let defaultValue: string | null = null;
		for (const c of group.contributions) {
			if (c.dependency.defaultValue !== null) {
				defaultValue = c.dependency.defaultValue;
				break;
			}
		}

		const evidenceCount = group.contributions.reduce(
			(acc, c) => acc + c.evidenceCount,
			0,
		);
		const maxConfidence = group.contributions.reduce(
			(acc, c) => Math.max(acc, c.dependency.confidence),
			0,
		);

		out.push({
			envName,
			accessKind,
			defaultValue,
			hasConflictingDefaults: group.distinctDefaults.size > 1,
			surfaceCount: group.surfaces.size,
			evidenceCount,
			maxConfidence,
		});
	}

	out.sort((a, b) => a.envName.localeCompare(b.envName));
	return out;
}

/**
 * Required dominates optional dominates unknown.
 * Exported for unit testing of the precedence rule in isolation.
 */
export function aggregateEnvAccessKind(
	kinds: readonly EnvAccessKind[],
): EnvAccessKind {
	let sawOptional = false;
	for (const k of kinds) {
		if (k === "required") return "required";
		if (k === "optional") sawOptional = true;
	}
	return sawOptional ? "optional" : "unknown";
}

// ── FS aggregation ─────────────────────────────────────────────────

/**
 * One fs mutation contribution from a single surface, plus the
 * count of evidence rows backing it on that surface (literal-path
 * evidence only — dynamic-path evidence is summarized separately).
 */
export interface FsAggregationInput {
	readonly surfaceUid: string;
	readonly mutation: SurfaceFsMutation;
	readonly evidenceCount: number;
}

/**
 * One (target_path, mutation_kind) pair rolled up across all
 * contributing surfaces of a module.
 *
 * Aggregation rules (D5):
 *  - identity: union by (target_path, mutation_kind).
 *  - surfaceCount: distinct surface UIDs contributing this pair.
 *  - evidenceCount: sum of per-contribution evidence counts.
 *  - destinationPaths: union across all per-surface metadataJson
 *    `destinationPaths` arrays (rename/copy only). Sorted, distinct.
 *  - maxConfidence: max confidence across contributions.
 */
export interface FsRollupItem {
	readonly targetPath: string;
	readonly mutationKind: MutationKind;
	readonly surfaceCount: number;
	readonly evidenceCount: number;
	readonly destinationPaths: string[];
	readonly maxConfidence: number;
}

/**
 * Aggregate fs mutation identities across one or more surfaces by
 * (target_path, mutation_kind).
 *
 * Output is sorted by targetPath then mutationKind ascending (D6).
 */
export function aggregateFsAcrossSurfaces(
	input: readonly FsAggregationInput[],
): FsRollupItem[] {
	const groups = new Map<
		string,
		{
			targetPath: string;
			mutationKind: MutationKind;
			contributions: FsAggregationInput[];
			surfaces: Set<string>;
			destinations: Set<string>;
		}
	>();

	for (const item of input) {
		const key = `${item.mutation.targetPath}\u0000${item.mutation.mutationKind}`;
		let group = groups.get(key);
		if (!group) {
			group = {
				targetPath: item.mutation.targetPath,
				mutationKind: item.mutation.mutationKind,
				contributions: [],
				surfaces: new Set<string>(),
				destinations: new Set<string>(),
			};
			groups.set(key, group);
		}
		group.contributions.push(item);
		group.surfaces.add(item.surfaceUid);

		// Pull destinationPaths from metadata if present.
		if (item.mutation.metadataJson) {
			try {
				const meta = JSON.parse(item.mutation.metadataJson) as {
					destinationPaths?: unknown;
				};
				if (Array.isArray(meta.destinationPaths)) {
					for (const d of meta.destinationPaths) {
						if (typeof d === "string") group.destinations.add(d);
					}
				}
			} catch {
				// Malformed metadata is ignored at the rollup layer.
				// The detector layer is responsible for well-formed payloads.
			}
		}
	}

	const out: FsRollupItem[] = [];
	for (const group of groups.values()) {
		const evidenceCount = group.contributions.reduce(
			(acc, c) => acc + c.evidenceCount,
			0,
		);
		const maxConfidence = group.contributions.reduce(
			(acc, c) => Math.max(acc, c.mutation.confidence),
			0,
		);
		out.push({
			targetPath: group.targetPath,
			mutationKind: group.mutationKind,
			surfaceCount: group.surfaces.size,
			evidenceCount,
			destinationPaths: [...group.destinations].sort(),
			maxConfidence,
		});
	}

	out.sort((a, b) => {
		const byPath = a.targetPath.localeCompare(b.targetPath);
		if (byPath !== 0) return byPath;
		return a.mutationKind.localeCompare(b.mutationKind);
	});
	return out;
}

// ── FS dynamic evidence summary (shared by both commands) ─────────

/**
 * Summary of dynamic-path fs mutation evidence.
 *
 * Dynamic-path occurrences have no identity row (the target path is
 * non-literal). They are still observable as evidence and matter for
 * orientation: a surface that mutates "some path" computed at runtime
 * is a surface that mutates the filesystem.
 *
 * Pure transformation. Used by both `surfaces show` (single surface)
 * and `modules show` (rolled up across surfaces). The caller is
 * responsible for assembling the input evidence array.
 */
export interface FsDynamicSummary {
	readonly totalCount: number;
	readonly distinctFileCount: number;
	readonly byKind: Readonly<Partial<Record<MutationKind, number>>>;
}

export function summarizeFsDynamicEvidence(
	evidence: readonly SurfaceFsMutationEvidence[],
): FsDynamicSummary {
	const distinctFiles = new Set<string>();
	const byKind: Partial<Record<MutationKind, number>> = {};
	let totalCount = 0;

	for (const e of evidence) {
		if (!e.dynamicPath) continue;
		totalCount++;
		distinctFiles.add(e.sourceFilePath);
		byKind[e.mutationKind] = (byKind[e.mutationKind] ?? 0) + 1;
	}

	return {
		totalCount,
		distinctFileCount: distinctFiles.size,
		byKind,
	};
}
