//! Repo registry for daemon-managed repository tracking.
//!
//! The registry is the daemon's authoritative map of known repositories.
//! It persists to `registry.json` and provides path-based resolution.
//!
//! # Identity Model
//!
//! - `repo_uid`: Stable opaque identifier (`repo_<ulid>`), generated once per registry entry
//! - `snapshot_uid`: Per-index identifier (`<repo_uid>/<timestamp>/<hash>`)
//! - `canonical_path`: Absolute path with symlinks resolved, the registry key
//!
//! # Resolution
//!
//! CLI sends a canonicalized path. Daemon resolves by:
//! 1. Exact match in registry
//! 2. Longest registered ancestor prefix
//! 3. "Not indexed" error
//!
//! The CLI does NOT walk markers. The daemon owns resolution.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};
use ulid::Ulid;

/// A single registered repository in the daemon registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryEntry {
    /// Canonical absolute path to the repository root.
    /// This is the registry key (symlinks resolved, normalized).
    pub canonical_path: PathBuf,

    /// Optional human-friendly alias (unique within registry).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,

    /// Path to the daemon-managed database file.
    pub db_path: PathBuf,

    /// Stable opaque repository identifier (`repo_<ulid>`).
    /// Generated once when repo is first indexed, never changes.
    pub repo_uid: String,

    /// ISO8601 timestamp of last successful index.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_indexed_at: Option<String>,

    /// Snapshot UID from last successful index.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_snapshot_uid: Option<String>,
}

impl RegistryEntry {
    /// Create a new registry entry for a repo path.
    ///
    /// Generates a new `repo_uid` and allocates a database path.
    pub fn new(canonical_path: PathBuf, db_dir: &Path) -> Self {
        let repo_uid = generate_repo_uid();
        let db_path = allocate_db_path(&canonical_path, db_dir);

        Self {
            canonical_path,
            alias: None,
            db_path,
            repo_uid,
            last_indexed_at: None,
            last_snapshot_uid: None,
        }
    }

    /// Create entry with a specific alias.
    pub fn with_alias(mut self, alias: String) -> Self {
        self.alias = Some(alias);
        self
    }

    /// Update the last indexed timestamp and snapshot.
    pub fn record_index(&mut self, timestamp: String, snapshot_uid: String) {
        self.last_indexed_at = Some(timestamp);
        self.last_snapshot_uid = Some(snapshot_uid);
    }
}

/// The persistent registry file format.
#[derive(Debug, Serialize, Deserialize)]
pub struct RegistryFile {
    /// Schema version for forward compatibility.
    pub version: u32,

    /// Registered repositories.
    pub repos: Vec<RegistryEntry>,
}

impl Default for RegistryFile {
    fn default() -> Self {
        Self {
            version: 1,
            repos: Vec::new(),
        }
    }
}

/// In-memory repo registry with persistence.
///
/// The registry is the daemon's authoritative list of known repos.
/// It maps canonical paths to registry entries and provides resolution.
pub struct RepoRegistry {
    /// State root directory (parent of registry.json and databases/).
    /// Used for sandbox mode detection.
    state_root: PathBuf,

    /// Path to the registry JSON file.
    registry_path: PathBuf,

    /// Directory for daemon-managed databases.
    db_dir: PathBuf,

    /// Repos indexed by canonical path.
    by_path: HashMap<PathBuf, RegistryEntry>,

    /// Repos indexed by alias (if set).
    by_alias: HashMap<String, PathBuf>,

    /// Dirty flag for persistence.
    dirty: bool,
}

impl RepoRegistry {
    /// Create a new registry with resolved state root.
    ///
    /// State root resolution:
    /// 1. `RMAP_STATE_ROOT` environment variable (testing, isolated runs)
    /// 2. Platform data directory:
    ///    - macOS: `~/Library/Application Support/repo-graph/`
    ///    - Linux: `~/.local/share/rmap/`
    pub fn new() -> Result<Self, RegistryError> {
        let state_root = state_root_dir()?;
        let registry_path = state_root.join("registry.json");
        let db_dir = state_root.join("databases");

        // Ensure directories exist
        fs::create_dir_all(&db_dir)
            .map_err(|e| RegistryError::Io(format!("failed to create db directory: {}", e)))?;

        let mut registry = Self {
            state_root,
            registry_path,
            db_dir,
            by_path: HashMap::new(),
            by_alias: HashMap::new(),
            dirty: false,
        };

        // Load existing registry if present
        registry.load()?;

        Ok(registry)
    }

