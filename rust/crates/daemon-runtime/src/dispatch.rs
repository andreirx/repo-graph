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
//! - `enrich`: DB write lock, then repo refresh lock (modifies existing snapshot)

use std::ops::ControlFlow;
use std::path::Path;
use std::sync::Arc;

use enrichment::{EnrichmentConfig, EnrichmentLanguage, EnrichmentPipeline, ResolverRegistry};
use jdtls_resolver::{JdtlsConfig, JdtlsResolver};
use repo_graph_agent::Budget;
use repo_graph_daemon_transport::{
    DispatchResult, Dispatcher, ErrorCode, ErrorDetail, ProgressDetail, ProgressEmitter, Request,
};
use repo_graph_repo_index::compose::{
    index_path_with_progress, refresh_path_with_progress, ComposeOptions, ProgressEvent,
};
use repo_graph_storage::types::RepoRef;
use repo_graph_storage::StorageConnection;
use rust_analyzer_resolver::RustAnalyzerResolver;
use serde_json::Value;
use tsserver_resolver::TsServerResolver;

use crate::state::{DaemonState, RepoKey};
use crate::util::{compute_storage_root_path, compute_trust_overlay_for_snapshot, utc_now_iso8601};

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
    fn dispatch(&self, request: &Request, emitter: &mut dyn ProgressEmitter) -> DispatchResult {
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

            // ── Agent services ──────────────────────────────────────
            "orient" => self.handle_orient(request),
            "check" => self.handle_check(request),
            "explain" => self.handle_explain(request),

            // ── Write operations (with progress) ────────────────────
            "index" => self.handle_index(request, emitter),
            "refresh" => self.handle_refresh(request, emitter),
            "enrich" => self.handle_enrich(request, emitter),

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

    fn handle_index(&self, request: &Request, emitter: &mut dyn ProgressEmitter) -> DispatchResult {
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
        let storage_root_path = match compute_storage_root_path(repo_path, db_path) {
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

        // Create progress callback that maps repo-index events to daemon protocol.
        // Returns Continue on successful emit, Break on transport failure.
        // Break causes the orchestrator to abort at this checkpoint.
        let mut progress_callback = |event: &ProgressEvent| -> ControlFlow<()> {
            match emitter.emit(ProgressDetail {
                phase: event.phase.clone(),
                current: event.current,
                total: event.total,
            }) {
                Ok(()) => ControlFlow::Continue(()),
                Err(_) => ControlFlow::Break(()),
            }
        };

        // Execute index under DB write lock (with progress)
        match index_path_with_progress(repo_path, db_path, repo_uid, &options, Some(&mut progress_callback)) {
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
            Err(repo_graph_repo_index::compose::ComposeError::Aborted) => DispatchResult::error(
                &request.id,
                ErrorDetail::new(
                    ErrorCode::ProgressDeliveryFailed,
                    "operation aborted: progress delivery failed",
                ),
            ),
            Err(e) => DispatchResult::error(
                &request.id,
                ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
            ),
        }
        // _db_write_guard drops here, releasing the lock
    }

    fn handle_refresh(&self, request: &Request, emitter: &mut dyn ProgressEmitter) -> DispatchResult {
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
        let storage_root_path = match compute_storage_root_path(&repo_path, canonical_db_path) {
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

        // Create progress callback that maps repo-index events to daemon protocol.
        // Returns Continue on successful emit, Break on transport failure.
        // Break causes the orchestrator to abort at this checkpoint.
        let mut progress_callback = |event: &ProgressEvent| -> ControlFlow<()> {
            match emitter.emit(ProgressDetail {
                phase: event.phase.clone(),
                current: event.current,
                total: event.total,
            }) {
                Ok(()) => ControlFlow::Continue(()),
                Err(_) => ControlFlow::Break(()),
            }
        };

        // Execute refresh under both locks (with progress)
        match refresh_path_with_progress(&repo_path, canonical_db_path, repo_uid, &options, Some(&mut progress_callback)) {
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
            Err(repo_graph_repo_index::compose::ComposeError::Aborted) => DispatchResult::error(
                &request.id,
                ErrorDetail::new(
                    ErrorCode::ProgressDeliveryFailed,
                    "operation aborted: progress delivery failed",
                ),
            ),
            Err(e) => DispatchResult::error(
                &request.id,
                ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
            ),
        }
        // Guards drop here: _refresh_guard then _db_write_guard
    }

    fn handle_enrich(&self, request: &Request, emitter: &mut dyn ProgressEmitter) -> DispatchResult {
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

        // Then acquire repo refresh lock (enrich is a write operation on existing snapshot)
        let _refresh_guard = repo_state.coordinator.acquire_refresh();

        // Parse optional parameters
        let snapshot_uid_param = Self::get_optional_string_param(&request.params, "snapshot_uid");
        let dry_run = request.params.get("dry_run").and_then(|v| v.as_bool()).unwrap_or(false);
        let promote = request.params.get("promote").and_then(|v| v.as_bool()).unwrap_or(false);
        let force = request.params.get("force").and_then(|v| v.as_bool()).unwrap_or(false);
        let limit = request.params.get("limit").and_then(|v| v.as_u64()).map(|n| n as usize);
        let jdtls_path = Self::get_optional_string_param(&request.params, "jdtls_path")
            .map(String::from)
            .or_else(|| std::env::var("JDTLS_PATH").ok());

        // Parse language filter
        let languages: Vec<EnrichmentLanguage> = request
            .params
            .get("languages")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .filter_map(|s| match s.to_lowercase().as_str() {
                        "rust" | "rs" => Some(EnrichmentLanguage::Rust),
                        "typescript" | "ts" | "javascript" | "js" => Some(EnrichmentLanguage::TypeScript),
                        "java" => Some(EnrichmentLanguage::Java),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default();

        // Resolve snapshot (latest or specified)
        let snapshot_uid = match snapshot_uid_param {
            Some(uid) => {
                // Validate snapshot exists and belongs to this repo
                match repo_state.storage.get_snapshot(uid) {
                    Ok(Some(snap)) => {
                        if snap.repo_uid != repo_uid {
                            return DispatchResult::error(
                                &request.id,
                                ErrorDetail::invalid_request(format!(
                                    "snapshot '{}' belongs to repo '{}', not '{}'",
                                    uid, snap.repo_uid, repo_uid
                                )),
                            );
                        }
                        if snap.status != "ready" {
                            return DispatchResult::error(
                                &request.id,
                                ErrorDetail::invalid_request(format!(
                                    "snapshot '{}' is not ready (status: {})",
                                    uid, snap.status
                                )),
                            );
                        }
                        snap.snapshot_uid
                    }
                    Ok(None) => {
                        return DispatchResult::error(
                            &request.id,
                            ErrorDetail::invalid_request(format!("snapshot '{}' not found", uid)),
                        );
                    }
                    Err(e) => {
                        return DispatchResult::error(
                            &request.id,
                            ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                        );
                    }
                }
            }
            None => {
                // Use latest ready snapshot
                match repo_state.storage.get_latest_snapshot(repo_uid) {
                    Ok(Some(snap)) => {
                        if snap.status != "ready" {
                            return DispatchResult::error(
                                &request.id,
                                ErrorDetail::invalid_request(format!(
                                    "latest snapshot for '{}' is not ready (status: {})",
                                    repo_uid, snap.status
                                )),
                            );
                        }
                        snap.snapshot_uid
                    }
                    Ok(None) => {
                        return DispatchResult::error(
                            &request.id,
                            ErrorDetail::invalid_request(format!(
                                "no snapshot found for repo '{}'",
                                repo_uid
                            )),
                        );
                    }
                    Err(e) => {
                        return DispatchResult::error(
                            &request.id,
                            ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                        );
                    }
                }
            }
        };

        // Emit initial progress
        if emitter.emit(ProgressDetail {
            phase: "initializing".to_string(),
            current: 0,
            total: 1,
        }).is_err() {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::new(ErrorCode::ProgressDeliveryFailed, "progress delivery failed"),
            );
        }

        // Build resolver registry
        let mut registry = ResolverRegistry::new();
        let mut available_languages = Vec::new();

        // Register Rust resolver if not filtered out
        let should_register_rust = languages.is_empty()
            || languages.contains(&EnrichmentLanguage::Rust);
        if should_register_rust {
            registry.register(Box::new(RustAnalyzerResolver::new()));
            available_languages.push("rust".to_string());
        }

        // Register TypeScript resolver if not filtered out
        let should_register_typescript = languages.is_empty()
            || languages.contains(&EnrichmentLanguage::TypeScript);
        if should_register_typescript {
            registry.register(Box::new(TsServerResolver::new()));
            available_languages.push("typescript".to_string());
        }

        // Register Java resolver if not filtered out and jdtls available
        let should_register_java = languages.is_empty()
            || languages.contains(&EnrichmentLanguage::Java);
        if should_register_java {
            if let Some(path) = &jdtls_path {
                let config = JdtlsConfig {
                    jdtls_path: Some(path.clone()),
                    ..Default::default()
                };
                registry.register(Box::new(JdtlsResolver::with_config(config)));
                available_languages.push("java".to_string());
            } else if languages.contains(&EnrichmentLanguage::Java) {
                // User explicitly requested Java but no jdtls path
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::invalid_request(
                        "language 'java' requires jdtls_path parameter or JDTLS_PATH env var"
                    ),
                );
            }
        }

        if available_languages.is_empty() {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::invalid_request("no resolvers available for requested languages"),
            );
        }

        // Emit resolving progress
        if emitter.emit(ProgressDetail {
            phase: "resolving".to_string(),
            current: 0,
            total: 0,
        }).is_err() {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::new(ErrorCode::ProgressDeliveryFailed, "progress delivery failed"),
            );
        }

        // Build config
        let mut config = EnrichmentConfig::new();
        if let Some(n) = limit {
            config = config.with_limit(n);
        }
        if !languages.is_empty() {
            config = config.with_languages(languages);
        }
        if force {
            config = config.with_force();
        }
        if promote {
            config = config.with_promotion();
        }
        if dry_run {
            config = config.with_dry_run();
        }

        // Open fresh storage connection for pipeline (EnrichmentPipeline takes ownership)
        // We acquire a separate connection since the pipeline consumes it.
        // This is safe under the coordinator's refresh lock.
        let storage = match StorageConnection::open(repo_state.db_path()) {
            Ok(s) => s,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, format!("failed to open storage for enrichment: {}", e)),
                );
            }
        };

        // Run enrichment pipeline
        let mut pipeline = EnrichmentPipeline::with_registry(storage, registry);
        let report = match pipeline.run(repo_uid, &snapshot_uid, &config) {
            Ok(r) => r,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, format!("enrichment failed: {}", e)),
                );
            }
        };

        // Emit completion progress
        let _ = emitter.emit(ProgressDetail {
            phase: "complete".to_string(),
            current: 1,
            total: 1,
        });

        // Build result matching CLI EnrichOutput contract exactly:
        // - by_language: Vec<(String, LanguageStats)> serializes as [["rust", {...}], ...]
        // - top_failure_reasons: Vec<(String, usize)> serializes as [["reason", count], ...]
        // - No dry_run field in output (CLI doesn't include it)
        let by_language: Vec<serde_json::Value> = report
            .by_language
            .iter()
            .map(|(lang, stats)| {
                serde_json::json!([
                    format!("{:?}", lang).to_lowercase(),
                    {
                        "eligible": stats.eligible,
                        "enriched": stats.enriched,
                        "failed": stats.failed,
                        "rate": stats.rate,
                    }
                ])
            })
            .collect();

        let top_failure_reasons: Vec<serde_json::Value> = report
            .top_failure_reasons
            .iter()
            .take(10)
            .map(|fc| serde_json::json!([fc.reason, fc.count]))
            .collect();

        let top_types: Vec<serde_json::Value> = report
            .top_types
            .iter()
            .take(10)
            .map(|tc| serde_json::json!({
                "type_name": tc.type_name,
                "is_external": tc.is_external,
                "count": tc.count,
            }))
            .collect();

        let promotion = report.promotion.as_ref().map(|p| serde_json::json!({
            "candidates": p.candidates,
            "promoted": p.promoted,
            "persisted_count": p.persisted_count,
        }));

        DispatchResult::success(
            &request.id,
            serde_json::json!({
                "command": "enrich",
                "repo_uid": repo_uid,
                "snapshot_uid": snapshot_uid,
                "promote": promote,
                "eligible_count": report.eligible_count,
                "enriched_count": report.enriched_count,
                "failed_count": report.failed_count,
                "attempted_persist_count": report.attempted_persist_count(),
                "persisted_count": report.persisted_count.unwrap_or(0),
                "has_storage_discrepancy": report.has_storage_discrepancy(),
                "enrichment_rate": report.enrichment_rate,
                "promotion": promotion,
                "by_language": by_language,
                "top_failure_reasons": top_failure_reasons,
                "top_types": top_types,
                "available_resolvers": available_languages,
            }),
        )
        // Guards drop here: _refresh_guard then _db_write_guard
    }

    // ── Agent services ──────────────────────────────────────────────

    fn handle_orient(&self, request: &Request) -> DispatchResult {
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

        // Parse optional focus
        let focus = Self::get_optional_string_param(&request.params, "focus");

        // Parse optional budget (default: small)
        let budget = match request.params.get("budget").and_then(|v| v.as_str()) {
            None | Some("small") => Budget::Small,
            Some("medium") => Budget::Medium,
            Some("large") => Budget::Large,
            Some(other) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::invalid_request(format!(
                        "invalid budget value: {} (expected small|medium|large)",
                        other
                    )),
                );
            }
        };

        // Acquire read lock
        let _read_guard = repo_state.coordinator.acquire_read();

        // Get wall-clock timestamp for waiver expiry evaluation
        let now = utc_now_iso8601();

        // Call the agent orient use case
        let result = match repo_graph_agent::orient(&repo_state.storage, repo_uid, focus, budget, &now) {
            Ok(r) => r,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                );
            }
        };

        // Apply trust overlay (matches CLI contract)
        let mut output = match serde_json::to_value(&result) {
            Ok(v) => v,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                );
            }
        };

        // Add trust section if degraded (briefing surface pattern)
        if let Ok(Some(snapshot)) = repo_state.storage.get_snapshot(&result.snapshot) {
            if let Some(trust) = compute_trust_overlay_for_snapshot(
                &repo_state.storage, repo_uid, &snapshot, "CALLS+IMPORTS"
            ) {
                if trust.has_degradation() || !trust.caveats.is_empty() {
                    if let serde_json::Value::Object(ref mut map) = output {
                        if let Ok(trust_value) = serde_json::to_value(&trust) {
                            map.insert("trust".to_string(), trust_value);
                        }
                    }
                }
            }
        }

        DispatchResult::success(&request.id, output)
    }

    fn handle_check(&self, request: &Request) -> DispatchResult {
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

        // Get wall-clock timestamp for waiver expiry evaluation
        let now = utc_now_iso8601();

        // Call the agent check use case
        match repo_graph_agent::run_check(&repo_state.storage, repo_uid, &now) {
            Ok(result) => {
                match serde_json::to_value(&result) {
                    Ok(v) => DispatchResult::success(&request.id, v),
                    Err(e) => DispatchResult::error(
                        &request.id,
                        ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                    ),
                }
            }
            Err(e) => DispatchResult::error(
                &request.id,
                ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
            ),
        }
    }

    fn handle_explain(&self, request: &Request) -> DispatchResult {
        let db_path = match Self::get_string_param(&request.params, "db_path") {
            Ok(p) => p,
            Err(e) => return DispatchResult::error(&request.id, e),
        };
        let repo_uid = match Self::get_string_param(&request.params, "repo_uid") {
            Ok(r) => r,
            Err(e) => return DispatchResult::error(&request.id, e),
        };
        let target = match Self::get_string_param(&request.params, "target") {
            Ok(t) => t,
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

        // Parse optional budget (default: medium for explain)
        // CLI contract: explain only accepts medium|large, not small
        let budget = match request.params.get("budget").and_then(|v| v.as_str()) {
            None | Some("medium") => Budget::Medium,
            Some("large") => Budget::Large,
            Some(other) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::invalid_request(format!(
                        "invalid budget value: {} (expected medium|large)",
                        other
                    )),
                );
            }
        };

        // Acquire read lock
        let _read_guard = repo_state.coordinator.acquire_read();

        // Get wall-clock timestamp for waiver expiry evaluation
        let now = utc_now_iso8601();

        // Call the agent explain use case
        let result = match repo_graph_agent::run_explain(&repo_state.storage, repo_uid, target, budget, &now) {
            Ok(r) => r,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                );
            }
        };

        // Apply trust overlay (matches CLI contract)
        let mut output = match serde_json::to_value(&result) {
            Ok(v) => v,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                );
            }
        };

        // Add trust section if degraded (briefing surface pattern)
        if let Ok(Some(snapshot)) = repo_state.storage.get_snapshot(&result.snapshot) {
            if let Some(trust) = compute_trust_overlay_for_snapshot(
                &repo_state.storage, repo_uid, &snapshot, "CALLS+IMPORTS"
            ) {
                if trust.has_degradation() || !trust.caveats.is_empty() {
                    if let serde_json::Value::Object(ref mut map) = output {
                        if let Ok(trust_value) = serde_json::to_value(&trust) {
                            map.insert("trust".to_string(), trust_value);
                        }
                    }
                }
            }
        }

        DispatchResult::success(&request.id, output)
    }
}
