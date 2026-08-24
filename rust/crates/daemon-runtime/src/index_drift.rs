//! INDEX-BASIS-1 daemon glue: classify the index basis at WRITE time, and compute
//! working-tree drift at QUERY time.
//!
//! The pure `repo-graph-agent` crate defines the [`IndexDrift`] sum type but does
//! no I/O; this module is where the I/O lives — it reads git (via
//! `repo-graph-git`) and the indexed-file / module facts (via the agent storage
//! port) and constructs an `IndexDrift`. The daemon is the composition root, the
//! only place these concrete mechanisms meet.
//!
//! ## Write vs read (operator RULING 3)
//!
//! The WRITE-time basis outcome is now PERSISTED BY COMPOSE, not by this crate: the
//! daemon classifies the git probe into a [`BasisOutcome`] ([`basis_outcome_from_probe`]),
//! hands it to `repo-index` on `ComposeOptions`, and compose records it into the snapshot's
//! extraction diagnostics IN THE SAME WRITE FLOW that writes the snapshot row (deterministic,
//! propagates on failure — replacing the deleted best-effort daemon write that review-5
//! flagged). At QUERY time this module READS that record back ([`read_basis_outcome`]) and
//! maps it onto `IndexDrift` ([`basis_none_state`]) — never a live HEAD re-probe.
//!
//! ABSTRACTION (pub(crate) module — NOT a new crate/public API):
//!   - what: daemon-side glue turning the git probe into a persisted [`BasisOutcome`]
//!     (write), and (repo_path, recorded basis, recorded outcome, snapshot) into an
//!     `IndexDrift` (read).
//!   - concrete users: `handle_index` / `handle_refresh` (`basis_at_index` +
//!     `basis_outcome_from_probe`, passed to compose) and `handle_orient` /
//!     `handle_check` / `handle_explain` (drift, via `compute_query_drift` which reads
//!     the record with `read_basis_outcome`).
//!   - axis: query-time git-drift MECHANISM (git CLI + storage intersection) — a
//!     demonstrated volatile mechanism, isolated behind one seam.
//!   - rejected simpler alternative: inlining ~120 lines three times in
//!     `dispatch.rs` (already ~9k lines, far past the 500-line guardrail).

use std::collections::BTreeSet;
use std::path::Path;

use repo_graph_agent::dto::index_drift::IndexDrift;
use repo_graph_agent::storage_port::AgentStorageRead;
use repo_graph_repo_index::compose::{BasisOutcome, INDEX_BASIS_DIAG_KEY};
use repo_graph_storage::StorageConnection;
use repo_graph_trust::TrustStorageRead;

