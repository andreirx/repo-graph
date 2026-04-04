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

	/**
	 * List files changed in the given diff scope, relative to repo root.
	 *
	 * Paths are returned repo-relative with forward slashes, matching
	 * the convention used by getFileChurn.
	 *
	 * Throws if the repo path is not a git repository, or if the
	 * specified commit ref cannot be resolved. The throwing contract
	 * is explicit so callers can surface the exact failure mode.
	 */
	getChangedFiles(repoPath: string, scope: GitDiffScope): Promise<string[]>;
}

export interface FileChurnEntry {
	/** Repo-relative file path (forward slashes). */
	filePath: string;
	/** Number of commits that touched this file in the window. */
	commitCount: number;
	/** Total lines added + deleted in the window. */
	linesChanged: number;
}

/**
 * Scope for a git diff that returns changed file paths.
 *
 *   staged                     — diff --cached (staged changes only)
 *   working_tree_vs_commit     — diff against the given commit ref,
 *                                includes BOTH staged and unstaged
 *                                (equivalent to `git diff <ref>`)
 */
export type GitDiffScope =
	| { kind: "staged" }
	| { kind: "working_tree_vs_commit"; commit: string };
