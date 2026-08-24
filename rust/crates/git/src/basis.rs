//! Index-basis commit + working-tree drift extraction.
//!
//! INDEX-BASIS-1: the git side of "repo-graph owns the structure of the last
//! indexed commit; git owns the delta". Two surfaces:
//!
//!   - [`head_commit`] — the commit an index/refresh is being built FROM,
//!     stamped into `snapshots.basis_commit` at WRITE time.
//!   - [`working_tree_drift`] — how far the working tree has moved past that
//!     basis, computed at QUERY time so orient/check/explain can say "the facts
//!     describe commit X; you have moved N commits / M files past it".
//!
//! Contract (shared with the rest of this crate):
//!   - Paths are repo-relative, forward slashes, no `./` prefix, deduped, sorted.
//!   - Git is authoritative; this is a derived analytical view, never persisted here.
//!   - Honesty: "not a git repo" is `Ok(None)` (a real, known state), NOT an error
//!     and NOT an empty drift. A git *failure* is `Err`, surfaced with its reason —
//!     never silently coerced to zero/clean.

use std::path::Path;
use std::process::Command;

use crate::error::GitError;

/// Return true when `repo_path` is inside a git working tree.
///
/// Deterministic single probe (`git rev-parse --git-dir`). Three outcomes, never
/// collapsed:
///   - `Ok(true)`  — inside a git working tree.
///   - `Ok(false)` — the KNOWN answer "not a git repo" (git ran, exit non-zero,
///     stderr is git's canonical `not a git repository` message).
///   - `Err(..)`   — a spawn failure (git absent / not executable) OR any OTHER
///     non-zero exit (corrupt repo, dubious-ownership refusal, permission denied,
///     an unexpected git error). A FAILED probe is unknown-with-reason, NEVER
///     coerced to the "not a git repo" state (honesty rule #1).
pub fn is_git_repo(repo_path: &Path) -> Result<bool, GitError> {
    let output = Command::new("git")
        .args(["rev-parse", "--git-dir"])
        .current_dir(repo_path)
        .output()?;
    classify_git_dir_probe(
        output.status.success(),
        output.status.code(),
        &String::from_utf8_lossy(&output.stderr),
    )
}

/// Classify the exit of `git rev-parse --git-dir` into the git-repo predicate.
///
/// PURE (no I/O) so the "genuine failure ≠ not-a-repo" distinction is
/// deterministically testable WITHOUT fabricating a corrupt/dubious repository
/// (not portable across platforms and git versions). A success is `Ok(true)`; a
/// non-zero exit whose stderr carries git's canonical "not a git repository"
/// message is the known `Ok(false)`; every other non-zero exit is a real
/// `CommandFailed` — surfaced, never silently coerced to `Ok(false)`.
fn classify_git_dir_probe(
    success: bool,
    exit_code: Option<i32>,
    stderr: &str,
) -> Result<bool, GitError> {
    if success {
        return Ok(true);
    }
    if stderr.contains("not a git repository") {
        return Ok(false);
    }
    Err(GitError::CommandFailed {
        command: "git rev-parse --git-dir".to_string(),
        exit_code,
        stderr: stderr.to_string(),
    })
}

/// The current `HEAD` commit of `repo_path`, for stamping as the index basis.
///
/// Returns:
///   - `Ok(Some(sha))` — full 40-char `HEAD` sha (the basis to stamp).
///   - `Ok(None)` — the path is **not a git repo** (an honest "no basis exists",
///     rendered downstream as "not a git repo", never as a fake/empty basis).
///   - `Err(GitError)` — git is present and the path IS a repo, but `rev-parse
///     HEAD` failed (e.g. a repo with zero commits, or an unexpected git error).
///     The caller decides how to record an unknown basis; it is never silently
///     treated as `None` at this layer.
pub fn head_commit(repo_path: &Path) -> Result<Option<String>, GitError> {
    // Not a git repo → a real "no basis" state, not an error.
    if !is_git_repo(repo_path)? {
        return Ok(None);
    }

    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo_path)
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(GitError::CommandFailed {
            command: "git rev-parse HEAD".to_string(),
            exit_code: output.status.code(),
            stderr: stderr.to_string(),
        });
    }

    let sha = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if sha.is_empty() {
        return Err(GitError::ParseError(
            "git rev-parse HEAD returned empty output".to_string(),
        ));
    }
    Ok(Some(sha))
}