/// Cap on named modules in a drift line; the remainder folds into "+N more" so the
/// footer stays one line. Deterministic (modules are sorted before capping).
const MODULE_NAME_CAP: usize = 6;

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
fn classify_head_failure(
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
fn parse_basis_outcome(blob: Option<&str>) -> Result<Option<BasisOutcome>, String> {
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

/// QUERY path: the working-tree drift of `repo_path` since the recorded `basis`.
///
/// Honesty contract (matches the [`IndexDrift`] variants):
///   - not a git repo now (`is_git_repo` == Ok(false)) → `NotGit`
///   - git could not be PROBED at all (git absent/unrunnable — `is_git_repo` Err) →
///     `Unknown{reason}` (a FAILED probe is unknown-with-reason, NEVER `NotGit`/Pass)
///   - git repo but no recorded basis → resolved from the WRITE-time [`BasisOutcome`]
///     record (`basis_outcome`, read by the caller from the snapshot's extraction
///     diagnostics), see [`basis_none_state`]: recorded non-git → `NotGit`; recorded
///     failure → Unknown surfacing the recorded reason (unborn → "no commits yet"); NO
///     record → `BasisUnknown` (pre-slice); record read-error → Unknown-with-reason. NO
///     live HEAD re-probe — the recorded fact is authoritative, so an unborn-then-committed
///     repo is never mis-attributed to pre-slice history (operator RULING 2/3).
///   - git failed to compute drift (basis sha gone, git error) → `Unknown{reason}`
///   - a storage read needed to classify K-indexed / modules failed → `Unknown{reason}`
///     (a failed read is unknown, never coerced to K=0 / no-modules)
///   - HEAD == basis, clean tree → `Clean`
///   - moved → `Drifted{ commits_ahead, files_changed=M, indexed_changed=K, modules }`
///
/// `basis_outcome` is the caller's read of the snapshot's recorded `index_basis` record
/// (via [`read_basis_outcome`]); it is consulted ONLY on the no-basis branch. The read
/// lives in the caller because it needs the concrete `StorageConnection`
/// (`TrustStorageRead`), while this function stays generic over the narrow agent port.
pub(crate) fn compute_index_drift<S: AgentStorageRead + ?Sized>(
    storage: &S,
    repo_path: &Path,
    snapshot_uid: &str,
    basis: Option<&str>,
    basis_outcome: Result<Option<BasisOutcome>, String>,
) -> IndexDrift {
    // `is_git_repo` returns Ok(false) for a plain dir (→ NotGit, drift not tracked).
    // An Err means git could NOT be probed at all (git absent/unrunnable) — that is
    // an honest UNKNOWN with the reason, never `NotGit`/Pass (a failed read must not
    // masquerade as the known "not a git repo" state). This probes only the git-DIR;
    // it never re-reads HEAD, so the recorded basis outcome stays authoritative.
    match repo_graph_git::is_git_repo(repo_path) {
        Ok(true) => {}
        Ok(false) => return IndexDrift::NotGit,
        Err(e) => {
            eprintln!(
                "warning: git could not be probed for drift at {}: {}",
                repo_path.display(),
                e
            );
            return IndexDrift::Unknown {
                basis: basis.filter(|b| !b.is_empty()).map(str::to_string),
                reason: format!("git could not be probed ({e})"),
            };
        }
    }

    let basis = match basis {
        Some(b) if !b.is_empty() => b,
        // No basis recorded on a git repo. NULL alone cannot tell "predates basis
        // tracking" from "HEAD was unreadable at index time" (the schema is frozen to one
        // nullable column), so the WRITE path recorded the distinction in the snapshot's
        // extraction diagnostics; `basis_outcome` is the caller's read of it. This does
        // NOT re-probe HEAD live — the recorded fact is authoritative, so an
        // unborn-then-committed repo is never mis-attributed to pre-slice history
        // (operator RULING 2/3, closing review-4/review-5).
        _ => return basis_none_state(basis_outcome),
    };

    let drift = match repo_graph_git::working_tree_drift(repo_path, basis) {
        Ok(d) => d,
        Err(e) => {
            return IndexDrift::Unknown {
                basis: Some(basis.to_string()),
                reason: git_reason(&e),
            }
        }
    };

    if drift.commits_ahead == 0 && drift.changed_files.is_empty() {
        return IndexDrift::Clean {
            basis: basis.to_string(),
        };
    }

    // Classify the M changed files against the index. A storage read failure here is
    // UNKNOWN (honest), never K=0 — we know there IS drift but cannot classify it.
    let (indexed_changed, modules) =
        match classify_changed(storage, snapshot_uid, &drift.changed_files) {
            Ok(v) => v,
            Err(reason) => {
                return IndexDrift::Unknown {
                    basis: Some(basis.to_string()),
                    reason,
                }
            }
        };

    IndexDrift::Drifted {
        basis: basis.to_string(),
        commits_ahead: drift.commits_ahead,
        files_changed: drift.changed_files.len() as u64,
        indexed_changed,
        modules,
    }
}

/// The `IndexDrift` to render when the repo row could not be resolved to an on-disk
/// path at query time — a storage MISS (`get_repo` → `Ok(None)`) or a storage READ
/// ERROR (`Err`). Git was never reached, so drift is genuinely UNKNOWN: return
/// `IndexDrift::Unknown` carrying the recorded `basis` verbatim (possibly `None` →
/// the surface renders "index basis: unknown") and the caller's `reason`.
///
/// Crucially this is NEVER `BasisUnknown`: "indexed before basis tracking" is a claim
/// about the SNAPSHOT's history that a failed/absent repo-metadata read does NOT
/// establish (review-3 finding 1) — and never a false `Clean`. Both `Unknown` and
/// `BasisUnknown` make `check` Incomplete, so the exit code is unchanged; only the
/// (honest) reason text differs.
///
/// Pure (takes the already-formatted `reason`, which at the `Err` call site embeds the
/// `StorageError`'s `Display`) so the "unresolved → Unknown, never BasisUnknown"
/// invariant is unit-testable without constructing a full dispatcher + storage stub —
/// the seam review-3's "add a deterministic test" requires, unobtainable more simply.
pub(crate) fn unresolved_repo_drift(basis_commit: Option<String>, reason: String) -> IndexDrift {
    IndexDrift::Unknown {
        // An empty-string basis is treated as no-basis, matching `compute_index_drift`.
        basis: basis_commit.filter(|b| !b.is_empty()),
        reason,
    }
}

/// Resolve the `IndexDrift` when a git repo carries NO stamped basis, from the WRITE-time
/// [`BasisOutcome`] record persisted by compose at index time (operator RULING 3 — replaces
/// the fragile query-time live HEAD re-probe that mis-attributed an unborn-then-committed
/// repo to pre-slice history, and the best-effort daemon write whose loss impersonated
/// pre-slice history, closing review-4/review-5). The recorded fact is authoritative for
/// "why no basis was stamped":
///   - `Ok(Some(NonGit))`  → `NotGit` (recorded non-git — the query need not re-probe).
///   - `Ok(Some(Failure))` → `Unknown` surfacing the recorded reason VERBATIM (unborn →
///     "repository has no commits yet"; other → "git HEAD unreadable at index time (…)").
///   - `Ok(Some(Basis))`   → an INCONSISTENT record (a basis was recorded but the column
///     is NULL — compose never writes this, so it can only be a hand-edited/corrupt blob);
///     `Unknown` with an honest reason, NEVER a silent `Clean`/`BasisUnknown`.
///   - `Ok(None)`          → NO record on a git repo ⇒ the snapshot predates this slice's
///     basis tracking → `BasisUnknown` (a `rmap refresh` WILL stamp it — actionable).
///   - `Err(reason)`       → the diagnostics blob was UNREADABLE ⇒ genuinely `Unknown` with
///     the reason, NEVER a false `BasisUnknown`/`Clean` (honesty rule #1).
///
/// Pure (takes the read outcome) so the distinctions are unit-testable without a live
/// repo or storage per case.
fn basis_none_state(basis_outcome: Result<Option<BasisOutcome>, String>) -> IndexDrift {
    match basis_outcome {
        Ok(Some(BasisOutcome::NonGit)) => IndexDrift::NotGit,
        Ok(Some(BasisOutcome::Failure { reason })) => IndexDrift::Unknown {
            basis: None,
            reason,
        },
        Ok(Some(BasisOutcome::Basis { .. })) => IndexDrift::Unknown {
            basis: None,
            reason: "inconsistent index-basis record: a basis was recorded but the snapshot's \
                     basis_commit is NULL"
                .to_string(),
        },
        Ok(None) => IndexDrift::BasisUnknown,
        Err(reason) => IndexDrift::Unknown {
            basis: None,
            reason,
        },
    }
}

/// Intersect the git-changed paths with the indexed file set (K) and derive the
/// modules those indexed-changed files belong to. Returns `Err(reason)` if any
/// backing storage read fails — the caller renders that as `Unknown`.
fn classify_changed<S: AgentStorageRead + ?Sized>(
    storage: &S,
    snapshot_uid: &str,
    changed_files: &[String],
) -> Result<(u64, Vec<String>), String> {
    // Module roots (declared/inferred). Empty is valid (repos without module
    // discovery) → no modules named, an honest omission. A READ ERROR is not empty.
    let module_roots = storage
        .list_module_sizes(snapshot_uid, usize::MAX)
        .map_err(|e| format!("module read failed: {e}"))?;
    let mut roots: Vec<String> = module_roots.into_iter().map(|m| m.path).collect();
    // Longest root first so the most specific module wins the prefix match.
    roots.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));

    let mut indexed_changed: u64 = 0;
    let mut modules: BTreeSet<String> = BTreeSet::new();

    for path in changed_files {
        // Exact-path membership in the index (list_files_in_path matches `path = ?`
        // plus `path/%`; a git-changed FILE path only exact-matches an indexed file).
        let entries = storage
            .list_files_in_path(snapshot_uid, path)
            .map_err(|e| format!("indexed-file read failed: {e}"))?;
        let is_indexed = entries.iter().any(|f| &f.path == path);
        if !is_indexed {
            continue;
        }
        indexed_changed += 1;
        if let Some(root) = module_of(path, &roots) {
            modules.insert(root);
        }
    }

    Ok((indexed_changed, cap_modules(modules)))
}

