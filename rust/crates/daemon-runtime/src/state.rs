//! Daemon state management.
//!
//! Holds per-repo coordinators and storage connections.
//!
//! # Identity Model
//!
//! The daemon manages multiple databases, each containing multiple repos.
//! Identity is composite:
//!
//! - **Database identity**: canonical `db_path`
//! - **Repo identity**: `(db_path, repo_uid)` via `RepoKey`
//!
//! # Coordination Model
//!
//! Two coordination levels:
//!
//! 1. **Database-scoped write coordination** (`DatabaseState`):
//!    - Ensures single-writer for any DB mutation (index, refresh, enrich)
//!    - Keyed by canonical `db_path`
//!    - Acquired before any write-class operation
//!
//! 2. **Repo-scoped read/write coordination** (`RepoCoordinator`):
//!    - Reader/writer semantics for loaded repo queries
//!    - Keyed by `RepoKey`
//!    - Acquired for repo-level operations after DB coordination
//!
//! # State Root Mode (STATE-ROOT-SEPARATION-1)
//!
//! The daemon operates in one of two modes based on its state root:
//!
//! - **Global mode**: Normal operation, all writes allowed
//! - **Sandbox-local mode**: Ephemeral sandbox, A1 authority writes blocked
//!
//! See `agent_docs/storage-architecture-v2.md` for the A1/A2/B tier model.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

// ── State Root Mode ─────────────────────────────────────────────────────

/// State root operation mode.
///
/// Determines which classes of writes are permitted:
///
/// | Mode | A1 (User Authority) | A2 (Operational) | B (Cache) |
/// |------|---------------------|------------------|-----------|
/// | Global | Allowed | Allowed | Allowed |
/// | SandboxLocal | **Blocked** | Allowed | Allowed |
///
/// See `agent_docs/storage-architecture-v2.md` for tier definitions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateRootMode {
    /// Normal operation: global state root, all writes allowed.
    ///
    /// Examples:
    /// - `~/Library/Application Support/repo-graph/` (macOS)
    /// - `~/.local/share/rmap/` (Linux)
    Global,

    /// Sandbox fallback: sandbox-local state root, A1 writes blocked.
    ///
    /// Sandbox mode is detected when the state root is under `/private/tmp/`.
    /// This is the macOS sandbox-writable temp directory used by Codex and
    /// similar sandboxed environments.
    ///
    /// Example: `/private/tmp/repo-graph-agent/501/`
    SandboxLocal,
}

impl StateRootMode {
    /// Returns true if A1 (user authority) writes are allowed.
    pub fn allows_authority_writes(&self) -> bool {
        matches!(self, StateRootMode::Global)
    }

    /// Returns the mode as a string for diagnostics.
    pub fn as_str(&self) -> &'static str {
        match self {
            StateRootMode::Global => "global",
            StateRootMode::SandboxLocal => "sandbox-local",
        }
    }
}

use parking_lot::{Mutex, MutexGuard};
use repo_graph_daemon_policy::RepoCoordinator;
use repo_graph_storage::types::RepoRef;
use repo_graph_storage::StorageConnection;

use crate::registry::{RegistryError, RepoRegistry};

// ── Database-scoped coordination ────────────────────────────────────

/// Database-level runtime state.
///
/// Provides write coordination for a single database file. Any operation
/// that mutates the database (index, refresh, enrich, declarations) must
/// acquire the write lock before proceeding.
///
/// This is separate from repo-level coordination: a database may contain
/// multiple repos, or may not have any repos loaded yet (during initial index).
pub struct DatabaseState {
    /// Canonical (absolute) path to the database file.
    db_path: PathBuf,

    /// Write lock for exclusive database mutations.
    ///
    /// Only one write-class operation can proceed at a time per database.
    /// Read operations on loaded repos use their own repo-level coordinators.
    write_lock: Mutex<()>,
}

impl DatabaseState {
    /// Create a new database state for the given path.
    ///
    /// The path should already be canonicalized.
    fn new(db_path: PathBuf) -> Self {
        Self {
            db_path,
            write_lock: Mutex::new(()),
        }
    }

    /// Get the canonical database path.
    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    /// Acquire exclusive write access to the database.
    ///
    /// Returns a guard that releases the lock when dropped.
    pub fn acquire_write(&self) -> DbWriteGuard<'_> {
        DbWriteGuard {
            _guard: self.write_lock.lock(),
        }
    }

    /// Try to acquire write access without blocking.
    pub fn try_acquire_write(&self) -> Option<DbWriteGuard<'_>> {
        self.write_lock
            .try_lock()
            .map(|g| DbWriteGuard { _guard: g })
    }
}

/// Guard that holds exclusive write access to a database.
pub struct DbWriteGuard<'a> {
    _guard: MutexGuard<'a, ()>,
}

/// Unique key for a loaded repo in the daemon.
///
/// The key combines the canonical database path and repo_uid to avoid
/// collisions when the same repo_uid exists in different databases.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RepoKey {
    /// Canonical (absolute) path to the database file.
    pub db_path: PathBuf,
    /// The repo identifier within that database.
    pub repo_uid: String,
}

impl RepoKey {
    /// Create a new repo key.
    ///
    /// Canonicalizes the db_path to ensure consistent identity.
    pub fn new(db_path: &Path, repo_uid: &str) -> Result<Self, String> {
        let canonical = db_path
            .canonicalize()
            .map_err(|e| format!("cannot canonicalize db path '{}': {}", db_path.display(), e))?;
        Ok(Self {
            db_path: canonical,
            repo_uid: repo_uid.to_string(),
        })
    }

    /// Format as a display string for listing.
    pub fn display(&self) -> String {
        format!("{}:{}", self.db_path.display(), self.repo_uid)
    }
}

/// Per-repo state held by the daemon.
pub struct RepoState {
    /// The unique key for this repo.
    pub key: RepoKey,

    /// Concurrency coordinator for this repo.
    pub coordinator: RepoCoordinator,

    /// LIVEGRAPH-INTEGRATION-1B: optional in-memory LiveGraph, populated by the dev-only
    /// `livegraph_preload` method (`None` until preloaded). Interior mutability because `RepoState`
    /// is shared as `Arc<RepoState>` — preload write-locks, callers/callees read-lock.
    pub livegraph: parking_lot::RwLock<Option<repo_graph_livegraph::LiveGraph>>,

    /// IMPORTS-LIVEGRAPH-DEFAULT-FASTPATH-1: the in-memory repo-level import NO-LOSS certificate (`None` until
    /// lazily built on the first eligible default `imports` query). Keyed by the import-cert fingerprint; a
    /// fingerprint mismatch invalidates + rebuilds. NOT durable (rebuilt on restart). Interior mutability:
    /// the fastpath read-locks; the lazy build write-locks.
    pub import_cert: parking_lot::RwLock<Option<crate::livegraph_feed::ImportNoLossCert>>,

    /// CYCLES-LIVEGRAPH-DEFAULT-FASTPATH-1: the in-memory repo-level MODULE-cycle NO-LOSS certificate (`None`
    /// until lazily built on the first eligible default `cycles` query). Keyed by the SAME SQLite-free
    /// fingerprint as `import_cert` (partitions + snapshot + policy); a fingerprint mismatch invalidates +
    /// rebuilds. NOT durable (rebuilt on restart). Interior mutability: the fastpath read-locks; the lazy build
    /// write-locks.
    pub cycles_cert: parking_lot::RwLock<Option<crate::livegraph_feed::CycleNoLossCert>>,

    /// STATS-LIVEGRAPH-IMPL-1: the in-memory repo-level STATS NO-LOSS certificate (`None` until lazily built on
    /// the first eligible default `stats` query). Keyed by the SAME SQLite-free fingerprint as
    /// `import_cert`/`cycles_cert`; a fingerprint mismatch invalidates + rebuilds. NOT durable (rebuilt on
    /// restart). Interior mutability: the fastpath read-locks; the lazy build write-locks.
    pub stats_cert: parking_lot::RwLock<Option<crate::livegraph_feed::StatsNoLossCert>>,

    /// ORIENT-LIVEGRAPH-IMPL: the in-memory repo-level COMPLEXITY NO-LOSS certificate (`None` until lazily
    /// built on the first eligible `orient` repo-focus query that emits HIGH_COMPLEXITY). `verdict == GREEN`
    /// iff the LiveGraph repo-wide `high_complexity` set is field-exact equal to the SQLite `measurements`
    /// high-complexity set. Keyed by the SAME SQLite-free fingerprint as `import_cert`/`cycles_cert`/
    /// `stats_cert`; a fingerprint mismatch invalidates + rebuilds. NOT durable (rebuilt on restart).
    /// Interior mutability: the orient decision read-locks; the lazy build write-locks.
    pub complexity_cert:
        parking_lot::RwLock<Option<crate::orient_lg_decisions::ComplexityNoLossCert>>,