/// Positively establish whether `repo_path`'s HEAD is **unborn** — the repo was
/// `git init`ed but carries ZERO commits (no ref points at any commit), as opposed to
/// a committed repo whose HEAD is merely unreadable (a missing/corrupt/detached-broken
/// HEAD, a permission refusal, an unexpected git error).
///
/// WHY a dedicated probe (review-9 #1): git's `rev-parse HEAD` stderr does NOT
/// establish the unborn state. Both an unborn repo AND a committed repo whose `HEAD`
/// points at a now-missing branch emit the SAME `fatal: ambiguous argument 'HEAD':
/// unknown revision …`. Classifying "no commits yet" from that text alone falsely
/// labels a broken-HEAD-but-committed repo as empty. So we do NOT read stderr; we
/// POSITIVELY probe the commit graph:
///
///   `git rev-list -n 1 --all` lists at most one commit reachable from ANY ref.
///     - success + EMPTY stdout ⟹ no ref points at any commit ⟹ genuinely unborn.
///     - success + a sha       ⟹ the repo HAS commits (HEAD was just unreadable) ⟹
///       NOT unborn.
///
/// Returns:
///   - `Ok(true)`  — positively unborn (repo has no commits).
///   - `Ok(false)` — the repo HAS at least one commit (so an unreadable HEAD is a
///     generic failure, never the empty-repo state).
///   - `Err(..)`   — the probe itself could not run (git absent, or `rev-list` failed).
///     The caller must NOT claim unborn on an `Err`; unborn is asserted only when
///     `Ok(true)` POSITIVELY establishes it.
///
/// Only meaningful on a path already known to be a git repo (the caller reaches this
/// after `head_commit` returned `Err` on a confirmed repo); on a non-git path
/// `rev-list` fails and this returns `Err`.
pub fn is_unborn_head(repo_path: &Path) -> Result<bool, GitError> {
    let output = Command::new("git")
        .args(["rev-list", "-n", "1", "--all"])
        .current_dir(repo_path)
        .output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(GitError::CommandFailed {
            command: "git rev-list -n 1 --all".to_string(),
            exit_code: output.status.code(),
            stderr: stderr.to_string(),
        });
    }
    // Empty output ⟹ no commit reachable from any ref ⟹ unborn. A sha ⟹ has commits.
    Ok(String::from_utf8_lossy(&output.stdout).trim().is_empty())
}

/// How far the working tree has moved past a recorded basis commit.
///
/// `commits_ahead` counts commits reachable from `HEAD` but not from `basis`
/// (`git rev-list --count <basis>..HEAD`). `changed_files` is the union of:
///   - `git diff -z --name-only <basis>` — tracked files that differ between the
///     basis and the working tree (committed OR uncommitted edits), and
///   - `git status --porcelain -z --untracked-files=all` — additionally
///     untracked/added files. `--untracked-files=all` is REQUIRED: git's default
///     `normal` mode collapses a wholly-untracked directory to a single
///     `?? dir/` placeholder, which would (a) count one directory as one changed
///     "file", (b) undercount the nested files, and (c) be un-intersectable with
///     the indexed file set (a directory is never an indexed path). `all` emits
///     every nested untracked file individually so K-of-M stays honest.
///
/// Both reads use git's **NUL-delimited** (`-z`) machine format, so paths are
/// emitted VERBATIM — no C-style quoting/escaping of spaces, quotes, or non-ASCII
/// bytes (the default line format mangles those under `core.quotepath`). The union
/// is deduped and sorted for a deterministic total order. A rename under `-z`
/// porcelain is `XY <new>\0<old>\0`; `changed_files` contributes the post-rename
/// (`<new>`) path, matching the prior line-format behaviour.
///
/// # Errors
/// - `GitError::NotARepository` if `repo_path` is not a git repo.
/// - `GitError::CommandFailed` if `basis` is unknown to the repo (e.g. HEAD was
///   rewritten and the basis sha no longer exists) or any git call fails — the
///   caller renders this as "drift unknown (<reason>)", never as "clean".
pub fn working_tree_drift(repo_path: &Path, basis: &str) -> Result<WorkingTreeDrift, GitError> {
    if !is_git_repo(repo_path)? {
        return Err(GitError::NotARepository(repo_path.display().to_string()));
    }

    let commits_ahead = rev_list_count(repo_path, basis)?;

    let mut changed: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for p in diff_name_only(repo_path, basis)? {
        changed.insert(p);
    }
    for p in status_porcelain_paths(repo_path)? {
        changed.insert(p);
    }

    Ok(WorkingTreeDrift {
        commits_ahead,
        changed_files: changed.into_iter().collect(),
    })
}

/// Working-tree drift since a basis commit. Layer-0 git facts only — the caller
/// intersects `changed_files` with the indexed file set to derive "K of M indexed".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkingTreeDrift {
    /// Commits reachable from HEAD but not from the basis (`<basis>..HEAD`).
    pub commits_ahead: u64,
    /// Repo-relative paths (forward slashes) that differ from the basis in the
    /// working tree, tracked-or-untracked, deduped and sorted.
    pub changed_files: Vec<String>,
}

