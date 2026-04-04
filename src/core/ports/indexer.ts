/**
 * Indexer port — orchestrates file walking, extraction, edge resolution,
 * and storage for a repository indexing operation.
 */
export interface IndexerPort {
	/**
	 * Perform a full index of a repository.
	 * Creates a new snapshot, extracts all files, resolves edges, persists.
	 *
	 * @param repoUid - The repository to index.
	 * @param options - Indexing options.
	 * @returns The snapshot UID of the completed index.
	 */
	indexRepo(repoUid: string, options?: IndexOptions): Promise<IndexResult>;

	/**
	 * Perform an incremental refresh.
	 * Re-extracts only files that changed since the last snapshot.
	 *
	 * @param repoUid - The repository to refresh.
	 * @param options - Indexing options.
	 * @returns The snapshot UID of the refresh snapshot.
	 */
	refreshRepo(repoUid: string, options?: IndexOptions): Promise<IndexResult>;
}

export interface IndexOptions {
	/** Additional glob patterns to exclude (on top of .gitignore). */
	exclude?: string[];
	/** If set, only include files matching these globs. */
	include?: string[];
	/** Log progress callbacks. */
	onProgress?: (event: IndexProgressEvent) => void;
	/**
	 * Git commit hash that the working tree was at when indexing started.
	 * Stored on the snapshot as `basis_commit` and used later by change-
	 * impact analysis and snapshot comparability checks. The indexer
	 * itself does not call git; the composition root (CLI) resolves the
	 * commit and passes it here.
	 */
	basisCommit?: string;
}

export interface IndexResult {
	snapshotUid: string;
	filesTotal: number;
	nodesTotal: number;
	edgesTotal: number;
	edgesUnresolved: number;
	/**
	 * Breakdown of unresolved edges by machine-stable diagnostic category.
	 * Keys are from UnresolvedEdgeCategory (see
	 * src/core/diagnostics/unresolved-edge-categories.ts).
	 * Human-readable labels are rendered at display time.
	 */
	unresolvedBreakdown: Record<string, number>;
	durationMs: number;
	/**
	 * Number of active symbol-targeting declarations (entrypoint, invariant)
	 * whose target_stable_key does not match any node in the new snapshot.
	 * Non-zero indicates stale declarations from a prior stable_key format.
	 */
	orphanedDeclarations: number;
}

export interface IndexProgressEvent {
	phase: "scanning" | "extracting" | "resolving" | "persisting";
	current: number;
	total: number;
	file?: string;
}