    /// FOCUS-RESOLUTION-LIVEGRAPH-IMPL: the in-memory repo-level FOCUS-RESOLUTION NO-LOSS certificate
    /// (`None` until lazily built by `focus_resolution_cert::build_and_store_focus_resolution_cert`).
    /// `verdict == GREEN` iff the LiveGraph focus resolution (`focus_resolver`) is field-exact equal
    /// to the SQLite `resolve_*` resolution over the resident corpus. Keyed by the SAME SQLite-free
    /// fingerprint as `import_cert`/`cycles_cert`/`stats_cert`/`complexity_cert`; a fingerprint
    /// mismatch invalidates + rebuilds. NOT durable (rebuilt on restart). The later
    /// COHERENCE-LEAF-SERVE consumer reads this to gate its focused-orient/explain fastpath; this
    /// slice builds + stores it standalone (no consumer wiring yet).
    pub focus_resolution_cert:
        parking_lot::RwLock<Option<crate::focus_resolution_cert::FocusResolutionNoLossCert>>,

    /// COHERENCE-LEAF-SERVE-IMPL-1: the in-memory repo-level CALLGRAPH NO-LOSS certificate (`None`
    /// until lazily built by `callgraph_cert::build_and_store_callgraph_cert`). `verdict == GREEN`
    /// iff the LiveGraph callers/callees rows (`callers`/`callees` + `symbol_context` enrichment) are
    /// field-exact equal — as multisets — to the SQLite `find_symbol_callers`/`find_symbol_callees`
    /// rows for EVERY symbol in the resident∪SQLite corpus. This is the cacheable, ZERO-read serve
    /// mechanism the orient bounded (b)-leaf fastpath needs (the shipped per-call `gate_callgraph_no_loss`
    /// reads SQLite EVERY call — disqualifying). Keyed by the SAME SQLite-free fingerprint as
    /// `import_cert`/`cycles_cert`/`stats_cert`/`complexity_cert`/`focus_resolution_cert`; a fingerprint
    /// mismatch invalidates + rebuilds. NOT durable (rebuilt on restart). Interior mutability: the orient
    /// serve decision read-locks; the lazy build write-locks.
    pub callgraph_cert: parking_lot::RwLock<Option<crate::callgraph_cert::CallgraphNoLossCert>>,

    /// RECON-M-R1: the WITNESS LEDGER — the callgraph cert's compare generalized into the
    /// full-walk, instance-level, kind-aligned witness-agreement classification (`None` until
    /// lazily built; written by the SAME `callgraph_cert::build_and_store_callgraph_cert` that
    /// stores the cert, under the SAME fingerprint key). The stored cert's GREEN/RED verdict is
    /// now DERIVED from this ledger (`WitnessLedger::derived_green` — behavior byte-unchanged).
    /// In-memory ONLY, non-durable, dead on any fingerprint movement — the ratified D-R8 lifecycle
    /// (no persisted family; a persisted rate could misdescribe the current witness pair).
    /// M-R1 stores it as measurement infrastructure; M-R2 union serving and M-R3 read surfaces are
    /// its consumers.
    pub witness_ledger: parking_lot::RwLock<Option<crate::callgraph_cert::ledger::WitnessLedger>>,

    /// RECON-M-R2 (the §4.2 transient-2 retention): the LAST witness-ledger BUILD FAILURE, retained
    /// so doctor CAN report "ledger absent + build-failure reason" (recon-design-1 §5.4 — the
    /// RENDERING is M-R3a's, exactly like the M-R1 collision-rendering amendment; this field is the
    /// SUBSTANCE). `Some` iff the most recent ledger build attempt returned `None` (M-R1 contract:
    /// that happens ONLY on a SQLite error during the walk — the reason granularity is therefore
    /// the class, not the underlying error string). Cleared by every successful ledger store. An
    /// OPERATIONAL fact about US (our capture), never a per-edge or regime label — it changes NO
    /// served bytes.
    pub witness_ledger_build_failure:
        parking_lot::RwLock<Option<crate::callgraph_cert::ledger::LedgerBuildFailure>>,

    /// EC-M2-LEAF-SERVE-1: the in-memory repo-level MODULE-SUMMARY structural-count NO-LOSS
    /// certificate — the DR-2/DR-E3 `module_stats`-pattern IDENTITY-RECONCILIATION cert (`None`
    /// until lazily built by `module_summary_cert::build_and_store_module_summary_cert`).
    /// `verdict == GREEN` iff the LiveGraph per-file structural inventory reconciles with the
    /// SQLite one at EVERY granularity: per-file (path presence + AST-symbol count + language),
    /// per-module (dirname rollup — the ratified per-module identity reconciliation), AND the
    /// exact `compute_repo_summary` totals (file/symbol/languages). ANY divergence ⇒ RED ⇒ the
    /// decorator keeps serving `compute_{repo,path,file}_summary` from SQLite (no silent drift —
    /// the RISK-E answer). Keyed by the SAME SQLite-free fingerprint as its sibling certs; a
    /// fingerprint mismatch invalidates + rebuilds. NOT durable (rebuilt on restart).
    pub module_summary_cert:
        parking_lot::RwLock<Option<crate::module_summary_cert::ModuleSummaryNoLossCert>>,
}

impl RepoState {
    /// Open a repo's state from the database path.
    ///
    /// Validates that the repo actually exists in the database before
    /// returning success. This prevents silent failures at query time.
    pub fn open(db_path: &Path, repo_uid: &str) -> Result<Self, String> {
        // review-10: classify with `fs::metadata`, NOT `exists()`. `exists()` collapses every
        // metadata fault (permission denied, ENOTDIR on an ancestor) into `false`, which would
        // mislabel a real I/O fault as "database not found". Only a genuine NotFound is that
        // fast-path absence message; any other stat fault falls through to `open_existing`, which
        // reports the true fault (`Sqlite`) rather than a false absence.
        match std::fs::metadata(db_path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(format!("database not found: {}", db_path.display()));
            }
            _ => {}
        }

        // DAEMON-CONCURRENCY-IMPL-1 (D-S = S-A): open a connection ONLY to validate
        // the repo exists at load time; it is dropped at the end of this fn. Reads
        // open their own connection per operation (see `storage()`) — `RepoState`
        // holds NO shared `!Sync` connection, which is what makes it `Send + Sync`.
        // FORGET-REPO-1: NO-CREATE (`open_existing`) — a load must never recreate a
        // DB removed out-of-band between the `exists()` check above and this open;
        // only the index/create path may create.
        let validation_conn = StorageConnection::open_existing(db_path)
            .map_err(|e| format!("failed to open database: {}", e))?;

        // Validate repo exists in the database
        match validation_conn.get_repo(&RepoRef::Uid(repo_uid.to_string())) {
            Ok(Some(_)) => {}
            Ok(None) => {
                return Err(format!(
                    "repo '{}' not found in database '{}'",
                    repo_uid,
                    db_path.display()
                ));
            }
            Err(e) => {
                return Err(format!("failed to verify repo: {}", e));
            }
        }
        drop(validation_conn);

        let key = RepoKey::new(db_path, repo_uid)?;

