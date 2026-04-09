/**
 * Invalidation planner — pure core logic for delta indexing.
 *
 * Takes parent snapshot file versions (content hashes) and current
 * file content hashes. Produces an InvalidationPlan classifying
 * every file as unchanged/changed/new/deleted/config-widened.
 *
 * No filesystem access. No storage calls. No side effects.
 *
 * Slice 1 invalidation rules:
 *   - Content hash match → unchanged (copy forward)
 *   - Content hash mismatch → changed (re-extract)
 *   - Not in parent → new (extract fresh)
 *   - In parent but not in current tree → deleted
 *   - Root manifest/config changed → widen all files (conservative)
 *   - Nested manifest/config changed → widen files under that directory
 *
 * NOT in slice 1:
 *   - Interface-change narrowing (body_only vs interface_changed)
 *   - Reverse-importer invalidation
 *   - Selective derived-artifact recomputation scoping
 */

import {
	CONFIG_FILE_PATTERNS,
	type ChangedConfig,
	type ConfigType,
	type FileClassification,
	type FileDisposition,
	type InvalidationCounts,
	type InvalidationPlan,
} from "./invalidation-plan.js";

// ── Input types ────────────────────────────────────────────────────

/**
 * Parent snapshot's per-file content hashes.
 * Map: fileUid → contentHash.
 */
export type ParentFileHashes = ReadonlyMap<string, string>;

/**
 * Current filesystem state: which files exist and their content hashes.
 */
export interface CurrentFileState {
	/** File UID (repoUid:path format). */
	readonly fileUid: string;
	/** Repo-relative path. */
	readonly path: string;
	/** Content hash (SHA256/16). */
	readonly contentHash: string;
}

// ── Planner ────────────────────────────────────────────────────────

/**
 * Build an invalidation plan by comparing current files against the
 * parent snapshot's file versions.
 */
export function buildInvalidationPlan(
	parentSnapshotUid: string,
	parentHashes: ParentFileHashes,
	currentFiles: CurrentFileState[],
	repoUid: string,
): InvalidationPlan {
	const classifications: FileClassification[] = [];
	const changedConfigs: ChangedConfig[] = [];

	// Track which parent files are still present.
	const seenParentFileUids = new Set<string>();

	// Phase 1: Classify current files against parent.
	for (const file of currentFiles) {
		const parentHash = parentHashes.get(file.fileUid) ?? null;

		let disposition: FileDisposition;
		let reason: string;

		if (parentHash === null) {
			disposition = "new";
			reason = "not in parent snapshot";
		} else if (parentHash === file.contentHash) {
			disposition = "unchanged";
			reason = "content hash matches parent";
		} else {
			disposition = "changed";
			reason = "content hash differs from parent";
		}

		seenParentFileUids.add(file.fileUid);

		// Detect config file changes.
		if (disposition === "changed" || disposition === "new") {
			const configType = detectConfigType(file.path);
			if (configType) {
				changedConfigs.push({ path: file.path, configType });
			}
		}

		classifications.push({
			path: file.path,
			fileUid: file.fileUid,
			disposition,
			reason,
			currentHash: file.contentHash,
			parentHash,
		});
	}

	// Phase 2: Detect deleted files.
	for (const [fileUid, hash] of parentHashes) {
		if (!seenParentFileUids.has(fileUid)) {
			// Extract path from fileUid (format: repoUid:path).
			const path = fileUid.startsWith(repoUid + ":")
				? fileUid.slice(repoUid.length + 1)
				: fileUid;

			classifications.push({
				path,
				fileUid,
				disposition: "deleted",
				reason: "in parent snapshot but not in current tree",
				currentHash: null,
				parentHash: hash,
			});
		}
	}

	// Phase 3: Config-widening.
	// If any config changed, widen unchanged files in scope to config_widened.
	if (changedConfigs.length > 0) {
		applyConfigWidening(classifications, changedConfigs);
	}

	// Build computed subsets.
	const filesToExtract: string[] = [];
	const filesToCopy: string[] = [];
	const filesToDelete: string[] = [];
	const counts: InvalidationCounts = {
		unchanged: 0,
		changed: 0,
		new: 0,
		deleted: 0,
		configWidened: 0,
		total: classifications.length,
	};

	for (const fc of classifications) {
		switch (fc.disposition) {
			case "unchanged":
				filesToCopy.push(fc.path);
				(counts as { unchanged: number }).unchanged++;
				break;
			case "changed":
				filesToExtract.push(fc.path);
				(counts as { changed: number }).changed++;
				break;
			case "new":
				filesToExtract.push(fc.path);
				(counts as { new: number }).new++;
				break;
			case "deleted":
				filesToDelete.push(fc.path);
				(counts as { deleted: number }).deleted++;
				break;
			case "config_widened":
				filesToExtract.push(fc.path);
				(counts as { configWidened: number }).configWidened++;
				break;
		}
	}

	return {
		parentSnapshotUid,
		files: classifications,
		changedConfigs,
		filesToExtract,
		filesToCopy,
		filesToDelete,
		counts,
	};
}

// ── Config widening ────────────────────────────────────────────────

/**
 * Apply config-widening: if a manifest/config file changed, widen
 * unchanged files under its scope to config_widened.
 *
 * Widening rules (slice 1, conservative):
 *   - Root-level config (no directory prefix): widen ALL unchanged files
 *   - Nested config (under a directory): widen unchanged files under
 *     that directory
 *
 * Root-level configs:
 *   package.json, pnpm-workspace.yaml, tsconfig.json, Cargo.toml,
 *   settings.gradle, compile_commands.json
 *
 * Nested configs:
 *   packages/api/package.json → widen packages/api/**
 *   packages/api/tsconfig.json → widen packages/api/**
 */
function applyConfigWidening(
	classifications: FileClassification[],
	changedConfigs: ChangedConfig[],
): void {
	// Determine the widening scopes.
	let widenAll = false;
	const widenPrefixes: string[] = [];

	for (const cc of changedConfigs) {
		const dir = cc.path.includes("/")
			? cc.path.slice(0, cc.path.lastIndexOf("/"))
			: null;

		if (dir === null) {
			// Root-level config changed → widen everything.
			widenAll = true;
			break;
		} else {
			widenPrefixes.push(dir + "/");
		}
	}

	// Apply widening to unchanged files.
	for (let i = 0; i < classifications.length; i++) {
		const fc = classifications[i];
		if (fc.disposition !== "unchanged") continue;

		let shouldWiden = false;
		let widenReason = "";

		if (widenAll) {
			shouldWiden = true;
			widenReason = "root-level config changed";
		} else {
			for (const prefix of widenPrefixes) {
				if (fc.path.startsWith(prefix)) {
					shouldWiden = true;
					widenReason = `config changed: ${prefix.slice(0, -1)}`;
					break;
				}
			}
		}

		if (shouldWiden) {
			// Mutate the classification in place.
			(classifications[i] as { disposition: FileDisposition }).disposition = "config_widened";
			(classifications[i] as { reason: string }).reason = widenReason;
		}
	}
}

// ── Helpers ────────────────────────────────────────────────────────

/**
 * Detect if a file path is a known config/manifest file.
 */
function detectConfigType(filePath: string): ConfigType | null {
	const basename = filePath.includes("/")
		? filePath.slice(filePath.lastIndexOf("/") + 1)
		: filePath;
	return CONFIG_FILE_PATTERNS.get(basename) ?? null;
}
