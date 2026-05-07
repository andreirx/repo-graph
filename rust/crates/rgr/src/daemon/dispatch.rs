//! Service dispatcher for the daemon.
//!
//! Implements the Dispatcher trait from daemon-transport,
//! routing requests to real application services.
//!
//! # API Identity Model
//!
//! All repo-scoped methods require composite identity: `db_path` + `repo_uid`.
//! This ensures unambiguous repo lookup in multi-database mode.
//!
//! # Coordination Model
//!
//! Write operations acquire database-level coordination before executing:
//! - `index`: DB write lock only (repo may not exist yet)
//! - `refresh`: DB write lock, then repo refresh lock

use std::path::Path;
use std::sync::Arc;

use repo_graph_daemon_transport::{
    DispatchResult, Dispatcher, ErrorCode, ErrorDetail, Request,
};
use repo_graph_repo_index::compose::{index_path, refresh_path, ComposeOptions};
use repo_graph_storage::types::RepoRef;
use serde_json::Value;

use super::state::{DaemonState, RepoKey};

/// Dispatcher that routes requests to real services.
pub struct ServiceDispatcher {
    state: Arc<DaemonState>,
}

impl ServiceDispatcher {
    /// Create a new service dispatcher with the given daemon state.
    pub fn new(state: Arc<DaemonState>) -> Self {
        Self { state }
    }

    /// Get a required string parameter.
    fn get_string_param<'a>(params: &'a Value, key: &str) -> Result<&'a str, ErrorDetail> {
        params
            .get(key)
            .and_then(|v| v.as_str())
            .ok_or_else(|| ErrorDetail::invalid_request(format!("missing or invalid '{}' parameter", key)))
    }

    /// Get an optional string parameter.
    #[allow(dead_code)] // For future use with optional params
    fn get_optional_string_param<'a>(params: &'a Value, key: &str) -> Option<&'a str> {
        params.get(key).and_then(|v| v.as_str())
    }
}

impl Dispatcher for ServiceDispatcher {
    fn dispatch(&self, request: &Request) -> DispatchResult {
        match request.method.as_str() {
            // ── Test methods ────────────────────────────────────────
            "ping" => DispatchResult::success(&request.id, serde_json::json!({"pong": true})),

            "echo" => DispatchResult::success(&request.id, request.params.clone()),

            // ── Daemon management ───────────────────────────────────
            "load_repo" => self.handle_load_repo(request),
            "unload_repo" => self.handle_unload_repo(request),
            "list_repos" => self.handle_list_repos(request),

            // ── Read operations ─────────────────────────────────────
            "callers" => self.handle_callers(request),
            "callees" => self.handle_callees(request),
            "imports" => self.handle_imports(request),

            // ── Write operations ────────────────────────────────────
            "index" => self.handle_index(request),
            "refresh" => self.handle_refresh(request),

            // ── Unknown method ──────────────────────────────────────
            _ => DispatchResult::unknown_method(&request.id, &request.method),
        }
    }
}

// ── Method handlers ─────────────────────────────────────────────────

impl ServiceDispatcher {
    fn handle_load_repo(&self, request: &Request) -> DispatchResult {
        let db_path = match Self::get_string_param(&request.params, "db_path") {
            Ok(p) => p,
            Err(e) => return DispatchResult::error(&request.id, e),
        };
        let repo_uid = match Self::get_string_param(&request.params, "repo_uid") {
            Ok(r) => r,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        match self.state.load_repo(std::path::Path::new(db_path), repo_uid) {
            Ok(_) => DispatchResult::success(
                &request.id,
                serde_json::json!({"loaded": repo_uid}),
            ),
            Err(e) => DispatchResult::error(
                &request.id,
                ErrorDetail::new(ErrorCode::InternalError, e),
            ),
        }
    }

    fn handle_unload_repo(&self, request: &Request) -> DispatchResult {
        let db_path = match Self::get_string_param(&request.params, "db_path") {
            Ok(p) => p,
            Err(e) => return DispatchResult::error(&request.id, e),
        };
        let repo_uid = match Self::get_string_param(&request.params, "repo_uid") {
            Ok(r) => r,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        // Build composite key
        let key = match RepoKey::new(Path::new(db_path), repo_uid) {
            Ok(k) => k,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::invalid_request(e),
                );
            }
        };

        let unloaded = self.state.unload_repo_by_key(&key);
        DispatchResult::success(
            &request.id,
            serde_json::json!({"unloaded": unloaded}),
        )
    }