fn rev_list_count(repo_path: &Path, basis: &str) -> Result<u64, GitError> {
    let range = format!("{basis}..HEAD");
    let output = Command::new("git")
        .args(["rev-list", "--count", &range])
        .current_dir(repo_path)
        .output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(GitError::CommandFailed {
            command: format!("git rev-list --count {range}"),
            exit_code: output.status.code(),
            stderr: stderr.to_string(),
        });
    }
    let s = String::from_utf8_lossy(&output.stdout);
    s.trim()
        .parse::<u64>()
        .map_err(|e| GitError::ParseError(format!("git rev-list --count: {e} (output: {:?})", s)))
}

fn diff_name_only(repo_path: &Path, basis: &str) -> Result<Vec<String>, GitError> {
    let output = Command::new("git")
        .args(["diff", "-z", "--name-only", basis])
        .current_dir(repo_path)
        .output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(GitError::CommandFailed {
            command: format!("git diff -z --name-only {basis}"),
            exit_code: output.status.code(),
            stderr: stderr.to_string(),
        });
    }
    // `-z` emits NUL-terminated RAW path bytes (no C-quoting). Decode each record
    // STRICTLY from bytes — a non-UTF-8 path is a typed `ParseError`, never lossily
    // substituted (`from_utf8_lossy` would replace bytes with U+FFFD and could
    // misclassify the path against the UTF-8 indexed-file set). The caller renders
    // drift as Unknown-with-reason on that error.
    let mut out = Vec::new();
    for chunk in output.stdout.split(|b| *b == 0) {
        if chunk.is_empty() {
            continue;
        }
        let norm = normalize_path(&decode_z_path(chunk, "git diff -z --name-only")?);
        if !norm.is_empty() {
            out.push(norm);
        }
    }
    Ok(out)
}

/// Decode one raw NUL-record path (the bytes between two NULs) into a repo-relative
/// string. A path that is NOT valid UTF-8 is a typed `ParseError` — we do NOT lossily
/// decode. Lossy decode substitutes U+FFFD for the offending bytes, which changes the
/// path and can make an indexed path miss the intersection (misclassified as
/// unindexed); the lossless-path contract requires either verbatim bytes or an honest
/// error, and this crate's downstream (`Vec<String>`, intersected with UTF-8 indexed
/// paths) takes the error branch → the surface renders drift Unknown.
fn decode_z_path(bytes: &[u8], command: &str) -> Result<String, GitError> {
    std::str::from_utf8(bytes).map(str::to_string).map_err(|e| {
        GitError::ParseError(format!(
            "{command}: path is not valid UTF-8 ({e}) — drift cannot be classified losslessly"
        ))
    })
}

fn status_porcelain_paths(repo_path: &Path) -> Result<Vec<String>, GitError> {
    // `--untracked-files=all` expands wholly-untracked directories into their
    // individual nested files (default `normal` emits only a `?? dir/` placeholder),
    // so every untracked file is counted and can be intersected with indexed paths.
    let output = Command::new("git")
        .args(["status", "--porcelain", "-z", "--untracked-files=all"])
        .current_dir(repo_path)
        .output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(GitError::CommandFailed {
            command: "git status --porcelain -z --untracked-files=all".to_string(),
            exit_code: output.status.code(),
            stderr: stderr.to_string(),
        });
    }
    parse_porcelain_z(&output.stdout)
}

/// Extract affected paths from `git status --porcelain -z` output.
///
/// Under `-z` each record is `XY <path>\0` (2-char status, a space, then the raw
/// path — never C-quoted). A rename/copy record (an `R` or `C` in either status
/// column) is followed by an EXTRA NUL-terminated token, the OLD path (the field
/// order is reversed vs the line format): `XY <new>\0<old>\0`. We take `<new>` (the
/// current path) and consume the trailing `<old>` token so it is not miscounted as
/// its own changed file — matching the prior `R  old -> new` → `new` behaviour.
///
/// Operates on RAW bytes (not a lossily-decoded `&str`): each record's path is
/// decoded strictly via [`decode_z_path`], so a non-UTF-8 path is a typed
/// `ParseError` rather than a silently mangled string.
fn parse_porcelain_z(stdout: &[u8]) -> Result<Vec<String>, GitError> {
    let mut tokens = stdout.split(|b| *b == 0).filter(|t| !t.is_empty());
    let mut out = Vec::new();
    while let Some(record) = tokens.next() {
        // Need at least "XY " + one path byte.
        if record.len() < 4 {
            continue;
        }
        let status = &record[..2];
        let path_bytes = &record[3..]; // skip the 2 status bytes + the separator space at index 2
                                       // A rename/copy consumes the following token (the old path); drop it.
        if status.iter().any(|b| *b == b'R' || *b == b'C') {
            let _old = tokens.next();
        }
        let norm = normalize_path(&decode_z_path(path_bytes, "git status --porcelain -z")?);
        if !norm.is_empty() {
            out.push(norm);
        }
    }
    Ok(out)
}

