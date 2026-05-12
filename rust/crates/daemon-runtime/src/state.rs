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

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use parking_lot::{Mutex, MutexGuard};
use repo_graph_daemon_policy::RepoCoordinator;
use repo_graph_storage::types::RepoRef;
use repo_graph_storage::StorageConnection;

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

    /// Storage connection (owned by daemon, not opened per-request).
    pub storage: StorageConnection,
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

        let storage = StorageConnection::open(db_path)
            .map_err(|e| format!("failed to open database: {}", e))?;

        // Validate repo exists in the database
        match storage.get_repo(&RepoRef::Uid(repo_uid.to_string())) {
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

        let key = RepoKey::new(db_path, repo_uid)?;

        Ok(Self {
            key,
            coordinator: RepoCoordinator::new(),
            storage,
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
}

/// Daemon state holding all loaded repos and database runtimes.
///
/// # Coordination Hierarchy
///
/// 1. Database-level: `db_runtimes` provides write coordination per DB file
/// 2. Repo-level: `repos` provides reader/writer coordination per loaded repo
///
/// Write operations must acquire DB write lock first, then repo lock if applicable.
pub struct DaemonState {
    /// Repos indexed by RepoKey (db_path + repo_uid).
    repos: RwLock<HashMap<RepoKey, Arc<RepoState>>>,

    /// Database runtimes indexed by canonical db_path.
    ///
    /// Provides write coordination for database-level operations (index, refresh, enrich).
    /// Created lazily on first access to a database path.
    db_runtimes: RwLock<HashMap<PathBuf, Arc<DatabaseState>>>,
}

impl DaemonState {
    /// Create empty daemon state.
    pub fn new() -> Self {
        Self {
            repos: RwLock::new(HashMap::new()),
            db_runtimes: RwLock::new(HashMap::new()),
        }
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

        // Open and insert
        // Note: RepoState is !Sync due to interior RefCell. Arc is used for shared
        // ownership across the RwLock, not for cross-thread access. The daemon is
        // single-threaded, so this is safe.
        #[allow(clippy::arc_with_non_send_sync)]
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
}
