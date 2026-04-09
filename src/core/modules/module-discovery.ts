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
 *   - Assigns file ownership by root containment (longest prefix match)
 *   - Computes aggregate confidence per candidate from evidence items
 *   - Generates stable module keys anchored by repo-relative root path
 *
 * Slice 1 constraints:
 *   - Declared modules only (manifest/workspace evidence)
 *   - Single owner per file (longest-prefix-match wins)
 *   - Additive — does not replace or interact with directory MODULE nodes
 */

import { v4 as uuidv4 } from "uuid";
import type { TrackedFile } from "../model/index.js";
import {
	buildModuleKey,
	type ModuleCandidate,
	type ModuleCandidateEvidence,
	type ModuleFileOwnership,
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
 * Assign each tracked file to its closest declared module candidate.
 *
 * Uses longest-prefix match on the file path against candidate root
 * paths. If a file is not under any candidate root, it gets no
 * ownership row (unowned files are expected in slice 1).
 *
 * In slice 1, each file has at most one owner (the most specific
 * containing module root).
 */
function assignFileOwnership(
	candidates: ModuleCandidate[],
	trackedFiles: TrackedFile[],
	snapshotUid: string,
	repoUid: string,
): ModuleFileOwnership[] {
	if (candidates.length === 0) return [];

	// Sort candidates by root path length descending for longest-prefix-first.
	const sorted = [...candidates].sort(
		(a, b) => b.canonicalRootPath.length - a.canonicalRootPath.length,
	);

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
				rule: "longest_prefix_match",
				rootPath: owner.canonicalRootPath,
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

// ── Helpers ────────────────────────────────────────────────────────

/**
 * Normalize a root path: strip trailing slashes, collapse "." to ".".
 */
function normalizeRootPath(path: string): string {
	let p = path.replace(/\/+$/, "");
	if (p === "" || p === ".") return ".";
	return p;
}
