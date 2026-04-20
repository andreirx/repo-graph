/**
 * Module discovery orchestrator — pure core logic.
 *
 * Takes detected manifest/workspace facts and tracked files as input.
 * Produces ModuleCandidate[], ModuleCandidateEvidence[], and
 * ModuleFileOwnership[] for persistence.
 *
 * No filesystem access. No storage calls. No side effects.
 *
 * Responsibilities:
 *   - Deduplicates module candidates by root path
 *   - Merges evidence from multiple sources for the same root
 *   - Assigns file ownership with kind-precedence tiebreaks
 *   - Computes aggregate confidence per candidate from evidence items
 *   - Generates stable module keys anchored by repo-relative root path
 *
 * Ownership algorithm (see docs/architecture/module-discovery-layers.txt §5):
 *   - Longest-prefix match on file path vs candidate root
 *   - Kind precedence tiebreak for equal roots: declared > operational > inferred
 *
 * Additive: does not replace or interact with directory MODULE nodes.
 */

import { v4 as uuidv4 } from "uuid";
import type { TrackedFile } from "../model/index.js";
import {
	buildModuleKey,
	type ModuleCandidate,
	type ModuleCandidateEvidence,
	type ModuleFileOwnership,
	type ModuleKind,
} from "./module-candidate.js";
import type { DiscoveredModuleRoot } from "./manifest-detectors.js";

// ── Input/Output types ─────────────────────────────────────────────

export interface ModuleDiscoveryInput {
	repoUid: string;
	snapshotUid: string;
	/** All discovered module roots from manifest detectors. */
	discoveredRoots: DiscoveredModuleRoot[];
	/** All tracked files in the snapshot (for ownership assignment). */
	trackedFiles: TrackedFile[];
}

export interface ModuleDiscoveryResult {
	candidates: ModuleCandidate[];
	evidence: ModuleCandidateEvidence[];
	ownership: ModuleFileOwnership[];
}

// ── Orchestrator ───────────────────────────────────────────────────

/**
 * Run module discovery: deduplicate candidates, merge evidence,
 * assign file ownership.
 */
export function discoverModules(input: ModuleDiscoveryInput): ModuleDiscoveryResult {
	const { repoUid, snapshotUid, discoveredRoots, trackedFiles } = input;

	// 1. Group discovered roots by canonical root path.
	//    Multiple evidence items may point to the same root
	//    (e.g., package.json workspace + pnpm-workspace.yaml).
	const rootGroups = new Map<string, {
		roots: DiscoveredModuleRoot[];
		displayName: string | null;
	}>();

	for (const root of discoveredRoots) {
		const normalized = normalizeRootPath(root.rootPath);
		const existing = rootGroups.get(normalized);
		if (existing) {
			existing.roots.push(root);
			// Prefer the first non-null display name.
			if (!existing.displayName && root.displayName) {
				existing.displayName = root.displayName;
			}
		} else {
			rootGroups.set(normalized, {
				roots: [root],
				displayName: root.displayName,
			});
		}
	}

	// 2. Create candidates and evidence.
	const candidates: ModuleCandidate[] = [];
	const evidence: ModuleCandidateEvidence[] = [];

	for (const [rootPath, group] of rootGroups) {
		const candidateUid = uuidv4();
		const moduleKey = buildModuleKey(repoUid, rootPath);

		// Aggregate confidence: max of evidence confidences.
		const maxConfidence = Math.max(...group.roots.map((r) => r.confidence));

		candidates.push({
			moduleCandidateUid: candidateUid,
			snapshotUid,
			repoUid,
			moduleKey,
			moduleKind: "declared",
			canonicalRootPath: rootPath,
			confidence: maxConfidence,
			displayName: group.displayName,
			metadataJson: null,
		});

		for (const root of group.roots) {
			evidence.push({
				evidenceUid: uuidv4(),
				moduleCandidateUid: candidateUid,
				snapshotUid,
				repoUid,
				sourceType: root.sourceType,
				sourcePath: root.sourcePath,
				evidenceKind: root.evidenceKind,
				confidence: root.confidence,
				payloadJson: root.payload ? JSON.stringify(root.payload) : null,
			});
		}
	}

	// 3. Assign file ownership by root containment.
	//    For each file, find the longest-prefix-matching candidate root.
	//    This gives the most specific declared module ownership.
	const ownership = assignFileOwnership(
		candidates,
		trackedFiles,
		snapshotUid,
		repoUid,
	);

	return { candidates, evidence, ownership };
}