/// The most-specific module root that owns `path`, if any. `roots` must be sorted
/// longest-first. A file belongs to root `r` when `path == r` or `path` starts with
/// `r/` (a path-boundary match — `src/ab` never matches root `src/a`).
fn module_of(path: &str, roots: &[String]) -> Option<String> {
    roots
        .iter()
        .find(|r| path == r.as_str() || path.starts_with(&format!("{r}/")))
        .cloned()
}

/// Sort + cap the module set for a one-line footer; overflow folds into "+N more".
fn cap_modules(modules: BTreeSet<String>) -> Vec<String> {
    let all: Vec<String> = modules.into_iter().collect();
    if all.len() <= MODULE_NAME_CAP {
        return all;
    }
    let mut capped: Vec<String> = all.iter().take(MODULE_NAME_CAP).cloned().collect();
    capped.push(format!("+{} more", all.len() - MODULE_NAME_CAP));
    capped
}

/// One-line reason from a git error raised while computing working-tree drift, for the
/// `Unknown` variant (reader-facing).
///
/// `working_tree_drift` runs THREE git commands (`rev-list`, `diff`, `status`); only a
/// failure git reports as a MISSING revision/object licenses the "basis commit not found
/// / history rewritten" wording (the recorded basis sha is no longer reachable). Any OTHER
/// failure — a `git status`/`git diff` failure unrelated to the basis, a permission or
/// corruption error — is surfaced TRUTHFULLY via the error's own message (which names the
/// failing command + stderr), never mis-attributed to a rewritten basis (review-2 finding 2).
fn git_reason(e: &repo_graph_git::GitError) -> String {
    use repo_graph_git::GitError;
    match e {
        GitError::CommandFailed { stderr, .. } if references_missing_revision(stderr) => {
            "basis commit not found in git history (history may have been rewritten)".to_string()
        }
        other => other.to_string(),
    }
}

/// True when git's stderr indicates the failure is a MISSING revision/object — i.e. the
/// recorded basis sha is no longer reachable (a rewritten or pruned history), as opposed
/// to an unrelated git failure. Git emits these for a bad `<basis>..HEAD` range
/// (`rev-list`) or a `git diff <basis>` against an unknown sha.
///
/// Pure so the missing-basis-vs-generic distinction is unit-testable without forging a
/// pruned repository per case.
fn references_missing_revision(stderr: &str) -> bool {
    stderr.contains("bad revision")
        || stderr.contains("unknown revision")
        || stderr.contains("bad object")
        || stderr.contains("ambiguous argument")
}

// Unit tests live in a sibling file to keep this module under the 500-line structural
// guardrail (operator RULING 3). It is a submodule of `index_drift`, so `use super::*`
// reaches the private items it exercises.
#[cfg(test)]
#[path = "index_drift_tests.rs"]
mod tests;