    /// Create a registry with explicit state root directory.
    ///
    /// Use this for:
    /// - Isolated test environments
    /// - Custom state locations
    /// - Integration testing with hermetic state
    pub fn with_state_root(state_root: &Path) -> Result<Self, RegistryError> {
        let state_root = state_root.to_path_buf();
        let registry_path = state_root.join("registry.json");
        let db_dir = state_root.join("databases");

        fs::create_dir_all(&db_dir)
            .map_err(|e| RegistryError::Io(format!("failed to create db directory: {}", e)))?;

        let mut registry = Self {
            state_root,
            registry_path,
            db_dir,
            by_path: HashMap::new(),
            by_alias: HashMap::new(),
            dirty: false,
        };

        registry.load()?;

        Ok(registry)
    }

    /// Create an empty in-memory registry that does not persist.
    ///
    /// Use this as a fallback when normal initialization fails and
    /// persistence is not required.
    pub fn empty_non_persistent() -> Self {
        Self {
            state_root: PathBuf::new(),
            registry_path: PathBuf::from("/dev/null"), // Will fail to write, which is intentional
            db_dir: PathBuf::new(),
            by_path: HashMap::new(),
            by_alias: HashMap::new(),
            dirty: false,
        }
    }

    /// Create a registry with a specific state root for testing.
    ///
    /// Does not require the path to exist or create any directories.
    /// Used to test sandbox mode detection without filesystem access.
    #[cfg(test)]
    pub fn with_test_state_root(state_root: PathBuf) -> Self {
        Self {
            registry_path: state_root.join("registry.json"),
            db_dir: state_root.join("databases"),
            state_root,
            by_path: HashMap::new(),
            by_alias: HashMap::new(),
            dirty: false,
        }
    }

    // ── State Root Access ───────────────────────────────────────────────

    /// Returns the state root directory.
    ///
    /// Used for sandbox mode detection. The state root is the parent directory
    /// containing registry.json and databases/.
    pub fn state_root(&self) -> &Path {
        &self.state_root
    }

    /// Returns the daemon-managed databases directory (`<state_root>/databases`).
    ///
    /// Single source of truth for the db-dir path (the field set by `new` /
    /// `with_state_root`). DOCTOR-RESOURCE-REPORT sums this directory for the
    /// `rmap doctor` total-storage line. Exposing the field (rather than letting
    /// callers rebuild `state_root().join("databases")`) keeps the path authoritative
    /// — notably for `empty_non_persistent`, where `db_dir` is empty but
    /// `state_root` is too, so a `join` would diverge.
    pub fn db_dir(&self) -> &Path {
        &self.db_dir
    }

    // ── Path Resolution ─────────────────────────────────────────────────

    /// Resolve a path to a registry entry.
    ///
    /// Resolution order:
    /// 1. Exact match on canonical path
    /// 2. Longest registered ancestor prefix
    ///
    /// Returns `None` if no match found.
    pub fn resolve(&self, path: &Path) -> Option<&RegistryEntry> {
        // Canonicalize the input path
        let canonical = match path.canonicalize() {
            Ok(p) => p,
            Err(_) => return None,
        };

        // Exact match
        if let Some(entry) = self.by_path.get(&canonical) {
            return Some(entry);
        }

        // Ancestor match: find longest registered ancestor
        let mut best_match: Option<&RegistryEntry> = None;
        let mut best_len = 0;

        for (registered_path, entry) in &self.by_path {
            if canonical.starts_with(registered_path) {
                let len = registered_path.as_os_str().len();
                if len > best_len {
                    best_len = len;
                    best_match = Some(entry);
                }
            }
        }

        best_match
    }

    /// Resolve by alias.
    pub fn resolve_alias(&self, alias: &str) -> Option<&RegistryEntry> {
        self.by_alias
            .get(alias)
            .and_then(|path| self.by_path.get(path))
    }

