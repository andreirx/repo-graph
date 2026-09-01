//! History-shape diagnosis for churn framing.
//!
//! CHURN-SHALLOW-1: cheap deterministic git facts, computed at query time, that
//! tell churn/hotspots/risk WHAT the history actually is BEFORE they render a
//! count. Two measured failure shapes motivate this:
//!
//!   1. A depth-1 (shallow) clone has ONE commit whose numstat is the whole tree
//!      (no parent → every file appears added). Rendered as "N files changed in
//!      the last 90 days" that is a confident lie: it is the import snapshot, not
//!      recent activity.
//!   2. A stale clone with real history but nothing in the window renders the
//!      either/or hedge "no files changed … or no git history available" — yet the
//!      tool has the repo open and can say WHICH.
//!
//! The diagnosis is a Layer-2 overlay input (it FRAMES the churn count); it never
//! changes what churn counts. It is a fixed taxonomy of four cells plus an
//! I/O-failure escape hatch the caller renders as unknown-with-reason (honesty
//! rule #1: a failed git read is NEVER coerced to a guessed state).

use std::path::Path;
use std::process::Command;

use crate::churn::ChurnWindow;
use crate::error::GitError;

/// The shape of a repo's git history as it bears on churn framing.
///
/// A FIXED sum type (the four cells are the CHURN-SHALLOW-1 §2.1 contract): adding
/// a cell must break every render site's match, which is the point. An I/O failure
/// is NOT a variant here — `diagnose_history` returns `Err` and the caller renders
/// unknown-with-reason, so a real git failure can never masquerade as one of these
/// known states.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HistoryShape {
    /// No commits reachable at all — an unborn repo (`git init`, zero commits) or a
    /// path git does not treat as a repository. "no git history available."
    NoHistory,

    /// A shallow clone OR a single-commit history: the available commit(s) are a
    /// snapshot, not time-windowed activity. A whole-tree initial commit would
    /// otherwise be miscounted as recent churn.
    ///
    /// `commits_available` is how many commits are reachable from HEAD (`git rev-list
    /// --count HEAD` — the same domain churn walks, so an unrelated ref's history never
    /// inflates this); `is_shallow` distinguishes a depth-limited
    /// clone (`.git/shallow`) from a genuinely single-commit repo — the two demand
    /// DIFFERENT next actions (`git fetch --unshallow` vs "this IS the full history").
    /// `head_commit_date` (short `YYYY-MM-DD`) is the newest available commit, and
    /// `commits_in_window` says whether that snapshot falls inside the churn window:
    /// `> 0` → the count IS the imported snapshot (VCMI: a recent depth-1 clone whose
    /// whole tree lands in-window); `0` → nothing recent, and the snapshot's age +
    /// the depth limit are why (a stale shallow clone). This is why a shallow repo is
    /// NEVER routed to [`Self::ZeroInWindow`]: `--since` would be misleading advice
    /// (no deeper history is reachable by widening the window).
    ShallowOrSingle {
        commits_available: u64,
        is_shallow: bool,
        head_commit_date: String,
        commits_in_window: u64,
    },

    /// Real multi-commit history, but ZERO commits fall in the requested window.
    /// The determinable cause (a HEAD older than the window) replaces the hedge;
    /// `head_commit_date` (short `YYYY-MM-DD`) drives a concrete `--since`
    /// suggestion.
    ZeroInWindow { head_commit_date: String },

    /// Multi-commit, non-shallow history with activity in the window — the churn
    /// count means what it says. Framing is unchanged.
    Healthy,
}