/// Normalize to repo-relative, forward-slash form with no leading `./` or `/`.
/// (Mirrors `churn::normalize_path`; kept local so the two surfaces stay
/// independent — churn is shipped and untouched.)
fn normalize_path(path: &str) -> String {
    let mut normalized = path.replace('\\', "/");
    while normalized.starts_with("./") {
        normalized = normalized[2..].to_string();
    }
    while normalized.starts_with('/') {
        normalized = normalized[1..].to_string();
    }
    normalized
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_porcelain_z_modified_and_untracked() {
        // Two NUL-terminated records: unstaged-modified + untracked.
        let out = parse_porcelain_z(b" M src/main.rs\0?? new_file.txt\0").unwrap();
        assert_eq!(
            out,
            vec!["src/main.rs".to_string(), "new_file.txt".to_string()]
        );
    }

    #[test]
    fn parse_porcelain_z_staged_added() {
        assert_eq!(
            parse_porcelain_z(b"A  src/added.rs\0").unwrap(),
            vec!["src/added.rs".to_string()]
        );
    }

    #[test]
    fn parse_porcelain_z_rename_takes_new_and_consumes_old() {
        // Under -z a rename is `R  <new>\0<old>\0` (order reversed vs the line
        // format). We keep <new> and must NOT emit <old> as a separate file.
        let out = parse_porcelain_z(b"R  src/new.rs\0src/old.rs\0 M src/other.rs\0").unwrap();
        assert_eq!(
            out,
            vec!["src/new.rs".to_string(), "src/other.rs".to_string()],
            "new path kept, old path consumed, following record still parsed"
        );
    }

    #[test]
    fn parse_porcelain_z_preserves_quoted_special_paths_verbatim() {
        // The whole point of -z: a path with a space or a non-ASCII byte that the
        // LINE format would C-quote (e.g. `"caf\303\251.txt"`) arrives RAW here.
        // (Valid-UTF-8 bytes fed as a byte slice, exactly as git emits them.)
        let out = parse_porcelain_z("?? café dir/résumé.txt\0".as_bytes()).unwrap();
        assert_eq!(out, vec!["café dir/résumé.txt".to_string()]);
    }

    #[test]
    fn parse_porcelain_z_non_utf8_path_is_typed_error_not_lossy() {
        // A path with an invalid UTF-8 byte (0xFF) must NOT be lossily decoded to a
        // U+FFFD-mangled string — it is a typed ParseError so the surface renders
        // drift Unknown-with-reason (honesty rule #1 + the lossless-path contract).
        let mut input = b"?? bad".to_vec();
        input.push(0xff);
        input.push(0); // record terminator
        match parse_porcelain_z(&input) {
            Err(GitError::ParseError(msg)) => {
                assert!(msg.contains("not valid UTF-8"), "{msg}");
            }
            other => panic!("expected ParseError on non-UTF-8 path, got {other:?}"),
        }
    }

    #[test]
    fn decode_z_path_rejects_non_utf8() {
        assert!(decode_z_path(&[0xff, 0xfe], "cmd").is_err());
        assert_eq!(decode_z_path(b"src/x.rs", "cmd").unwrap(), "src/x.rs");
    }

    #[test]
    fn parse_porcelain_z_too_short_records_skipped() {
        assert!(parse_porcelain_z(b"").unwrap().is_empty());
        assert!(parse_porcelain_z(b"M\0").unwrap().is_empty());
    }

    #[test]
    fn classify_git_dir_probe_distinguishes_not_a_repo_from_failure() {
        // Success → inside a repo.
        assert!(classify_git_dir_probe(true, Some(0), "").unwrap());
        // Non-zero + canonical "not a git repository" stderr → the KNOWN false.
        assert!(!classify_git_dir_probe(
            false,
            Some(128),
            "fatal: not a git repository (or any of the parent directories): .git\n"
        )
        .unwrap());
        // Non-zero with ANY OTHER stderr → a real failure, NEVER coerced to false.
        match classify_git_dir_probe(
            false,
            Some(128),
            "fatal: detected dubious ownership in repository at '/repo'\n",
        ) {
            Err(GitError::CommandFailed { exit_code, .. }) => assert_eq!(exit_code, Some(128)),
            other => panic!("expected CommandFailed on non-not-a-repo failure, got {other:?}"),
        }
    }

    #[test]
    fn normalize_strips_leading_dot_slash() {
        assert_eq!(normalize_path("./src/x.rs"), "src/x.rs");
        assert_eq!(normalize_path("src/x.rs"), "src/x.rs");
    }
}
