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
}

impl RepoState {
    /// Open a repo's state from the database path.
    ///
    /// Validates that the repo actually exists in the database before
    /// returning success. This prevents silent failures at query time.
    pub fn open(db_path: &Path, repo_uid: &str) -> Result<Self, String> {
        if !db_path.exists() {
            return Err(format!("database not found: {}", db_path.display()));
        }

        // DAEMON-CONCURRENCY-IMPL-1 (D-S = S-A): open a connection ONLY to validate
        // the repo exists at load time; it is dropped at the end of this fn. Reads
        // open their own connection per operation (see `storage()`) — `RepoState`
        // holds NO shared `!Sync` connection, which is what makes it `Send + Sync`.
        let validation_conn = StorageConnection::open(db_path)
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
    /// own connection here, via the NORMAL [`StorageConnection::open`] — which runs
    /// the idempotent migration check (NO fast-open that could serve an unmigrated
    /// schema, per the §14 ratification; that would be a Layer-0 honesty violation).
    /// SQLite WAL gives true concurrent reads across these per-operation connections.
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
        StorageConnection::open(&self.key.db_path)
            .map_err(|e| format!("failed to open storage connection: {}", e))
    }
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
        }
    }

    /// Access the in-flight-operation registry (DAEMON-VISIBILITY-1 contract D).
    ///
    /// Write handlers call `activity().begin(..)` on entry (the returned guard deregisters on
    /// drop); the visibility surfaces call `activity().snapshot()` / `active_for_db(..)`.
    pub fn activity(&self) -> &crate::activity::ActivityRegistry {
        &self.activity
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
    pub fn enrich_coord(&self) -> &crate::enrich_pass::EnrichCoordinator {
        &self.enrich
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