        Ok(Self {
            key,
            coordinator: RepoCoordinator::new(),
            livegraph: parking_lot::RwLock::new(None),
            import_cert: parking_lot::RwLock::new(None),
            cycles_cert: parking_lot::RwLock::new(None),
            stats_cert: parking_lot::RwLock::new(None),
            complexity_cert: parking_lot::RwLock::new(None),
            focus_resolution_cert: parking_lot::RwLock::new(None),
            callgraph_cert: parking_lot::RwLock::new(None),
            witness_ledger: parking_lot::RwLock::new(None),
            witness_ledger_build_failure: parking_lot::RwLock::new(None),
            module_summary_cert: parking_lot::RwLock::new(None),
        })
    }

    /// Get the repo_uid.
    pub fn repo_uid(&self) -> &str {
        &self.key.repo_uid
    }

    /// Get the database path.
    pub fn db_path(&self) -> &Path {
        &self.key.db_path
    }

    /// Open a fresh storage connection for one read operation (D-S = S-A,
    /// connection-per-operation).
    ///
    /// DAEMON-CONCURRENCY-IMPL-1: `RepoState` no longer holds a shared
    /// `StorageConnection`. `rusqlite::Connection` is `Send` but `!Sync`, so a
    /// shared connection would make `RepoState` `!Sync` and unshareable across the
    /// concurrent connection-handler threads. Instead each read operation opens its
    /// own connection here, via the NO-CREATE [`StorageConnection::open_existing`] —
    /// which runs the idempotent migration check (NO fast-open that could serve an
    /// unmigrated schema, per the §14 ratification; that would be a Layer-0 honesty
    /// violation). SQLite WAL gives true concurrent reads across these per-operation
    /// connections.
    ///
    /// FORGET-REPO-1 (operator ruling 2): this is the serving choke point, so it MUST
    /// be no-create. A request holding a stale `Arc<RepoState>` from before a
    /// `reclaim::forget_repo` could otherwise reach here after the deletion and
    /// recreate the removed DB as an unregistered orphan (`open` passes
    /// `SQLITE_OPEN_CREATE`). `open_existing` makes that stale read fail honestly with
    /// [`StorageError::DatabaseMissing`] and writes nothing. Only the index/create
    /// path (`compose::open_or_create_storage`, re-verifying registration under the
    /// held DB write slot) may bring a DB into existence.
    ///
    /// REQUEST-LEVEL CONSISTENCY: a read handler holds its coordinator read guard
    /// (`acquire_read`) for the whole request, which — under W-A — excludes every
    /// coordinated writer (index/refresh/enrich and, after this slice, the LiveGraph
    /// preload/refresh writers). So all connections a handler opens during one
    /// request observe the SAME committed snapshot; no writer can commit mid-request.
    ///
    /// COST: the normal open re-runs migration 001's idempotent DDL + a version-gate
    /// scan on each call (read-only on an already-migrated WAL DB). This is the
    /// ratified per-op-open cost; a per-repo connection pool (D-S = S-B) is the named
    /// upgrade lever if profiling shows it is hot.
    ///
    /// Writers are unaffected: they already open their own connection in the compose
    /// pipeline (`index_path_with_progress`/`refresh_path_with_progress`).
    pub fn storage(&self) -> Result<StorageConnection, String> {
        // FOREGROUND-LOCK-1: foreground opens now carry SHORT bounded patience (was ZERO) so a
        // transient `SQLITE_BUSY` from a concurrent background pass's lock-upgrade clears within a
        // sub-half-second wait instead of failing the request outright (the audit + test-flake
        // family, root-caused 5535092). Non-lock faults still surface immediately (see
        // [`OpenPatience`] / [`open_existing_with_busy_retry`]). The SIGNATURE is unchanged: the
        // ~140 internal/secondary callers keep the flat `String` error; the honest `Busy`
        // holder-naming re-code (§2.2) lives in [`crate::foreground_open`], which the dispatch
        // handlers call in place of the bare `storage()` + `InternalError` wrap.
        open_existing_with_busy_retry(&self.key.db_path, OpenPatience::Foreground)
            .map_err(|e| e.to_string())
    }

    /// [`storage`](Self::storage) with the LONGER [`OpenPatience::Background`] busy budget for
    /// BACKGROUND passes (seed / retention). SQLite returns `SQLITE_BUSY` IMMEDIATELY — bypassing
    /// the 5s busy handler — on transaction lock-upgrade conflicts (the migration check's write
    /// colliding with a concurrent read transaction), so an open using zero patience could SKIP on
    /// a transient lock (bitten 2026-08-31: `op seed skipped: could not open storage: database is
    /// locked` killed the v0.12.0 cut's seed_seam run). Retries the open up to 4× at 250ms ONLY
    /// when the error text names a lock; any other error (missing DB, corruption) surfaces
    /// immediately and honestly.
    ///
    /// FOREGROUND-LOCK-1: foreground request paths ([`storage`](Self::storage)) now also retry, but
    /// on the SHORT [`OpenPatience::Foreground`] budget (450ms) — responsiveness preserved — and the
    /// dispatch handlers re-code an exhausted foreground lock as `Busy` (see
    /// [`crate::foreground_open`]); background passes keep this longer budget since a detached pass
    /// may wait without harming a client.
    pub fn storage_with_busy_retry(&self) -> Result<StorageConnection, String> {
        open_existing_with_busy_retry(&self.key.db_path, OpenPatience::Background)
            .map_err(|e| e.to_string())
    }
}

/// FOREGROUND-LOCK-1: the open-patience budget for [`open_existing_with_busy_retry`]. Two ratified
/// budgets, chosen by caller class — foreground dispatch opens stay responsive; detached background
/// passes may wait longer without harming a client.
///
/// Abstraction ledger:
/// - **what:** a two-variant sum naming the ratified open-patience budgets.
/// - **concrete current users:** [`RepoState::storage`] (Foreground) +
///   [`RepoState::storage_with_busy_retry`] and `retention_pass` (Background).
/// - **named axis:** open patience by caller class — variants FIXED, ONE shared retry body
///   dispatches on the variant (operations-fixed / variants-fixed → sum type + exhaustive match).
/// - **rejected simpler:** bare `(attempts, delay)` args — rejected because it scatters the two
///   FROZEN budgets (§3) as magic numbers across call sites, where a drifted number is invisible.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OpenPatience {
    /// Foreground request open: 3 retries × 150ms = 450ms max (sub-half-second; responsiveness).
    Foreground,
    /// Background pass open (seed / retention): 4 retries × 250ms = 1s (the pre-existing budget,
    /// FROZEN by §3).
    Background,
}

impl OpenPatience {
    /// Total open attempts (initial attempt + retries).
    fn attempts(self) -> u32 {
        match self {
            OpenPatience::Foreground => 4,
            OpenPatience::Background => 5,
        }
    }

    /// Delay slept between attempts (never before the first).
    fn delay(self) -> std::time::Duration {
        match self {
            OpenPatience::Foreground => std::time::Duration::from_millis(150),
            OpenPatience::Background => std::time::Duration::from_millis(250),
        }
    }
}

/// FOREGROUND-LOCK-1: a bounded-busy-retry open failure, TYPED so the foreground dispatch path can
/// re-code a transient lock honestly (`Busy` + holder naming, §2.2) instead of the flat
/// `InternalError` a `String` forced. Variants are FIXED and matched exhaustively in
/// [`crate::foreground_open`].
///
/// - `LockedAfterRetries`: the retry budget expired with the DB busy/locked the whole time — a
///   transient the reader can retry.
/// - `Other`: any non-lock fault (missing DB, corruption, I/O), surfaced immediately and unchanged.
///
/// Both variants carry the RAW storage error text (no prefix). The shared render paths — [`Display`]
/// (the `String` callers via `.to_string()`) and the foreground `InternalError` render in
/// [`crate::foreground_open::open_repo_storage_for_request`] — re-apply the historical
/// `"failed to open storage connection: "` prefix, so their text is byte-identical to pre-slice. A
/// secondary open with a DISTINCT pre-existing non-lock message (assess/coverage/enrich/docs-extract:
/// "storage open failed: …" / "failed to open storage for enrichment: …") reads the raw text out of
/// `Other` and renders it under its OWN prefix, so §2.3 keeps those messages unchanged too. Carrying
/// the prefix in `Other` instead would double-prefix those callers.
///
/// [`Display`]: std::fmt::Display
#[derive(Debug)]
pub(crate) enum OpenError {
    LockedAfterRetries { attempts: u32, last: String },
    Other(String),
}

impl std::fmt::Display for OpenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            // Preserves the pre-existing exhausted-retry text verbatim for the `String` callers
            // (Background attempts == 5 → "failed to open storage connection: … (after 5 bounded
            // busy-retry attempts)", unchanged — the prefix now lives here, not in the raw `last`).
            OpenError::LockedAfterRetries { attempts, last } => {
                write!(
                    f,
                    "failed to open storage connection: {last} (after {attempts} bounded busy-retry attempts)"
                )
            }
            OpenError::Other(msg) => write!(f, "failed to open storage connection: {msg}"),
        }
    }
}

/// The bounded-busy-retry open shared by every patience class (foreground dispatch via
/// [`RepoState::storage`], background seed via [`RepoState::storage_with_busy_retry`], retention
/// directly by `db_path`). SQLite returns `SQLITE_BUSY` IMMEDIATELY — bypassing its 5s busy handler
/// — on a transaction lock-upgrade conflict (the migration check's write colliding with a
/// concurrent read transaction), so a single-shot open SKIPS on a transient lock. Retries ONLY
/// lock/busy errors up to the [`OpenPatience`] budget; everything else (missing DB, corruption) is
/// returned immediately as [`OpenError::Other`].
pub(crate) fn open_existing_with_busy_retry(
    db_path: &Path,
    patience: OpenPatience,
) -> Result<StorageConnection, OpenError> {
    let mut last_err = String::new();
    for attempt in 0..patience.attempts() {
        if attempt > 0 {
            std::thread::sleep(patience.delay());
        }
        // The RAW storage error (no prefix) — carried in `OpenError` so a caller with a distinct
        // pre-existing non-lock message renders it under its own prefix (§2.3). The shared paths
        // (Display / foreground `InternalError`) re-apply the historical prefix.
        match StorageConnection::open_existing(db_path).map_err(|e| e.to_string()) {
            Ok(s) => return Ok(s),
            // SQLite's OWN busy/locked signal (the mechanism's error text, not a semantic guess
            // from a path/module name) — the only class we wait on. The prefix we no longer add
            // never contained "locked"/"busy", so detecting on the raw text is equivalent.
            Err(e) if e.contains("locked") || e.contains("busy") => last_err = e,
            Err(e) => return Err(OpenError::Other(e)),
        }
    }
    Err(OpenError::LockedAfterRetries {
        attempts: patience.attempts(),
        last: last_err,
    })
}