    /// Resolve by alias or path.
    ///
    /// First tries alias lookup, then path resolution.
    pub fn resolve_alias_or_path(&self, alias_or_path: &str) -> Option<&RegistryEntry> {
        // Try alias first
        if let Some(entry) = self.resolve_alias(alias_or_path) {
            return Some(entry);
        }

        // Try as path
        self.resolve(Path::new(alias_or_path))
    }

    // ── Registration ────────────────────────────────────────────────────

    /// Register a new repository or get existing entry.
    ///
    /// If the canonical path is already registered, returns the existing entry.
    /// Otherwise creates a new entry with generated `repo_uid` and allocated db path.
    pub fn register(&mut self, repo_path: &Path) -> Result<&RegistryEntry, RegistryError> {
        let canonical = repo_path.canonicalize().map_err(|e| {
            RegistryError::InvalidPath(format!(
                "cannot canonicalize '{}': {}",
                repo_path.display(),
                e
            ))
        })?;

        // Already registered?
        if self.by_path.contains_key(&canonical) {
            return Ok(self.by_path.get(&canonical).unwrap());
        }

        // Create new entry
        let entry = RegistryEntry::new(canonical.clone(), &self.db_dir);
        self.by_path.insert(canonical.clone(), entry);
        self.dirty = true;

        Ok(self.by_path.get(&canonical).unwrap())
    }

    /// Register with an alias.
    pub fn register_with_alias(
        &mut self,
        repo_path: &Path,
        alias: String,
    ) -> Result<&RegistryEntry, RegistryError> {
        // Check alias uniqueness
        if self.by_alias.contains_key(&alias) {
            return Err(RegistryError::AliasConflict(alias));
        }

        let canonical = repo_path.canonicalize().map_err(|e| {
            RegistryError::InvalidPath(format!(
                "cannot canonicalize '{}': {}",
                repo_path.display(),
                e
            ))
        })?;

        // Already registered?
        if self.by_path.contains_key(&canonical) {
            // Update alias on existing entry
            let entry = self.by_path.get_mut(&canonical).unwrap();

            // Remove old alias mapping if exists
            if let Some(old_alias) = &entry.alias {
                self.by_alias.remove(old_alias);
            }

            entry.alias = Some(alias.clone());
            self.by_alias.insert(alias, canonical.clone());
            self.dirty = true;

            return Ok(self.by_path.get(&canonical).unwrap());
        }

        // Create new entry with alias
        let entry = RegistryEntry::new(canonical.clone(), &self.db_dir).with_alias(alias.clone());
        self.by_alias.insert(alias, canonical.clone());
        self.by_path.insert(canonical.clone(), entry);
        self.dirty = true;

        Ok(self.by_path.get(&canonical).unwrap())
    }

    /// Update index timestamp and snapshot for a registered repo.
    pub fn record_index(
        &mut self,
        canonical_path: &Path,
        timestamp: String,
        snapshot_uid: String,
    ) -> Result<(), RegistryError> {
        let entry = self
            .by_path
            .get_mut(canonical_path)
            .ok_or_else(|| RegistryError::NotFound(canonical_path.display().to_string()))?;

        entry.record_index(timestamp, snapshot_uid);
        self.dirty = true;
        Ok(())
    }

    /// Set or change alias for a registered repo.
    pub fn set_alias(&mut self, canonical_path: &Path, alias: String) -> Result<(), RegistryError> {
        // Check alias uniqueness (unless it's the same repo)
        if let Some(existing_path) = self.by_alias.get(&alias) {
            if existing_path != canonical_path {
                return Err(RegistryError::AliasConflict(alias));
            }
        }

        let entry = self
            .by_path
            .get_mut(canonical_path)
            .ok_or_else(|| RegistryError::NotFound(canonical_path.display().to_string()))?;

        // Remove old alias mapping if exists
        if let Some(old_alias) = &entry.alias {
            self.by_alias.remove(old_alias);
        }

        entry.alias = Some(alias.clone());
        self.by_alias.insert(alias, canonical_path.to_path_buf());
        self.dirty = true;

        Ok(())
    }

