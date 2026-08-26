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
use repo_graph_doc_facts as doc_facts;
use repo_graph_repo_index::compose::BasisOutcome;

/// Cap on named modules in a drift line; the remainder folds into "+N more" so the
/// footer stays one line. Deterministic (modules are sorted before capping).
const MODULE_NAME_CAP: usize = 6;

// The write-side basis vocabulary lives in `index_basis_probe` (guardrail split);
// re-exported here so existing call sites keep one import path.
pub(crate) use crate::index_basis_probe::{
    basis_at_index, basis_outcome_from_probe, read_basis_outcome,
};

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

    // SELF-POLLUTION-1 §2.2: rmap must not measure its OWN exhaust. Split the changed
    // set into the READER's paths and rmap's own `map` sidecars / `.rgr/` / OS noise.
    // Only the reader's paths count toward `files_changed` (M) and get classified; the
    // excluded count is surfaced on the drift line so nothing is silently hidden.
    // `unreadable` is the sub-count of reader paths that are sidecar-NAMED but could
    // not be read to check the marker — counted (conservative) yet flagged, never
    // silently asserted "not generated" (operator RULING 3).
    let (reader_changed, excluded, unreadable) = partition_changed(repo_path, &drift.changed_files);

    // Classify the reader's M changed files against the index. A storage read failure
    // here is UNKNOWN (honest), never K=0 — we know there IS drift but cannot classify it.
    let (indexed_changed, modules) = match classify_changed(storage, snapshot_uid, &reader_changed)
    {
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
        files_changed: reader_changed.len() as u64,
        indexed_changed,
        modules,
        excluded,
        unreadable,
    }
}

/// How a single git-changed path classifies against rmap's own exhaust.
enum DriftClass {
    /// rmap's OWN exhaust / OS noise — not the reader's change (dropped from M, counted
    /// in `excluded`).
    Excluded,
    /// The reader's change (counts toward M).
    Reader,
    /// A sidecar-NAMED path we could NOT read to check the marker (a genuine read
    /// failure — permission/IO — NOT a `NotFound`). Counted as the reader's change
    /// (conservative), but ALSO flagged so the drift line can surface it — we never
    /// assert "not generated" from a failed read (operator RULING 3, honesty rule #1).
    UnreadableReader,
}

/// Partition git-changed paths into (the reader's changed paths, count of rmap's own
/// exhaust / OS noise excluded, count of sidecar-named-but-unreadable reader paths).
/// Excluded = rmap's OWN `map` sidecars (confirmed by the first-line marker — read
/// ONLY for sidecar-NAMED candidates, honesty rule: a bare name is not evidence), the
/// `.rgr/` tool-state dir, and `.DS_Store`-class OS noise. Unreadable sidecar-named
/// paths stay the reader's (counted in M) and are reported separately. Both counts are
/// surfaced on the drift line (SELF-POLLUTION-1 §2.2, operator RULING 3).
fn partition_changed(repo_path: &Path, changed_files: &[String]) -> (Vec<String>, u64, u64) {
    let mut readers = Vec::new();
    let mut excluded: u64 = 0;
    let mut unreadable: u64 = 0;
    for path in changed_files {
        match classify_for_drift(repo_path, path) {
            DriftClass::Excluded => excluded += 1,
            DriftClass::Reader => readers.push(path.clone()),
            DriftClass::UnreadableReader => {
                readers.push(path.clone());
                unreadable += 1;
            }
        }
    }
    (readers, excluded, unreadable)
}

/// Classify one changed path. The `rmap map` marker read is gated to sidecar-NAMED
/// candidates so a large changed-set is never fully read (DEC-1: §3's name-only
/// exception is NOT invoked — reads are bounded to candidates and honesty is
/// preferred). `.rgr/` and OS noise are name-definitional (no read). A read failure on
/// a sidecar candidate distinguishes `NotFound` (the file is genuinely gone — a
/// deleted sidecar IS a real reader change, `Reader`) from any other IO error
/// (`UnreadableReader` — counted but flagged, never silently "not generated").
fn classify_for_drift(repo_path: &Path, rel_path: &str) -> DriftClass {
    if doc_facts::is_os_noise(rel_path) || doc_facts::is_tool_state_path(rel_path) {
        return DriftClass::Excluded;
    }
    if !doc_facts::has_map_sidecar_name(rel_path) {
        return DriftClass::Reader;
    }
    match read_candidate_first_line(&repo_path.join(rel_path)) {
        CandidateLine::Line(line) => {
            if doc_facts::is_self_generated(rel_path, Some(&line)) {
                DriftClass::Excluded
            } else {
                DriftClass::Reader
            }
        }
        // NotFound: the file is gone (a deleted sidecar) → a real reader change, no
        // marker to read, honestly not-generated.
        CandidateLine::Absent => DriftClass::Reader,
        // A genuine read failure: we cannot prove exhaust, so keep it as the reader's
        // change AND flag it (never assert "not generated" from a failed read).
        CandidateLine::Unreadable => DriftClass::UnreadableReader,
    }
}

/// The outcome of reading a sidecar candidate's first line for the marker check.
enum CandidateLine {
    /// The file was read; carries its first line (empty string for an empty file).
    Line(String),
    /// `io::NotFound` — the file is genuinely absent (deleted). Only `NotFound` means
    /// absent (honesty rule #1).
    Absent,
    /// Any other IO error — the file exists (or its state is unknown) but could not be
    /// read; we must not treat it as evidence either way.
    Unreadable,
}

/// Read a candidate sidecar's first line, distinguishing genuinely-absent (`NotFound`)
/// from unreadable (permission/IO/etc.) — the honesty distinction operator RULING 3
/// requires (a `.ok()` collapse would erase it).
fn read_candidate_first_line(abs_path: &Path) -> CandidateLine {
    match std::fs::read_to_string(abs_path) {
        Ok(content) => CandidateLine::Line(content.lines().next().unwrap_or("").to_string()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => CandidateLine::Absent,
        Err(_) => CandidateLine::Unreadable,
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
