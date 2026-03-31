import type { ParseStatus } from "./types.js";

/**
 * A tracked file in a repository.
 * Path is ALWAYS relative to the repo root.
 */
export interface TrackedFile {
	fileUid: string;
	repoUid: string;
	path: string;
	language: string | null;
	isTest: boolean;
	isGenerated: boolean;
	isExcluded: boolean;
}

/**
 * The parse state of a file at a specific snapshot.
 * Used for invalidation: if contentHash changes, the file is stale.
 */
export interface FileVersion {
	snapshotUid: string;
	fileUid: string;
	contentHash: string;
	astHash: string | null;
	extractor: string | null;
	parseStatus: ParseStatus;
	sizeBytes: number | null;
	lineCount: number | null;
	indexedAt: string;
}