/// Diagnose `repo_path`'s history shape for the given churn `window`.
///
/// Cheap deterministic git facts only (a handful of `git rev-list` / `rev-parse`
/// calls, all bounded work). Every fallible read propagates as `Err` — the caller
/// renders unknown-with-reason and NEVER guesses a shape.
///
/// # Errors
/// Any underlying git probe failure (spawn failure, unexpected non-zero exit,
/// unparseable output). Not-a-git-repo is NOT an error here — it maps to
/// [`HistoryShape::NoHistory`], a real known state.
pub fn diagnose_history(repo_path: &Path, window: &ChurnWindow) -> Result<HistoryShape, GitError> {
    // Not a git repo → a real "no history" state (never an error, never a guess).
    if !crate::basis::is_git_repo(repo_path)? {
        return Ok(HistoryShape::NoHistory);
    }

    // The count that frames churn MUST come from the SAME reachability domain churn
    // walks — HEAD — NOT `rev-list --all` (review-1 #2). A repo whose HEAD is a single
    // whole-tree commit while an unrelated ref (an orphan branch, a stale
    // remote-tracking ref) holds deep history would otherwise count as multi-commit, and
    // its whole-tree HEAD snapshot would be mis-rendered as ordinary churn — the exact
    // lie this slice exists to kill. Unborn HEAD → NoHistory, established POSITIVELY.
    let total_commits = match rev_list_count_head(repo_path)? {
        Some(n) => n,
        None => return Ok(HistoryShape::NoHistory),
    };
    if total_commits == 0 {
        // Defensive: a resolvable HEAD implies >= 1 commit, but never fabricate a shape
        // from a 0 count — a zero here is treated as the honest no-history state.
        return Ok(HistoryShape::NoHistory);
    }

    let is_shallow = is_shallow_repository(repo_path)?;
    let commits_in_window = rev_list_count_since(repo_path, &window.since)?;
    // `total_commits > 0` guarantees HEAD resolves, so the date read is meaningful;
    // a FAILED read still propagates as Err (unknown-with-reason), never `None`.
    let head_commit_date = head_commit_date(repo_path)?;

    Ok(classify_history(
        total_commits,
        is_shallow,
        commits_in_window,
        Some(head_commit_date),
    ))
}

/// Pure classification of the four cells from already-extracted facts.
///
/// Separated from I/O so the branch logic (which cell wins, in what order) is
/// deterministically testable WITHOUT constructing shallow/stale/unborn fixtures
/// (not portable across git versions) — the same pattern `basis::classify_git_dir_probe`
/// uses. Precedence, top to bottom:
///   1. no commits → NoHistory
///   2. shallow OR single → ShallowOrSingle (a shallow depth-1 clone lands here, NOT in
///      ZeroInWindow even when its one commit is recent — `--since` is wrong advice for it)
///   3. zero commits in window → ZeroInWindow (needs a HEAD date; its absence with
///      total>0 is impossible in practice → defensive NoHistory)
///   4. otherwise → Healthy
pub(crate) fn classify_history(
    total_commits: u64,
    is_shallow: bool,
    commits_in_window: u64,
    head_commit_date: Option<String>,
) -> HistoryShape {
    if total_commits == 0 {
        return HistoryShape::NoHistory;
    }
    // Every remaining non-empty cell reports the HEAD date. Its absence is impossible
    // when total>0 (a failed read errors in `diagnose_history` before this) — defended
    // as NoHistory rather than a fabricated date (honesty rule #1).
    let head_commit_date = match head_commit_date {
        Some(d) => d,
        None => return HistoryShape::NoHistory,
    };
    // Shallow-first: a shallow clone is NEVER routed to ZeroInWindow even when its one
    // commit is out of window, because `--since` (ZeroInWindow's advice) is misleading
    // for a depth-limited clone — the honest fix is `git fetch --unshallow`.
    if is_shallow || total_commits == 1 {
        return HistoryShape::ShallowOrSingle {
            commits_available: total_commits,
            is_shallow,
            head_commit_date,
            commits_in_window,
        };
    }
    if commits_in_window == 0 {
        return HistoryShape::ZeroInWindow { head_commit_date };
    }
    HistoryShape::Healthy
}