/// Daemon state holding all loaded repos, database runtimes, and the registry.
///
/// # Coordination Hierarchy
///
/// 1. Database-level: `db_runtimes` provides write coordination per DB file
/// 2. Repo-level: `repos` provides reader/writer coordination per loaded repo
///
/// Write operations must acquire DB write lock first, then repo lock if applicable.
///
/// # Registry
///
/// The registry is the authoritative list of known repos. It maps canonical paths
/// to repo metadata (db_path, repo_uid, alias). The registry is persisted to disk
/// and loaded on daemon startup.
pub struct DaemonState {
    /// Repos indexed by RepoKey (db_path + repo_uid).
    repos: RwLock<HashMap<RepoKey, Arc<RepoState>>>,

    /// Database runtimes indexed by canonical db_path.
    ///
    /// Provides write coordination for database-level operations (index, refresh, enrich).
    /// Created lazily on first access to a database path.
    db_runtimes: RwLock<HashMap<PathBuf, Arc<DatabaseState>>>,

    /// Repo registry for path-based resolution.
    ///
    /// The registry is daemon-owned and persisted to `registry.json`.
    ///
    /// DAEMON-CONCURRENCY-IMPL-1 (D-S = S-A): `parking_lot::Mutex` (was `RefCell`,
    /// which is `!Sync`) so `DaemonState` is `Send + Sync` and shareable across the
    /// concurrent connection-handler threads. Access is brief (resolve/list/save) and
    /// never nested on one thread (audited), so a `Mutex` — which deadlocks on a
    /// re-entrant lock, unlike `RefCell` which would panic — is safe. `RwLock` is not
    /// needed: registry critical sections are short and contention is negligible vs
    /// the per-request query work.
    registry: Mutex<RepoRegistry>,

    /// DAEMON-VISIBILITY-1 (contract D): in-flight write operations (index/refresh/enrich),
    /// stamped by their handlers and read by the visibility surfaces (`daemon_info`,
    /// `storage_health`, `repo_info`). Interior-mutable (own `Mutex`), so this field does not
    /// affect `DaemonState: Send + Sync`. See `crate::activity`.
    activity: crate::activity::ActivityRegistry,

    /// SNAPSHOT-RETENTION-1: the most-recent completed background retention pass, for the `rmap
    /// doctor` "cleanup: pruned N, reclaimed X" honesty line. The pass is async (spawned after the
    /// index response is sent), so the synchronous index reply cannot carry its result — doctor reads
    /// this instead. Most-recent-wins across repos, mirroring the daemon_info `last_snapshot` single
    /// global line. Interior-mutable (own `Mutex`), so it does not affect `DaemonState: Send + Sync`.
    last_retention: Mutex<Option<crate::retention_pass::RetentionReport>>,

    /// ENRICH-LIFECYCLE-1: daemon-global enrichment-lifecycle coordination — the per-repo trigger
    /// generation (supersede rule), the "one background enrichment at a time per daemon" run slot,
    /// and the most-recent completed pass for the `rmap doctor` lifecycle line. Own interior
    /// mutability (see `EnrichCoordinator`), so it does not affect `DaemonState: Send + Sync`.
    enrich: crate::enrich_pass::EnrichCoordinator,

    /// EMBED-SEED-IMPL-1: daemon-global seed-lifecycle coordination — the per-repo
    /// trigger generation (supersede), the "one background embed pass at a time"
    /// run slot, and the running pass's cancel flag. Own interior mutability, so it
    /// does not affect `DaemonState: Send + Sync`.
    seed: crate::seed_pass::SeedCoordinator,
}

/// FORGET-REPO-1 (review-8 slot-lifecycle fix): the outcome of evicting a repo's IN-MEMORY state,
/// carrying the `db_runtimes` keys whose coordination slot must be dropped AFTER on-disk deletion.
///
/// Split out of the former one-shot `evict_repo_and_runtime` because the slot must stay DISCOVERABLE
/// to a late (re-)index for the WHOLE of forget's registry + file deletion window. If forget dropped
/// the slot first — while still holding its write guard — a concurrent
/// `get_or_create_db_runtime_for_new_db` for the same path would no longer find it, MINT A FRESH slot
/// with a FRESH lock, and write past forget's held guard (review-8). So forget now:
/// (1) [`DaemonState::evict_repo_memory`] — drop the in-memory `RepoState`, KEEP the slot;
/// (2) delete registry + `.db`/`-wal`/`-shm` + `.rgr/`;
/// (3) [`DaemonState::drop_db_runtime_slots`] — drop the slot LAST, still under the held guard.
///
/// `runtime_keys` are captured in step (1) (while the loaded `RepoState` keys are still visible) and
/// consumed in step (3). Concrete current user: `reclaim::forget_repo` + same-module tests.
/// `pub(crate)`, not `pub` — not a ratified external API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MemoryEviction {
    /// An in-memory `RepoState` was present and removed.
    pub(crate) memory_evicted: bool,
    /// The `db_runtimes` keys to drop once on-disk deletion is complete (the evicted `RepoState`
    /// keys, plus `canonicalize(db_path)` when the file still exists, plus the raw path).
    pub(crate) runtime_keys: Vec<PathBuf>,
}

impl DaemonState {
    /// Create daemon state with registry loaded from disk.
    ///
    /// Registry resolution uses `RMAP_STATE_ROOT` if set, otherwise platform data dir.
    /// If initialization fails, logs a warning and uses a non-persistent empty registry.
    pub fn new() -> Self {
        let registry = match RepoRegistry::new() {
            Ok(r) => r,
            Err(e) => {
                eprintln!(
                    "warning: failed to load registry: {} (starting with empty non-persistent registry)",
                    e
                );
                // Use explicit non-persistent fallback instead of retrying the same constructor
                RepoRegistry::empty_non_persistent()
            }
        };

        Self {
            repos: RwLock::new(HashMap::new()),
            db_runtimes: RwLock::new(HashMap::new()),
            registry: Mutex::new(registry),
            activity: crate::activity::ActivityRegistry::new(),
            last_retention: Mutex::new(None),
            enrich: crate::enrich_pass::EnrichCoordinator::new(),
            seed: crate::seed_pass::SeedCoordinator::new(),
        }
    }

    /// Create daemon state with a specific registry.
    ///
    /// Use this for:
    /// - Isolated test environments
    /// - Custom state root configurations
    pub fn with_registry(registry: RepoRegistry) -> Self {
        Self {
            repos: RwLock::new(HashMap::new()),
            db_runtimes: RwLock::new(HashMap::new()),
            registry: Mutex::new(registry),
            activity: crate::activity::ActivityRegistry::new(),
            last_retention: Mutex::new(None),
            enrich: crate::enrich_pass::EnrichCoordinator::new(),
            seed: crate::seed_pass::SeedCoordinator::new(),
        }
    }

    /// Access the in-flight-operation registry (DAEMON-VISIBILITY-1 contract D).
    ///
    /// Write handlers call `activity().begin(..)` on entry (the returned guard deregisters on
    /// drop); the visibility surfaces call `activity().snapshot()` / `active_for_db(..)`.
    pub fn activity(&self) -> &crate::activity::ActivityRegistry {
        &self.activity
    }

    /// FOREGROUND-LOCK-1 (§2.2): the ONE foreground-open choke for the EXTRACTED request handlers
    /// (`handlers/*`), the peer of `ServiceDispatcher::open_storage` for the handlers still inline
    /// in `dispatch.rs`. Opens `repo_state`'s storage with the SHORT foreground patience budget and
    /// re-codes an exhausted transient lock as `Busy` + holder naming — never the flat
    /// `InternalError` the bare `repo_state.storage()` + call-site wrap produced.
    ///
    /// Abstraction ledger:
    /// - **what:** a thin `DaemonState` method binding `self.activity()` to the
    ///   [`crate::foreground_open::open_repo_storage_for_request`] classification seam.
    /// - **concrete current users:** the extracted foreground handlers (assess, map, reliability,
    ///   violations, coverage, churn, risk, hotspots, dead_causes, policy, classify_retention,
    ///   mark/unmark_baseline, perf) + `ServiceDispatcher::open_storage` (delegates here).
    /// - **named axis:** none new — same FIXED two-way classification as the free fn; the method
    ///   only removes the `state.activity()` plumbing repeated at 15 call sites.
    /// - **rejected simpler:** call the free fn directly at each site with `state.activity()` —
    ///   rejected: repeats the activity-registry coupling across 15 handlers and diverges from the
    ///   dispatcher seam, so a future budget/holder change would have to be found at 16 places.
    pub(crate) fn open_repo_storage_for_request(
        &self,
        repo_state: &RepoState,
    ) -> Result<StorageConnection, repo_graph_daemon_transport::ErrorDetail> {
        crate::foreground_open::open_repo_storage_for_request(repo_state, self.activity())
    }