    /// Remove a repo from the registry.
    ///
    /// Does not delete the database file (caller can do that separately).
    pub fn remove(&mut self, canonical_path: &Path) -> Result<RegistryEntry, RegistryError> {
        let entry = self
            .by_path
            .remove(canonical_path)
            .ok_or_else(|| RegistryError::NotFound(canonical_path.display().to_string()))?;

        // Remove alias mapping if exists
        if let Some(alias) = &entry.alias {
            self.by_alias.remove(alias);
        }

        self.dirty = true;
        Ok(entry)
    }

    // ── Listing ─────────────────────────────────────────────────────────

    /// List all registered repos.
    pub fn list(&self) -> Vec<&RegistryEntry> {
        let mut entries: Vec<_> = self.by_path.values().collect();
        entries.sort_by(|a, b| a.canonical_path.cmp(&b.canonical_path));
        entries
    }

    /// Get entry by canonical path.
    pub fn get(&self, canonical_path: &Path) -> Option<&RegistryEntry> {
        self.by_path.get(canonical_path)
    }

    /// Get mutable entry by canonical path.
    pub fn get_mut(&mut self, canonical_path: &Path) -> Option<&mut RegistryEntry> {
        self.dirty = true; // Assume mutation
        self.by_path.get_mut(canonical_path)
    }

    // ── Persistence ─────────────────────────────────────────────────────

    /// Load registry from disk.
    fn load(&mut self) -> Result<(), RegistryError> {
        if !self.registry_path.exists() {
            return Ok(()); // Empty registry
        }

        let file = File::open(&self.registry_path)
            .map_err(|e| RegistryError::Io(format!("failed to open registry: {}", e)))?;
        let reader = BufReader::new(file);

        let registry_file: RegistryFile = serde_json::from_reader(reader)
            .map_err(|e| RegistryError::Parse(format!("failed to parse registry: {}", e)))?;

        // Populate in-memory structures
        for entry in registry_file.repos {
            if let Some(alias) = &entry.alias {
                self.by_alias
                    .insert(alias.clone(), entry.canonical_path.clone());
            }
            self.by_path.insert(entry.canonical_path.clone(), entry);
        }

        Ok(())
    }

    /// Save registry to disk with atomic write.
    pub fn save(&mut self) -> Result<(), RegistryError> {
        if !self.dirty {
            return Ok(());
        }

        let registry_file = RegistryFile {
            version: 1,
            repos: self.by_path.values().cloned().collect(),
        };

        // Atomic write: write to temp file, then rename
        let temp_path = self.registry_path.with_extension("json.tmp");

        let file = File::create(&temp_path)
            .map_err(|e| RegistryError::Io(format!("failed to create temp file: {}", e)))?;
        let writer = BufWriter::new(file);

        serde_json::to_writer_pretty(writer, &registry_file)
            .map_err(|e| RegistryError::Io(format!("failed to write registry: {}", e)))?;

        fs::rename(&temp_path, &self.registry_path)
            .map_err(|e| RegistryError::Io(format!("failed to rename temp file: {}", e)))?;

        self.dirty = false;
        Ok(())
    }

    /// Force save even if not dirty (for testing).
    #[cfg(test)]
    pub fn force_save(&mut self) -> Result<(), RegistryError> {
        self.dirty = true;
        self.save()
    }
}

impl Default for RepoRegistry {
    fn default() -> Self {
        Self::new().expect("failed to create default registry")
    }
}

// ── Helper Functions ────────────────────────────────────────────────────

/// Generate a new stable `repo_uid` using ULID.
fn generate_repo_uid() -> String {
    format!("repo_{}", Ulid::new().to_string().to_lowercase())
}

/// Allocate a database path based on the canonical repo path.
///
/// Uses first 16 chars of SHA256 hash of the path.
fn allocate_db_path(canonical_path: &Path, db_dir: &Path) -> PathBuf {
    let mut hasher = Sha256::new();
    hasher.update(canonical_path.as_os_str().as_encoded_bytes());
    let hash = hasher.finalize();
    let hash_hex = hex::encode(&hash[..8]); // 16 hex chars
    db_dir.join(format!("{}.db", hash_hex))
}

/// Get platform-appropriate data directory.
/// Resolve the state root directory.
///
/// Resolution order:
/// 1. `RMAP_STATE_ROOT` environment variable (for testing and isolated runs)
/// 2. Platform-specific data directory
///
/// This allows hermetic testing and deterministic local smoke runs.
fn state_root_dir() -> Result<PathBuf, RegistryError> {
    // Check for explicit override first (testing, isolated runs)
    if let Ok(root) = std::env::var("RMAP_STATE_ROOT") {
        return Ok(PathBuf::from(root));
    }

    // Fall back to platform-specific directory
    platform_data_dir()
}

