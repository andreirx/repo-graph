//! WRITE-side basis probing + persisted-outcome parsing for `index_drift`.
//!
//! Extracted from `index_drift.rs` per the 500-line structural guardrail
//! (SELF-POLLUTION-1 review-6 #3). Abstraction record: crate-private module;
//! concrete users = the daemon index/refresh arms (stamping) and
//! `index_drift`'s query path (reading the recorded outcome); axis = the
//! write-side basis vocabulary (probe/record/parse) kept apart from
//! query-time drift computation; rejected simpler alternative = leaving one
//! 516-line mixed-responsibility file (guardrail-prohibited).

use std::path::Path;

use repo_graph_repo_index::compose::{BasisOutcome, INDEX_BASIS_DIAG_KEY};
use repo_graph_storage::StorageConnection;
use repo_graph_trust::TrustStorageRead;

/// WRITE path: the git `HEAD` to stamp as `snapshots.basis_commit`.
///
/// Three OUTCOMES, never collapsed:
///   - `Ok(Some(sha))` — a git repo with a HEAD; the basis to stamp.
///   - `Ok(None)`      — NOT a git repo; the contract reserves `None` for this KNOWN
///     "no basis exists" state, and only this state.
///   - `Err(e)`        — a git repo whose HEAD could not be read (a repo with zero
///     commits, or a git error).
///
/// The caller feeds this straight into [`basis_outcome_from_probe`] to build the
/// persisted [`BasisOutcome`]; see that function for how each outcome is recorded.
pub(crate) fn basis_at_index(repo_path: &Path) -> Result<Option<String>, repo_graph_git::GitError> {
    repo_graph_git::head_commit(repo_path)
}

/// WRITE path: classify a git-HEAD probe into the persisted [`BasisOutcome`] (operator
/// RULING 3). The daemon owns the git probe (the composition root), so the unborn-vs-generic
/// determination is made HERE, from git's actual state at index time (the freshest evidence),
/// and stored as an already-rendered reason — so the query path surfaces it verbatim and
/// never re-derives it live (closing review-4's unborn-then-committed mis-attribution).
///
///   - `Ok(Some(sha))` → `Basis { commit }` (also stamped in the `basis_commit` column;
///     compose writes NO diagnostic for this — the column carries it).
///   - `Ok(None)`      → `NonGit` (recorded, so the query path can tell a THIS-slice
///     non-git NULL from a pre-slice NULL).
///   - `Err(e)`        → the HEAD is a git repo we could not read; a SECOND, POSITIVE probe
///     ([`repo_graph_git::is_unborn_head`]) decides unborn-vs-generic — see
///     [`classify_head_failure`]. review-9 #1: git's `unknown revision`/`ambiguous
///     argument 'HEAD'` stderr does NOT establish unborn (a committed repo with a broken
///     HEAD emits the identical text), so we never classify from that stderr; unborn is
///     asserted ONLY when the commit-graph probe positively establishes it.
pub(crate) fn basis_outcome_from_probe(
    repo_path: &Path,
    probe: Result<Option<String>, repo_graph_git::GitError>,
) -> BasisOutcome {
    match probe {
        Ok(Some(commit)) => BasisOutcome::Basis { commit },
        Ok(None) => BasisOutcome::NonGit,
        Err(e) => classify_head_failure(&e, repo_graph_git::is_unborn_head(repo_path)),
    }
}

/// PURE decision: given the HEAD-read error and the result of the POSITIVE unborn probe
/// (`git rev-list -n 1 --all`, via [`repo_graph_git::is_unborn_head`]), pick the recorded
/// failure reason. Split out so the "unborn is claimed ONLY on positive establishment"
/// contract is unit-testable without forging repositories per case (review-9 #1).
///
///   - `Ok(true)`         → `Failure { "repository has no commits yet" }` — the commit graph
///     is empty, so the repo is genuinely unborn.
///   - `Ok(false)`        → `Failure { "git HEAD unreadable at index time (<head_err>)" }` —
///     the repo HAS commits, so an unreadable HEAD is a GENERIC failure (a broken-HEAD
///     committed repo, NOT an empty one).
///   - `Err(_)` (probe failed) → the generic reason too — we could NOT positively establish
///     unborn, so we must NOT claim it (honest degradation). The reason carries the ORIGINAL
///     HEAD error, not the probe's.
pub(crate) fn classify_head_failure(
    head_err: &repo_graph_git::GitError,
    unborn_probe: Result<bool, repo_graph_git::GitError>,
) -> BasisOutcome {
    match unborn_probe {
        Ok(true) => BasisOutcome::Failure {
            reason: "repository has no commits yet".to_string(),
        },
        Ok(false) | Err(_) => BasisOutcome::Failure {
            reason: format!("git HEAD unreadable at index time ({head_err})"),
        },
    }
}

/// READ path: the recorded [`BasisOutcome`] for a snapshot, if any. THREE outcomes, never
/// collapsed (honesty rule #1 — a failed read is unknown, never silently `None`):
///   - `Ok(Some(o))` — an index-basis outcome WAS recorded at index time.
///   - `Ok(None)`    — the diagnostics blob was read and carries NO `index_basis` key.
///     For a `basis_commit=None` snapshot this means it predates basis tracking →
///     `BasisUnknown`.
///   - `Err(reason)` — the diagnostics blob could NOT be read/parsed → genuinely UNKNOWN;
///     the query renders `Unknown`-with-reason, never a false `BasisUnknown`/`Clean`.
///
/// Only consulted when `basis_commit` is `None`; a stamped basis needs no record.
pub(crate) fn read_basis_outcome(
    storage: &StorageConnection,
    snapshot_uid: &str,
) -> Result<Option<BasisOutcome>, String> {
    match TrustStorageRead::get_snapshot_extraction_diagnostics(storage, snapshot_uid) {
        Ok(blob) => parse_basis_outcome(blob.as_deref()),
        Err(e) => Err(format!("extraction diagnostics unreadable ({e})")),
    }
}

/// Extract the `index_basis` record from a diagnostics blob. Pure (operates on the JSON
/// string) so the absent-key / malformed-key / valid-key distinctions are unit-testable
/// without a storage connection. A blob that is absent or carries no key → `Ok(None)`; a
/// blob that is not valid JSON or a malformed key → `Err` (rendered as Unknown, never
/// silently dropped to `None` — honesty rule #1).
pub(crate) fn parse_basis_outcome(blob: Option<&str>) -> Result<Option<BasisOutcome>, String> {
    let Some(s) = blob else { return Ok(None) };
    let value: serde_json::Value = serde_json::from_str(s)
        .map_err(|e| format!("extraction diagnostics not valid JSON ({e})"))?;
    match value.get(INDEX_BASIS_DIAG_KEY) {
        None => Ok(None),
        Some(entry) => serde_json::from_value::<BasisOutcome>(entry.clone())
            .map(Some)
            .map_err(|e| format!("index_basis diagnostic malformed ({e})")),
    }
}