    /// FOREGROUND-LOCK-1 (§2.2/§2.3): the SPLIT foreground-open choke for the EXTRACTED handlers
    /// whose SECONDARY open has a DISTINCT pre-existing non-lock error message (assess/coverage:
    /// "storage open failed: …"). Same bounded patience + honest `Busy` re-code as
    /// [`Self::open_repo_storage_for_request`], but on a genuine NON-lock fault it hands the caller
    /// the RAW error via [`crate::foreground_open::ForegroundOpenFault::Other`] so the caller can
    /// preserve its own §2.3 message verbatim — never the shared "failed to open storage
    /// connection: …" the flat choke renders. Peer of `ServiceDispatcher::open_storage_split`.
    pub(crate) fn open_repo_storage_for_request_split(
        &self,
        repo_state: &RepoState,
    ) -> Result<StorageConnection, crate::foreground_open::ForegroundOpenFault> {
        crate::foreground_open::open_repo_storage_for_request_split(repo_state, self.activity())
    }

    /// SNAPSHOT-RETENTION-1: record the outcome of a completed background retention pass. Most-recent
    /// wins; `rmap doctor` reads it via [`Self::last_retention_json`]. Called by the detached pass, so
    /// it never blocks a request path.
    pub fn record_retention_report(&self, report: crate::retention_pass::RetentionReport) {
        *self.last_retention.lock() = Some(report);
    }

    /// The most-recent background retention pass outcome as `daemon_info.last_retention` JSON (`None`
    /// if no pass has completed since the daemon started).
    pub fn last_retention_json(&self) -> Option<serde_json::Value> {
        self.last_retention.lock().as_ref().map(|r| r.to_json())
    }

    /// ENRICH-LIFECYCLE-1: the daemon-global enrichment coordinator (trigger generations, the
    /// one-at-a-time run slot, and the last-completed pass). The auto-enrich pass drives it;
    /// `rmap doctor` reads the last pass via [`Self::last_enrichment_json`].
    /// EMBED-SEED-IMPL-1: access the seed-lifecycle coordinator (generation +
    /// run slot + cancel flag).
    pub fn seed_coord(&self) -> &crate::seed_pass::SeedCoordinator {
        &self.seed
    }

    pub fn enrich_coord(&self) -> &crate::enrich_pass::EnrichCoordinator {
        &self.enrich
    }

    /// ORIENT-FACT-COHERENCE-1 (review-1 F1): is ANY enrichment pass — the AUTO background pass OR an
    /// explicit `rmap enrich` — queued or running for this db right now? The two kinds live in two
    /// different subsystems, and each knows only its own half:
    /// - the [`EnrichCoordinator`](crate::enrich_pass::EnrichCoordinator) tracks the AUTO pass (queued
    ///   via its `in_flight` counter, running via its `running.flags` cancel registry);
    /// - the [`ActivityRegistry`](crate::activity) tracks an EXPLICIT enrich, which stamps
    ///   `OpKind::Enrich` for its whole duration (`handle_enrich` stamps it right after taking the repo
    ///   coordinator's refresh permit, and it stays stamped until the handler returns).
    ///
    /// orient/check/reliability consult THIS composed fact — not the coordinator alone — so the stale
    /// "run `rmap enrich`" CTA / "did not run" line is suppressed for EITHER kind of pass. Under the W-B
    /// epoch a refresh (auto or explicit enrich) ADMITS concurrent readers (`Refreshing` →
    /// `RefreshingWithReaders`), so a reader really can run WHILE an enrich holds the refresh permit and
    /// read the pre-enrichment epoch — exactly the window that would otherwise hand the reader a stale
    /// CTA. The coordinator-only signal missed the explicit-enrich case (review-1 F1); this union closes
    /// it. Repo-scoped by db_path (never a second repo). `false` when no pass of either kind is in flight.
    ///
    /// (Abstraction ledger — **What:** a composition-point predicate on `DaemonState`, the one place that
    /// holds BOTH the coordinator and the activity registry, unioning their two halves of the in-flight
    /// fact. **Concrete current users:** `handle_orient`, `handle_check`, `handle_reliability`. **Axis of
    /// variation:** none — a cohesion point, not a plugin seam. **Rejected simpler alternative:** inline
    /// the `auto || activity-Enrich` union at each of the three call sites — rejected: it duplicates a
    /// correctness-sensitive predicate three times, and a reader that gets only half the union renders the
    /// exact stale CTA this slice removes.)
    pub(crate) fn enrichment_in_flight_for_db(&self, db_path: &Path) -> bool {
        if self.enrich.auto_enrichment_in_flight_for_db(db_path) {
            return true;
        }
        self.activity
            .active_for_db(db_path)
            .is_some_and(|op| op.kind == crate::activity::OpKind::Enrich)
    }

    /// Record the outcome of a completed background enrichment pass. Most-recent wins; `rmap doctor`
    /// reads it via [`Self::last_enrichment_json`]. Called by the detached pass, so it never blocks a
    /// request path.
    pub fn record_enrichment_report(&self, report: crate::enrich_pass::EnrichmentReport) {
        self.enrich.record(report);
    }

    /// The most-recent background enrichment pass outcome as `daemon_info.last_enrichment` JSON
    /// (`None` if no pass has completed since the daemon started).
    pub fn last_enrichment_json(&self) -> Option<serde_json::Value> {
        self.enrich.last_json()
    }

    // ── State Root Mode (STATE-ROOT-SEPARATION-1) ───────────────────

    /// Returns the current state root mode.
    ///
    /// Determines which classes of writes are permitted:
    /// - Global: all writes allowed
    /// - SandboxLocal: A1 (user authority) writes blocked
    ///
    /// Detection: sandbox mode if state root is under `/private/tmp/`.
    pub fn state_root_mode(&self) -> StateRootMode {
        let state_root = self.registry.lock().state_root().to_path_buf();

        // macOS sandbox environments use /private/tmp/ as writable root
        // This is where STDIO-STATE-ROOT-1 places sandbox state
        if state_root.starts_with("/private/tmp/") {
            StateRootMode::SandboxLocal
        } else {
            StateRootMode::Global
        }
    }

    /// Convenience: returns true if in sandbox-local mode.
    ///
    /// Equivalent to `self.state_root_mode() == StateRootMode::SandboxLocal`.
    pub fn is_sandbox_mode(&self) -> bool {
        self.state_root_mode() == StateRootMode::SandboxLocal
    }

    /// Returns true if A1 (user authority) writes are allowed.
    ///
    /// A1 writes include: aliases, explicit baselines, declarations, waivers.
    /// Blocked in sandbox-local mode to prevent silent authority loss.
    pub fn allows_authority_writes(&self) -> bool {
        self.state_root_mode().allows_authority_writes()
    }

    // ── Database-level coordination ─────────────────────────────────

    /// Get or create a database runtime for the given path.
    ///
    /// The path is canonicalized to ensure consistent identity.
    /// Returns the database state for write coordination.
    pub fn get_or_create_db_runtime(&self, db_path: &Path) -> Result<Arc<DatabaseState>, String> {
        let canonical = db_path
            .canonicalize()
            .map_err(|e| format!("cannot canonicalize db path '{}': {}", db_path.display(), e))?;

        // Check if already exists
        {
            let runtimes = self.db_runtimes.read().unwrap();
            if let Some(state) = runtimes.get(&canonical) {
                return Ok(Arc::clone(state));
            }
        }

        // Create and insert
        let state = Arc::new(DatabaseState::new(canonical.clone()));
        {
            let mut runtimes = self.db_runtimes.write().unwrap();
            // Double-check after acquiring write lock
            if let Some(existing) = runtimes.get(&canonical) {
                return Ok(Arc::clone(existing));
            }
            runtimes.insert(canonical, Arc::clone(&state));
        }

        Ok(state)
    }

    /// Get or create a database runtime for an uncanonicalized path.
    ///
    /// Used for index operations where the DB file may not exist yet.
    /// Creates the runtime keyed by the parent directory + filename.
    ///
    /// Handles relative paths like "repo.db" where parent() returns an empty path
    /// by using the current working directory.
    pub fn get_or_create_db_runtime_for_new_db(
        &self,
        db_path: &Path,
    ) -> Result<Arc<DatabaseState>, String> {
        // For new DBs, canonicalize the parent and append the filename
        let filename = db_path
            .file_name()
            .ok_or_else(|| format!("invalid db path (no filename): {}", db_path.display()))?;

        // Handle relative paths like "repo.db" where parent() is empty
        let parent = db_path.parent();
        let canonical_parent = match parent {
            Some(p) if !p.as_os_str().is_empty() => {
                // Normal case: parent directory exists
                p.canonicalize()
                    .map_err(|e| format!("cannot canonicalize parent '{}': {}", p.display(), e))?
            }
            _ => {
                // Empty or no parent (e.g., "repo.db") — use current working directory
                std::env::current_dir()
                    .map_err(|e| format!("cannot get current directory: {}", e))?
            }
        };
        let canonical = canonical_parent.join(filename);

        // Check if already exists
        {
            let runtimes = self.db_runtimes.read().unwrap();
            if let Some(state) = runtimes.get(&canonical) {
                return Ok(Arc::clone(state));
            }
        }

        // Create and insert
        let state = Arc::new(DatabaseState::new(canonical.clone()));
        {
            let mut runtimes = self.db_runtimes.write().unwrap();
            if let Some(existing) = runtimes.get(&canonical) {
                return Ok(Arc::clone(existing));
            }
            runtimes.insert(canonical, Arc::clone(&state));
        }

        Ok(state)
    }

