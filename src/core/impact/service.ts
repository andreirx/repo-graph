/**
 * Change impact orchestrator.
 *
 * Composes GitPort + StoragePort to compute ImpactResult from a
 * change-scope request. Pure orchestration — no I/O outside the port
 * interfaces, no CLI formatting concerns.
 *
 * Scope handling:
 *   against_snapshot : diff working tree vs snapshot basis_commit.
 *                      Requires basis_commit to be present. Fails
 *                      loudly otherwise.
 *   staged           : diff --cached
 *   since_ref        : diff working tree vs the specified ref
 *
 * The resolved scope is echoed in ImpactResult.scope so consumers
 * always know which diff was taken.
 */

import type { GitDiffScope, GitPort } from "../ports/git.js";
import type { StoragePort } from "../ports/storage.js";
import {
	type ChangedFileSeed,
	type ImpactResult,
	type ImpactScope,
	type ImpactedModule,
	STANDARD_CAVEATS,
	type TrustNote,
} from "./types.js";

export type ScopeRequest =
	| { kind: "against_snapshot" }
	| { kind: "staged" }
	| { kind: "since_ref"; ref: string };

export interface ComputeChangeImpactInput {
	git: GitPort;
	storage: StoragePort;
	repoUid: string;
	repoPath: string;
	snapshotUid: string;
	/**
	 * Basis commit of the snapshot. Required for `against_snapshot`
	 * scope. Null snapshots without commit anchoring cannot be used
	 * with that scope.
	 */
	snapshotBasisCommit: string | null;
	scopeRequest: ScopeRequest;
	/** Undefined = unbounded. Positive integer = hop cap on propagation. */
	maxDepth?: number;
}

export class ImpactScopeError extends Error {
	constructor(message: string) {
		super(message);
		this.name = "ImpactScopeError";
	}
}

/**
 * Compute change impact. Returns an ImpactResult with trust metadata.
 * Throws ImpactScopeError on scope resolution failures.
 */
export async function computeChangeImpact(
	input: ComputeChangeImpactInput,
): Promise<ImpactResult> {
	// 1. Resolve ScopeRequest → ImpactScope (for output) + GitDiffScope (for git)
	const { impactScope, gitScope } = resolveScope(
		input.scopeRequest,
		input.snapshotBasisCommit,
	);

	// 2. Get changed files from git
	const rawChangedFiles = await input.git.getChangedFiles(
		input.repoPath,
		gitScope,
	);

	// Dedupe and sort for determinism. Git diff can repeat entries
	// under rename/copy scenarios, and we want stable output order.
	const changedPaths = Array.from(new Set(rawChangedFiles)).sort();

	// 3. Resolve changed files to modules
	const resolutions = input.storage.resolveChangedFilesToModules({
		snapshotUid: input.snapshotUid,
		repoUid: input.repoUid,
		filePaths: changedPaths,
	});

	// Build ChangedFileSeed list with unmatched reasons
	const changedFiles: ChangedFileSeed[] = resolutions.map((r) => ({
		path: r.filePath,
		matched_to_index: r.matched,
		owning_module: r.owningModuleKey,
		unmatched_reason: r.matched
			? r.owningModuleKey === null
				? "no_owning_module"
				: null
			: "not_in_snapshot",
	}));

	// 4. Build seed module set (unique, sorted for determinism)
	const seedModuleSet = new Set<string>();
	for (const r of resolutions) {
		if (r.matched && r.owningModuleKey !== null) {
			seedModuleSet.add(r.owningModuleKey);
		}
	}
	const seedModules = Array.from(seedModuleSet).sort();

	// 5. Reverse-IMPORTS traversal
	const reverseImports =
		seedModules.length === 0
			? []
			: input.storage.findReverseModuleImports({
					snapshotUid: input.snapshotUid,
					seedModuleKeys: seedModules,
					maxDepth: input.maxDepth,
				});

	// 6. Assemble ImpactedModule list: seeds at distance 0, then traversal
	const impactedModules: ImpactedModule[] = [];
	for (const seed of seedModules) {
		impactedModules.push({
			module: seed,
			distance: 0,
			reason: "seed",
		});
	}
	for (const r of reverseImports) {
		impactedModules.push({
			module: r.moduleStableKey,
			distance: r.distance,
			reason: "reverse_import",
		});
	}
	// Sort: distance ascending, then module key alphabetically
	impactedModules.sort((a, b) => {
		if (a.distance !== b.distance) return a.distance - b.distance;
		return a.module < b.module ? -1 : a.module > b.module ? 1 : 0;
	});

	// 7. Counts
	const matched = changedFiles.filter((f) => f.matched_to_index).length;
	const maxDistance = impactedModules.reduce(
		(max, m) => (m.distance > max ? m.distance : max),
		0,
	);

	// 8. Trust note
	const trust: TrustNote = {
		graph_basis: "reverse_module_imports_only",
		calls_included: false,
		caveats: [...STANDARD_CAVEATS],
	};

	return {
		scope: impactScope,
		snapshot_uid: input.snapshotUid,
		snapshot_basis_commit: input.snapshotBasisCommit,
		trust,
		changed_files: changedFiles,
		seed_modules: seedModules,
		impacted_modules: impactedModules,
		counts: {
			changed_files: changedFiles.length,
			changed_files_matched: matched,
			changed_files_unmatched: changedFiles.length - matched,
			seed_modules: seedModules.length,
			impacted_modules: impactedModules.length,
			max_distance: maxDistance,
		},
	};
}

/**
 * Translate a ScopeRequest into the pair (ImpactScope, GitDiffScope).
 * Throws ImpactScopeError if the request is infeasible (e.g.
 * against_snapshot with no basis_commit).
 *
 * Exported for direct testing.
 */
export function resolveScope(
	request: ScopeRequest,
	snapshotBasisCommit: string | null,
): { impactScope: ImpactScope; gitScope: GitDiffScope } {
	switch (request.kind) {
		case "against_snapshot":
			if (snapshotBasisCommit === null) {
				throw new ImpactScopeError(
					"Scope 'against_snapshot' requires the snapshot to have a basis_commit. " +
						"Re-index the repository from a git working tree, or use --staged or --since <ref>.",
				);
			}
			return {
				impactScope: {
					kind: "against_snapshot",
					basis_commit: snapshotBasisCommit,
				},
				gitScope: {
					kind: "working_tree_vs_commit",
					commit: snapshotBasisCommit,
				},
			};
		case "staged":
			return {
				impactScope: { kind: "staged" },
				gitScope: { kind: "staged" },
			};
		case "since_ref":
			return {
				impactScope: { kind: "since_ref", ref: request.ref },
				gitScope: { kind: "working_tree_vs_commit", commit: request.ref },
			};
	}
}