fn platform_data_dir() -> Result<PathBuf, RegistryError> {
    // Use platform-paths crate for canonical home lookup
    // This ensures stable paths across sandboxed shells
    repo_graph_platform_paths::data_dir()
        .ok_or_else(|| RegistryError::Io("could not determine data directory".to_string()))
}

/// Canonicalize a path, returning a descriptive error on failure.
pub fn canonicalize_path(path: &Path) -> Result<PathBuf, RegistryError> {
    path.canonicalize().map_err(|e| {
        RegistryError::InvalidPath(format!("cannot canonicalize '{}': {}", path.display(), e))
    })
}

// ── Error Types ─────────────────────────────────────────────────────────

/// Registry operation errors.
#[derive(Debug)]
pub enum RegistryError {
    /// I/O error (file read/write).
    Io(String),
    /// JSON parse error.
    Parse(String),
    /// Invalid path (cannot canonicalize).
    InvalidPath(String),
    /// Repo not found in registry.
    NotFound(String),
    /// Alias already in use by another repo.
    AliasConflict(String),
}

impl std::fmt::Display for RegistryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(msg) => write!(f, "registry I/O error: {}", msg),
            Self::Parse(msg) => write!(f, "registry parse error: {}", msg),
            Self::InvalidPath(msg) => write!(f, "invalid path: {}", msg),
            Self::NotFound(path) => write!(f, "repo not indexed: {}", path),
            Self::AliasConflict(alias) => write!(f, "alias already in use: {}", alias),
        }
    }
}