    // ── Repo-level operations ───────────────────────────────────────

    /// Load a repo into the daemon.
    ///
    /// If the repo is already loaded (same db_path + repo_uid), returns
    /// the existing state. Different databases with the same repo_uid
    /// are tracked separately.
    pub fn load_repo(&self, db_path: &Path, repo_uid: &str) -> Result<Arc<RepoState>, String> {
        let key = RepoKey::new(db_path, repo_uid)?;

        // Check if already loaded
        {
            let repos = self.repos.read().unwrap();
            if let Some(state) = repos.get(&key) {
                return Ok(Arc::clone(state));
            }
        }

        // Open and insert. DAEMON-CONCURRENCY-IMPL-1: `RepoState` is now `Send + Sync`
        // (registry behind a `Mutex`; reads open their own connection per operation —
        // no shared `!Sync` connection), so it is shared across threads as a normal
        // `Arc`. The previous `arc_with_non_send_sync` allow is GONE — its removal
        // compiling is the proof the state became `Send + Sync`.
        let state = Arc::new(RepoState::open(db_path, repo_uid)?);
        {
            let mut repos = self.repos.write().unwrap();
            repos.insert(key, Arc::clone(&state));
        }

        // DAEMON-CRASH-RECOVERY-1 (F7/F11): the first time this daemon loads a repo, reconcile any
        // crash-orphaned `building` snapshots left by a previous (dead) daemon — flip each to the
        // terminal `failed` state, classify it `prunable`, and log it (retention stats then count it;
        // the non-READY prune + VACUUM reclaims it).
        // Runs ONCE per repo per daemon lifetime (cache-miss branch), is
        // two-gate guarded (a live op or a contended lock → skips, never blocks/deadlocks — the
        // `try_acquire_write` is non-blocking), and is a no-op query in the common no-orphan case.
        crate::reconcile::reconcile_repo(self, db_path, repo_uid, repo_uid);

        Ok(state)
    }

    /// Get a loaded repo's state by composite key.
    ///
    /// This is the only correct way to look up a repo in multi-database mode.
    pub fn get_repo_by_key(&self, key: &RepoKey) -> Option<Arc<RepoState>> {
        let repos = self.repos.read().unwrap();
        repos.get(key).cloned()
    }

    /// List all loaded repos as display strings.
    ///
    /// Each entry is formatted as "db_path:repo_uid".
    /// Results are sorted for deterministic output.
    pub fn list_repos(&self) -> Vec<String> {
        let repos = self.repos.read().unwrap();
        let mut list: Vec<String> = repos.keys().map(|k| k.display()).collect();
        list.sort();
        list
    }

    /// Unload a repo from the daemon by composite key.
    ///
    /// This is the only correct way to unload a repo in multi-database mode.
    pub fn unload_repo_by_key(&self, key: &RepoKey) -> bool {
        let mut repos = self.repos.write().unwrap();
        repos.remove(key).is_some()
    }

    /// FORGET-REPO-1: evict a repo's in-memory state, keyed on the registry `repo_uid` — NOT on
    /// [`RepoKey::new`], which canonicalizes `db_path` and FAILS once the DB file is unlinked. Forget
    /// must evict regardless of whether the file still exists (the field bug: eviction was gated on the
    /// DB file canonicalizing, so a forget-after-delete left the in-memory state stuck). The
    /// `db_runtimes` slot is NOT dropped here — its keys are returned for a deferred drop (review-8, see
    /// [`MemoryEviction`]).
    ///
    /// Disambiguation: `repo_uid` is a globally-unique ULID minted once per registry entry, so in
    /// production it uniquely identifies the loaded state. The identity model nonetheless permits two
    /// DIFFERENT db files to carry the same `repo_uid` (see `different_dbs_same_repo_uid_are_separate`),
    /// so when `db_path` still canonicalizes we require an exact db-path match to evict only the target
    /// entry; only when the file is already gone do we fall back to a `repo_uid`-only match (the file is
    /// unlinked, and in production the uid is unique — the shared-uid + deleted-file combination is not
    /// a reachable production state).
    ///
    /// Returns a [`MemoryEviction`] carrying `memory_evicted` (the SEPARATE forget artifact, review-1
    /// #3) and the `runtime_keys` the caller drops LAST via [`Self::drop_db_runtime_slots`] — AFTER
    /// on-disk deletion, so the slot stays discoverable to a late (re-)index throughout forget's
    /// deletion window (review-8). This method does NOT touch `db_runtimes`.
    ///
    /// `pub(crate)` — only `reclaim::forget_repo` and same-crate tests call it.
    pub(crate) fn evict_repo_memory(&self, repo_uid: &str, db_path: &Path) -> MemoryEviction {
        let target_canonical_db = db_path.canonicalize().ok();

        // Candidate canonical db paths whose runtime slot to drop: the loaded RepoState's key
        // (canonicalized at load), plus canonicalize(db_path) while the file may still exist, plus the
        // raw path — covering both a loaded and an indexed-but-never-loaded repo.
        let mut runtime_keys: Vec<PathBuf> = Vec::new();
        let evicted = {
            let mut repos = self.repos.write().unwrap();
            let before = repos.len();
            repos.retain(|k, _| {
                if k.repo_uid != repo_uid {
                    return true;
                }
                match &target_canonical_db {
                    // File exists → evict only the entry whose canonical db_path matches.
                    Some(c) if k.db_path != *c => true,
                    _ => {
                        runtime_keys.push(k.db_path.clone());
                        false
                    }
                }
            });
            repos.len() != before
        };
        if let Some(c) = target_canonical_db {
            runtime_keys.push(c);
        }
        runtime_keys.push(db_path.to_path_buf());

        MemoryEviction {
            memory_evicted: evicted,
            runtime_keys,
        }
    }

    /// FORGET-REPO-1 (review-8): drop the `db_runtimes` coordination slot(s) named by `keys`, returning
    /// whether any slot was present and dropped (the SEPARATE `runtime-slot` forget artifact). Called
    /// LAST by `reclaim::forget_repo`, under the still-held DB write guard, AFTER every registry + file
    /// artifact is processed — so a late (re-)index that fetched the same slot mid-deletion blocked on
    /// forget's guard rather than minting a fresh slot. `pub(crate)`; sole caller `reclaim::forget_repo`
    /// (+ same-crate tests).
    pub(crate) fn drop_db_runtime_slots(&self, keys: &[PathBuf]) -> bool {
        let mut runtimes = self.db_runtimes.write().unwrap();
        let before = runtimes.len();
        runtimes.retain(|k, _| !keys.iter().any(|c| c == k));
        runtimes.len() != before
    }

    /// FORGET-REPO-1 (review-2 atomicity): the loaded [`RepoState`] (matched by `repo_uid`, preferring
    /// an exact db-path match) whose `RepoCoordinator` write lock `forget_repo` TRY-acquires to
    /// serialize against active READERS (which take `coordinator.acquire_read`, NOT the DB write lock).
    /// `None` when the repo is not loaded — an unloaded repo has no coordinator and no reader can be
    /// mid-flight on it (reads load the repo first).
    ///
    /// `pub(crate)`: sole caller is `reclaim::forget_repo`.
    pub(crate) fn loaded_repo_by_uid(
        &self,
        repo_uid: &str,
        db_path: &Path,
    ) -> Option<Arc<RepoState>> {
        let canonical = db_path.canonicalize().ok();
        let repos = self.repos.read().unwrap();
        // Prefer the entry whose canonical db_path matches (disambiguates a shared uid across db
        // files); fall back to a uid-only match when the file is already gone.
        if let Some(c) = &canonical {
            if let Some(st) = repos
                .iter()
                .find(|(k, _)| k.repo_uid == repo_uid && &k.db_path == c)
                .map(|(_, st)| Arc::clone(st))
            {
                return Some(st);
            }
        }
        repos
            .iter()
            .find(|(k, _)| k.repo_uid == repo_uid)
            .map(|(_, st)| Arc::clone(st))
    }

    // ── Registry operations ─────────────────────────────────────────────

