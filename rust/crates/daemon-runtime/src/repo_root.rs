//! STATE-ROOT-RELATIVE-REPO-ROOT-1 (RG-REQ-011-L13): a repository's working tree is found from
//! the daemon REGISTRY, checked, and passed explicitly to every working-tree reader.
//!
//! The registry entry's absolute `canonical_path` is the ONE source of a repository's root
//! (D-SRR-SCOPE-1 A). The store's `repos.root_path` is relative to the store file's directory,
//! retained for compatibility and never read to find files (D-SRR-ROOTPATH-1 A): joined onto the
//! store's CURRENT directory it pointed at a directory that does not exist once a state root moved
//! to another depth, and every reader then failed with a wrong cause or reported an absence.
//!
//! Abstraction ledger —
//! - **What:** the registry-root lookup (for a request, and for a store path + repo uid) and the
//!   check of that root, as `Result<PathBuf, RepoRootError>`; `RepoRootError`'s texts are the
//!   slice's §2.2 wording; `orientation_docs_for_root` is the one root-taking orientation-docs read.
//! - **Concrete current users:** the eleven working-tree readers of SLICE_DOC §1 — refresh (and the
//!   auto-enrich and seed passes it starts), docs list, docs extract, the drift probe behind orient /
//!   check / explain, churn, hotspots, risk, coverage, map, the document inventory behind orient and
//!   `modules list`, and enrichment (manual and auto).
//! - **Axis of variation:** none — one shared mechanism; it deduplicates the check the eleven sites
//!   would otherwise each repeat.
//! - **Rejected simpler alternative:** an inline check at each site — eleven copies of one rule, most
//!   in files over the 500-line guardrail.
//!
//! Not a replacement for `dispatch_seed::canonical_root`, which returns the UNCHECKED registry path
//! for cursors and `find --text` (a different contract; out of this slice's scope).

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use repo_graph_daemon_transport::{ErrorCode, ErrorDetail};
use serde_json::Value;

use crate::state::{DaemonState, RepoState};

/// Why a repository's working tree cannot be read. One variant per state of SLICE_DOC §2.3 that is
/// not a readable directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RepoRootError {
    /// The registered `canonical_path` does not exist, or exists and is not a directory.
    NotFound { path: PathBuf },
    /// A store-keyed request or pass found no registry entry for its store path + repo uid.
    Unknown { db_path: PathBuf, repo_uid: String },
}

/// The short §2.2 form: the drift reason (orient / check / explain), the orientation-docs
/// unavailable reason (orient / `modules list`), and the auto-enrich / seed pass failure reason.
impl fmt::Display for RepoRootError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RepoRootError::NotFound { path } => {
                write!(f, "repo root not found: {}", path.display())
            }
            RepoRootError::Unknown { db_path, repo_uid } => write!(
                f,
                "repo root unknown: no registry entry for {} ({})",
                db_path.display(),
                repo_uid
            ),
        }
    }
}

impl RepoRootError {
    /// The §2.2 error-message form for the surfaces that answer with an error (docs list, docs
    /// extract, churn, hotspots, risk, coverage, map, refresh, enrich). It names only the observed
    /// condition and the two next actions; the `rmap repo remove` path is shell-quoted so the printed
    /// command runs as printed (`rmap repo remove` resolves a dead path — FORGET-REPO-1).
    pub(crate) fn request_error_message(&self) -> String {
        match self {
            RepoRootError::NotFound { path } => format!(
                "{self} (missing or not a directory) — if the repository still exists, run rmap \
                 index inside its directory; or remove this registration: rmap repo remove {}",
                crate::reclaim::shell_quote(&path.to_string_lossy())
            ),
            RepoRootError::Unknown { .. } => self.to_string(),
        }
    }

    /// The error a request surface returns: the code it uses today for a bad request
    /// (`InvalidRequest`) — no new protocol error code.
    pub(crate) fn to_error_detail(&self) -> ErrorDetail {
        ErrorDetail::invalid_request(self.request_error_message())
    }
}

/// A repository root as the registry gives it, checked: `Ok` is an existing directory; `Err` names
/// why the working tree cannot be read.
pub(crate) type CheckedRepoRoot = Result<PathBuf, RepoRootError>;

/// Check a registered root: `Ok(path)` iff it exists and is a directory, else
/// [`RepoRootError::NotFound`] carrying the path as registered.
pub(crate) fn check_registered_root(canonical_path: &Path) -> CheckedRepoRoot {
    if canonical_path.is_dir() {
        Ok(canonical_path.to_path_buf())
    } else {
        Err(RepoRootError::NotFound {
            path: canonical_path.to_path_buf(),
        })
    }
}