/// Commits reachable from HEAD (`git rev-list --count HEAD`) — the SAME reachability
/// domain `get_file_churn` walks (`git log` on HEAD). Chosen over `rev-list --count
/// --all` deliberately (review-1 #2): `--all` counts commits on refs churn never walks
/// (orphan branches, stale remote-tracking refs), which would misclassify a
/// single-commit HEAD as multi-commit and mis-frame its whole-tree snapshot as churn.
///
/// Returns:
///   - `Ok(Some(n))` — HEAD resolves to `n` reachable commits.
///   - `Ok(None)`    — HEAD is UNBORN (no commit reachable from ANY ref): a KNOWN
///     no-history state, established POSITIVELY via [`crate::basis::is_unborn_head`],
///     never inferred from stderr text.
///   - `Err(..)`     — a genuine probe failure, INCLUDING a broken-but-committed HEAD
///     (points at a missing branch while commits live only on other refs): churn
///     cannot walk that HEAD either, so the caller renders unknown-with-reason rather
///     than guessing a shape (honesty rule #1).
fn rev_list_count_head(repo_path: &Path) -> Result<Option<u64>, GitError> {
    let output = Command::new("git")
        .args(["rev-list", "--count", "HEAD"])
        .current_dir(repo_path)
        .output()?;
    if output.status.success() {
        return Ok(Some(parse_count(
            &String::from_utf8_lossy(&output.stdout),
            "git rev-list --count HEAD",
        )?));
    }
    // HEAD did not resolve. Distinguish an UNBORN repo (a real no-history state) from a
    // broken-but-committed HEAD via a POSITIVE probe of the commit graph — never from
    // stderr text, since an unborn repo and a repo whose HEAD points at a now-missing
    // branch emit the SAME `ambiguous argument 'HEAD'` message (honesty rule #1).
    if crate::basis::is_unborn_head(repo_path)? {
        return Ok(None);
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(GitError::CommandFailed {
        command: "git rev-list --count HEAD".to_string(),
        exit_code: output.status.code(),
        stderr: stderr.to_string(),
    })
}

/// Commits reachable from HEAD within the window (`git rev-list --count --since HEAD`).
/// Mirrors what `get_file_churn` walks (`git log --since` on HEAD).
fn rev_list_count_since(repo_path: &Path, since: &str) -> Result<u64, GitError> {
    let output = Command::new("git")
        .args(["rev-list", "--count", &format!("--since={since}"), "HEAD"])
        .current_dir(repo_path)
        .output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(GitError::CommandFailed {
            command: format!("git rev-list --count --since={since} HEAD"),
            exit_code: output.status.code(),
            stderr: stderr.to_string(),
        });
    }
    parse_count(
        &String::from_utf8_lossy(&output.stdout),
        "git rev-list --count --since HEAD",
    )
}

/// Whether the repository is a shallow clone (`git rev-parse --is-shallow-repository`,
/// which prints `true`/`false`). Preferred over a bare `.git/shallow` `.exists()`
/// probe: that is a filesystem read whose result would be CLASSIFIED, which honesty
/// rule #1 forbids coercing; git's own predicate is the deterministic source and
/// surfaces failure as `Err`.
fn is_shallow_repository(repo_path: &Path) -> Result<bool, GitError> {
    let output = Command::new("git")
        .args(["rev-parse", "--is-shallow-repository"])
        .current_dir(repo_path)
        .output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(GitError::CommandFailed {
            command: "git rev-parse --is-shallow-repository".to_string(),
            exit_code: output.status.code(),
            stderr: stderr.to_string(),
        });
    }
    match String::from_utf8_lossy(&output.stdout).trim() {
        "true" => Ok(true),
        "false" => Ok(false),
        other => Err(GitError::ParseError(format!(
            "git rev-parse --is-shallow-repository: expected true/false, got {other:?}"
        ))),
    }
}

/// HEAD's committer date in short `YYYY-MM-DD` form (`git show -s --format=%cs HEAD`).
/// Only called once a commit is known to exist (`total_commits > 0`); a read failure
/// propagates as `Err` (unknown-with-reason).
fn head_commit_date(repo_path: &Path) -> Result<String, GitError> {
    let output = Command::new("git")
        .args(["show", "-s", "--format=%cs", "HEAD"])
        .current_dir(repo_path)
        .output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(GitError::CommandFailed {
            command: "git show -s --format=%cs HEAD".to_string(),
            exit_code: output.status.code(),
            stderr: stderr.to_string(),
        });
    }
    let date = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if date.is_empty() {
        return Err(GitError::ParseError(
            "git show -s --format=%cs HEAD returned empty output".to_string(),
        ));
    }
    Ok(date)
}