    /// Access the registry.
    ///
    /// Returns a `parking_lot::MutexGuard`. The guard gives shared, read-style
    /// access (callers use it read-only); it must be dropped before any other
    /// registry access on the SAME thread — the `Mutex` is non-reentrant, so a
    /// nested `registry()`/`registry_mut()`/`save_registry()` while holding it
    /// deadlocks (the call sites were audited for this — all are sequential,
    /// none nested). Held briefly; never across a blocking call.
    pub fn registry(&self) -> MutexGuard<'_, RepoRegistry> {
        self.registry.lock()
    }

    /// Access the registry for mutation.
    ///
    /// Same `MutexGuard` as [`registry`](Self::registry) (one lock; `MutexGuard`
    /// derefs to `&mut`). Kept as a distinct method so write call sites read
    /// intentionally. Same non-reentrancy caveat: do not nest registry access
    /// while holding the returned guard.
    pub fn registry_mut(&self) -> MutexGuard<'_, RepoRegistry> {
        self.registry.lock()
    }

    /// Resolve a path to a registered repo.
    ///
    /// Uses registry resolution: exact match or longest ancestor prefix.
    pub fn resolve_repo_path(&self, path: &Path) -> Option<crate::registry::RegistryEntry> {
        self.registry.lock().resolve(path).cloned()
    }

    /// Resolve by alias or path.
    pub fn resolve_alias_or_path(
        &self,
        alias_or_path: &str,
    ) -> Option<crate::registry::RegistryEntry> {
        self.registry
            .lock()
            .resolve_alias_or_path(alias_or_path)
            .cloned()
    }

    /// Save the registry to disk.
    ///
    /// Should be called after mutations to persist changes.
    pub fn save_registry(&self) -> Result<(), RegistryError> {
        self.registry.lock().save()
    }
}