impl std::error::Error for RegistryError {}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn test_registry(state_root: &Path) -> RepoRegistry {
        RepoRegistry::with_state_root(state_root).unwrap()
    }

    #[test]
    fn register_creates_entry() {
        let dir = tempdir().unwrap();
        let mut registry = test_registry(dir.path());

        // Create a real directory to register
        let repo_dir = dir.path().join("my-repo");
        fs::create_dir(&repo_dir).unwrap();

        let entry = registry.register(&repo_dir).unwrap();

        assert_eq!(entry.canonical_path, repo_dir.canonicalize().unwrap());
        assert!(entry.repo_uid.starts_with("repo_"));
        assert!(entry.db_path.to_string_lossy().ends_with(".db"));
        assert!(entry.alias.is_none());
    }

    #[test]
    fn register_same_path_returns_existing() {
        let dir = tempdir().unwrap();
        let mut registry = test_registry(dir.path());

        let repo_dir = dir.path().join("my-repo");
        fs::create_dir(&repo_dir).unwrap();

        let entry1 = registry.register(&repo_dir).unwrap();
        let uid1 = entry1.repo_uid.clone();

        let entry2 = registry.register(&repo_dir).unwrap();
        let uid2 = entry2.repo_uid.clone();

        // Same repo_uid, not regenerated
        assert_eq!(uid1, uid2);
    }

    #[test]
    fn register_with_alias() {
        let dir = tempdir().unwrap();
        let mut registry = test_registry(dir.path());

        let repo_dir = dir.path().join("my-repo");
        fs::create_dir(&repo_dir).unwrap();

        let entry = registry
            .register_with_alias(&repo_dir, "myalias".to_string())
            .unwrap();

        assert_eq!(entry.alias.as_deref(), Some("myalias"));
    }

    #[test]
    fn alias_conflict_detected() {
        let dir = tempdir().unwrap();
        let mut registry = test_registry(dir.path());

        let repo1 = dir.path().join("repo1");
        let repo2 = dir.path().join("repo2");
        fs::create_dir(&repo1).unwrap();
        fs::create_dir(&repo2).unwrap();

        registry
            .register_with_alias(&repo1, "shared".to_string())
            .unwrap();

        let result = registry.register_with_alias(&repo2, "shared".to_string());
        assert!(matches!(result, Err(RegistryError::AliasConflict(_))));
    }

    #[test]
    fn resolve_exact_match() {
        let dir = tempdir().unwrap();
        let mut registry = test_registry(dir.path());

        let repo_dir = dir.path().join("my-repo");
        fs::create_dir(&repo_dir).unwrap();

        registry.register(&repo_dir).unwrap();

        let entry = registry.resolve(&repo_dir).unwrap();
        assert_eq!(entry.canonical_path, repo_dir.canonicalize().unwrap());
    }

    #[test]
    fn resolve_ancestor_match() {
        let dir = tempdir().unwrap();
        let mut registry = test_registry(dir.path());

        let repo_dir = dir.path().join("my-repo");
        let subdir = repo_dir.join("src").join("core");
        fs::create_dir_all(&subdir).unwrap();

        registry.register(&repo_dir).unwrap();

        // Resolve from subdirectory should find parent repo
        let entry = registry.resolve(&subdir).unwrap();
        assert_eq!(entry.canonical_path, repo_dir.canonicalize().unwrap());
    }

    #[test]
    fn resolve_alias() {
        let dir = tempdir().unwrap();
        let mut registry = test_registry(dir.path());

        let repo_dir = dir.path().join("my-repo");
        fs::create_dir(&repo_dir).unwrap();

        registry
            .register_with_alias(&repo_dir, "myalias".to_string())
            .unwrap();

        let entry = registry.resolve_alias("myalias").unwrap();
        assert_eq!(entry.canonical_path, repo_dir.canonicalize().unwrap());
    }

    #[test]
    fn persistence_roundtrip() {
        let dir = tempdir().unwrap();

        let repo_dir = dir.path().join("my-repo");
        fs::create_dir(&repo_dir).unwrap();

        let repo_uid;
        {
            let mut registry = test_registry(dir.path());
            let entry = registry
                .register_with_alias(&repo_dir, "myalias".to_string())
                .unwrap();
            repo_uid = entry.repo_uid.clone();
            registry.save().unwrap();
        }

        // Load fresh registry
        let registry = test_registry(dir.path());
        let entry = registry.resolve_alias("myalias").unwrap();

        assert_eq!(entry.repo_uid, repo_uid);
        assert_eq!(entry.alias.as_deref(), Some("myalias"));
    }

    #[test]
    fn remove_entry() {
        let dir = tempdir().unwrap();
        let mut registry = test_registry(dir.path());

        let repo_dir = dir.path().join("my-repo");
        fs::create_dir(&repo_dir).unwrap();

        registry
            .register_with_alias(&repo_dir, "myalias".to_string())
            .unwrap();

        let canonical = repo_dir.canonicalize().unwrap();
        registry.remove(&canonical).unwrap();

        assert!(registry.resolve(&repo_dir).is_none());
        assert!(registry.resolve_alias("myalias").is_none());
    }

    #[test]
    fn record_index_updates_entry() {
        let dir = tempdir().unwrap();
        let mut registry = test_registry(dir.path());

        let repo_dir = dir.path().join("my-repo");
        fs::create_dir(&repo_dir).unwrap();

        registry.register(&repo_dir).unwrap();

        let canonical = repo_dir.canonicalize().unwrap();
        registry
            .record_index(
                &canonical,
                "2026-05-15T10:30:00Z".to_string(),
                "repo_abc123/2026-05-15T10:30:00Z/def456".to_string(),
            )
            .unwrap();

        let entry = registry.get(&canonical).unwrap();
        assert_eq!(
            entry.last_indexed_at.as_deref(),
            Some("2026-05-15T10:30:00Z")
        );
        assert!(entry.last_snapshot_uid.is_some());
    }

    #[test]
    fn db_path_deterministic() {
        let dir = tempdir().unwrap();
        let db_dir = dir.path().join("databases");
        fs::create_dir(&db_dir).unwrap();

        let repo_path = PathBuf::from("/Users/alice/projects/my-app");

        let path1 = allocate_db_path(&repo_path, &db_dir);
        let path2 = allocate_db_path(&repo_path, &db_dir);

        // Same input -> same hash
        assert_eq!(path1, path2);
    }

    #[test]
    fn repo_uid_format() {
        let uid = generate_repo_uid();
        assert!(uid.starts_with("repo_"));
        // ULID is 26 chars
        assert_eq!(uid.len(), 5 + 26); // "repo_" + ulid
    }
}