// ── File ownership assignment ──────────────────────────────────────

/**
 * Kind precedence for ownership tiebreaks.
 * Lower value = stronger precedence.
 *
 * See docs/architecture/module-discovery-layers.txt §5.
 */
const KIND_PRECEDENCE: Record<ModuleKind, number> = {
	declared: 0,
	operational: 1,
	inferred: 2,
};

/**
 * Assign each tracked file to its owning module candidate.
 *
 * Algorithm:
 *   1. Longest-prefix match on file path against candidate root paths
 *   2. Kind precedence tiebreak for equal roots (declared > operational > inferred)
 *
 * See docs/architecture/module-discovery-layers.txt §5 for the full contract.
 */
function assignFileOwnership(
	candidates: ModuleCandidate[],
	trackedFiles: TrackedFile[],
	snapshotUid: string,
	repoUid: string,
): ModuleFileOwnership[] {
	if (candidates.length === 0) return [];

	// Sort candidates by:
	//   1. Root path length descending (longest prefix first)
	//   2. Kind precedence ascending (declared > operational > inferred)
	const sorted = [...candidates].sort((a, b) => {
		const lengthDiff = b.canonicalRootPath.length - a.canonicalRootPath.length;
		if (lengthDiff !== 0) return lengthDiff;
		return KIND_PRECEDENCE[a.moduleKind] - KIND_PRECEDENCE[b.moduleKind];
	});

	const ownership: ModuleFileOwnership[] = [];

	for (const file of trackedFiles) {
		if (file.isExcluded) continue;

		const owner = findOwner(file.path, sorted);
		if (!owner) continue;

		ownership.push({
			snapshotUid,
			repoUid,
			fileUid: file.fileUid,
			moduleCandidateUid: owner.moduleCandidateUid,
			assignmentKind: "root_containment",
			confidence: owner.confidence,
			basisJson: JSON.stringify({
				rule: "longest_prefix_kind_precedence",
				rootPath: owner.canonicalRootPath,
				moduleKind: owner.moduleKind,
			}),
		});
	}

	return ownership;
}

/**
 * Find the most specific candidate that contains this file path.
 * A root of "." matches everything. A root of "packages/api" matches
 * "packages/api/src/index.ts" but not "packages/web/src/index.ts".
 */
function findOwner(
	filePath: string,
	sortedCandidates: ModuleCandidate[],
): ModuleCandidate | null {
	for (const candidate of sortedCandidates) {
		const root = candidate.canonicalRootPath;
		if (root === ".") return candidate;
		if (filePath === root || filePath.startsWith(root + "/")) {
			return candidate;
		}
	}
	return null;
}

// ── Exported ownership computation ─────────────────────────────────

/**
 * Compute file ownership for a set of candidates.
 *
 * Exported for use by the indexer when adding operational modules
 * after declared modules have already been processed.
 *
 * Uses the same kind-precedence-aware longest-prefix algorithm as
 * the internal assignFileOwnership function.
 */
export function assignFileOwnershipForCandidates(
	candidates: ModuleCandidate[],
	trackedFiles: TrackedFile[],
	snapshotUid: string,
	repoUid: string,
): ModuleFileOwnership[] {
	return assignFileOwnership(candidates, trackedFiles, snapshotUid, repoUid);
}

// ── Helpers ────────────────────────────────────────────────────────

/**
 * Normalize a root path: strip trailing slashes, collapse "." to ".".
 */
function normalizeRootPath(path: string): string {
	let p = path.replace(/\/+$/, "");
	if (p === "" || p === ".") return ".";
	return p;
}