impl Default for DaemonState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use repo_graph_storage::types::Repo;
    use tempfile::tempdir;

    fn create_test_db(dir: &Path, repo_uid: &str) -> PathBuf {
        let db_path = dir.join(format!("{}.db", repo_uid));
        // Create a minimal valid database
        let storage = StorageConnection::open(&db_path).unwrap();
        // Add a repo
        storage
            .add_repo(&Repo {
                repo_uid: repo_uid.to_string(),
                name: format!("Test Repo {}", repo_uid),
                root_path: ".".to_string(),
                default_branch: Some("main".to_string()),
                created_at: "2024-01-01T00:00:00Z".to_string(),
                metadata_json: None,
            })
            .unwrap();
        db_path
    }

    #[test]
    fn load_repo_creates_state() {
        let dir = tempdir().unwrap();
        let db_path = create_test_db(dir.path(), "test-repo");

        let daemon = DaemonState::new();
        let state = daemon.load_repo(&db_path, "test-repo").unwrap();

        assert_eq!(state.repo_uid(), "test-repo");
    }

    #[test]
    fn load_repo_validates_repo_exists() {
        let dir = tempdir().unwrap();
        let db_path = create_test_db(dir.path(), "real-repo");

        let daemon = DaemonState::new();
        // Try to load a repo that doesn't exist in the DB
        let result = daemon.load_repo(&db_path, "nonexistent-repo");

        match result {
            Err(msg) => assert!(
                msg.contains("not found in database"),
                "unexpected error: {}",
                msg
            ),
            Ok(_) => panic!("expected error for nonexistent repo"),
        }
    }

    #[test]
    fn load_repo_twice_returns_same_state() {
        let dir = tempdir().unwrap();
        let db_path = create_test_db(dir.path(), "test-repo");

        let daemon = DaemonState::new();
        let state1 = daemon.load_repo(&db_path, "test-repo").unwrap();
        let state2 = daemon.load_repo(&db_path, "test-repo").unwrap();

        assert!(Arc::ptr_eq(&state1, &state2));
    }

    #[test]
    fn different_dbs_same_repo_uid_are_separate() {
        let dir = tempdir().unwrap();

        // Create two different databases with the same repo_uid
        let db_path_a = dir.path().join("db_a.db");
        let db_path_b = dir.path().join("db_b.db");

        let storage_a = StorageConnection::open(&db_path_a).unwrap();
        storage_a
            .add_repo(&Repo {
                repo_uid: "shared-uid".to_string(),
                name: "Repo A".to_string(),
                root_path: ".".to_string(),
                default_branch: None,
                created_at: "2024-01-01T00:00:00Z".to_string(),
                metadata_json: None,
            })
            .unwrap();

        let storage_b = StorageConnection::open(&db_path_b).unwrap();
        storage_b
            .add_repo(&Repo {
                repo_uid: "shared-uid".to_string(),
                name: "Repo B".to_string(),
                root_path: ".".to_string(),
                default_branch: None,
                created_at: "2024-01-01T00:00:00Z".to_string(),
                metadata_json: None,
            })
            .unwrap();

        let daemon = DaemonState::new();

        // Load both - they should be separate entries
        let state_a = daemon.load_repo(&db_path_a, "shared-uid").unwrap();
        let state_b = daemon.load_repo(&db_path_b, "shared-uid").unwrap();

        // They should NOT be the same Arc (different databases)
        assert!(!Arc::ptr_eq(&state_a, &state_b));

        // They should have different db_paths
        assert_ne!(state_a.db_path(), state_b.db_path());

        // list_repos should show both
        let repos = daemon.list_repos();
        assert_eq!(repos.len(), 2);
    }

    #[test]
    fn get_repo_by_key_returns_loaded_state() {
        let dir = tempdir().unwrap();
        let db_path = create_test_db(dir.path(), "test-repo");

        let daemon = DaemonState::new();
        daemon.load_repo(&db_path, "test-repo").unwrap();

        let key = RepoKey::new(&db_path, "test-repo").unwrap();
        let state = daemon.get_repo_by_key(&key);
        assert!(state.is_some());
        assert_eq!(state.unwrap().repo_uid(), "test-repo");
    }

    #[test]
    fn get_repo_by_key_returns_none_for_unknown() {
        let dir = tempdir().unwrap();
        let db_path = create_test_db(dir.path(), "test-repo");

        let daemon = DaemonState::new();
        let key = RepoKey::new(&db_path, "unknown").unwrap();
        assert!(daemon.get_repo_by_key(&key).is_none());
    }

    #[test]
    fn unload_repo_by_key_removes_state() {
        let dir = tempdir().unwrap();
        let db_path = create_test_db(dir.path(), "test-repo");

        let daemon = DaemonState::new();
        daemon.load_repo(&db_path, "test-repo").unwrap();

        let key = RepoKey::new(&db_path, "test-repo").unwrap();
        assert!(daemon.unload_repo_by_key(&key));
        assert!(daemon.get_repo_by_key(&key).is_none());
    }

    // FORGET-REPO-1: eviction is keyed on repo_uid, NOT on RepoKey::new (which canonicalizes and
    // would fail once the DB file is gone). After deleting the file out-of-band, the memory eviction
    // still removes the loaded state, and the deferred slot drop (review-8) still drops the slot.
    #[test]
    fn evict_repo_memory_then_drop_slot_works_after_db_file_deleted() {
        let dir = tempdir().unwrap();
        let db_path = create_test_db(dir.path(), "forget-repo");

        let daemon = DaemonState::new();
        daemon.load_repo(&db_path, "forget-repo").unwrap();
        // A db_runtime slot exists (canonical key).
        daemon.get_or_create_db_runtime(&db_path).unwrap();
        assert_eq!(daemon.list_repos().len(), 1);
        assert_eq!(daemon.db_runtimes.read().unwrap().len(), 1);

        // Delete the DB file — RepoKey::new would now fail to canonicalize it.
        assert!(RepoKey::new(&db_path, "forget-repo").is_err() || db_path.exists());
        std::fs::remove_file(&db_path).unwrap();
        assert!(RepoKey::new(&db_path, "forget-repo").is_err());

        // Step 1: memory eviction works, keyed on the uid, and does NOT touch the slot (review-8).
        let mem = daemon.evict_repo_memory("forget-repo", &db_path);
        assert!(mem.memory_evicted, "in-memory state reported evicted");
        assert!(daemon.list_repos().is_empty(), "in-memory state evicted");
        assert!(
            !daemon.db_runtimes.read().unwrap().is_empty(),
            "the slot stays discoverable until the deferred drop"
        );

        // Step 2 (deferred): dropping the captured keys removes the slot.
        assert!(
            daemon.drop_db_runtime_slots(&mem.runtime_keys),
            "db_runtimes slot reported dropped"
        );
        assert!(
            daemon.db_runtimes.read().unwrap().is_empty(),
            "db_runtimes slot dropped"
        );
    }

    // FORGET-REPO-1: with the DB file present, eviction disambiguates by db_path — two DIFFERENT DBs
    // sharing a repo_uid evict independently (only the targeted one is removed).
    #[test]
    fn evict_repo_memory_disambiguates_shared_uid_when_file_present() {
        let dir = tempdir().unwrap();
        let db_a = dir.path().join("a.db");
        let db_b = dir.path().join("b.db");
        for db in [&db_a, &db_b] {
            let storage = StorageConnection::open(db).unwrap();
            storage
                .add_repo(&Repo {
                    repo_uid: "shared".to_string(),
                    name: "n".to_string(),
                    root_path: ".".to_string(),
                    default_branch: None,
                    created_at: "2026-01-01T00:00:00Z".to_string(),
                    metadata_json: None,
                })
                .unwrap();
        }
        let daemon = DaemonState::new();
        daemon.load_repo(&db_a, "shared").unwrap();
        daemon.load_repo(&db_b, "shared").unwrap();
        assert_eq!(daemon.list_repos().len(), 2);

        // Evict only db_a's entry (the file exists → exact db-path match).
        assert!(daemon.evict_repo_memory("shared", &db_a).memory_evicted);
        assert_eq!(
            daemon.list_repos().len(),
            1,
            "only the targeted DB was evicted"
        );
    }

    #[test]
    fn repo_key_canonicalizes_path() {
        let dir = tempdir().unwrap();
        let db_path = create_test_db(dir.path(), "test-repo");

        // Create keys with relative and absolute paths
        let relative_key = RepoKey::new(&db_path, "test-repo").unwrap();
        let absolute_key = RepoKey::new(&db_path.canonicalize().unwrap(), "test-repo").unwrap();

        // They should be equal (both canonicalized)
        assert_eq!(relative_key, absolute_key);
    }

    // ── Database coordination tests ─────────────────────────────────

    #[test]
    fn db_runtime_created_on_demand() {
        let dir = tempdir().unwrap();
        let db_path = create_test_db(dir.path(), "test-repo");

        let daemon = DaemonState::new();

        // No DB runtimes initially
        assert!(daemon.db_runtimes.read().unwrap().is_empty());

        // Get or create a runtime
        let runtime = daemon.get_or_create_db_runtime(&db_path).unwrap();

        // Now we have one
        assert_eq!(daemon.db_runtimes.read().unwrap().len(), 1);

        // Getting again returns the same instance
        let runtime2 = daemon.get_or_create_db_runtime(&db_path).unwrap();
        assert!(Arc::ptr_eq(&runtime, &runtime2));
    }

    #[test]
    fn db_runtime_for_new_db_works_before_file_exists() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("new.db");

        // File doesn't exist yet
        assert!(!db_path.exists());

        let daemon = DaemonState::new();

        // Can still create a runtime (parent exists and can be canonicalized)
        let runtime = daemon
            .get_or_create_db_runtime_for_new_db(&db_path)
            .unwrap();
        assert_eq!(runtime.db_path().file_name().unwrap(), "new.db");
    }

    #[test]
    fn db_write_lock_is_exclusive() {
        let dir = tempdir().unwrap();
        let db_path = create_test_db(dir.path(), "test-repo");

        let daemon = DaemonState::new();
        let runtime = daemon.get_or_create_db_runtime(&db_path).unwrap();

        // Acquire write lock
        let _guard = runtime.acquire_write();

        // try_acquire_write should fail while locked
        assert!(runtime.try_acquire_write().is_none());
    }

    #[test]
    fn db_runtime_for_new_db_handles_relative_path() {
        // Test that a bare filename like "repo.db" (no directory component)
        // works correctly by using current working directory
        let daemon = DaemonState::new();

        // "repo.db" has no parent directory component
        let db_path = Path::new("relative-test.db");
        assert!(db_path.parent().is_none_or(|p| p.as_os_str().is_empty()));

        // Should succeed by using current working directory
        let runtime = daemon.get_or_create_db_runtime_for_new_db(db_path).unwrap();

        // The canonical path should be cwd + filename
        let expected_parent = std::env::current_dir().unwrap();
        assert_eq!(runtime.db_path().parent().unwrap(), expected_parent);
        assert_eq!(runtime.db_path().file_name().unwrap(), "relative-test.db");
    }

    #[test]
    fn list_repos_is_sorted() {
        let dir = tempdir().unwrap();

        // Create repos with names that would be out of order in HashMap
        let db_path_z = dir.path().join("z.db");
        let db_path_a = dir.path().join("a.db");
        let db_path_m = dir.path().join("m.db");

        for (path, uid) in [
            (&db_path_z, "z-repo"),
            (&db_path_a, "a-repo"),
            (&db_path_m, "m-repo"),
        ] {
            let storage = StorageConnection::open(path).unwrap();
            storage
                .add_repo(&Repo {
                    repo_uid: uid.to_string(),
                    name: uid.to_string(),
                    root_path: ".".to_string(),
                    default_branch: None,
                    created_at: "2024-01-01T00:00:00Z".to_string(),
                    metadata_json: None,
                })
                .unwrap();
        }

        let daemon = DaemonState::new();
        daemon.load_repo(&db_path_z, "z-repo").unwrap();
        daemon.load_repo(&db_path_a, "a-repo").unwrap();
        daemon.load_repo(&db_path_m, "m-repo").unwrap();

        let repos = daemon.list_repos();
        assert_eq!(repos.len(), 3);

        // Verify sorted order (by full display string)
        for i in 0..repos.len() - 1 {
            assert!(
                repos[i] < repos[i + 1],
                "list_repos not sorted: {:?}",
                repos
            );
        }
    }

    // ── State Root Mode Tests (STATE-ROOT-SEPARATION-1) ─────────────

    #[test]
    fn state_root_mode_global_for_normal_paths() {
        // A temp directory is NOT under /private/tmp/ on macOS
        // (tempfile uses /var/folders/... by default)
        let dir = tempdir().unwrap();
        let registry = RepoRegistry::with_state_root(dir.path()).unwrap();
        let daemon = DaemonState::with_registry(registry);

        assert_eq!(daemon.state_root_mode(), StateRootMode::Global);
        assert!(!daemon.is_sandbox_mode());
        assert!(daemon.allows_authority_writes());
    }

    #[test]
    fn state_root_mode_sandbox_for_private_tmp() {
        // Test sandbox mode detection through the actual implementation path
        let sandbox_path = PathBuf::from("/private/tmp/repo-graph-agent/501");
        let registry = RepoRegistry::with_test_state_root(sandbox_path);
        let daemon = DaemonState::with_registry(registry);

        // Verify full implementation path
        assert_eq!(daemon.state_root_mode(), StateRootMode::SandboxLocal);
        assert!(daemon.is_sandbox_mode());
        assert!(!daemon.allows_authority_writes());

        // Verify state_root() returns the expected path
        assert_eq!(
            daemon.registry().state_root(),
            Path::new("/private/tmp/repo-graph-agent/501")
        );
    }

    #[test]
    fn state_root_mode_enum_as_str() {
        assert_eq!(StateRootMode::Global.as_str(), "global");
        assert_eq!(StateRootMode::SandboxLocal.as_str(), "sandbox-local");
    }

    #[test]
    fn state_root_mode_allows_authority_writes() {
        assert!(StateRootMode::Global.allows_authority_writes());
        assert!(!StateRootMode::SandboxLocal.allows_authority_writes());
    }

    // ── DAEMON-CONCURRENCY-IMPL-1 ───────────────────────────────────

    /// The state must be `Send + Sync` to be shared as `Arc<ServiceDispatcher>` across the concurrent
    /// connection-handler threads. The `arc_with_non_send_sync` allows were removed (registry behind a
    /// `Mutex`, reads connection-per-op); this pins the invariant explicitly so a future `!Sync` field
    /// (e.g. a re-introduced shared connection or a `RefCell`) fails HERE, not as a confusing dispatch
    /// trait-bound error.
    #[test]
    fn daemon_and_repo_state_are_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<DaemonState>();
        assert_send_sync::<RepoState>();
        assert_send_sync::<Arc<DaemonState>>();
        assert_send_sync::<Arc<RepoState>>();
        assert_send_sync::<crate::ServiceDispatcher>();
        assert_send_sync::<Arc<crate::ServiceDispatcher>>();
    }

    /// DAEMON-CONCURRENCY-IMPL-1 behavior 2 (writer serialization): concurrent writers on the SAME DB
    /// serialize on the `DatabaseState` write lock — at most one is ever inside the critical section, so
    /// there is no interleaving / corruption window. The `max == 1` assertion holds regardless of timing
    /// (a correct lock NEVER admits two); `yield_now` only widens the window to catch a broken lock. No
    /// wall-clock correctness dependency. (Readers seeing only the last-good READY snapshot during a
    /// build is the storage layer's invariant, pinned by
    /// `get_latest_snapshot_excludes_building_snapshots` in `storage`.)
    #[test]
    fn concurrent_writers_serialize_on_db_write_lock() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let dir = tempdir().unwrap();
        let db_path = create_test_db(dir.path(), "writer-repo");
        let daemon = DaemonState::new();
        let runtime = daemon.get_or_create_db_runtime(&db_path).unwrap();

        let in_section = Arc::new(AtomicUsize::new(0));
        let max_seen = Arc::new(AtomicUsize::new(0));

        let mut handles = Vec::new();
        for _ in 0..8 {
            let runtime = Arc::clone(&runtime);
            let in_section = Arc::clone(&in_section);
            let max_seen = Arc::clone(&max_seen);
            handles.push(std::thread::spawn(move || {
                for _ in 0..50 {
                    let _guard = runtime.acquire_write();
                    let now = in_section.fetch_add(1, Ordering::SeqCst) + 1;
                    max_seen.fetch_max(now, Ordering::SeqCst);
                    std::thread::yield_now(); // widen the window; does not affect the max==1 invariant
                    in_section.fetch_sub(1, Ordering::SeqCst);
                    // _guard drops here, releasing the DB write lock
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }

        assert_eq!(
            max_seen.load(Ordering::SeqCst),
            1,
            "the DatabaseState write lock must serialize writers (never two concurrently)"
        );
    }
}