    fn handle_list_repos(&self, request: &Request) -> DispatchResult {
        let repos = self.state.list_repos();
        DispatchResult::success(
            &request.id,
            serde_json::json!({"repos": repos}),
        )
    }

    fn handle_callers(&self, request: &Request) -> DispatchResult {
        let db_path = match Self::get_string_param(&request.params, "db_path") {
            Ok(p) => p,
            Err(e) => return DispatchResult::error(&request.id, e),
        };
        let repo_uid = match Self::get_string_param(&request.params, "repo_uid") {
            Ok(r) => r,
            Err(e) => return DispatchResult::error(&request.id, e),
        };
        let symbol = match Self::get_string_param(&request.params, "symbol") {
            Ok(s) => s,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        // Build composite key
        let key = match RepoKey::new(Path::new(db_path), repo_uid) {
            Ok(k) => k,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::invalid_request(e),
                );
            }
        };

        // Get repo state by composite key
        let repo_state = match self.state.get_repo_by_key(&key) {
            Some(s) => s,
            None => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(
                        ErrorCode::RepoNotFound,
                        format!("repo not loaded: {}:{}", db_path, repo_uid),
                    ),
                );
            }
        };

        // Acquire read lock
        let _read_guard = repo_state.coordinator.acquire_read();

        // Get latest snapshot
        let snapshot = match repo_state.storage.get_latest_snapshot(repo_uid) {
            Ok(Some(snap)) => snap,
            Ok(None) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::SnapshotNotFound, "no snapshot found"),
                );
            }
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                );
            }
        };

        // Resolve symbol
        use repo_graph_storage::queries::SymbolResolveError;
        let target = match repo_state.storage.resolve_symbol(&snapshot.snapshot_uid, symbol) {
            Ok(sym) => sym,
            Err(SymbolResolveError::NotFound) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::invalid_request(format!("symbol not found: {}", symbol)),
                );
            }
            Err(SymbolResolveError::Ambiguous(keys)) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::invalid_request(format!(
                        "ambiguous symbol '{}', matches: {}",
                        symbol,
                        keys.join(", ")
                    )),
                );
            }
            Err(SymbolResolveError::Storage(e)) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                );
            }
        };

        // Find callers
        let edge_types = ["CALLS"];
        let callers = match repo_state.storage.find_direct_callers(
            &snapshot.snapshot_uid,
            &target.stable_key,
            &edge_types,
        ) {
            Ok(c) => c,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                );
            }
        };

        DispatchResult::success(
            &request.id,
            serde_json::json!({
                "target": target,
                "callers": callers,
                "count": callers.len(),
            }),
        )
    }

    fn handle_callees(&self, request: &Request) -> DispatchResult {
        let db_path = match Self::get_string_param(&request.params, "db_path") {
            Ok(p) => p,
            Err(e) => return DispatchResult::error(&request.id, e),
        };
        let repo_uid = match Self::get_string_param(&request.params, "repo_uid") {
            Ok(r) => r,
            Err(e) => return DispatchResult::error(&request.id, e),
        };
        let symbol = match Self::get_string_param(&request.params, "symbol") {
            Ok(s) => s,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        // Build composite key
        let key = match RepoKey::new(Path::new(db_path), repo_uid) {
            Ok(k) => k,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::invalid_request(e),
                );
            }
        };

        // Get repo state by composite key
        let repo_state = match self.state.get_repo_by_key(&key) {
            Some(s) => s,
            None => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(
                        ErrorCode::RepoNotFound,
                        format!("repo not loaded: {}:{}", db_path, repo_uid),
                    ),
                );
            }
        };

        // Acquire read lock
        let _read_guard = repo_state.coordinator.acquire_read();

        // Get latest snapshot
        let snapshot = match repo_state.storage.get_latest_snapshot(repo_uid) {
            Ok(Some(snap)) => snap,
            Ok(None) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::SnapshotNotFound, "no snapshot found"),
                );
            }
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                );
            }
        };

        // Resolve symbol
        use repo_graph_storage::queries::SymbolResolveError;
        let target = match repo_state.storage.resolve_symbol(&snapshot.snapshot_uid, symbol) {
            Ok(sym) => sym,
            Err(SymbolResolveError::NotFound) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::invalid_request(format!("symbol not found: {}", symbol)),
                );
            }
            Err(SymbolResolveError::Ambiguous(keys)) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::invalid_request(format!(
                        "ambiguous symbol '{}', matches: {}",
                        symbol,
                        keys.join(", ")
                    )),
                );
            }
            Err(SymbolResolveError::Storage(e)) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                );
            }
        };

        // Find callees
        let edge_types = ["CALLS"];
        let callees = match repo_state.storage.find_direct_callees(
            &snapshot.snapshot_uid,
            &target.stable_key,
            &edge_types,
        ) {
            Ok(c) => c,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                );
            }
        };

        DispatchResult::success(
            &request.id,
            serde_json::json!({
                "target": target,
                "callees": callees,
                "count": callees.len(),
            }),
        )
    }

    fn handle_imports(&self, request: &Request) -> DispatchResult {
        let db_path = match Self::get_string_param(&request.params, "db_path") {
            Ok(p) => p,
            Err(e) => return DispatchResult::error(&request.id, e),
        };
        let repo_uid = match Self::get_string_param(&request.params, "repo_uid") {
            Ok(r) => r,
            Err(e) => return DispatchResult::error(&request.id, e),
        };
        let file_path = match Self::get_string_param(&request.params, "file") {
            Ok(f) => f,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        // Build composite key
        let key = match RepoKey::new(Path::new(db_path), repo_uid) {
            Ok(k) => k,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::invalid_request(e),
                );
            }
        };

        // Get repo state by composite key
        let repo_state = match self.state.get_repo_by_key(&key) {
            Some(s) => s,
            None => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(
                        ErrorCode::RepoNotFound,
                        format!("repo not loaded: {}:{}", db_path, repo_uid),
                    ),
                );
            }
        };

        // Acquire read lock
        let _read_guard = repo_state.coordinator.acquire_read();

        // Get latest snapshot
        let snapshot = match repo_state.storage.get_latest_snapshot(repo_uid) {
            Ok(Some(snap)) => snap,
            Ok(None) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::SnapshotNotFound, "no snapshot found"),
                );
            }
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                );
            }
        };

        // Construct FILE stable key
        let file_stable_key = format!("{}:{}:FILE", repo_uid, file_path);

        // Verify file exists
        match repo_state.storage.node_exists(&snapshot.snapshot_uid, &file_stable_key) {
            Ok(true) => {}
            Ok(false) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::invalid_request(format!("file not found: {}", file_path)),
                );
            }
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                );
            }
        }

        // Find imports
        let imports = match repo_state.storage.find_imports(&snapshot.snapshot_uid, &file_stable_key) {
            Ok(i) => i,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                );
            }
        };

        DispatchResult::success(
            &request.id,
            serde_json::json!({
                "file": file_path,
                "imports": imports,
                "count": imports.len(),
            }),
        )
    }

    // ── Write operations ────────────────────────────────────────────

    fn handle_index(&self, request: &Request) -> DispatchResult {
        let repo_path_str = match Self::get_string_param(&request.params, "repo_path") {
            Ok(p) => p,
            Err(e) => return DispatchResult::error(&request.id, e),
        };
        let db_path_str = match Self::get_string_param(&request.params, "db_path") {
            Ok(p) => p,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        let repo_path = Path::new(repo_path_str);
        let db_path = Path::new(db_path_str);

        // Derive repo_uid from repo directory name
        let repo_uid = repo_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("repo");

        // Validate repo_path exists and is a directory
        if !repo_path.is_dir() {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::invalid_request(format!(
                    "repo_path does not exist or is not a directory: {}",
                    repo_path_str
                )),
            );
        }

        // Acquire DB write coordination (DB file may not exist yet)
        let db_runtime = match self.state.get_or_create_db_runtime_for_new_db(db_path) {
            Ok(r) => r,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e),
                );
            }
        };
        let _db_write_guard = db_runtime.acquire_write();

        // Compute storage_root_path relative to DB location
        let storage_root_path = match crate::cli::compute_storage_root_path(repo_path, db_path) {
            Ok(p) => Some(p),
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e),
                );
            }
        };

        // Parse optional include_roots
        let c_include_roots: Vec<String> = request
            .params
            .get("include_roots")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        let options = ComposeOptions {
            c_include_roots,
            storage_root_path,
            ..ComposeOptions::default()
        };

        // Execute index under DB write lock
        match index_path(repo_path, db_path, repo_uid, &options) {
            Ok(result) => DispatchResult::success(
                &request.id,
                serde_json::json!({
                    "repo_uid": repo_uid,
                    "snapshot_uid": result.snapshot_uid,
                    "files_total": result.files_total,
                    "nodes_total": result.nodes_total,
                    "edges_total": result.edges_total,
                    "edges_unresolved": result.edges_unresolved,
                }),
            ),
            Err(e) => DispatchResult::error(
                &request.id,
                ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
            ),
        }
        // _db_write_guard drops here, releasing the lock
    }

    fn handle_refresh(&self, request: &Request) -> DispatchResult {
        let db_path_str = match Self::get_string_param(&request.params, "db_path") {
            Ok(p) => p,
            Err(e) => return DispatchResult::error(&request.id, e),
        };
        let repo_uid = match Self::get_string_param(&request.params, "repo_uid") {
            Ok(r) => r,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        let db_path = Path::new(db_path_str);

        // Build composite key
        let key = match RepoKey::new(db_path, repo_uid) {
            Ok(k) => k,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::invalid_request(e),
                );
            }
        };

        // Get repo state (must be loaded)
        let repo_state = match self.state.get_repo_by_key(&key) {
            Some(s) => s,
            None => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(
                        ErrorCode::RepoNotFound,
                        format!("repo not loaded: {}:{}", db_path_str, repo_uid),
                    ),
                );
            }
        };

        // Acquire DB write coordination first
        let db_runtime = match self.state.get_or_create_db_runtime(db_path) {
            Ok(r) => r,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e),
                );
            }
        };
        let _db_write_guard = db_runtime.acquire_write();

        // Then acquire repo refresh lock (blocks new readers, waits for active readers)
        let _refresh_guard = repo_state.coordinator.acquire_refresh();

        // Resolve repo_path from stored root_path
        let canonical_db_path = repo_state.db_path();
        let repo_info = match repo_state.storage.get_repo(&RepoRef::Uid(repo_uid.to_string())) {
            Ok(Some(r)) => r,
            Ok(None) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, "repo metadata not found"),
                );
            }
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                );
            }
        };

        // root_path is stored relative to db_path parent
        let repo_path = canonical_db_path
            .parent()
            .map(|p| p.join(&repo_info.root_path))
            .unwrap_or_else(|| Path::new(&repo_info.root_path).to_path_buf());

        if !repo_path.is_dir() {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::invalid_request(format!(
                    "resolved repo_path does not exist or is not a directory: {}",
                    repo_path.display()
                )),
            );
        }

        // Parse optional include_roots
        let c_include_roots: Vec<String> = request
            .params
            .get("include_roots")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        // Compute storage_root_path for consistency
        let storage_root_path = match crate::cli::compute_storage_root_path(&repo_path, canonical_db_path) {
            Ok(p) => Some(p),
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e),
                );
            }
        };

        let options = ComposeOptions {
            c_include_roots,
            storage_root_path,
            ..ComposeOptions::default()
        };

        // Execute refresh under both locks
        match refresh_path(&repo_path, canonical_db_path, repo_uid, &options) {
            Ok(result) => DispatchResult::success(
                &request.id,
                serde_json::json!({
                    "snapshot_uid": result.snapshot_uid,
                    "files_total": result.files_total,
                    "nodes_total": result.nodes_total,
                    "edges_total": result.edges_total,
                    "edges_unresolved": result.edges_unresolved,
                }),
            ),
            Err(e) => DispatchResult::error(
                &request.id,
                ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
            ),
        }
        // Guards drop here: _refresh_guard then _db_write_guard
    }
}