/// A request's repository, resolved through the registry and loaded, WITH its checked root.
pub(crate) struct ResolvedRepo {
    pub(crate) repo_state: Arc<RepoState>,
    pub(crate) repo_uid: String,
    /// Registry alias if present, else the basename of the registered path (the CLI-OUT-2B
    /// display name `resolve_and_load_repo_with_display_name` derives).
    pub(crate) display_name: String,
    /// The registry entry's `canonical_path`, checked. A request surface decides what an `Err`
    /// means for it (an error, or a named drift / orientation state).
    pub(crate) root: CheckedRepoRoot,
}

/// Resolve the `repo` param through the registry (alias or path), load the repo, and keep the
/// registry entry's root, checked. The sibling of `handlers::support::resolve_and_load_repo` and of
/// `ServiceDispatcher::{resolve_and_load_repo, resolve_and_load_repo_with_display_name}`: the same
/// resolution and the same errors, plus the root those functions drop. They stay unchanged for their
/// callers that never read the working tree (assess, violations, policy, dead_causes, reliability…).
pub(crate) fn resolve_and_load_repo_with_root(
    state: &DaemonState,
    params: &Value,
) -> Result<ResolvedRepo, ErrorDetail> {
    let repo_ref = params
        .get("repo")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ErrorDetail::invalid_request("missing or invalid 'repo' parameter"))?;

    let entry = state.resolve_alias_or_path(repo_ref).ok_or_else(|| {
        ErrorDetail::new(
            ErrorCode::RepoNotFound,
            format!(
                "repo not indexed: {} (run: rmap index {})",
                repo_ref, repo_ref
            ),
        )
    })?;

    let display_name = entry.alias.clone().unwrap_or_else(|| {
        entry
            .canonical_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(&entry.repo_uid)
            .to_string()
    });

    let repo_state = state
        .load_repo(&entry.db_path, &entry.repo_uid)
        .map_err(|e| ErrorDetail::new(ErrorCode::InternalError, e))?;

    Ok(ResolvedRepo {
        repo_state,
        repo_uid: entry.repo_uid,
        display_name,
        root: check_registered_root(&entry.canonical_path),
    })
}

/// The checked registry root for a pass or request keyed by store path + repo uid (the auto-enrich
/// and seed passes, the legacy `enrich {db_path, repo_uid}` request). `db_path` is compared in
/// CANONICAL form against each entry's canonicalized `db_path` — never as the raw request string —
/// together with the repo uid. A registry entry's `db_path` is allocated from its own canonical path
/// (`registry::allocate_db_path`), so at most one entry matches a store. No match →
/// [`RepoRootError::Unknown`].
pub(crate) fn registered_root_for_store(
    state: &DaemonState,
    db_path: &Path,
    repo_uid: &str,
) -> CheckedRepoRoot {
    let canonical_db = db_path
        .canonicalize()
        .unwrap_or_else(|_| db_path.to_path_buf());
    // Copy the candidates out, then release the registry lock before any filesystem call.
    let candidates: Vec<(PathBuf, PathBuf)> = state
        .registry()
        .list()
        .into_iter()
        .filter(|e| e.repo_uid == repo_uid)
        .map(|e| (e.db_path.clone(), e.canonical_path.clone()))
        .collect();
    let matched = candidates.into_iter().find(|(entry_db, _)| {
        entry_db
            .canonicalize()
            .map(|c| c == canonical_db)
            .unwrap_or_else(|_| entry_db == &canonical_db)
    });
    match matched {
        Some((_, canonical_path)) => check_registered_root(&canonical_path),
        None => Err(RepoRootError::Unknown {
            db_path: canonical_db,
            repo_uid: repo_uid.to_string(),
        }),
    }
}