/// Parse a single non-negative integer git-count line.
fn parse_count(s: &str, command: &str) -> Result<u64, GitError> {
    s.trim()
        .parse::<u64>()
        .map_err(|e| GitError::ParseError(format!("{command}: {e} (output: {s:?})")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_no_commits_is_no_history() {
        assert_eq!(classify_history(0, false, 0, None), HistoryShape::NoHistory);
        // Even if a stray shallow flag is set, zero commits wins.
        assert_eq!(
            classify_history(0, true, 0, Some("2024-01-01".into())),
            HistoryShape::NoHistory
        );
    }

    #[test]
    fn classify_shallow_depth_one_clone_in_window() {
        // VCMI shape: one commit, shallow, and that commit IS recent (in window) —
        // must NOT be read as recent churn.
        assert_eq!(
            classify_history(1, true, 1, Some("2026-08-01".into())),
            HistoryShape::ShallowOrSingle {
                commits_available: 1,
                is_shallow: true,
                head_commit_date: "2026-08-01".into(),
                commits_in_window: 1,
            }
        );
    }

    #[test]
    fn classify_shallow_depth_one_clone_out_of_window() {
        // django/leveldb/… shape (measured): shallow depth-1, the single commit is
        // OLDER than the window → 0 commits in window. Still ShallowOrSingle (NOT
        // ZeroInWindow) so the advice is `unshallow`, not the misleading `--since`.
        assert_eq!(
            classify_history(1, true, 0, Some("2026-05-08".into())),
            HistoryShape::ShallowOrSingle {
                commits_available: 1,
                is_shallow: true,
                head_commit_date: "2026-05-08".into(),
                commits_in_window: 0,
            }
        );
    }

    #[test]
    fn classify_single_commit_not_shallow() {
        assert_eq!(
            classify_history(1, false, 1, Some("2026-08-01".into())),
            HistoryShape::ShallowOrSingle {
                commits_available: 1,
                is_shallow: false,
                head_commit_date: "2026-08-01".into(),
                commits_in_window: 1,
            }
        );
    }

    #[test]
    fn classify_shallow_with_multiple_commits_still_shallow() {
        // A shallow clone with depth>1 is still a truncated history, not healthy.
        assert_eq!(
            classify_history(5, true, 3, Some("2026-08-01".into())),
            HistoryShape::ShallowOrSingle {
                commits_available: 5,
                is_shallow: true,
                head_commit_date: "2026-08-01".into(),
                commits_in_window: 3,
            }
        );
    }

    #[test]
    fn classify_zero_in_window_with_history() {
        // django shape: real deep history, nothing in the 90-day window.
        assert_eq!(
            classify_history(5000, false, 0, Some("2024-03-15".into())),
            HistoryShape::ZeroInWindow {
                head_commit_date: "2024-03-15".into()
            }
        );
    }

    #[test]
    fn classify_zero_in_window_missing_head_date_is_defensive_no_history() {
        // total>1 but no head date is impossible in practice; defensively NoHistory
        // rather than fabricating a ZeroInWindow with an empty date.
        assert_eq!(
            classify_history(5000, false, 0, None),
            HistoryShape::NoHistory
        );
    }

    #[test]
    fn classify_healthy() {
        assert_eq!(
            classify_history(5000, false, 42, Some("2026-08-30".into())),
            HistoryShape::Healthy
        );
    }

    #[test]
    fn parse_count_ok_and_err() {
        assert_eq!(parse_count("42\n", "cmd").unwrap(), 42);
        assert_eq!(parse_count("0", "cmd").unwrap(), 0);
        assert!(parse_count("abc", "cmd").is_err());
        assert!(parse_count("", "cmd").is_err());
    }
}
