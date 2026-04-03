/**
 * Git port — interface for git repository operations.
 *
 * All git interaction goes through this port. No shell commands
 * scattered through CLI or indexer code. The adapter wraps the
 * git CLI behind a typed interface.
 *
 * v1.5 scope: current commit, dirty state, file churn.
 * Future: blame, diff, log, tag listing, branch info.
 */

export interface GitPort {
	/**
	 * Get the current HEAD commit hash for a repository.
	 * Returns null if not a git repo or no commits exist.
	 */
	getCurrentCommit(repoPath: string): Promise<string | null>;

	/**
	 * Check if the working tree has uncommitted changes.
	 */
	isDirty(repoPath: string): Promise<boolean>;

	/**
	 * Get per-file churn data for a time window.
	 * Returns commit count and lines changed (added + deleted)
	 * for each file modified in the window.
	 *
	 * @param repoPath - Absolute path to the repository root.
	 * @param since - ISO date string or git date format (e.g. "90.days.ago").
	 */
	getFileChurn(repoPath: string, since: string): Promise<FileChurnEntry[]>;
}

export interface FileChurnEntry {
	/** Repo-relative file path (forward slashes). */
	filePath: string;
	/** Number of commits that touched this file in the window. */
	commitCount: number;
	/** Total lines added + deleted in the window. */
	linesChanged: number;
}