/// The orientation-docs recommendation (MODULES-METHOD-1 §2.2) read at the checked registry root —
/// the one root-taking read shared by orient's additive fields and `modules list`.
///
/// - `Err(root error)` → `Unavailable` with the [`RepoRootError`] text (rendered by the existing
///   renderer as `Orientation docs unavailable: repo root not found: …`), never an
///   `AgentStorageError` wrapper and never the no-docs recommendation.
/// - a discovery failure under an existing root → `Unavailable` with that error (unchanged).
/// - otherwise the selection, with vendored paths demoted to kind `vendored` so they never become
///   an orientation recommendation (DOCS-LIST-2 review-1 fix #1, unchanged).
pub(crate) fn orientation_docs_for_root(
    root: &CheckedRepoRoot,
) -> crate::modules_method::OrientationDocsResult {
    let root = match root {
        Ok(root) => root,
        Err(e) => {
            return crate::modules_method::OrientationDocsResult::Unavailable {
                reason: e.to_string(),
            }
        }
    };
    match repo_graph_storage::doc_inventory_at_root(root) {
        Ok(doc_inventory) => {
            let orientation_inputs: Vec<crate::modules_method::OrientationDocInput> = doc_inventory
                .iter()
                .map(|d| {
                    let kind = if crate::handlers::quality::support::is_vendored_path(&d.path) {
                        "vendored"
                    } else {
                        d.kind.as_str()
                    };
                    crate::modules_method::OrientationDocInput {
                        path: d.path.as_str(),
                        kind,
                        generated: d.generated,
                    }
                })
                .collect();
            let paths = crate::modules_method::select_orientation_docs(&orientation_inputs);
            crate::modules_method::OrientationDocsResult::Ok {
                paths: paths.into_iter().map(|s| s.to_string()).collect(),
            }
        }
        Err(e) => crate::modules_method::OrientationDocsResult::Unavailable {
            reason: e.to_string(),
        },
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// TEST HOOK — the root the SEED pass received (D-SRR-SEED-OBSERVABLE-1 with Correction 1)
// ─────────────────────────────────────────────────────────────────────────────
//
// Abstraction ledger — **What:** a `#[doc(hidden)]` test hook: the seed pass's call site, right after
// its OWN successful registry lookup and before `LocalEmbedder::load`, reports the root it received;
// an installed observer records it as the seed pass's root and may ask the pass to end there.
// **Concrete current user:** `tests/repo_root_from_registry.rs::refresh_hands_the_registry_root_to_enrich_and_seed`.
// **Axis:** a test seam unobtainable more simply — the seed pass's working-tree read sits behind a
// network model load (`seed/local_engine.rs`), so a hermetic test cannot reach it. **Rejected simpler
// alternative:** observing the shared lookup [`registered_root_for_store`] — the enrich pass calls it
// first with the same arguments, so a record there exists whether or not the seed pass ever asks.
//
// INERT unless a test installs it: with no observer installed the hook records nothing and always
// answers [`AfterSeedRootLookup::Continue`]; no production caller installs it. A test clears it with
// [`clear_test_seed_root_observer`] after use (mirrors `enrich_pass::{set,clear}_test_registry_builder`).

/// What the seed pass does after it reported its root to the test hook.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AfterSeedRootLookup {
    /// No observer, or an observer that does not end the pass: the pass goes on unchanged.
    Continue,
    /// An installed observer asked the pass to end right after the lookup (no model load).
    EndPass,
}

struct SeedRootObserver {
    end_pass: bool,
    observed: Vec<(String, PathBuf)>,
}

static TEST_SEED_ROOT_OBSERVER: parking_lot::Mutex<Option<SeedRootObserver>> =
    parking_lot::const_mutex(None);

/// TEST HOOK — install an observer of the root each seed pass receives from its registry lookup,
/// replacing any earlier one (its records dropped). `end_pass = true` makes every seed pass end right
/// after that lookup, before any model load. Clear it with [`clear_test_seed_root_observer`].
#[doc(hidden)]
pub fn set_test_seed_root_observer(end_pass: bool) {
    *TEST_SEED_ROOT_OBSERVER.lock() = Some(SeedRootObserver {
        end_pass,
        observed: Vec::new(),
    });
}

/// TEST HOOK — remove the installed observer and its records (see [`set_test_seed_root_observer`]).
#[doc(hidden)]
pub fn clear_test_seed_root_observer() {
    *TEST_SEED_ROOT_OBSERVER.lock() = None;
}

/// TEST HOOK — the `(repo uid, root)` pairs the seed pass reported since the observer was installed,
/// in order; empty when no observer is installed.
#[doc(hidden)]
pub fn test_seed_roots_observed() -> Vec<(String, PathBuf)> {
    TEST_SEED_ROOT_OBSERVER
        .lock()
        .as_ref()
        .map(|o| o.observed.clone())
        .unwrap_or_default()
}

/// Called ONLY by the seed pass, right after its own successful registry lookup, with the root it
/// received. Records nothing and answers `Continue` unless a test installed an observer.
pub(crate) fn report_seed_root_to_test_observer(
    repo_uid: &str,
    root: &Path,
) -> AfterSeedRootLookup {
    let mut guard = TEST_SEED_ROOT_OBSERVER.lock();
    match guard.as_mut() {
        None => AfterSeedRootLookup::Continue,
        Some(observer) => {
            observer
                .observed
                .push((repo_uid.to_string(), root.to_path_buf()));
            if observer.end_pass {
                AfterSeedRootLookup::EndPass
            } else {
                AfterSeedRootLookup::Continue
            }
        }
    }
}
