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
use std::time::Instant;

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

use crate::handlers::inventory::classify_retention_only;
use crate::state::{DaemonState, RepoKey};
use crate::util::{compute_storage_root_path, compute_trust_overlay_for_snapshot, utc_now_iso8601};

/// RMAPD-PERF-1: Performance tracing macro.
/// Enabled with `--features perf-trace`. No-op otherwise.
///
/// Build with: `cargo build --features perf-trace`
/// Or for release: `cargo build --release --features perf-trace`
#[cfg(feature = "perf-trace")]
macro_rules! perf_trace {
    ($($arg:tt)*) => {
        eprintln!($($arg)*);
    };
}

#[cfg(not(feature = "perf-trace"))]
macro_rules! perf_trace {
    ($($arg:tt)*) => {};
}

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
        params.get(key).and_then(|v| v.as_str()).ok_or_else(|| {
            ErrorDetail::invalid_request(format!("missing or invalid '{}' parameter", key))
        })
    }

    /// Get an optional string parameter.
    fn get_optional_string_param<'a>(params: &'a Value, key: &str) -> Option<&'a str> {
        params.get(key).and_then(|v| v.as_str())
    }

    /// Parse stable_keys into structured match data for ambiguous symbol errors.
    ///
    /// Stable key format: `{repo_uid}:{file_path}#{qualified_name}:{kind}:{subtype}`
    /// Returns JSON array of match objects for CLI rendering.
    fn parse_ambiguous_matches(stable_keys: &[String]) -> Value {
        let matches: Vec<Value> = stable_keys
            .iter()
            .filter_map(|key| {
                // Split on first # to separate file path from symbol info
                let hash_pos = key.find('#')?;
                let before_hash = &key[..hash_pos];
                let after_hash = &key[hash_pos + 1..];

                // before_hash is "{repo_uid}:{file_path}"
                // Find first : to skip repo_uid
                let colon_pos = before_hash.find(':')?;
                let file_path = &before_hash[colon_pos + 1..];

                // after_hash is "{qualified_name}:{kind}:{subtype}"
                // Split from the end to handle qualified_names with colons
                let parts: Vec<&str> = after_hash.rsplitn(3, ':').collect();
                if parts.len() < 3 {
                    return None;
                }

                let subtype = parts[0];
                let kind = parts[1];
                let qualified_name = parts[2];

                Some(serde_json::json!({
                    "qualified_name": qualified_name,
                    "kind": format!("{}:{}", kind, subtype),
                    "file": file_path
                }))
            })
            .collect();

        Value::Array(matches)
    }

    // ── REG-1 repo resolution ───────────────────────────────────────────
    //
    // Helper for commands that need to resolve a repo reference to loaded state.
    // This is the core REG-1 pattern: CLI sends repo path/alias, daemon resolves
    // and auto-loads, command proceeds with resolved state.

    /// Resolve a `repo` param to a loaded repo state (REG-1 pattern).
    ///
    /// Resolution order:
    /// 1. Try as alias in registry
    /// 2. Try as path in registry (exact match or ancestor)
    /// 3. If not found in registry: error "repo not indexed"
    /// 4. If found but not loaded: auto-load
    /// 5. Return repo state
    ///
    /// The `repo` param can be:
    /// - An alias (e.g., "pmc")
    /// - An absolute path (e.g., "/Users/x/projects/pmc")
    /// - A relative path that will be canonicalized
    fn resolve_and_load_repo(
        &self,
        params: &Value,
    ) -> Result<(std::sync::Arc<crate::state::RepoState>, String), ErrorDetail> {
        let repo_ref = Self::get_string_param(params, "repo")?;

        // Resolve via registry (alias or path)
        let entry = self.state.resolve_alias_or_path(repo_ref).ok_or_else(|| {
            ErrorDetail::new(
                ErrorCode::RepoNotFound,
                format!(
                    "repo not indexed: {} (run: rmap index {})",
                    repo_ref, repo_ref
                ),
            )
        })?;

        let db_path = std::path::Path::new(&entry.db_path);
        let repo_uid = &entry.repo_uid;

        // Auto-load if not already loaded
        let repo_state = self
            .state
            .load_repo(db_path, repo_uid)
            .map_err(|e| ErrorDetail::new(ErrorCode::InternalError, e))?;

        Ok((repo_state, repo_uid.clone()))
    }

    /// Variant of `resolve_and_load_repo` that also returns a human-readable
    /// display name for CLI presentation.
    ///
    /// The display name is derived from:
    /// 1. Registry alias (if present)
    /// 2. Otherwise, basename of the canonical repo path
    ///
    /// Used by CLI-OUT-2B handlers (orient, check, trust, cycles) to populate
    /// the `display_name` field in user-facing response DTOs.
    fn resolve_and_load_repo_with_display_name(
        &self,
        params: &Value,
    ) -> Result<(Arc<crate::state::RepoState>, String, String), ErrorDetail> {
        let repo_ref = Self::get_string_param(params, "repo")?;

        // Resolve via registry (alias or path)
        let entry = self.state.resolve_alias_or_path(repo_ref).ok_or_else(|| {
            ErrorDetail::new(
                ErrorCode::RepoNotFound,
                format!(
                    "repo not indexed: {} (run: rmap index {})",
                    repo_ref, repo_ref
                ),
            )
        })?;

        let db_path = Path::new(&entry.db_path);
        let repo_uid = &entry.repo_uid;

        // Compute display_name: alias if present, else path basename
        let display_name = entry.alias.clone().unwrap_or_else(|| {
            Path::new(&entry.canonical_path)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or(repo_uid)
                .to_string()
        });

        // Auto-load if not already loaded
        let repo_state = self
            .state
            .load_repo(db_path, repo_uid)
            .map_err(|e| ErrorDetail::new(ErrorCode::InternalError, e))?;

        Ok((repo_state, repo_uid.clone(), display_name))
    }
}

impl Dispatcher for ServiceDispatcher {
    fn dispatch(&self, request: &Request, emitter: &mut dyn ProgressEmitter) -> DispatchResult {
        match request.method.as_str() {
            // ── Test methods ────────────────────────────────────────
            "ping" => DispatchResult::success(&request.id, serde_json::json!({"pong": true})),

            "echo" => DispatchResult::success(&request.id, request.params.clone()),

            // ── Daemon management ───────────────────────────────────
            "daemon_info" => self.handle_daemon_info(request),
            "load_repo" => self.handle_load_repo(request),
            "unload_repo" => self.handle_unload_repo(request),
            "list_loaded_repos" => self.handle_list_loaded_repos(request),

            // ── Registry operations (REG-1) ─────────────────────────
            "resolve_repo" => self.handle_resolve_repo(request),
            "list_repos" => self.handle_list_repos(request),
            "repo_info" => self.handle_repo_info(request),
            "repo_alias" => self.handle_repo_alias(request),
            "repo_remove" => self.handle_repo_remove(request),

            // ── Read operations ─────────────────────────────────────
            "callers" => self.handle_callers(request),
            "callees" => self.handle_callees(request),
            "livegraph_preload" => self.handle_livegraph_preload(request),
            "livegraph_refresh" => self.handle_livegraph_refresh(request),
            "imports" => self.handle_imports(request),
            // RMAPD-PERF-1: These operations emit heartbeat for long queries
            "stats" => self.handle_stats(request, emitter),
            "cycles" => self.handle_cycles(request, emitter),
            "path" => self.handle_path(request),

            // ── Agent services ──────────────────────────────────────
            // RMAPD-PERF-1: These operations emit heartbeat for long queries
            "orient" => self.handle_orient(request, emitter),
            "check" => self.handle_check(request, emitter),
            "explain" => self.handle_explain(request),

            // ── Trust and governance ────────────────────────────────
            // RMAPD-PERF-1: trust emits heartbeat for long queries
            "trust" => self.handle_trust(request, emitter),
            "gate" => self.handle_gate(request),

            // ── Quality queries (LEGACY-CONTRACT-MIGRATION-1B) ──────
            // Handlers extracted to handlers/quality.rs
            "churn" => crate::handlers::quality::handle_churn(&self.state, request),
            "hotspots" => crate::handlers::quality::handle_hotspots(&self.state, request),
            "risk" => crate::handlers::quality::handle_risk(&self.state, request),
            "coverage" => crate::handlers::quality::handle_coverage(&self.state, request),

            // ── Governance (LEGACY-CONTRACT-MIGRATION-1C) ────────────
            // Handlers extracted to handlers/governance.rs
            "assess" => crate::handlers::governance::handle_assess(&self.state, request),
            "violations" => crate::handlers::governance::handle_violations(&self.state, request),

            // ── Inventory (LEGACY-CONTRACT-MIGRATION-1D) ─────────────
            // Handler extracted to handlers/inventory.rs
            "policy" => crate::handlers::inventory::handle_policy(&self.state, request),
            // CACHE-SEMANTICS-1: retention classification and baseline management
            "classify_retention" => {
                crate::handlers::inventory::handle_classify_retention(&self.state, request)
            }
            "mark_baseline" => {
                crate::handlers::inventory::handle_mark_baseline(&self.state, request)
            }
            "unmark_baseline" => {
                crate::handlers::inventory::handle_unmark_baseline(&self.state, request)
            }

            // ── Metrics (PERF-OBS-1) ────────────────────────────────
            // Storage performance observability
            "perf" => crate::handlers::metrics::handle_perf(&self.state, request),
            // DEV-INSTALL-DOCTOR-WAIT-1: cheap storage health summary for `rmap doctor` (no per-table
            // scan); distinct from the heavy `perf` diagnostic.
            "storage_health" => {
                crate::handlers::metrics::handle_storage_health(&self.state, request)
            }

            // ── Documentation ───────────────────────────────────────
            "docs_list" => self.handle_docs_list(request),
            "docs_extract" => self.handle_docs_extract(request),

            // ── Resource queries ────────────────────────────────────
            "resource_list" => self.handle_resource_list(request),
            "resource_readers" => self.handle_resource_readers(request),
            "resource_writers" => self.handle_resource_writers(request),

            // ── Contract queries ────────────────────────────────────
            "contracts_list" => self.handle_contracts_list(request),
            "contracts_show" => self.handle_contracts_show(request),
            "contracts_elements" => self.handle_contracts_elements(request),
            "contracts_usages" => self.handle_contracts_usages(request),

            // ── Inference queries ───────────────────────────────────
            "inferences_list" => self.handle_inferences_list(request),

            // ── Dependency queries ──────────────────────────────────
            "deps_list" => self.handle_deps_list(request),
            "deps_why" => self.handle_deps_why(request),
            "deps_drift" => self.handle_deps_drift(request),

            // ── Surfaces queries ────────────────────────────────────
            "surfaces_list" => self.handle_surfaces_list(request),
            "surfaces_show" => self.handle_surfaces_show(request),

            // ── Boundaries queries ──────────────────────────────────
            "boundaries_list" => self.handle_boundaries_list(request),
            "boundaries_show" => self.handle_boundaries_show(request),
            "boundaries_summary" => self.handle_boundaries_summary(request),
            "boundaries_links" => self.handle_boundaries_links(request),

            // ── Modules queries ─────────────────────────────────────
            "modules_files" => self.handle_modules_files(request),
            "modules_deps" => self.handle_modules_deps(request),
            "modules_violations" => self.handle_modules_violations(request),
            "modules_unowned" => self.handle_modules_unowned(request),
            "modules_show" => self.handle_modules_show(request),
            "modules_list" => self.handle_modules_list(request),

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
    /// Return daemon-level diagnostic information.
    ///
    /// STATE-ROOT-SEPARATION-1: Reports state root mode and authority write policy.
    ///
    /// Request: `{"method": "daemon_info", "params": {}}`
    ///
    /// Response:
    /// ```json
    /// {
    ///   "state_root": "/path/to/state",
    ///   "state_root_mode": "global" | "sandbox-local",
    ///   "authority_writes_allowed": true | false
    /// }
    /// ```
    fn handle_daemon_info(&self, request: &Request) -> DispatchResult {
        let state_root = self
            .state
            .registry()
            .state_root()
            .to_string_lossy()
            .to_string();
        let mode = self.state.state_root_mode();

        DispatchResult::success(
            &request.id,
            serde_json::json!({
                "state_root": state_root,
                "state_root_mode": mode.as_str(),
                "authority_writes_allowed": mode.allows_authority_writes()
            }),
        )
    }

    fn handle_load_repo(&self, request: &Request) -> DispatchResult {
        let db_path = match Self::get_string_param(&request.params, "db_path") {
            Ok(p) => p,
            Err(e) => return DispatchResult::error(&request.id, e),
        };
        let repo_uid = match Self::get_string_param(&request.params, "repo_uid") {
            Ok(r) => r,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        match self
            .state
            .load_repo(std::path::Path::new(db_path), repo_uid)
        {
            Ok(_) => DispatchResult::success(&request.id, serde_json::json!({"loaded": repo_uid})),
            Err(e) => {
                DispatchResult::error(&request.id, ErrorDetail::new(ErrorCode::InternalError, e))
            }
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
                return DispatchResult::error(&request.id, ErrorDetail::invalid_request(e));
            }
        };

        let unloaded = self.state.unload_repo_by_key(&key);
        DispatchResult::success(&request.id, serde_json::json!({"unloaded": unloaded}))
    }

    /// List currently loaded repos (daemon operational state).
    ///
    /// Request: `{"method": "list_loaded_repos", "params": {}}`
    ///
    /// This is an operational/debug method. For the registry-backed list
    /// of indexed repos, use `list_repos`.
    fn handle_list_loaded_repos(&self, request: &Request) -> DispatchResult {
        let repos = self.state.list_repos();
        DispatchResult::success(&request.id, serde_json::json!({"loaded_repos": repos}))
    }

    // ── Registry operations (REG-1) ─────────────────────────────────

    /// Resolve a path to a registered repo.
    ///
    /// Request: `{"method": "resolve_repo", "params": {"path": "/abs/path"}}`
    ///
    /// Success response includes full registry entry.
    /// Error response with code "RepoNotIndexed" if not found.
    fn handle_resolve_repo(&self, request: &Request) -> DispatchResult {
        let path_str = match Self::get_string_param(&request.params, "path") {
            Ok(p) => p,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        let path = Path::new(path_str);

        match self.state.resolve_repo_path(path) {
            Some(entry) => DispatchResult::success(
                &request.id,
                serde_json::json!({
                    "canonical_path": entry.canonical_path,
                    "alias": entry.alias,
                    "db_path": entry.db_path,
                    "repo_uid": entry.repo_uid,
                    "last_indexed_at": entry.last_indexed_at,
                    "last_snapshot_uid": entry.last_snapshot_uid,
                }),
            ),
            None => DispatchResult::error(
                &request.id,
                ErrorDetail::new(
                    ErrorCode::RepoNotFound,
                    format!(
                        "repo not indexed: {} (run: rmap index {})",
                        path_str, path_str
                    ),
                ),
            ),
        }
    }

    /// List all registered repos (from registry).
    ///
    /// Request: `{"method": "list_repos", "params": {}}`
    ///
    /// This is the primary repo inventory. Returns all repos known to the
    /// registry, regardless of whether they are currently loaded in memory.
    fn handle_list_repos(&self, request: &Request) -> DispatchResult {
        let registry = self.state.registry();
        let entries: Vec<serde_json::Value> = registry
            .list()
            .iter()
            .map(|entry| {
                serde_json::json!({
                    "canonical_path": entry.canonical_path,
                    "alias": entry.alias,
                    "repo_uid": entry.repo_uid,
                    "last_indexed_at": entry.last_indexed_at,
                })
            })
            .collect();

        DispatchResult::success(&request.id, serde_json::json!({"repos": entries}))
    }

    /// Get info for a specific registered repo.
    ///
    /// Request: `{"method": "repo_info", "params": {"repo": "<alias_or_path>"}}`
    fn handle_repo_info(&self, request: &Request) -> DispatchResult {
        let repo_ref = match Self::get_string_param(&request.params, "repo") {
            Ok(r) => r,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        match self.state.resolve_alias_or_path(repo_ref) {
            Some(entry) => {
                // Check if repo is currently loaded
                let key = match crate::state::RepoKey::new(&entry.db_path, &entry.repo_uid) {
                    Ok(k) => k,
                    Err(_) => {
                        return DispatchResult::success(
                            &request.id,
                            serde_json::json!({
                                "canonical_path": entry.canonical_path,
                                "alias": entry.alias,
                                "db_path": entry.db_path,
                                "repo_uid": entry.repo_uid,
                                "last_indexed_at": entry.last_indexed_at,
                                "last_snapshot_uid": entry.last_snapshot_uid,
                                "loaded": false,
                            }),
                        );
                    }
                };

                let loaded = self.state.get_repo_by_key(&key).is_some();

                DispatchResult::success(
                    &request.id,
                    serde_json::json!({
                        "canonical_path": entry.canonical_path,
                        "alias": entry.alias,
                        "db_path": entry.db_path,
                        "repo_uid": entry.repo_uid,
                        "last_indexed_at": entry.last_indexed_at,
                        "last_snapshot_uid": entry.last_snapshot_uid,
                        "loaded": loaded,
                    }),
                )
            }
            None => DispatchResult::error(
                &request.id,
                ErrorDetail::new(
                    ErrorCode::RepoNotFound,
                    format!("repo not found: {}", repo_ref),
                ),
            ),
        }
    }

    /// Set or change alias for a registered repo.
    ///
    /// # Authority Classification
    ///
    /// This is an A1 (User Authority) write. Blocked in sandbox-local mode.
    ///
    /// Request: `{"method": "repo_alias", "params": {"repo": "<path>", "alias": "<new_alias>"}}`
    fn handle_repo_alias(&self, request: &Request) -> DispatchResult {
        // STATE-ROOT-SEPARATION-1: A1 authority write guard
        if let Err(e) =
            crate::require_global_mode_for_authority_write(&self.state, request, "repo_alias")
        {
            return e;
        }

        let repo_path = match Self::get_string_param(&request.params, "repo") {
            Ok(r) => r,
            Err(e) => return DispatchResult::error(&request.id, e),
        };
        let alias = match Self::get_string_param(&request.params, "alias") {
            Ok(a) => a,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        // First resolve the repo to get canonical path
        let canonical_path = match self.state.resolve_alias_or_path(repo_path) {
            Some(entry) => entry.canonical_path.clone(),
            None => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(
                        ErrorCode::RepoNotFound,
                        format!("repo not found: {}", repo_path),
                    ),
                );
            }
        };

        // Set the alias
        let mut registry = self.state.registry_mut();
        if let Err(e) = registry.set_alias(&canonical_path, alias.to_string()) {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
            );
        }

        // Persist
        if let Err(e) = registry.save() {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::new(
                    ErrorCode::InternalError,
                    format!("failed to save registry: {}", e),
                ),
            );
        }

        DispatchResult::success(
            &request.id,
            serde_json::json!({
                "alias": alias,
                "canonical_path": canonical_path,
            }),
        )
    }

    /// Remove a repo from the registry.
    ///
    /// Request: `{"method": "repo_remove", "params": {"repo": "<alias_or_path>", "delete_db": false}}`
    fn handle_repo_remove(&self, request: &Request) -> DispatchResult {
        let repo_ref = match Self::get_string_param(&request.params, "repo") {
            Ok(r) => r,
            Err(e) => return DispatchResult::error(&request.id, e),
        };
        let delete_db = request
            .params
            .get("delete_db")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        // Resolve the repo to get canonical path and db_path
        let (canonical_path, db_path) = match self.state.resolve_alias_or_path(repo_ref) {
            Some(entry) => (entry.canonical_path.clone(), entry.db_path.clone()),
            None => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(
                        ErrorCode::RepoNotFound,
                        format!("repo not found: {}", repo_ref),
                    ),
                );
            }
        };

        // Remove from registry
        let mut registry = self.state.registry_mut();
        let entry = match registry.remove(&canonical_path) {
            Ok(e) => e,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                );
            }
        };

        // Persist
        if let Err(e) = registry.save() {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::new(
                    ErrorCode::InternalError,
                    format!("failed to save registry: {}", e),
                ),
            );
        }

        // Unload if loaded
        if let Ok(key) = crate::state::RepoKey::new(&entry.db_path, &entry.repo_uid) {
            drop(registry); // Release borrow before unload
            self.state.unload_repo_by_key(&key);
        }

        // Delete database file if requested
        let db_deleted = if delete_db {
            std::fs::remove_file(&db_path).is_ok()
        } else {
            false
        };

        DispatchResult::success(
            &request.id,
            serde_json::json!({
                "removed": true,
                "canonical_path": canonical_path,
                "db_path": db_path,
                "db_deleted": db_deleted,
            }),
        )
    }

    /// LIVEGRAPH-INTEGRATION-1B (dev-only): decode a SUPPLIED `index.scip`, ingest it, and feed the
    /// resulting partition + value facts into the repo's in-memory LiveGraph. The daemon does NOT run
    /// scip-typescript / package discovery / refresh (that is 1C) — it only decodes + ingests the
    /// supplied file. Additive: changes no existing serving behavior.
    fn handle_livegraph_preload(&self, request: &Request) -> DispatchResult {
        let (repo_state, repo_uid) = match self.resolve_and_load_repo(&request.params) {
            Ok(r) => r,
            Err(e) => return DispatchResult::error(&request.id, e),
        };
        let partition_id = match Self::get_string_param(&request.params, "partition_id") {
            Ok(s) => s.to_string(),
            Err(e) => return DispatchResult::error(&request.id, e),
        };
        let scip = match Self::get_string_param(&request.params, "scip") {
            Ok(s) => s.to_string(),
            Err(e) => return DispatchResult::error(&request.id, e),
        };
        let source_root = match Self::get_string_param(&request.params, "source_root") {
            Ok(s) => s.to_string(),
            Err(e) => return DispatchResult::error(&request.id, e),
        };
        // KEY-NAMESPACE-REPO-RELATIVE-1: the partition's repo-relative prefix = source_root relative to the
        // repo path (the `repo` param). Empty for a repo-root package (keys stay doc-relative, byte-stable).
        let repo_path = Self::get_optional_string_param(&request.params, "repo").unwrap_or("");
        let partition_prefix = crate::livegraph_feed::repo_relative_prefix(repo_path, &source_root);
        match crate::livegraph_feed::preload_partition(
            &repo_state,
            &repo_uid,
            &partition_id,
            &scip,
            &source_root,
            &partition_prefix,
        ) {
            Ok(summary) => DispatchResult::success(&request.id, summary),
            Err(e) => {
                DispatchResult::error(&request.id, ErrorDetail::new(ErrorCode::InternalError, e))
            }
        }
    }

    /// LIVEGRAPH-INTEGRATION-1C (steps 2–3, dev-only): discover the SCIP producer (D0) and return a
    /// STRUCTURED result. No background worker / subprocess / SCIP generation yet (step 4). Absent
    /// producer → `ProducerUnavailable` (D6); the LiveGraph last-good + the sqlite default are
    /// untouched. Read-only — this handler changes no runtime state.
    fn handle_livegraph_refresh(&self, request: &Request) -> DispatchResult {
        let (repo_state, repo_uid) = match self.resolve_and_load_repo(&request.params) {
            Ok(r) => r,
            Err(e) => return DispatchResult::error(&request.id, e),
        };
        let repo_path = match Self::get_string_param(&request.params, "repo") {
            Ok(s) => s.to_string(),
            Err(e) => return DispatchResult::error(&request.id, e),
        };
        // IMPORTS-XPART-ENUMERATION-1 (D4): repeated `--source-root` arrives as a `source_roots` array.
        // Present + non-empty -> multi-partition BEST-EFFORT refresh (per-partition + aggregate, D5);
        // absent/empty -> the single-partition path below (byte-stable; 0/1 root preserves behaviour).
        let source_roots: Vec<String> = request
            .params
            .get("source_roots")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        if !source_roots.is_empty() {
            let body = crate::livegraph_refresh::run_refresh_multi(
                &repo_state,
                &repo_uid,
                &repo_path,
                &source_roots,
            );
            return DispatchResult::success(&request.id, body);
        }
        let partition = Self::get_optional_string_param(&request.params, "partition")
            .unwrap_or("default")
            .to_string();
        // 1C step 4: SYNCHRONOUS daemon-owned refresh (producer runs inline; single-threaded daemon,
        // DAEMON-ASYNC-REFRESH-1 is the non-blocking follow-up). Absent producer → structured
        // ProducerUnavailable (steps 2-3 behavior preserved); on any failure the last-good is untouched.
        match crate::livegraph_refresh::run_refresh(
            &repo_state,
            &repo_uid,
            &partition,
            &repo_path,
            "",
        ) {
            Ok(body) => DispatchResult::success(&request.id, body),
            Err(failure) => DispatchResult::success(
                &request.id,
                serde_json::json!({
                    "status": failure.code(),
                    "detail": failure.detail(),
                    "partition": partition,
                    "refreshed": false,
                    "warmed_from_cache": false,
                    "producer_unavailable": false,
                    "value_facts_warmed": false,
                }),
            ),
        }
    }

    fn handle_callers(&self, request: &Request) -> DispatchResult {
        // REG-1: resolve repo from path/alias and auto-load
        let (repo_state, repo_uid) = match self.resolve_and_load_repo(&request.params) {
            Ok(r) => r,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        let symbol = match Self::get_string_param(&request.params, "symbol") {
            Ok(s) => s,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        // Acquire read lock
        let _read_guard = repo_state.coordinator.acquire_read();

        // Get latest snapshot
        let snapshot = match repo_state.storage.get_latest_snapshot(&repo_uid) {
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
        let target = match repo_state
            .storage
            .resolve_symbol(&snapshot.snapshot_uid, symbol)
        {
            Ok(sym) => sym,
            Err(SymbolResolveError::NotFound) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::invalid_request(format!("symbol not found: {}", symbol)),
                );
            }
            Err(SymbolResolveError::Ambiguous(keys)) => {
                let matches = Self::parse_ambiguous_matches(&keys);
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::ambiguous_symbol(symbol, matches),
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

        // LIVEGRAPH-INTEGRATION-1B: engine selector (default sqlite = byte-compatible above).
        let engine = crate::livegraph_feed::Engine::parse(Self::get_optional_string_param(
            &request.params,
            "engine",
        ));
        let repo_root = Self::get_optional_string_param(&request.params, "repo")
            .unwrap_or("")
            .to_string();
        let value = crate::livegraph_feed::callers_engine_response(
            engine,
            &repo_state,
            &target,
            callers,
            symbol,
            &repo_root,
        );
        DispatchResult::success(&request.id, value)
    }

    fn handle_callees(&self, request: &Request) -> DispatchResult {
        // REG-1: resolve repo from path/alias and auto-load
        let (repo_state, repo_uid) = match self.resolve_and_load_repo(&request.params) {
            Ok(r) => r,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        let symbol = match Self::get_string_param(&request.params, "symbol") {
            Ok(s) => s,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        // Acquire read lock
        let _read_guard = repo_state.coordinator.acquire_read();

        // Get latest snapshot
        let snapshot = match repo_state.storage.get_latest_snapshot(&repo_uid) {
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
        let target = match repo_state
            .storage
            .resolve_symbol(&snapshot.snapshot_uid, symbol)
        {
            Ok(sym) => sym,
            Err(SymbolResolveError::NotFound) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::invalid_request(format!("symbol not found: {}", symbol)),
                );
            }
            Err(SymbolResolveError::Ambiguous(keys)) => {
                let matches = Self::parse_ambiguous_matches(&keys);
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::ambiguous_symbol(symbol, matches),
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

        // LIVEGRAPH-INTEGRATION-1B: engine selector (default sqlite = byte-compatible above).
        let engine = crate::livegraph_feed::Engine::parse(Self::get_optional_string_param(
            &request.params,
            "engine",
        ));
        let repo_root = Self::get_optional_string_param(&request.params, "repo")
            .unwrap_or("")
            .to_string();
        let value = crate::livegraph_feed::callees_engine_response(
            engine,
            &repo_state,
            &target,
            callees,
            symbol,
            &repo_root,
        );
        DispatchResult::success(&request.id, value)
    }

    fn handle_imports(&self, request: &Request) -> DispatchResult {
        // REG-1: resolve repo from path/alias and auto-load
        let (repo_state, repo_uid) = match self.resolve_and_load_repo(&request.params) {
            Ok(r) => r,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        let file_path = match Self::get_string_param(&request.params, "file") {
            Ok(f) => f,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        // Acquire read lock
        let _read_guard = repo_state.coordinator.acquire_read();

        // Get latest snapshot
        let snapshot = match repo_state.storage.get_latest_snapshot(&repo_uid) {
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
        match repo_state
            .storage
            .node_exists(&snapshot.snapshot_uid, &file_stable_key)
        {
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
        let imports = match repo_state
            .storage
            .find_imports(&snapshot.snapshot_uid, &file_stable_key)
        {
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

    /// RMAPD-PERF-1: Added emitter for heartbeat during long queries.
    /// CLI-OUT-2C: Added display_name for human renderer.
    #[allow(unused_variables)] // Timing variables used only with perf-trace feature
    fn handle_stats(&self, request: &Request, emitter: &mut dyn ProgressEmitter) -> DispatchResult {
        let handler_start = Instant::now();

        // REG-1: resolve repo from path/alias and auto-load (with display_name for CLI-OUT-2C)
        let resolve_start = Instant::now();
        let (repo_state, repo_uid, display_name) =
            match self.resolve_and_load_repo_with_display_name(&request.params) {
                Ok(r) => r,
                Err(e) => return DispatchResult::error(&request.id, e),
            };
        let resolve_ms = resolve_start.elapsed().as_millis();

        // Acquire read lock
        let lock_start = Instant::now();
        let _read_guard = repo_state.coordinator.acquire_read();
        let lock_ms = lock_start.elapsed().as_millis();

        // Get latest snapshot
        let snapshot_start = Instant::now();
        let snapshot = match repo_state.storage.get_latest_snapshot(&repo_uid) {
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
        let snapshot_ms = snapshot_start.elapsed().as_millis();

        // RMAPD-PERF-1: Emit heartbeat before potentially long query
        let _ = emitter.emit(ProgressDetail {
            phase: "computing_module_stats".to_string(),
            current: 0,
            total: 1,
        });

        // Compute module stats
        let query_start = Instant::now();
        let stats = match repo_state
            .storage
            .compute_module_stats(&snapshot.snapshot_uid)
        {
            Ok(s) => s,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                );
            }
        };
        let query_ms = query_start.elapsed().as_millis();

        let total_ms = handler_start.elapsed().as_millis();

        // RMAPD-PERF-1: Timing instrumentation (enable with --features perf-trace)
        perf_trace!(
            "[PERF] stats: total={}ms resolve={}ms lock={}ms snapshot={}ms query={}ms",
            total_ms,
            resolve_ms,
            lock_ms,
            snapshot_ms,
            query_ms
        );

        DispatchResult::success(
            &request.id,
            serde_json::json!({
                "repo_uid": repo_uid,
                "snapshot_uid": snapshot.snapshot_uid,
                "display_name": display_name,
                "stats": stats,
                "count": stats.len(),
            }),
        )
    }

    /// RMAPD-PERF-1: Added emitter for heartbeat during long queries.
    #[allow(unused_variables)] // Timing variables used only with perf-trace feature
    fn handle_cycles(
        &self,
        request: &Request,
        emitter: &mut dyn ProgressEmitter,
    ) -> DispatchResult {
        let handler_start = Instant::now();

        // REG-1: resolve repo from path/alias and auto-load (with display_name for CLI-OUT-2B)
        let resolve_start = Instant::now();
        let (repo_state, repo_uid, display_name) =
            match self.resolve_and_load_repo_with_display_name(&request.params) {
                Ok(r) => r,
                Err(e) => return DispatchResult::error(&request.id, e),
            };
        let resolve_ms = resolve_start.elapsed().as_millis();

        // Acquire read lock
        let lock_start = Instant::now();
        let _read_guard = repo_state.coordinator.acquire_read();
        let lock_ms = lock_start.elapsed().as_millis();

        // Get latest snapshot
        let snapshot_start = Instant::now();
        let snapshot = match repo_state.storage.get_latest_snapshot(&repo_uid) {
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
        let snapshot_ms = snapshot_start.elapsed().as_millis();

        // CYCLES-LIVEGRAPH-CLI-1: engine/kind routing. Default (no flags / engine=sqlite, no kind) =
        // SQLite MODULE-import cycles (unchanged below). `livegraph` + `file-import` = the LiveGraph
        // captured FILE import-cycle graph (a DIFFERENT question; NO SQLite fallback — D7). The CLI
        // validates combinations; the daemon is defensive and rejects unsupported combos rather than
        // silently computing a different graph (D2/D6).
        let engine = Self::get_optional_string_param(&request.params, "engine").unwrap_or("sqlite");
        let kind = Self::get_optional_string_param(&request.params, "kind").unwrap_or("");
        match (engine, kind) {
            ("livegraph", "file-import") => {
                return DispatchResult::success(
                    &request.id,
                    crate::livegraph_feed::file_import_cycles_response(
                        &repo_state,
                        &repo_uid,
                        &display_name,
                        &snapshot.snapshot_uid,
                    ),
                );
            }
            // MODULE-CYCLES-CLI-1 (D2): LiveGraph directory-aggregated MODULE cycles (no SQLite fallback).
            ("livegraph", "module-import") => {
                return DispatchResult::success(
                    &request.id,
                    crate::livegraph_feed::module_import_cycles_response(
                        &repo_state,
                        &repo_uid,
                        &display_name,
                        &snapshot.snapshot_uid,
                    ),
                );
            }
            // Defensive rejects (the CLI gives the primary user-facing errors).
            ("livegraph", _) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(
                        ErrorCode::InvalidRequest,
                        "--engine livegraph requires --kind file-import or module-import",
                    ),
                );
            }
            ("sqlite", "file-import") => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(
                        ErrorCode::InvalidRequest,
                        "SQLite does not answer captured FILE import cycles; use --engine livegraph --kind file-import",
                    ),
                );
            }
            // MODULE-CYCLES-CLI-1 (D4=A): compare LiveGraph derived MODULE cycles vs SQLite (structural;
            // SQLite primary + classified sidecar). Only MODULE-import — FILE-import has no SQLite peer.
            ("compare", "module-import") => {
                let repo_root = Self::get_optional_string_param(&request.params, "repo")
                    .unwrap_or("")
                    .to_string();
                return match crate::livegraph_feed::module_cycle_compare_response(
                    &repo_state,
                    &repo_uid,
                    &display_name,
                    &snapshot.snapshot_uid,
                    &repo_root,
                ) {
                    Ok(v) => DispatchResult::success(&request.id, v),
                    Err(e) => DispatchResult::error(
                        &request.id,
                        ErrorDetail::new(ErrorCode::InternalError, e),
                    ),
                };
            }
            ("compare", _) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(
                        ErrorCode::InvalidRequest,
                        "--engine compare is only supported with --kind module-import (FILE-import has no SQLite peer graph)",
                    ),
                );
            }
            (_, "file-import") => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(
                        ErrorCode::InvalidRequest,
                        "--kind file-import requires --engine livegraph",
                    ),
                );
            }
            // sqlite / no-kind AND sqlite+module-import (D6: the SQLite MODULE default is the module-import
            // graph) -> the SQLite MODULE-import path below (unchanged).
            _ => {}
        }

        // RMAPD-PERF-1: Emit heartbeat before potentially long Tarjan SCC
        let _ = emitter.emit(ProgressDetail {
            phase: "finding_cycles".to_string(),
            current: 0,
            total: 1,
        });

        // Module-level cycles (default)
        let query_start = Instant::now();
        let cycles = match repo_state
            .storage
            .find_cycles(&snapshot.snapshot_uid, "module")
        {
            Ok(c) => c,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                );
            }
        };
        let query_ms = query_start.elapsed().as_millis();

        let total_ms = handler_start.elapsed().as_millis();

        // RMAPD-PERF-1: Timing instrumentation (enable with --features perf-trace)
        perf_trace!(
            "[PERF] cycles: total={}ms resolve={}ms lock={}ms snapshot={}ms query={}ms",
            total_ms,
            resolve_ms,
            lock_ms,
            snapshot_ms,
            query_ms
        );

        // CLI-OUT-2B: Include display_name for human renderers
        DispatchResult::success(
            &request.id,
            serde_json::json!({
                "repo_uid": repo_uid,
                "display_name": display_name,
                "snapshot_uid": snapshot.snapshot_uid,
                "cycles": cycles,
                "count": cycles.len(),
            }),
        )
    }

    fn handle_path(&self, request: &Request) -> DispatchResult {
        // REG-1: resolve repo from path/alias and auto-load
        let (repo_state, repo_uid) = match self.resolve_and_load_repo(&request.params) {
            Ok(r) => r,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        let from_query = match Self::get_string_param(&request.params, "from") {
            Ok(f) => f,
            Err(e) => return DispatchResult::error(&request.id, e),
        };
        let to_query = match Self::get_string_param(&request.params, "to") {
            Ok(t) => t,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        // Acquire read lock
        let _read_guard = repo_state.coordinator.acquire_read();

        // Get latest snapshot
        let snapshot = match repo_state.storage.get_latest_snapshot(&repo_uid) {
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

        // Resolve symbols
        use repo_graph_storage::queries::SymbolResolveError;

        let from_sym = match repo_state
            .storage
            .resolve_symbol(&snapshot.snapshot_uid, from_query)
        {
            Ok(sym) => sym,
            Err(SymbolResolveError::NotFound) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::invalid_request(format!("symbol not found: {}", from_query)),
                );
            }
            Err(SymbolResolveError::Ambiguous(keys)) => {
                let matches = Self::parse_ambiguous_matches(&keys);
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::ambiguous_symbol(from_query, matches),
                );
            }
            Err(SymbolResolveError::Storage(e)) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                );
            }
        };

        let to_sym = match repo_state
            .storage
            .resolve_symbol(&snapshot.snapshot_uid, to_query)
        {
            Ok(sym) => sym,
            Err(SymbolResolveError::NotFound) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::invalid_request(format!("symbol not found: {}", to_query)),
                );
            }
            Err(SymbolResolveError::Ambiguous(keys)) => {
                let matches = Self::parse_ambiguous_matches(&keys);
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::ambiguous_symbol(to_query, matches),
                );
            }
            Err(SymbolResolveError::Storage(e)) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                );
            }
        };

        // Shortest path: CALLS + IMPORTS, max depth 8
        let path_result = match repo_state.storage.find_shortest_path(
            &snapshot.snapshot_uid,
            &from_sym.stable_key,
            &to_sym.stable_key,
            8,
        ) {
            Ok(r) => r,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                );
            }
        };

        let sqlite_value = serde_json::json!({
            "repo_uid": repo_uid,
            "snapshot_uid": snapshot.snapshot_uid,
            "path": path_result,
            "found": path_result.found,
        });
        // PATH-CYCLES-LIVEGRAPH-1: engine branch. Path default = SQLite (Auto maps to SQLite — path does
        // NOT auto-migrate this slice); --engine livegraph/compare select the LiveGraph BFS path.
        let engine = crate::livegraph_feed::Engine::parse(Self::get_optional_string_param(
            &request.params,
            "engine",
        ));
        let repo_root =
            Self::get_optional_string_param(&request.params, "repo").unwrap_or_default();
        let response = crate::livegraph_feed::path_engine_response(
            engine,
            &repo_state,
            &from_sym.stable_key,
            &to_sym.stable_key,
            &repo_uid,
            &snapshot.snapshot_uid,
            sqlite_value,
            repo_root,
        );
        DispatchResult::success(&request.id, response)
    }

    // ── Write operations ────────────────────────────────────────────

    /// Index a repository (REG-1 contract).
    ///
    /// Request: `{"method": "index", "params": {"repo_path": "...", "alias": "..."}}`
    ///
    /// - `repo_path`: Required. Path to repository root.
    /// - `alias`: Optional. Human-friendly name for the repo.
    /// - `include_roots`: Optional. C/C++ include paths.
    ///
    /// The daemon:
    /// 1. Registers repo in registry (or retrieves existing entry)
    /// 2. Allocates db_path if new
    /// 3. Generates stable repo_uid if new
    /// 4. Indexes the repo
    /// 5. Updates registry with last_indexed_at and last_snapshot_uid
    fn handle_index(&self, request: &Request, emitter: &mut dyn ProgressEmitter) -> DispatchResult {
        let repo_path_str = match Self::get_string_param(&request.params, "repo_path") {
            Ok(p) => p,
            Err(e) => return DispatchResult::error(&request.id, e),
        };
        let alias = Self::get_optional_string_param(&request.params, "alias");

        let repo_path = Path::new(repo_path_str);

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

        // Register in registry (or get existing entry)
        let (canonical_path, db_path, repo_uid) = {
            let mut registry = self.state.registry_mut();
            let entry = if let Some(alias_str) = alias {
                match registry.register_with_alias(repo_path, alias_str.to_string()) {
                    Ok(e) => e,
                    Err(e) => {
                        return DispatchResult::error(
                            &request.id,
                            ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                        );
                    }
                }
            } else {
                match registry.register(repo_path) {
                    Ok(e) => e,
                    Err(e) => {
                        return DispatchResult::error(
                            &request.id,
                            ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                        );
                    }
                }
            };
            (
                entry.canonical_path.clone(),
                entry.db_path.clone(),
                entry.repo_uid.clone(),
            )
        };

        // Acquire DB write coordination (DB file may not exist yet)
        let db_runtime = match self.state.get_or_create_db_runtime_for_new_db(&db_path) {
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
        let storage_root_path = match compute_storage_root_path(&canonical_path, &db_path) {
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
        match index_path_with_progress(
            &canonical_path,
            &db_path,
            &repo_uid,
            &options,
            Some(&mut progress_callback),
        ) {
            Ok(result) => {
                // Update registry with index timestamp
                let now = crate::util::utc_now_iso8601();
                {
                    let mut registry = self.state.registry_mut();
                    if let Err(e) = registry.record_index(
                        &canonical_path,
                        now.clone(),
                        result.snapshot_uid.clone(),
                    ) {
                        eprintln!("warning: failed to update registry: {}", e);
                    }
                    if let Err(e) = registry.save() {
                        eprintln!("warning: failed to save registry: {}", e);
                    }
                }

                // Build response with all summary data
                let mut response = serde_json::json!({
                    "repo_uid": repo_uid,
                    "canonical_path": canonical_path,
                    "db_path": db_path,
                    "snapshot_uid": result.snapshot_uid,
                    "files_total": result.files_total,
                    "nodes_total": result.nodes_total,
                    "edges_total": result.edges_total,
                    "edges_unresolved": result.edges_unresolved,
                });

                // Include contract indexing results if present
                if let Some(ref contracts) = result.contracts {
                    response["contracts"] = serde_json::json!({
                        "schemas_indexed": contracts.schemas_indexed,
                        "elements_indexed": contracts.elements_indexed,
                        "parse_failures": contracts.parse_failures,
                        "storage_error": contracts.storage_error,
                    });
                }

                // Include generated code mapping results if present
                if let Some(ref mappings) = result.generated_code_mappings {
                    response["generated_code_mappings"] = serde_json::json!({
                        "mappings_persisted": mappings.mappings_persisted,
                        "high_confidence_count": mappings.high_confidence_count,
                        "element_query_error": mappings.element_query_error,
                        "symbol_query_error": mappings.symbol_query_error,
                        "storage_error": mappings.storage_error,
                    });
                }

                // Auto-load repo so subsequent queries work immediately
                match self.state.load_repo(&db_path, &repo_uid) {
                    Ok(repo_state) => {
                        // REFRESH-HANG-1: Classify only, do NOT prune on foreground path.
                        // Pruning can take 60+ seconds on large tables.
                        // Use classify_retention_only() for fast foreground classification.
                        // User runs explicit maintenance to prune if needed.
                        match classify_retention_only(&repo_state.storage, &repo_uid) {
                            Ok(lifecycle) => {
                                response["retention"] = serde_json::json!({
                                    "pruned_count": lifecycle.pruned_count,
                                    "prunable_count": lifecycle.prunable_count,
                                    "current": lifecycle.stats.current,
                                    "parent": lifecycle.stats.parent,
                                    "baseline_auto": lifecycle.stats.baseline_auto,
                                    "baseline_user": lifecycle.stats.baseline_user,
                                    "total": lifecycle.stats.total,
                                });
                            }
                            Err(e) => {
                                eprintln!(
                                    "warning: retention classification failed for {}: {}",
                                    repo_uid, e
                                );
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("warning: index succeeded but auto-load failed: {}", e);
                    }
                };

                DispatchResult::success(&request.id, response)
            }
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

    fn handle_refresh(
        &self,
        request: &Request,
        emitter: &mut dyn ProgressEmitter,
    ) -> DispatchResult {
        // REG-1: resolve repo from path/alias and auto-load
        let (repo_state, repo_uid) = match self.resolve_and_load_repo(&request.params) {
            Ok(r) => r,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        // Get db_path from resolved repo state
        let db_path = repo_state.db_path();

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
        let repo_info = match repo_state.storage.get_repo(&RepoRef::Uid(repo_uid.clone())) {
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
        match refresh_path_with_progress(
            &repo_path,
            canonical_db_path,
            &repo_uid,
            &options,
            Some(&mut progress_callback),
        ) {
            Ok(result) => {
                // Build response with all summary data for CLI parity
                let mut response = serde_json::json!({
                    "snapshot_uid": result.snapshot_uid,
                    "files_total": result.files_total,
                    "nodes_total": result.nodes_total,
                    "edges_total": result.edges_total,
                    "edges_unresolved": result.edges_unresolved,
                });

                // Include artifact copy-forward results (refresh-specific)
                if let Some(ref cf) = result.artifact_copy_forward {
                    response["artifact_copy_forward"] = serde_json::json!({
                        "measurements_copied": cf.measurements_copied,
                        "inferences_copied": cf.inferences_copied,
                        "boundary_surfaces_copied": cf.boundary_surfaces_copied,
                        "boundary_channels_copied": cf.boundary_channels_copied,
                        "contract_schemas_copied": cf.contract_schemas_copied,
                        "contract_elements_copied": cf.contract_elements_copied,
                    });
                }

                // Include contract indexing results if present
                if let Some(ref contracts) = result.contracts {
                    response["contracts"] = serde_json::json!({
                        "schemas_indexed": contracts.schemas_indexed,
                        "elements_indexed": contracts.elements_indexed,
                        "parse_failures": contracts.parse_failures,
                        "storage_error": contracts.storage_error,
                    });
                }

                // Include generated code mapping results if present
                if let Some(ref mappings) = result.generated_code_mappings {
                    response["generated_code_mappings"] = serde_json::json!({
                        "mappings_persisted": mappings.mappings_persisted,
                        "high_confidence_count": mappings.high_confidence_count,
                        "element_query_error": mappings.element_query_error,
                        "symbol_query_error": mappings.symbol_query_error,
                        "storage_error": mappings.storage_error,
                    });
                }

                // REFRESH-HANG-1: Classify only, do NOT prune on foreground path.
                // Pruning can take 60+ seconds on large tables.
                // Use classify_retention_only() for fast foreground classification.
                // User runs explicit maintenance to prune if needed.
                match classify_retention_only(&repo_state.storage, &repo_uid) {
                    Ok(lifecycle) => {
                        response["retention"] = serde_json::json!({
                            "pruned_count": lifecycle.pruned_count,
                            "prunable_count": lifecycle.prunable_count,
                            "current": lifecycle.stats.current,
                            "parent": lifecycle.stats.parent,
                            "baseline_auto": lifecycle.stats.baseline_auto,
                            "baseline_user": lifecycle.stats.baseline_user,
                            "total": lifecycle.stats.total,
                        });
                    }
                    Err(e) => {
                        // Non-fatal: log warning but don't fail the refresh
                        eprintln!(
                            "warning: retention classification failed for {}: {}",
                            repo_uid, e
                        );
                    }
                }

                DispatchResult::success(&request.id, response)
            }
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

    fn handle_enrich(
        &self,
        request: &Request,
        emitter: &mut dyn ProgressEmitter,
    ) -> DispatchResult {
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
                return DispatchResult::error(&request.id, ErrorDetail::invalid_request(e));
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
        let dry_run = request
            .params
            .get("dry_run")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let promote = request
            .params
            .get("promote")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let force = request
            .params
            .get("force")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let limit = request
            .params
            .get("limit")
            .and_then(|v| v.as_u64())
            .map(|n| n as usize);
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
                        "typescript" | "ts" | "javascript" | "js" => {
                            Some(EnrichmentLanguage::TypeScript)
                        }
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
        if emitter
            .emit(ProgressDetail {
                phase: "initializing".to_string(),
                current: 0,
                total: 1,
            })
            .is_err()
        {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::new(
                    ErrorCode::ProgressDeliveryFailed,
                    "progress delivery failed",
                ),
            );
        }

        // Build resolver registry
        let mut registry = ResolverRegistry::new();
        let mut available_languages = Vec::new();

        // Register Rust resolver if not filtered out
        let should_register_rust =
            languages.is_empty() || languages.contains(&EnrichmentLanguage::Rust);
        if should_register_rust {
            registry.register(Box::new(RustAnalyzerResolver::new()));
            available_languages.push("rust".to_string());
        }

        // Register TypeScript resolver if not filtered out
        let should_register_typescript =
            languages.is_empty() || languages.contains(&EnrichmentLanguage::TypeScript);
        if should_register_typescript {
            registry.register(Box::new(TsServerResolver::new()));
            available_languages.push("typescript".to_string());
        }

        // Register Java resolver if not filtered out and jdtls available
        let should_register_java =
            languages.is_empty() || languages.contains(&EnrichmentLanguage::Java);
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
                        "language 'java' requires jdtls_path parameter or JDTLS_PATH env var",
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
        if emitter
            .emit(ProgressDetail {
                phase: "resolving".to_string(),
                current: 0,
                total: 0,
            })
            .is_err()
        {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::new(
                    ErrorCode::ProgressDeliveryFailed,
                    "progress delivery failed",
                ),
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
                    ErrorDetail::new(
                        ErrorCode::InternalError,
                        format!("failed to open storage for enrichment: {}", e),
                    ),
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
                    ErrorDetail::new(
                        ErrorCode::InternalError,
                        format!("enrichment failed: {}", e),
                    ),
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
            .map(|tc| {
                serde_json::json!({
                    "type_name": tc.type_name,
                    "is_external": tc.is_external,
                    "count": tc.count,
                })
            })
            .collect();

        let promotion = report.promotion.as_ref().map(|p| {
            serde_json::json!({
                "candidates": p.candidates,
                "promoted": p.promoted,
                "persisted_count": p.persisted_count,
            })
        });

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

    /// RMAPD-PERF-1: Added emitter for heartbeat during long queries.
    #[allow(unused_variables)] // Timing variables used only with perf-trace feature
    fn handle_orient(
        &self,
        request: &Request,
        emitter: &mut dyn ProgressEmitter,
    ) -> DispatchResult {
        let handler_start = Instant::now();

        // REG-1: resolve repo from path/alias and auto-load (with display_name for CLI-OUT-2B)
        let resolve_start = Instant::now();
        let (repo_state, repo_uid, display_name) =
            match self.resolve_and_load_repo_with_display_name(&request.params) {
                Ok(r) => r,
                Err(e) => return DispatchResult::error(&request.id, e),
            };
        let resolve_ms = resolve_start.elapsed().as_millis();

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
        let lock_start = Instant::now();
        let _read_guard = repo_state.coordinator.acquire_read();
        let lock_ms = lock_start.elapsed().as_millis();

        // Get wall-clock timestamp for waiver expiry evaluation
        let now = utc_now_iso8601();

        // RMAPD-PERF-1: Emit heartbeat before potentially long orient computation
        let _ = emitter.emit(ProgressDetail {
            phase: "computing_orient".to_string(),
            current: 0,
            total: 1,
        });

        // Call the agent orient use case
        let orient_start = Instant::now();
        let mut result =
            match repo_graph_agent::orient(&repo_state.storage, &repo_uid, focus, budget, &now) {
                Ok(r) => r,
                Err(e) => {
                    return DispatchResult::error(
                        &request.id,
                        ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                    );
                }
            };
        let orient_ms = orient_start.elapsed().as_millis();

        // CLI-OUT-2B: Inject display_name for human renderers
        result.display_name = Some(display_name);

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

        // RMAPD-PERF-1: Emit heartbeat before trust overlay computation
        let _ = emitter.emit(ProgressDetail {
            phase: "computing_trust_overlay".to_string(),
            current: 0,
            total: 1,
        });

        // Add trust section if degraded (briefing surface pattern)
        let overlay_start = Instant::now();
        if let Ok(Some(snapshot)) = repo_state.storage.get_snapshot(&result.snapshot) {
            if let Some(trust) = compute_trust_overlay_for_snapshot(
                &repo_state.storage,
                &repo_uid,
                &snapshot,
                "CALLS+IMPORTS",
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
        let overlay_ms = overlay_start.elapsed().as_millis();

        let total_ms = handler_start.elapsed().as_millis();

        // RMAPD-PERF-1: Timing instrumentation (enable with --features perf-trace)
        perf_trace!(
            "[PERF] orient: total={}ms resolve={}ms lock={}ms orient={}ms overlay={}ms",
            total_ms,
            resolve_ms,
            lock_ms,
            orient_ms,
            overlay_ms
        );

        DispatchResult::success(&request.id, output)
    }

    /// RMAPD-PERF-1: Added emitter for heartbeat during long queries.
    #[allow(unused_variables)] // Timing variables used only with perf-trace feature
    fn handle_check(&self, request: &Request, emitter: &mut dyn ProgressEmitter) -> DispatchResult {
        let handler_start = Instant::now();

        // REG-1: resolve repo from path/alias and auto-load (with display_name for CLI-OUT-2B)
        let resolve_start = Instant::now();
        let (repo_state, repo_uid, display_name) =
            match self.resolve_and_load_repo_with_display_name(&request.params) {
                Ok(r) => r,
                Err(e) => return DispatchResult::error(&request.id, e),
            };
        let resolve_ms = resolve_start.elapsed().as_millis();

        // Acquire read lock
        let lock_start = Instant::now();
        let _read_guard = repo_state.coordinator.acquire_read();
        let lock_ms = lock_start.elapsed().as_millis();

        // Get wall-clock timestamp for waiver expiry evaluation
        let now = utc_now_iso8601();

        // RMAPD-PERF-1: Emit heartbeat before potentially long check computation
        let _ = emitter.emit(ProgressDetail {
            phase: "running_check".to_string(),
            current: 0,
            total: 1,
        });

        // Call the agent check use case
        let check_start = Instant::now();
        let check_result = repo_graph_agent::run_check(&repo_state.storage, &repo_uid, &now);
        let check_ms = check_start.elapsed().as_millis();

        let total_ms = handler_start.elapsed().as_millis();

        // RMAPD-PERF-1: Timing instrumentation (enable with --features perf-trace)
        perf_trace!(
            "[PERF] check: total={}ms resolve={}ms lock={}ms check={}ms",
            total_ms,
            resolve_ms,
            lock_ms,
            check_ms
        );

        match check_result {
            Ok(mut result) => {
                // CLI-OUT-2B: Inject display_name for human renderers
                result.display_name = Some(display_name);
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
        // REG-1: resolve repo from path/alias and auto-load (with display_name for CLI-OUT-3)
        let (repo_state, repo_uid, display_name) =
            match self.resolve_and_load_repo_with_display_name(&request.params) {
                Ok(r) => r,
                Err(e) => return DispatchResult::error(&request.id, e),
            };

        let target = match Self::get_string_param(&request.params, "target") {
            Ok(t) => t,
            Err(e) => return DispatchResult::error(&request.id, e),
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
        let mut result = match repo_graph_agent::run_explain(
            &repo_state.storage,
            &repo_uid,
            target,
            budget,
            &now,
        ) {
            Ok(r) => r,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                );
            }
        };

        // CLI-OUT-3: Inject display_name for human renderers (explain deferred to CLI-OUT-3)
        result.display_name = Some(display_name);

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
                &repo_state.storage,
                &repo_uid,
                &snapshot,
                "CALLS+IMPORTS",
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

    // ── Trust and governance handlers ───────────────────────────────

    /// RMAPD-PERF-1: Added emitter for heartbeat during long queries.
    #[allow(unused_variables)] // Timing variables used only with perf-trace feature
    fn handle_trust(&self, request: &Request, emitter: &mut dyn ProgressEmitter) -> DispatchResult {
        let handler_start = Instant::now();

        // REG-1: resolve repo from path/alias and auto-load (with display_name for CLI-OUT-2B)
        let resolve_start = Instant::now();
        let (repo_state, repo_uid, display_name) =
            match self.resolve_and_load_repo_with_display_name(&request.params) {
                Ok(r) => r,
                Err(e) => return DispatchResult::error(&request.id, e),
            };
        let resolve_ms = resolve_start.elapsed().as_millis();

        // Acquire read lock
        let lock_start = Instant::now();
        let _read_guard = repo_state.coordinator.acquire_read();
        let lock_ms = lock_start.elapsed().as_millis();

        // Get latest snapshot
        let snapshot_start = Instant::now();
        let snapshot = match repo_state.storage.get_latest_snapshot(&repo_uid) {
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
        let snapshot_ms = snapshot_start.elapsed().as_millis();

        if snapshot.status != "ready" {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::new(
                    ErrorCode::SnapshotNotFound,
                    format!("latest snapshot is not ready (status: {})", snapshot.status),
                ),
            );
        }

        // RMAPD-PERF-1: Emit heartbeat before potentially long trust computation
        let _ = emitter.emit(ProgressDetail {
            phase: "assembling_trust_report".to_string(),
            current: 0,
            total: 1,
        });

        // Compute trust report
        let trust_start = Instant::now();
        use repo_graph_trust::service::assemble_trust_report;
        let mut report = match assemble_trust_report(
            &repo_state.storage,
            &repo_uid,
            &snapshot.snapshot_uid,
            snapshot.basis_commit.as_deref(),
            snapshot.toolchain_json.as_deref(),
        ) {
            Ok(r) => r,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                );
            }
        };
        let trust_ms = trust_start.elapsed().as_millis();

        // CLI-OUT-2B: Inject display_name for human renderers
        report.display_name = Some(display_name);

        let total_ms = handler_start.elapsed().as_millis();

        // RMAPD-PERF-1: Timing instrumentation (enable with --features perf-trace)
        perf_trace!(
            "[PERF] trust: total={}ms resolve={}ms lock={}ms snapshot={}ms trust={}ms",
            total_ms,
            resolve_ms,
            lock_ms,
            snapshot_ms,
            trust_ms
        );

        match serde_json::to_value(&report) {
            Ok(v) => DispatchResult::success(&request.id, v),
            Err(e) => DispatchResult::error(
                &request.id,
                ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
            ),
        }
    }

    fn handle_gate(&self, request: &Request) -> DispatchResult {
        // REG-1: resolve repo from path/alias and auto-load
        let (repo_state, repo_uid) = match self.resolve_and_load_repo(&request.params) {
            Ok(r) => r,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        // Parse mode flag: strict (default) or advisory
        let strict = match request.params.get("mode").and_then(|v| v.as_str()) {
            None | Some("strict") => true,
            Some("advisory") => false,
            Some(other) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::invalid_request(format!(
                        "invalid mode value: {} (expected strict|advisory)",
                        other
                    )),
                );
            }
        };

        // Acquire read lock
        let _read_guard = repo_state.coordinator.acquire_read();

        // Get latest snapshot
        let snapshot = match repo_state.storage.get_latest_snapshot(&repo_uid) {
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

        if snapshot.status != "ready" {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::new(
                    ErrorCode::SnapshotNotFound,
                    format!("latest snapshot is not ready (status: {})", snapshot.status),
                ),
            );
        }

        // Get wall-clock timestamp for waiver expiry evaluation
        let now = utc_now_iso8601();

        // Determine gate mode
        let mode = if strict {
            repo_graph_gate::GateMode::Strict
        } else {
            repo_graph_gate::GateMode::Advisory
        };

        // Evaluate gate
        let report = match repo_graph_gate::assemble(
            &repo_state.storage,
            &repo_uid,
            &snapshot.snapshot_uid,
            mode,
            &now,
        ) {
            Ok(r) => r,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                );
            }
        };

        // Get repo name for the report
        use repo_graph_storage::types::RepoRef;
        let repo_name = repo_state
            .storage
            .get_repo(&RepoRef::Uid(repo_uid.clone()))
            .ok()
            .flatten()
            .map(|r| r.name)
            .unwrap_or_else(|| repo_uid.clone());

        // Toolchain metadata from snapshot (may be null)
        let toolchain: serde_json::Value = snapshot
            .toolchain_json
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or(serde_json::Value::Null);

        // Gate report JSON (TS-compatible shape)
        let response = serde_json::json!({
            "command": "gate",
            "repo": repo_name,
            "snapshot": snapshot.snapshot_uid,
            "toolchain": toolchain,
            "obligations": report.obligations,
            "gate": report.outcome,
        });

        DispatchResult::success(&request.id, response)
    }

    // ── Quality handlers ─────────────────────────────────────────────────
    // LEGACY-CONTRACT-MIGRATION-1B: Handlers extracted to handlers/quality.rs
    // Dispatch wiring is in the match statement above.

    // ── Documentation handlers ──────────────────────────────────────

    /// List documentation inventory (live filesystem discovery).
    ///
    /// Request: `{"method": "docs_list", "params": {"repo": "<path_or_alias>"}}`
    fn handle_docs_list(&self, request: &Request) -> DispatchResult {
        // REG-1: resolve repo from path/alias and auto-load
        let (repo_state, repo_uid) = match self.resolve_and_load_repo(&request.params) {
            Ok(r) => r,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        // Acquire read lock
        let _read_guard = repo_state.coordinator.acquire_read();

        // Get repo to find root_path
        let repo = match repo_state.storage.get_repo(&RepoRef::Uid(repo_uid.clone())) {
            Ok(Some(r)) => r,
            Ok(None) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(
                        ErrorCode::RepoNotFound,
                        format!("repo '{}' not found", repo_uid),
                    ),
                );
            }
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                );
            }
        };

        // Resolve repo root relative to DB location
        let db_path = repo_state.db_path();
        let repo_path = db_path
            .parent()
            .map(|p| p.join(&repo.root_path))
            .unwrap_or_else(|| Path::new(&repo.root_path).to_path_buf());

        if !repo_path.is_dir() {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::invalid_request(format!(
                    "repo root does not exist: {}",
                    repo_path.display()
                )),
            );
        }

        // Discover documentation inventory (live filesystem, not semantic_facts)
        let inventory = match repo_graph_doc_facts::discover_doc_inventory(&repo_path, true) {
            Ok(r) => r,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, format!("discovery failed: {}", e)),
                );
            }
        };

        DispatchResult::success(
            &request.id,
            serde_json::json!({
                "command": "docs list",
                "repo": repo_uid,
                "repo_path": repo.root_path,
                "entries": inventory.entries,
                "count": inventory.entries.len(),
                "counts_by_kind": inventory.counts_by_kind,
                "generated_count": inventory.generated_count
            }),
        )
    }

    /// Extract semantic facts from documentation (write operation).
    ///
    /// Request: `{"method": "docs_extract", "params": {"repo": "<path_or_alias>"}}`
    fn handle_docs_extract(&self, request: &Request) -> DispatchResult {
        // REG-1: resolve repo from path/alias and auto-load
        let (repo_state, repo_uid) = match self.resolve_and_load_repo(&request.params) {
            Ok(r) => r,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        // Get db_path for coordination
        let db_path = repo_state.db_path().to_path_buf();

        // Acquire DB write coordination first (semantic_facts is a write)
        let db_runtime = match self.state.get_or_create_db_runtime(&db_path) {
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

        // Get repo to find root_path
        let repo = match repo_state.storage.get_repo(&RepoRef::Uid(repo_uid.clone())) {
            Ok(Some(r)) => r,
            Ok(None) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(
                        ErrorCode::RepoNotFound,
                        format!("repo '{}' not found", repo_uid),
                    ),
                );
            }
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                );
            }
        };

        // Resolve repo root relative to DB location
        let repo_path = db_path
            .parent()
            .map(|p| p.join(&repo.root_path))
            .unwrap_or_else(|| Path::new(&repo.root_path).to_path_buf());

        if !repo_path.is_dir() {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::invalid_request(format!(
                    "repo root does not exist: {}",
                    repo_path.display()
                )),
            );
        }

        // Extract semantic facts from documentation
        let extraction_result = match repo_graph_doc_facts::extract_semantic_facts(&repo_path) {
            Ok(r) => r,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(
                        ErrorCode::InternalError,
                        format!("extraction failed: {}", e),
                    ),
                );
            }
        };

        // Open fresh storage connection for write (under coordination)
        let mut storage = match StorageConnection::open(&db_path) {
            Ok(s) => s,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(
                        ErrorCode::InternalError,
                        format!("storage open failed: {}", e),
                    ),
                );
            }
        };

        // Map ExtractedFact to NewSemanticFact
        let new_facts: Vec<repo_graph_storage::crud::semantic_facts::NewSemanticFact> =
            extraction_result
                .facts
                .iter()
                .map(|f| map_extracted_to_storage(&repo_uid, f))
                .collect();

        // Replace facts in storage atomically
        let replace_result = match storage.replace_semantic_facts_for_repo(&repo_uid, &new_facts) {
            Ok(r) => r,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, format!("storage failed: {}", e)),
                );
            }
        };

        // Build counts by fact kind
        let mut counts_by_kind: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for fact in &extraction_result.facts {
            *counts_by_kind
                .entry(fact.fact_kind.as_str().to_string())
                .or_insert(0) += 1;
        }

        // Build files by kind
        let mut files_by_kind: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for (kind, count) in &extraction_result.files_by_kind {
            files_by_kind.insert(kind.as_str().to_string(), *count);
        }

        DispatchResult::success(
            &request.id,
            serde_json::json!({
                "command": "docs extract",
                "repo": repo_uid,
                "repo_path": repo.root_path,
                "files_scanned": extraction_result.files_scanned,
                "files_by_kind": files_by_kind,
                "facts_extracted": extraction_result.facts.len(),
                "facts_inserted": replace_result.inserted,
                "facts_deleted": replace_result.deleted,
                "counts_by_kind": counts_by_kind,
                "generated_docs_count": extraction_result.generated_docs_count,
                "warnings": extraction_result.warnings.iter()
                    .map(|w| serde_json::json!({
                        "file": w.file,
                        "message": w.message
                    }))
                    .collect::<Vec<_>>()
            }),
        )
    }

    // ── Resource handlers ───────────────────────────────────────────

    /// List all resources in the repo.
    ///
    /// Request: `{"method": "resource_list", "params": {"repo": "<path_or_alias>", "kind": "<optional>"}}`
    fn handle_resource_list(&self, request: &Request) -> DispatchResult {
        // REG-1: resolve repo from path/alias and auto-load
        let (repo_state, repo_uid) = match self.resolve_and_load_repo(&request.params) {
            Ok(r) => r,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        // Parse optional kind filter
        let kind_filter = Self::get_optional_string_param(&request.params, "kind");

        // Acquire read lock
        let _read_guard = repo_state.coordinator.acquire_read();

        // Get latest snapshot
        let snapshot = match repo_state.storage.get_latest_snapshot(&repo_uid) {
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

        // List resources
        let resources = match repo_state
            .storage
            .list_resources(&snapshot.snapshot_uid, kind_filter)
        {
            Ok(r) => r,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                );
            }
        };

        // Compute summary counts
        let total_reads: i64 = resources.iter().map(|r| r.readers).sum();
        let total_writes: i64 = resources.iter().map(|r| r.writers).sum();
        let count = resources.len();

        let mut response = serde_json::json!({
            "command": "resource list",
            "repo": repo_uid,
            "snapshot": snapshot.snapshot_uid,
            "results": resources,
            "count": count,
            "total_reads": total_reads,
            "total_writes": total_writes,
        });

        if let Some(k) = kind_filter {
            if let serde_json::Value::Object(ref mut map) = response {
                map.insert("filter_kind".to_string(), serde_json::json!(k));
            }
        }

        DispatchResult::success(&request.id, response)
    }

    /// Find readers of a resource.
    ///
    /// Request: `{"method": "resource_readers", "params": {"repo": "<path_or_alias>", "resource": "<stable_key>"}}`
    fn handle_resource_readers(&self, request: &Request) -> DispatchResult {
        // REG-1: resolve repo from path/alias and auto-load
        let (repo_state, repo_uid) = match self.resolve_and_load_repo(&request.params) {
            Ok(r) => r,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        let resource_key = match Self::get_string_param(&request.params, "resource") {
            Ok(r) => r,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        // Acquire read lock
        let _read_guard = repo_state.coordinator.acquire_read();

        // Get latest snapshot
        let snapshot = match repo_state.storage.get_latest_snapshot(&repo_uid) {
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

        // Resolve resource
        use repo_graph_storage::queries::ResourceResolveError;
        let target = match repo_state
            .storage
            .resolve_resource(&snapshot.snapshot_uid, resource_key)
        {
            Ok(r) => r,
            Err(ResourceResolveError::NotFound) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::invalid_request(format!("resource not found: {}", resource_key)),
                );
            }
            Err(ResourceResolveError::NotAResource(kind)) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::invalid_request(format!(
                        "'{}' is not a resource node (kind: {})",
                        resource_key, kind
                    )),
                );
            }
            Err(ResourceResolveError::Storage(e)) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                );
            }
        };

        // Find readers
        let readers = match repo_state
            .storage
            .find_resource_readers(&snapshot.snapshot_uid, &target.stable_key)
        {
            Ok(r) => r,
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
                "command": "resource readers",
                "repo": repo_uid,
                "snapshot": snapshot.snapshot_uid,
                "target": target.stable_key,
                "results": readers,
                "count": readers.len(),
            }),
        )
    }

    /// Find writers of a resource.
    ///
    /// Request: `{"method": "resource_writers", "params": {"repo": "<path_or_alias>", "resource": "<stable_key>"}}`
    fn handle_resource_writers(&self, request: &Request) -> DispatchResult {
        // REG-1: resolve repo from path/alias and auto-load
        let (repo_state, repo_uid) = match self.resolve_and_load_repo(&request.params) {
            Ok(r) => r,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        let resource_key = match Self::get_string_param(&request.params, "resource") {
            Ok(r) => r,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        // Acquire read lock
        let _read_guard = repo_state.coordinator.acquire_read();

        // Get latest snapshot
        let snapshot = match repo_state.storage.get_latest_snapshot(&repo_uid) {
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

        // Resolve resource
        use repo_graph_storage::queries::ResourceResolveError;
        let target = match repo_state
            .storage
            .resolve_resource(&snapshot.snapshot_uid, resource_key)
        {
            Ok(r) => r,
            Err(ResourceResolveError::NotFound) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::invalid_request(format!("resource not found: {}", resource_key)),
                );
            }
            Err(ResourceResolveError::NotAResource(kind)) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::invalid_request(format!(
                        "'{}' is not a resource node (kind: {})",
                        resource_key, kind
                    )),
                );
            }
            Err(ResourceResolveError::Storage(e)) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                );
            }
        };

        // Find writers
        let writers = match repo_state
            .storage
            .find_resource_writers(&snapshot.snapshot_uid, &target.stable_key)
        {
            Ok(w) => w,
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
                "command": "resource writers",
                "repo": repo_uid,
                "snapshot": snapshot.snapshot_uid,
                "target": target.stable_key,
                "results": writers,
                "count": writers.len(),
            }),
        )
    }

    // ── Contract handlers ───────────────────────────────────────────

    /// List contract schemas.
    ///
    /// Request: `{"method": "contracts_list", "params": {"repo": "<path_or_alias>", "kind": "<optional>"}}`
    fn handle_contracts_list(&self, request: &Request) -> DispatchResult {
        // REG-1: resolve repo from path/alias and auto-load
        let (repo_state, repo_uid) = match self.resolve_and_load_repo(&request.params) {
            Ok(r) => r,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        // Parse optional kind filter
        let kind_filter = Self::get_optional_string_param(&request.params, "kind");

        // Acquire read lock
        let _read_guard = repo_state.coordinator.acquire_read();

        // Get latest snapshot
        let snapshot = match repo_state.storage.get_latest_snapshot(&repo_uid) {
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

        // Query schemas
        use repo_graph_storage::contract_schema_port::ContractSchemaStoragePort;
        let schemas = match repo_state
            .storage
            .list_contract_schemas(&snapshot.snapshot_uid, kind_filter)
        {
            Ok(s) => s,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                );
            }
        };

        // Map to JSON (ContractSchemaRow doesn't implement Serialize)
        let results: Vec<serde_json::Value> = schemas
            .iter()
            .map(|s| {
                serde_json::json!({
                    "schema_uid": s.schema_uid,
                    "file_path": s.file_path,
                    "schema_kind": s.schema_kind,
                    "package_name": s.package_name,
                    "syntax_version": s.syntax_version,
                    "parsed_at": s.parsed_at,
                })
            })
            .collect();

        let count = results.len();
        let mut response = serde_json::json!({
            "command": "contracts list",
            "repo": repo_uid,
            "snapshot": snapshot.snapshot_uid,
            "results": results,
            "count": count,
        });

        if let Some(k) = kind_filter {
            if let serde_json::Value::Object(ref mut map) = response {
                map.insert("filter_kind".to_string(), serde_json::json!(k));
            }
        }

        DispatchResult::success(&request.id, response)
    }

    /// Show a contract schema with its elements.
    ///
    /// Request: `{"method": "contracts_show", "params": {"repo": "<path_or_alias>", "file": "<file_path>"}}`
    fn handle_contracts_show(&self, request: &Request) -> DispatchResult {
        // REG-1: resolve repo from path/alias and auto-load
        let (repo_state, repo_uid) = match self.resolve_and_load_repo(&request.params) {
            Ok(r) => r,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        let file_path = match Self::get_string_param(&request.params, "file") {
            Ok(f) => f,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        // Acquire read lock
        let _read_guard = repo_state.coordinator.acquire_read();

        // Get latest snapshot
        let snapshot = match repo_state.storage.get_latest_snapshot(&repo_uid) {
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

        // Query schema by file path
        use repo_graph_storage::contract_schema_port::ContractSchemaStoragePort;
        let schema = match repo_state
            .storage
            .get_schema_by_file(&snapshot.snapshot_uid, file_path)
        {
            Ok(Some(s)) => s,
            Ok(None) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::invalid_request(format!("schema not found: {}", file_path)),
                );
            }
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                );
            }
        };

        // Query elements for this schema
        let elements = match repo_state
            .storage
            .list_elements_for_schema(&schema.schema_uid, None)
        {
            Ok(e) => e,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                );
            }
        };

        // Map elements to JSON (ContractElementRow doesn't implement Serialize)
        let element_entries: Vec<serde_json::Value> = elements
            .iter()
            .map(|e| {
                serde_json::json!({
                    "element_uid": e.element_uid,
                    "element_kind": e.element_kind,
                    "name": e.name,
                    "full_name": e.full_name,
                    "parent_element_uid": e.parent_element_uid,
                    "line_start": e.line_start,
                    "line_end": e.line_end,
                    "metadata": e.metadata_json.as_ref().and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok()),
                })
            })
            .collect();

        // Build detail with elements
        let detail = serde_json::json!({
            "schema_uid": schema.schema_uid,
            "file_path": schema.file_path,
            "schema_kind": schema.schema_kind,
            "package_name": schema.package_name,
            "syntax_version": schema.syntax_version,
            "content_hash": schema.content_hash,
            "extractor": schema.extractor,
            "parsed_at": schema.parsed_at,
            "elements": element_entries,
        });

        DispatchResult::success(
            &request.id,
            serde_json::json!({
                "command": "contracts show",
                "repo": repo_uid,
                "snapshot": snapshot.snapshot_uid,
                "results": detail,
                "count": 1,
            }),
        )
    }

    /// List contract elements.
    ///
    /// Request: `{"method": "contracts_elements", "params": {"repo": "<path_or_alias>", "kind": "<optional>", "file": "<optional>"}}`
    fn handle_contracts_elements(&self, request: &Request) -> DispatchResult {
        // REG-1: resolve repo from path/alias and auto-load
        let (repo_state, repo_uid) = match self.resolve_and_load_repo(&request.params) {
            Ok(r) => r,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        // Parse optional filters
        let kind_filter = Self::get_optional_string_param(&request.params, "kind");
        let file_filter = Self::get_optional_string_param(&request.params, "file");

        // Acquire read lock
        let _read_guard = repo_state.coordinator.acquire_read();

        // Get latest snapshot
        let snapshot = match repo_state.storage.get_latest_snapshot(&repo_uid) {
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

        use repo_graph_storage::contract_schema_port::ContractSchemaStoragePort;

        // Get schemas (optionally filtered by file)
        let schemas = match file_filter {
            Some(path) => match repo_state
                .storage
                .get_schema_by_file(&snapshot.snapshot_uid, path)
            {
                Ok(Some(s)) => vec![s],
                Ok(None) => {
                    return DispatchResult::error(
                        &request.id,
                        ErrorDetail::invalid_request(format!("schema not found: {}", path)),
                    );
                }
                Err(e) => {
                    return DispatchResult::error(
                        &request.id,
                        ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                    );
                }
            },
            None => match repo_state
                .storage
                .list_contract_schemas(&snapshot.snapshot_uid, None)
            {
                Ok(s) => s,
                Err(e) => {
                    return DispatchResult::error(
                        &request.id,
                        ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                    );
                }
            },
        };

        // Collect elements from all schemas
        let mut results: Vec<serde_json::Value> = Vec::new();
        for schema in &schemas {
            let elements = match repo_state
                .storage
                .list_elements_for_schema(&schema.schema_uid, kind_filter)
            {
                Ok(e) => e,
                Err(e) => {
                    return DispatchResult::error(
                        &request.id,
                        ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                    );
                }
            };

            for elem in elements {
                results.push(serde_json::json!({
                    "element_uid": elem.element_uid,
                    "schema_uid": schema.schema_uid,
                    "file_path": schema.file_path,
                    "element_kind": elem.element_kind,
                    "name": elem.name,
                    "full_name": elem.full_name,
                    "line_start": elem.line_start,
                }));
            }
        }

        let count = results.len();
        let mut response = serde_json::json!({
            "command": "contracts elements",
            "repo": repo_uid,
            "snapshot": snapshot.snapshot_uid,
            "results": results,
            "count": count,
        });

        if let Some(k) = kind_filter {
            if let serde_json::Value::Object(ref mut map) = response {
                map.insert("filter_kind".to_string(), serde_json::json!(k));
            }
        }
        if let Some(f) = file_filter {
            if let serde_json::Value::Object(ref mut map) = response {
                map.insert("filter_file".to_string(), serde_json::json!(f));
            }
        }

        DispatchResult::success(&request.id, response)
    }

    /// List generated code mappings (contract usages).
    ///
    /// Request: `{"method": "contracts_usages", "params": {"repo": "<path_or_alias>", "element": "<optional>", "min_confidence": <optional>}}`
    fn handle_contracts_usages(&self, request: &Request) -> DispatchResult {
        // REG-1: resolve repo from path/alias and auto-load
        let (repo_state, repo_uid) = match self.resolve_and_load_repo(&request.params) {
            Ok(r) => r,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        // Parse optional filters
        let element_filter = Self::get_optional_string_param(&request.params, "element");
        let min_confidence: f64 = request
            .params
            .get("min_confidence")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);

        // Acquire read lock
        let _read_guard = repo_state.coordinator.acquire_read();

        // Get latest snapshot
        let snapshot = match repo_state.storage.get_latest_snapshot(&repo_uid) {
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

        use repo_graph_storage::contract_schema_port::ContractSchemaStoragePort;

        // Query mappings
        let mappings = match repo_state
            .storage
            .list_generated_code_mappings(&snapshot.snapshot_uid, element_filter)
        {
            Ok(m) => m,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                );
            }
        };

        // Filter by min confidence and build results
        let results: Vec<serde_json::Value> = mappings
            .into_iter()
            .filter(|m| m.confidence >= min_confidence)
            .map(|m| {
                serde_json::json!({
                    "mapping_uid": m.mapping_uid,
                    "schema_element_uid": m.schema_element_uid,
                    "generated_symbol_key": m.generated_symbol_key,
                    "language": m.language,
                    "generated_file": m.generated_file,
                    "mapping_basis": m.mapping_basis,
                    "confidence": m.confidence,
                    "evidence": m.metadata_json.and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok()),
                })
            })
            .collect();

        let count = results.len();
        let mut response = serde_json::json!({
            "command": "contracts usages",
            "repo": repo_uid,
            "snapshot": snapshot.snapshot_uid,
            "results": results,
            "count": count,
        });

        if let Some(e) = element_filter {
            if let serde_json::Value::Object(ref mut map) = response {
                map.insert("filter_element".to_string(), serde_json::json!(e));
            }
        }
        if min_confidence > 0.0 {
            if let serde_json::Value::Object(ref mut map) = response {
                map.insert(
                    "filter_min_confidence".to_string(),
                    serde_json::json!(min_confidence),
                );
            }
        }

        DispatchResult::success(&request.id, response)
    }

    // ── Inference handlers ──────────────────────────────────────────

    /// List inferences for a repo.
    ///
    /// Request: `{"method": "inferences_list", "params": {"repo": "<path_or_alias>", "kind": "<optional>"}}`
    fn handle_inferences_list(&self, request: &Request) -> DispatchResult {
        // REG-1: resolve repo from path/alias and auto-load
        let (repo_state, repo_uid) = match self.resolve_and_load_repo(&request.params) {
            Ok(r) => r,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        // Parse optional kind filter
        let kind_filter = Self::get_optional_string_param(&request.params, "kind");

        // Acquire read lock
        let _read_guard = repo_state.coordinator.acquire_read();

        // Get latest snapshot
        let snapshot = match repo_state.storage.get_latest_snapshot(&repo_uid) {
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

        // Load inferences
        let inferences = match repo_state
            .storage
            .list_inferences_for_snapshot(&snapshot.snapshot_uid, kind_filter)
        {
            Ok(i) => i,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                );
            }
        };

        // Map to JSON output
        let results: Vec<serde_json::Value> = inferences
            .into_iter()
            .map(|i| {
                serde_json::json!({
                    "inference_uid": i.inference_uid,
                    "target_stable_key": i.target_stable_key,
                    "kind": i.kind,
                    "value": serde_json::from_str::<serde_json::Value>(&i.value_json).ok(),
                    "confidence": i.confidence,
                    "extractor": i.extractor,
                    "created_at": i.created_at,
                })
            })
            .collect();

        let count = results.len();
        let mut response = serde_json::json!({
            "command": "inferences list",
            "repo": repo_uid,
            "snapshot": snapshot.snapshot_uid,
            "results": results,
            "count": count,
        });

        if let Some(k) = kind_filter {
            if let serde_json::Value::Object(ref mut map) = response {
                map.insert("filter_kind".to_string(), serde_json::json!(k));
            }
        }

        DispatchResult::success(&request.id, response)
    }

    // ── Dependency handlers ──────────────────────────────────────────

    /// List package dependencies for a repo (REG-1 pattern).
    ///
    /// Request: `{"method": "deps_list", "params": {"repo": "<path_or_alias>", "module": "<optional>", "ecosystem": "<optional: npm|cargo>"}}`
    fn handle_deps_list(&self, request: &Request) -> DispatchResult {
        use repo_graph_module_queries::{
            cargo_runtime_builtins, compose_dependency_summaries, npm_runtime_builtins,
            ComposeDependenciesInput, DependencyCategory,
        };

        // REG-1: resolve repo from path/alias and auto-load
        let (repo_state, repo_uid) = match self.resolve_and_load_repo(&request.params) {
            Ok(r) => r,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        // Parse optional filters
        let module_filter = Self::get_optional_string_param(&request.params, "module");
        let ecosystem = Self::get_optional_string_param(&request.params, "ecosystem")
            .unwrap_or("npm")
            .to_string();

        // Acquire read lock
        let _read_guard = repo_state.coordinator.acquire_read();

        // Get latest snapshot
        let snapshot = match repo_state.storage.get_latest_snapshot(&repo_uid) {
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

        // Select runtime builtins based on ecosystem
        let runtime_builtins = match ecosystem.as_str() {
            "cargo" => cargo_runtime_builtins(),
            _ => npm_runtime_builtins(),
        };

        let input = ComposeDependenciesInput {
            snapshot_uid: &snapshot.snapshot_uid,
            runtime_builtins,
            ecosystem: ecosystem.clone(),
        };

        let result = match compose_dependency_summaries(&repo_state.storage, &input) {
            Ok(r) => r,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                );
            }
        };

        // Helper to format category
        fn format_category(cat: DependencyCategory) -> &'static str {
            match cat {
                DependencyCategory::DeclaredAndUsed => "declared_and_used",
                DependencyCategory::DeclaredButUnobserved => "declared_but_unobserved",
                DependencyCategory::ObservedButUndeclared => "observed_but_undeclared",
                DependencyCategory::RuntimeBuiltin => "runtime_builtin",
                DependencyCategory::UnknownExternalLike => "unknown_external_like",
            }
        }

        // Filter to specific module if requested
        let summaries: Vec<_> = if let Some(filter) = module_filter {
            result
                .summaries
                .into_iter()
                .filter(|s| {
                    s.module == filter
                        || s.module.ends_with(&format!("/{}", filter))
                        || s.module.starts_with(&format!("{}/", filter))
                })
                .collect()
        } else {
            result.summaries
        };

        // Build JSON output
        let results: Vec<serde_json::Value> = summaries
            .iter()
            .map(|s| {
                let entries: Vec<serde_json::Value> = s
                    .entries
                    .iter()
                    .map(|e| {
                        serde_json::json!({
                            "package": e.package,
                            "category": format_category(e.category),
                            "import_count": e.import_count,
                            "confidence": e.confidence,
                        })
                    })
                    .collect();

                serde_json::json!({
                    "module": s.module,
                    "manifest_path": s.manifest_path,
                    "manifest_scope_available": s.manifest_scope_available,
                    "declared_and_used": s.declared_and_used_count(),
                    "declared_but_unobserved": s.declared_but_unobserved_count(),
                    "observed_but_undeclared": s.observed_but_undeclared_count(),
                    "runtime_builtins": s.runtime_builtins_count(),
                    "entries": entries,
                })
            })
            .collect();

        let count = results.len();

        let mut response = serde_json::json!({
            "command": "deps list",
            "repo": repo_uid,
            "snapshot": snapshot.snapshot_uid,
            "results": results,
            "count": count,
            "ecosystem": ecosystem,
            "total_external_imports": result.total_external_imports,
            "modules_without_manifest_context": result.modules_without_manifest_context.len(),
        });

        if let Some(m) = module_filter {
            if let serde_json::Value::Object(ref mut map) = response {
                map.insert("module_filter".to_string(), serde_json::json!(m));
            }
        }

        DispatchResult::success(&request.id, response)
    }

    /// Explain why a package is used (REG-1 pattern).
    ///
    /// Request: `{"method": "deps_why", "params": {"repo": "<path_or_alias>", "package": "<name>", "ecosystem": "<optional: npm|cargo>"}}`
    fn handle_deps_why(&self, request: &Request) -> DispatchResult {
        use repo_graph_module_queries::{
            build_identifier_resolution_map, cargo_runtime_builtins, compose_dependency_summaries,
            normalize_cargo_specifier, normalize_npm_specifier, npm_runtime_builtins,
            resolve_import_specifier, ComposeDependenciesInput, DependencyCategory,
        };
        use std::collections::HashMap;

        // REG-1: resolve repo from path/alias and auto-load
        let (repo_state, repo_uid) = match self.resolve_and_load_repo(&request.params) {
            Ok(r) => r,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        // Parse required package param
        let package_query = match Self::get_string_param(&request.params, "package") {
            Ok(p) => p.to_string(),
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        let ecosystem = Self::get_optional_string_param(&request.params, "ecosystem")
            .unwrap_or("npm")
            .to_string();

        // Acquire read lock
        let _read_guard = repo_state.coordinator.acquire_read();

        // Get latest snapshot
        let snapshot = match repo_state.storage.get_latest_snapshot(&repo_uid) {
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

        // Helper to format category
        fn format_category(cat: DependencyCategory) -> &'static str {
            match cat {
                DependencyCategory::DeclaredAndUsed => "declared_and_used",
                DependencyCategory::DeclaredButUnobserved => "declared_but_unobserved",
                DependencyCategory::ObservedButUndeclared => "observed_but_undeclared",
                DependencyCategory::RuntimeBuiltin => "runtime_builtin",
                DependencyCategory::UnknownExternalLike => "unknown_external_like",
            }
        }

        // Load module_candidates for file → module mapping
        let modules = match repo_state
            .storage
            .get_module_candidates_for_snapshot(&snapshot.snapshot_uid)
        {
            Ok(m) => m,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                );
            }
        };
        let uid_to_canonical: HashMap<&str, &str> = modules
            .iter()
            .map(|m| {
                (
                    m.module_candidate_uid.as_str(),
                    m.canonical_root_path.as_str(),
                )
            })
            .collect();

        // Load file ownership
        let ownership = match repo_state
            .storage
            .get_file_ownership_for_snapshot(&snapshot.snapshot_uid)
        {
            Ok(o) => o,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                );
            }
        };
        let file_to_module: HashMap<&str, &str> = ownership
            .iter()
            .filter_map(|o| {
                uid_to_canonical
                    .get(o.module_candidate_uid.as_str())
                    .map(|&path| (o.file_uid.as_str(), path))
            })
            .collect();

        // Load external imports with file locations
        let imports_with_locations = match repo_state
            .storage
            .get_external_imports_with_locations(&snapshot.snapshot_uid)
        {
            Ok(i) => i,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                );
            }
        };

        // Load import bindings for identifier → specifier resolution
        let import_bindings = match repo_state
            .storage
            .get_external_import_bindings_for_snapshot(&snapshot.snapshot_uid)
        {
            Ok(b) => b,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                );
            }
        };
        let identifier_to_specifier = build_identifier_resolution_map(&import_bindings);

        // Get reconciliation summaries to check if package is declared
        let runtime_builtins = match ecosystem.as_str() {
            "cargo" => cargo_runtime_builtins(),
            _ => npm_runtime_builtins(),
        };
        let compose_input = ComposeDependenciesInput {
            snapshot_uid: &snapshot.snapshot_uid,
            runtime_builtins,
            ecosystem: ecosystem.clone(),
        };
        let reconciled = match compose_dependency_summaries(&repo_state.storage, &compose_input) {
            Ok(r) => r,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                );
            }
        };

        // Build lookup: module → (is_declared, category)
        let mut module_decl_info: HashMap<&str, (bool, &str)> = HashMap::new();
        for summary in &reconciled.summaries {
            for entry in &summary.entries {
                if entry.package == package_query {
                    let declared = matches!(
                        entry.category,
                        DependencyCategory::DeclaredAndUsed
                            | DependencyCategory::DeclaredButUnobserved
                    );
                    module_decl_info
                        .insert(&summary.module, (declared, format_category(entry.category)));
                }
            }
        }

        // Filter imports to queried package, group by module
        let normalizer: fn(&str) -> String = match ecosystem.as_str() {
            "cargo" => normalize_cargo_specifier,
            _ => normalize_npm_specifier,
        };

        let mut module_samples: HashMap<String, Vec<serde_json::Value>> = HashMap::new();

        for import in &imports_with_locations {
            let resolved = resolve_import_specifier(
                &import.specifier,
                &import.file_uid,
                &identifier_to_specifier,
            );
            let normalized = normalizer(&resolved);
            if normalized != package_query {
                continue;
            }

            if let Some(&module_path) = file_to_module.get(import.file_uid.as_str()) {
                let sample = serde_json::json!({
                    "file_path": import.file_path,
                    "specifier": import.specifier,
                    "resolved_to": resolved,
                    "line": import.line_start,
                    "column": import.col_start,
                });
                module_samples
                    .entry(module_path.to_string())
                    .or_default()
                    .push(sample);
            }
        }

        if module_samples.is_empty() {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::new(
                    ErrorCode::InvalidRequest,
                    format!("package '{}' not found in any module", package_query),
                ),
            );
        }

        // Build output with module summary + sample imports
        let mut usages: Vec<serde_json::Value> = Vec::new();

        for (module_path, samples) in &module_samples {
            let (declared, category) = module_decl_info
                .get(module_path.as_str())
                .copied()
                .unwrap_or((false, "unknown"));

            // Limit samples to 5 per module
            let limited_samples: Vec<_> = samples.iter().take(5).cloned().collect();

            usages.push(serde_json::json!({
                "module": module_path,
                "import_count": samples.len(),
                "declared": declared,
                "category": category,
                "sample_imports": limited_samples,
            }));
        }

        // Sort by module path
        usages.sort_by(|a, b| {
            a.get("module")
                .and_then(|v| v.as_str())
                .cmp(&b.get("module").and_then(|v| v.as_str()))
        });

        let count = usages.len();

        let response = serde_json::json!({
            "command": "deps why",
            "repo": repo_uid,
            "snapshot": snapshot.snapshot_uid,
            "package": package_query,
            "ecosystem": ecosystem,
            "results": usages,
            "count": count,
        });

        DispatchResult::success(&request.id, response)
    }

    /// Show dependency drift anomalies (REG-1 pattern).
    ///
    /// Request: `{"method": "deps_drift", "params": {"repo": "<path_or_alias>", "ecosystem": "<optional: npm|cargo>"}}`
    fn handle_deps_drift(&self, request: &Request) -> DispatchResult {
        use repo_graph_module_queries::{
            cargo_runtime_builtins, compose_dependency_summaries, npm_runtime_builtins,
            ComposeDependenciesInput, DependencyCategory,
        };

        // REG-1: resolve repo from path/alias and auto-load
        let (repo_state, repo_uid) = match self.resolve_and_load_repo(&request.params) {
            Ok(r) => r,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        let ecosystem = Self::get_optional_string_param(&request.params, "ecosystem")
            .unwrap_or("npm")
            .to_string();

        // Acquire read lock
        let _read_guard = repo_state.coordinator.acquire_read();

        // Get latest snapshot
        let snapshot = match repo_state.storage.get_latest_snapshot(&repo_uid) {
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

        // Select runtime builtins based on ecosystem
        let runtime_builtins = match ecosystem.as_str() {
            "cargo" => cargo_runtime_builtins(),
            _ => npm_runtime_builtins(),
        };

        let input = ComposeDependenciesInput {
            snapshot_uid: &snapshot.snapshot_uid,
            runtime_builtins,
            ecosystem: ecosystem.clone(),
        };

        let result = match compose_dependency_summaries(&repo_state.storage, &input) {
            Ok(r) => r,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                );
            }
        };

        // Collect drift anomalies across all modules
        let mut drift_entries: Vec<serde_json::Value> = Vec::new();

        for summary in &result.summaries {
            // DeclaredButUnobserved -> unused
            for entry in summary.by_category(DependencyCategory::DeclaredButUnobserved) {
                drift_entries.push(serde_json::json!({
                    "module": summary.module,
                    "package": entry.package,
                    "kind": "unused_declared",
                    "hint": "Package is declared in manifest but no imports found. Consider removing.",
                }));
            }

            // ObservedButUndeclared -> missing
            for entry in summary.by_category(DependencyCategory::ObservedButUndeclared) {
                drift_entries.push(serde_json::json!({
                    "module": summary.module,
                    "package": entry.package,
                    "kind": "undeclared_usage",
                    "import_count": entry.import_count,
                    "hint": "Package is imported but not declared in manifest. Add to dependencies.",
                }));
            }

            // UnknownExternalLike -> unclear
            for entry in summary.by_category(DependencyCategory::UnknownExternalLike) {
                drift_entries.push(serde_json::json!({
                    "module": summary.module,
                    "package": entry.package,
                    "kind": "unknown_external",
                    "import_count": entry.import_count,
                    "hint": "External-looking import but manifest context unavailable. Verify dependency.",
                }));
            }
        }

        let count = drift_entries.len();

        let response = serde_json::json!({
            "command": "deps drift",
            "repo": repo_uid,
            "snapshot": snapshot.snapshot_uid,
            "ecosystem": ecosystem,
            "modules_analyzed": result.summaries.len(),
            "modules_without_manifest_context": result.modules_without_manifest_context.len(),
            "results": drift_entries,
            "count": count,
        });

        DispatchResult::success(&request.id, response)
    }

    // ── Surfaces handlers ────────────────────────────────────────────

    /// List project surfaces for a repo (REG-1 pattern).
    ///
    /// Request: `{"method": "surfaces_list", "params": {"repo": "<path_or_alias>", "kind": "<opt>", "runtime": "<opt>", "source": "<opt>", "module": "<opt>"}}`
    fn handle_surfaces_list(&self, request: &Request) -> DispatchResult {
        use repo_graph_storage::crud::project_surfaces::SurfaceFilter;

        // REG-1: resolve repo from path/alias and auto-load
        let (repo_state, repo_uid) = match self.resolve_and_load_repo(&request.params) {
            Ok(r) => r,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        // Parse optional filters
        let mut filter = SurfaceFilter::default();
        if let Some(k) = Self::get_optional_string_param(&request.params, "kind") {
            filter.kind = Some(k.to_string());
        }
        if let Some(r) = Self::get_optional_string_param(&request.params, "runtime") {
            filter.runtime = Some(r.to_string());
        }
        if let Some(s) = Self::get_optional_string_param(&request.params, "source") {
            filter.source = Some(s.to_string());
        }
        if let Some(m) = Self::get_optional_string_param(&request.params, "module") {
            filter.module = Some(m.to_string());
        }

        // Acquire read lock
        let _read_guard = repo_state.coordinator.acquire_read();

        // Get latest snapshot
        let snapshot = match repo_state.storage.get_latest_snapshot(&repo_uid) {
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

        // Load surfaces with filtering
        let surfaces = match repo_state
            .storage
            .get_project_surfaces_for_snapshot(&snapshot.snapshot_uid, &filter)
        {
            Ok(s) => s,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                );
            }
        };

        // Load module candidates for enrichment
        let modules = match repo_state
            .storage
            .get_module_candidates_for_snapshot(&snapshot.snapshot_uid)
        {
            Ok(m) => m,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                );
            }
        };
        let module_map: std::collections::HashMap<&str, _> = modules
            .iter()
            .map(|m| (m.module_candidate_uid.as_str(), m))
            .collect();

        // Load evidence counts
        let evidence_counts = match repo_state
            .storage
            .count_evidence_by_surface(&snapshot.snapshot_uid)
        {
            Ok(c) => c,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                );
            }
        };

        // Build results
        let results: Vec<serde_json::Value> = surfaces
            .into_iter()
            .map(|s| {
                let module = module_map.get(s.module_candidate_uid.as_str());
                serde_json::json!({
                    "project_surface_uid": s.project_surface_uid,
                    "module_candidate_uid": s.module_candidate_uid,
                    "module_display_name": module.and_then(|m| m.display_name.clone()),
                    "module_root_path": module.map(|m| m.canonical_root_path.clone()),
                    "surface_kind": s.surface_kind,
                    "display_name": s.display_name,
                    "root_path": s.root_path,
                    "entrypoint_path": s.entrypoint_path,
                    "build_system": s.build_system,
                    "runtime_kind": s.runtime_kind,
                    "confidence": s.confidence,
                    "evidence_count": evidence_counts.get(&s.project_surface_uid).unwrap_or(&0),
                    "source_type": s.source_type,
                    "source_specific_id": s.source_specific_id,
                    "stable_surface_key": s.stable_surface_key,
                })
            })
            .collect();

        let count = results.len();

        let mut response = serde_json::json!({
            "command": "surfaces list",
            "repo": repo_uid,
            "snapshot": snapshot.snapshot_uid,
            "results": results,
            "count": count,
        });

        // Add filter info
        if let serde_json::Value::Object(ref mut map) = response {
            if filter.kind.is_some() {
                map.insert("filter_kind".to_string(), serde_json::json!(filter.kind));
            }
            if filter.runtime.is_some() {
                map.insert(
                    "filter_runtime".to_string(),
                    serde_json::json!(filter.runtime),
                );
            }
            if filter.source.is_some() {
                map.insert(
                    "filter_source".to_string(),
                    serde_json::json!(filter.source),
                );
            }
            if filter.module.is_some() {
                map.insert(
                    "filter_module".to_string(),
                    serde_json::json!(filter.module),
                );
            }

            // Add degradation info when surfaces not populated
            if results.is_empty() && modules.is_empty() {
                map.insert("degradation".to_string(), serde_json::json!({
                    "status": "unsupported",
                    "feature": "ProjectSurfaces",
                    "message": "project_surfaces and module_candidates are not populated on Rust indexer path",
                    "recommendation": "use TypeScript prototype indexer for project surface discovery"
                }));
            }
        }

        DispatchResult::success(&request.id, response)
    }

    /// Show project surface detail (REG-1 pattern).
    ///
    /// Request: `{"method": "surfaces_show", "params": {"repo": "<path_or_alias>", "surface": "<surface_ref>"}}`
    fn handle_surfaces_show(&self, request: &Request) -> DispatchResult {
        // REG-1: resolve repo from path/alias and auto-load
        let (repo_state, repo_uid) = match self.resolve_and_load_repo(&request.params) {
            Ok(r) => r,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        // Parse required surface ref
        let surface_ref = match Self::get_string_param(&request.params, "surface") {
            Ok(s) => s.to_string(),
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        // Acquire read lock
        let _read_guard = repo_state.coordinator.acquire_read();

        // Get latest snapshot
        let snapshot = match repo_state.storage.get_latest_snapshot(&repo_uid) {
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

        // Resolve surface by ref
        let surface = match repo_state
            .storage
            .get_project_surface_by_ref(&snapshot.snapshot_uid, &surface_ref)
        {
            Ok(Some(s)) => s,
            Ok(None) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(
                        ErrorCode::InvalidRequest,
                        format!("surface not found: {}", surface_ref),
                    ),
                );
            }
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InvalidRequest, e.to_string()),
                );
            }
        };

        // Load owning module by UID
        let module = match repo_state
            .storage
            .get_module_by_uid(&surface.module_candidate_uid)
        {
            Ok(m) => m,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                );
            }
        };

        // Load evidence
        let evidence_rows = match repo_state
            .storage
            .get_project_surface_evidence(&surface.project_surface_uid)
        {
            Ok(e) => e,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                );
            }
        };

        // Parse metadata_json with fallback
        let metadata = match &surface.metadata_json {
            None => serde_json::json!({ "parsed": null, "raw": null, "parse_error": null }),
            Some(raw) => match serde_json::from_str::<serde_json::Value>(raw) {
                Ok(parsed) => {
                    serde_json::json!({ "parsed": parsed, "raw": null, "parse_error": null })
                }
                Err(e) => {
                    serde_json::json!({ "parsed": null, "raw": raw, "parse_error": e.to_string() })
                }
            },
        };

        // Build evidence array
        let evidence: Vec<serde_json::Value> = evidence_rows
            .into_iter()
            .map(|e| {
                serde_json::json!({
                    "source_type": e.source_type,
                    "source_path": e.source_path,
                    "evidence_kind": e.evidence_kind,
                    "confidence": e.confidence,
                    "payload": e.payload_json.as_ref().and_then(|p| serde_json::from_str::<serde_json::Value>(p).ok()),
                })
            })
            .collect();

        let response = serde_json::json!({
            "command": "surfaces show",
            "repo": repo_uid,
            "snapshot": snapshot.snapshot_uid,
            "surface": {
                "project_surface_uid": surface.project_surface_uid,
                "surface_kind": surface.surface_kind,
                "display_name": surface.display_name,
                "root_path": surface.root_path,
                "entrypoint_path": surface.entrypoint_path,
                "build_system": surface.build_system,
                "runtime_kind": surface.runtime_kind,
                "confidence": surface.confidence,
                "source_type": surface.source_type,
                "source_specific_id": surface.source_specific_id,
                "stable_surface_key": surface.stable_surface_key,
                "metadata_json": metadata,
            },
            "module": module.map(|m| serde_json::json!({
                "module_candidate_uid": m.module_candidate_uid,
                "module_key": m.module_key,
                "display_name": m.display_name,
                "canonical_root_path": m.canonical_root_path,
            })),
            "evidence": evidence,
        });

        DispatchResult::success(&request.id, response)
    }

    // ── Boundaries handlers ─────────────────────────────────────────────

    /// List boundary interactions (REG-1 pattern).
    ///
    /// Request: `{"method": "boundaries_list", "params": {"repo": "<path_or_alias>", ...filters}}`
    fn handle_boundaries_list(&self, request: &Request) -> DispatchResult {
        use repo_graph_boundary_interaction::{
            BoundaryInteractionFilter, BoundaryInteractionReadPort,
        };

        // REG-1: resolve repo from path/alias and auto-load
        let (repo_state, repo_uid) = match self.resolve_and_load_repo(&request.params) {
            Ok(r) => r,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        // Parse optional filters
        let mut filter = BoundaryInteractionFilter::new();
        if let Some(k) = Self::get_optional_string_param(&request.params, "kind") {
            filter.channel_kind = parse_channel_kind(k);
        }
        if let Some(s) = Self::get_optional_string_param(&request.params, "scope") {
            filter.boundary_scope = parse_boundary_scope(s);
        }
        if let Some(d) = Self::get_optional_string_param(&request.params, "direction") {
            filter.direction = parse_direction(d);
        }
        if let Some(f) = Self::get_optional_string_param(&request.params, "family") {
            filter.protocol_family = parse_protocol_family(f);
        }
        if let Some(f) = Self::get_optional_string_param(&request.params, "file") {
            filter.file = Some(f.to_string());
        }
        if let Some(p) = Self::get_optional_string_param(&request.params, "file_prefix") {
            filter.file_prefix = Some(p.to_string());
        }
        if let Some(s) = Self::get_optional_string_param(&request.params, "symbol") {
            filter.symbol = Some(s.to_string());
        }

        // Acquire read lock
        let _read_guard = repo_state.coordinator.acquire_read();

        // Get latest snapshot
        let snapshot = match repo_state.storage.get_latest_snapshot(&repo_uid) {
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

        // Query boundary interactions
        let items = match repo_state
            .storage
            .list_boundary_interactions(&snapshot.snapshot_uid, &filter)
        {
            Ok(i) => i,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                );
            }
        };

        let count = items.len();

        let mut response = serde_json::json!({
            "command": "boundaries list",
            "repo": repo_uid,
            "snapshot": snapshot.snapshot_uid,
            "results": items,
            "count": count,
        });

        // Add filter info
        if let serde_json::Value::Object(ref mut map) = response {
            if filter.channel_kind.is_some() {
                map.insert(
                    "filter_kind".to_string(),
                    serde_json::json!(filter.channel_kind.map(|k| k.as_str())),
                );
            }
            if filter.boundary_scope.is_some() {
                map.insert(
                    "filter_scope".to_string(),
                    serde_json::json!(filter.boundary_scope.map(|s| s.as_str())),
                );
            }
            if filter.direction.is_some() {
                map.insert(
                    "filter_direction".to_string(),
                    serde_json::json!(filter.direction.map(|d| d.as_str())),
                );
            }
            if filter.protocol_family.is_some() {
                map.insert(
                    "filter_family".to_string(),
                    serde_json::json!(filter.protocol_family.map(|f| f.as_str())),
                );
            }
            if let Some(ref f) = filter.file {
                map.insert("filter_file".to_string(), serde_json::json!(f));
            }
            if let Some(ref p) = filter.file_prefix {
                map.insert("filter_file_prefix".to_string(), serde_json::json!(p));
            }
            if let Some(ref s) = filter.symbol {
                map.insert("filter_symbol".to_string(), serde_json::json!(s));
            }
        }

        DispatchResult::success(&request.id, response)
    }

    /// Show boundary interaction detail (REG-1 pattern).
    ///
    /// Request: `{"method": "boundaries_show", "params": {"repo": "<path_or_alias>", "surface": "<surface_uid>"}}`
    fn handle_boundaries_show(&self, request: &Request) -> DispatchResult {
        use repo_graph_boundary_interaction::BoundaryInteractionReadPort;

        // REG-1: resolve repo from path/alias and auto-load
        let (repo_state, repo_uid) = match self.resolve_and_load_repo(&request.params) {
            Ok(r) => r,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        let surface_uid = match Self::get_string_param(&request.params, "surface") {
            Ok(s) => s,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        // Acquire read lock
        let _read_guard = repo_state.coordinator.acquire_read();

        // Get latest snapshot for envelope
        let snapshot = match repo_state.storage.get_latest_snapshot(&repo_uid) {
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

        // Query boundary interaction detail
        let detail = match repo_state
            .storage
            .get_boundary_interaction_detail(surface_uid)
        {
            Ok(Some(d)) => d,
            Ok(None) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(
                        ErrorCode::InvalidRequest,
                        format!("surface not found: {}", surface_uid),
                    ),
                );
            }
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                );
            }
        };

        // Verify surface belongs to requested repo
        if detail.repo_uid != repo_uid {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::new(
                    ErrorCode::InvalidRequest,
                    format!(
                        "surface {} belongs to repo '{}', not '{}'",
                        surface_uid, detail.repo_uid, repo_uid
                    ),
                ),
            );
        }

        let response = serde_json::json!({
            "command": "boundaries show",
            "repo": repo_uid,
            "snapshot": snapshot.snapshot_uid,
            "detail": detail,
        });

        DispatchResult::success(&request.id, response)
    }

    /// Get boundary interaction summary (REG-1 pattern).
    ///
    /// Request: `{"method": "boundaries_summary", "params": {"repo": "<path_or_alias>"}}`
    fn handle_boundaries_summary(&self, request: &Request) -> DispatchResult {
        use repo_graph_boundary_interaction::BoundaryInteractionReadPort;

        // REG-1: resolve repo from path/alias and auto-load
        let (repo_state, repo_uid) = match self.resolve_and_load_repo(&request.params) {
            Ok(r) => r,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        // Acquire read lock
        let _read_guard = repo_state.coordinator.acquire_read();

        // Get latest snapshot
        let snapshot = match repo_state.storage.get_latest_snapshot(&repo_uid) {
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

        // Query summary
        let summary = match repo_state
            .storage
            .get_boundary_interaction_summary(&snapshot.snapshot_uid)
        {
            Ok(s) => s,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                );
            }
        };

        let response = serde_json::json!({
            "command": "boundaries summary",
            "repo": repo_uid,
            "snapshot": snapshot.snapshot_uid,
            "summary": summary,
        });

        DispatchResult::success(&request.id, response)
    }

    /// List boundary interaction links (REG-1 pattern).
    ///
    /// Request: `{"method": "boundaries_links", "params": {"repo": "<path_or_alias>", "service": "<opt>"}}`
    fn handle_boundaries_links(&self, request: &Request) -> DispatchResult {
        use repo_graph_boundary_interaction::{
            BoundaryInteractionLinkFilter, BoundaryInteractionReadPort,
        };

        // REG-1: resolve repo from path/alias and auto-load
        let (repo_state, repo_uid) = match self.resolve_and_load_repo(&request.params) {
            Ok(r) => r,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        // Parse optional filters
        let mut filter = BoundaryInteractionLinkFilter::new();
        if let Some(s) = Self::get_optional_string_param(&request.params, "service") {
            filter.contract_name = Some(s.to_string());
        }

        // Acquire read lock
        let _read_guard = repo_state.coordinator.acquire_read();

        // Get latest snapshot
        let snapshot = match repo_state.storage.get_latest_snapshot(&repo_uid) {
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

        // Query links
        let items = match repo_state
            .storage
            .list_boundary_interaction_links(&snapshot.snapshot_uid, &filter)
        {
            Ok(i) => i,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                );
            }
        };

        let count = items.len();

        let mut response = serde_json::json!({
            "command": "boundaries links",
            "repo": repo_uid,
            "snapshot": snapshot.snapshot_uid,
            "results": items,
            "count": count,
        });

        // Add filter info
        if let serde_json::Value::Object(ref mut map) = response {
            if let Some(ref s) = filter.contract_name {
                map.insert("filter_service".to_string(), serde_json::json!(s));
            }
        }

        DispatchResult::success(&request.id, response)
    }

    // ── Modules handlers ────────────────────────────────────────────────

    /// List files owned by a module (REG-1 pattern).
    ///
    /// Request: `{"method": "modules_files", "params": {"repo": "<path_or_alias>", "module": "<module_ref>"}}`
    fn handle_modules_files(&self, request: &Request) -> DispatchResult {
        use repo_graph_module_queries::ModuleQueryContext;

        // REG-1: resolve repo from path/alias and auto-load
        let (repo_state, repo_uid) = match self.resolve_and_load_repo(&request.params) {
            Ok(r) => r,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        let module_ref = match Self::get_string_param(&request.params, "module") {
            Ok(m) => m,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        // Acquire read lock
        let _read_guard = repo_state.coordinator.acquire_read();

        // Get latest snapshot
        let snapshot = match repo_state.storage.get_latest_snapshot(&repo_uid) {
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

        // Load module context (service layer)
        let ctx = match ModuleQueryContext::load(&repo_state.storage, &snapshot.snapshot_uid) {
            Ok(c) => c,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(
                        ErrorCode::InternalError,
                        format!("failed to load module context: {}", e),
                    ),
                );
            }
        };

        // Resolve module argument
        let resolved_module = match ctx.resolve_module(module_ref) {
            Some(m) => m.clone(),
            None => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(
                        ErrorCode::InvalidRequest,
                        format!(
                            "module not found: {} (use canonical path or module key)",
                            module_ref
                        ),
                    ),
                );
            }
        };

        // Load files for the resolved module
        let files = match repo_state.storage.get_files_for_module(
            &snapshot.snapshot_uid,
            &resolved_module.module_candidate_uid,
        ) {
            Ok(f) if !f.is_empty() => f,
            Ok(_) => {
                // Fallback: use context's files_for_module (degraded metadata)
                ctx.files_for_module(&resolved_module.module_candidate_uid)
                    .into_iter()
                    .map(
                        |of| repo_graph_storage::crud::module_edges_support::ModuleFileEntry {
                            file_uid: of.file_uid.clone(),
                            path: of.file_path.clone(),
                            language: None,
                            assignment_kind: "inferred".to_string(),
                            confidence: 1.0,
                        },
                    )
                    .collect()
            }
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(
                        ErrorCode::InternalError,
                        format!("failed to load module files: {}", e),
                    ),
                );
            }
        };

        // Build results
        let results: Vec<serde_json::Value> = files
            .into_iter()
            .map(|f| {
                serde_json::json!({
                    "file_uid": f.file_uid,
                    "path": f.path,
                    "language": f.language,
                    "assignment_kind": f.assignment_kind,
                    "confidence": f.confidence,
                })
            })
            .collect();

        let count = results.len();

        let response = serde_json::json!({
            "command": "modules files",
            "repo": repo_uid,
            "snapshot": snapshot.snapshot_uid,
            "module": {
                "module_uid": resolved_module.module_candidate_uid,
                "module_key": resolved_module.module_key,
                "canonical_root_path": resolved_module.canonical_root_path,
            },
            "results": results,
            "count": count,
        });

        DispatchResult::success(&request.id, response)
    }

    /// Get module dependency edges (REG-1 pattern).
    ///
    /// Request: `{"method": "modules_deps", "params": {"repo": "<path_or_alias>", "module": "<module_ref>", "direction": "all|outbound|inbound"}}`
    ///
    /// - `module` is optional; if omitted, returns all cross-module edges
    /// - `direction` is optional; defaults to "all"; only valid when module is specified
    fn handle_modules_deps(&self, request: &Request) -> DispatchResult {
        use repo_graph_module_queries::load_module_graph_facts;

        // REG-1: resolve repo from path/alias and auto-load
        let (repo_state, repo_uid) = match self.resolve_and_load_repo(&request.params) {
            Ok(r) => r,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        // Optional module filter
        let module_ref = Self::get_optional_string_param(&request.params, "module");

        // Optional direction filter (only meaningful with module)
        let direction_str =
            Self::get_optional_string_param(&request.params, "direction").unwrap_or("all");

        // Validate direction
        let direction = match direction_str.to_lowercase().as_str() {
            "all" => "all",
            "outbound" => "outbound",
            "inbound" => "inbound",
            other => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::invalid_request(format!(
                        "invalid direction: {} (expected: all, outbound, inbound)",
                        other
                    )),
                );
            }
        };

        // Direction without module is invalid
        if direction != "all" && module_ref.is_none() {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::invalid_request(
                    "direction filter requires module parameter".to_string(),
                ),
            );
        }

        // Acquire read lock
        let _read_guard = repo_state.coordinator.acquire_read();

        // Get latest snapshot
        let snapshot = match repo_state.storage.get_latest_snapshot(&repo_uid) {
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

        // Load module graph facts (service layer - single load with precomputed edges)
        let facts = match load_module_graph_facts(&repo_state.storage, &snapshot.snapshot_uid) {
            Ok(f) => f,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(
                        ErrorCode::InternalError,
                        format!("failed to load module graph facts: {}", e),
                    ),
                );
            }
        };

        // Resolve module if specified
        let resolved_module_path: Option<String> = match module_ref {
            Some(filter) => match facts.resolve_module(filter) {
                Some(m) => Some(m.canonical_root_path.clone()),
                None => {
                    return DispatchResult::error(
                        &request.id,
                        ErrorDetail::new(
                            ErrorCode::InvalidRequest,
                            format!(
                                "module not found: {} (use canonical path or module key)",
                                filter
                            ),
                        ),
                    );
                }
            },
            None => None,
        };

        // Filter precomputed edges
        let filtered_edges: Vec<_> = match &resolved_module_path {
            Some(module_path) => facts
                .edges
                .iter()
                .filter(|e| match direction {
                    "all" => {
                        e.source_canonical_path == *module_path
                            || e.target_canonical_path == *module_path
                    }
                    "outbound" => e.source_canonical_path == *module_path,
                    "inbound" => e.target_canonical_path == *module_path,
                    _ => false, // unreachable due to validation above
                })
                .collect(),
            None => facts.edges.iter().collect(),
        };

        // Build results
        let results: Vec<serde_json::Value> = filtered_edges
            .iter()
            .map(|e| {
                serde_json::json!({
                    "source": e.source_canonical_path,
                    "target": e.target_canonical_path,
                    "import_count": e.import_count,
                    "source_file_count": e.source_file_count,
                })
            })
            .collect();

        let count = results.len();

        // Build response
        let mut response = serde_json::json!({
            "command": "modules deps",
            "repo": repo_uid,
            "snapshot": snapshot.snapshot_uid,
            "direction": direction,
            "diagnostics": {
                "imports_total": facts.diagnostics.imports_total,
                "imports_cross_module": facts.diagnostics.imports_cross_module,
                "imports_intra_module": facts.diagnostics.imports_intra_module,
                "imports_source_unowned": facts.diagnostics.imports_source_unowned,
                "imports_target_unowned": facts.diagnostics.imports_target_unowned,
            },
            "results": results,
            "count": count,
        });

        // Add module info if filtered
        if let Some(ref module_path) = resolved_module_path {
            response["module"] = serde_json::json!(module_path);
        }

        DispatchResult::success(&request.id, response)
    }

    /// Evaluate discovered-module boundary violations (REG-1 pattern).
    ///
    /// Request: `{"method": "modules_violations", "params": {"repo": "<path_or_alias>"}}`
    fn handle_modules_violations(&self, request: &Request) -> DispatchResult {
        use repo_graph_classification::boundary_evaluator::StaleSide;
        use repo_graph_module_queries::{evaluate_violations_from_facts, load_module_graph_facts};

        // REG-1: resolve repo from path/alias and auto-load
        let (repo_state, repo_uid) = match self.resolve_and_load_repo(&request.params) {
            Ok(r) => r,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        // Acquire read lock
        let _read_guard = repo_state.coordinator.acquire_read();

        // Get latest snapshot
        let snapshot = match repo_state.storage.get_latest_snapshot(&repo_uid) {
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

        // Load module graph facts (service layer)
        let facts = match load_module_graph_facts(&repo_state.storage, &snapshot.snapshot_uid) {
            Ok(f) => f,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(
                        ErrorCode::InternalError,
                        format!("failed to load module graph facts: {}", e),
                    ),
                );
            }
        };

        // Evaluate violations using preloaded facts (service layer)
        let result = match evaluate_violations_from_facts(&repo_state.storage, &repo_uid, &facts) {
            Ok(r) => r,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(
                        ErrorCode::InternalError,
                        format!("failed to evaluate violations: {}", e),
                    ),
                );
            }
        };

        // Build violations JSON
        let violations_json: Vec<serde_json::Value> = result
            .evaluation
            .violations
            .iter()
            .map(|v| {
                serde_json::json!({
                    "declaration_uid": v.declaration_uid,
                    "source": v.source_canonical_path,
                    "target": v.target_canonical_path,
                    "import_count": v.import_count,
                    "source_file_count": v.source_file_count,
                    "reason": v.reason,
                })
            })
            .collect();

        // Build stale declarations JSON
        let stale_json: Vec<serde_json::Value> = result
            .evaluation
            .stale_declarations
            .iter()
            .map(|s| {
                serde_json::json!({
                    "declaration_uid": s.declaration_uid,
                    "stale_side": match s.stale_side {
                        StaleSide::Source => "source",
                        StaleSide::Target => "target",
                        StaleSide::Both => "both",
                    },
                    "missing_paths": s.missing_paths,
                })
            })
            .collect();

        let violation_count = result.evaluation.violations.len();
        let stale_count = result.evaluation.stale_declarations.len();

        // Build response
        let response = serde_json::json!({
            "command": "modules violations",
            "repo": repo_uid,
            "snapshot": snapshot.snapshot_uid,
            "results": {
                "violations": violations_json,
                "stale_declarations": stale_json,
            },
            "count": violation_count,
            "stale_count": stale_count,
            "diagnostics": {
                "imports_edges_total": result.diagnostics.imports_total,
                "imports_source_no_file": 0,
                "imports_target_no_file": 0,
                "imports_source_no_module": result.diagnostics.imports_source_unowned,
                "imports_target_no_module": result.diagnostics.imports_target_unowned,
                "imports_intra_module": result.diagnostics.imports_intra_module,
                "imports_cross_module": result.diagnostics.imports_cross_module,
            },
        });

        DispatchResult::success(&request.id, response)
    }

    /// List unowned source files (REG-1 pattern).
    ///
    /// Request: `{"method": "modules_unowned", "params": {"repo": "<path_or_alias>"}}`
    fn handle_modules_unowned(&self, request: &Request) -> DispatchResult {
        use std::collections::{HashMap, HashSet};

        // REG-1: resolve repo from path/alias and auto-load
        let (repo_state, repo_uid) = match self.resolve_and_load_repo(&request.params) {
            Ok(r) => r,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        // Acquire read lock
        let _read_guard = repo_state.coordinator.acquire_read();

        // Get latest snapshot
        let snapshot = match repo_state.storage.get_latest_snapshot(&repo_uid) {
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

        // Get all indexed files via file_version_hashes
        let file_version_hashes = match repo_state
            .storage
            .query_file_version_hashes(&snapshot.snapshot_uid)
        {
            Ok(f) => f,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(
                        ErrorCode::InternalError,
                        format!("failed to load files: {}", e),
                    ),
                );
            }
        };

        // Extract file paths from file_uids (format: repo_uid:path)
        let all_file_paths: Vec<(String, String)> = file_version_hashes
            .keys()
            .map(|file_uid| {
                let path = file_uid
                    .strip_prefix(&format!("{}:", repo_uid))
                    .unwrap_or(file_uid)
                    .to_string();
                (file_uid.clone(), path)
            })
            .collect();

        // Get owned files
        let ownership = match repo_state
            .storage
            .get_file_ownership_for_snapshot(&snapshot.snapshot_uid)
        {
            Ok(o) => o,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(
                        ErrorCode::InternalError,
                        format!("failed to load ownership: {}", e),
                    ),
                );
            }
        };

        // Get module candidates for context
        let modules = match repo_state
            .storage
            .get_module_candidates_for_snapshot(&snapshot.snapshot_uid)
        {
            Ok(m) => m,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(
                        ErrorCode::InternalError,
                        format!("failed to load modules: {}", e),
                    ),
                );
            }
        };

        // Build set of owned file UIDs
        let owned_uids: HashSet<&str> = ownership.iter().map(|o| o.file_uid.as_str()).collect();

        // Build set of module root paths for classification
        let module_roots: HashSet<&str> = modules
            .iter()
            .map(|m| m.canonical_root_path.as_str())
            .collect();

        // Find unowned files and classify reasons
        let mut unowned_files: Vec<serde_json::Value> = Vec::new();
        let mut by_reason: HashMap<String, u64> = HashMap::new();

        for (file_uid, path) in &all_file_paths {
            if owned_uids.contains(file_uid.as_str()) {
                continue;
            }

            // Only count source files as "eligible" unowned
            if !is_source_file_for_unowned(path) {
                continue;
            }

            let reason = classify_unowned_reason(path, &module_roots);
            let language = infer_language_for_unowned(path);

            *by_reason.entry(reason.clone()).or_insert(0) += 1;

            unowned_files.push(serde_json::json!({
                "file_path": path,
                "language": language,
                "reason": reason,
            }));
        }

        // Sort by reason then path
        unowned_files.sort_by(|a, b| {
            let reason_a = a["reason"].as_str().unwrap_or("");
            let reason_b = b["reason"].as_str().unwrap_or("");
            let path_a = a["file_path"].as_str().unwrap_or("");
            let path_b = b["file_path"].as_str().unwrap_or("");
            reason_a.cmp(reason_b).then_with(|| path_a.cmp(path_b))
        });

        // Compute summary
        let total_indexed = all_file_paths.len() as u64;
        let total_owned = ownership.len() as u64;
        let total_unowned = unowned_files.len() as u64;
        let unowned_pct = if total_indexed > 0 {
            (total_unowned as f64 / total_indexed as f64) * 100.0
        } else {
            0.0
        };

        // Build response
        let response = serde_json::json!({
            "command": "modules unowned",
            "repo": repo_uid,
            "snapshot": snapshot.snapshot_uid,
            "results": unowned_files,
            "count": total_unowned,
            "summary": {
                "total_indexed_files": total_indexed,
                "total_owned_files": total_owned,
                "total_unowned_files": total_unowned,
                "unowned_pct": unowned_pct,
                "by_reason": by_reason,
            },
        });

        DispatchResult::success(&request.id, response)
    }

    /// Show single module detail view (REG-1 pattern).
    ///
    /// Request: `{"method": "modules_show", "params": {"repo": "<path_or_alias>", "module": "<module_ref>"}}`
    fn handle_modules_show(&self, request: &Request) -> DispatchResult {
        use repo_graph_classification::module_rollup::{
            compute_module_rollups, DeadNodeFact, ModuleRollupInput, OwnedFileFact,
        };
        use repo_graph_classification::weighted_neighbors::compute_weighted_neighbors;
        use repo_graph_module_queries::{evaluate_violations_from_facts, load_module_graph_facts};
        use std::collections::HashMap;

        // REG-1: resolve repo from path/alias and auto-load
        let (repo_state, repo_uid) = match self.resolve_and_load_repo(&request.params) {
            Ok(r) => r,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        let module_ref = match Self::get_string_param(&request.params, "module") {
            Ok(m) => m,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        // Acquire read lock
        let _read_guard = repo_state.coordinator.acquire_read();

        // Get latest snapshot
        let snapshot = match repo_state.storage.get_latest_snapshot(&repo_uid) {
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

        // Load module graph facts
        let facts = match load_module_graph_facts(&repo_state.storage, &snapshot.snapshot_uid) {
            Ok(f) => f,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(
                        ErrorCode::InternalError,
                        format!("failed to load module graph facts: {}", e),
                    ),
                );
            }
        };

        // Resolve module argument
        let resolved_module = match facts.resolve_module(module_ref) {
            Some(m) => m.clone(),
            None => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(
                        ErrorCode::InvalidRequest,
                        format!(
                            "module not found: {} (use canonical path or module key)",
                            module_ref
                        ),
                    ),
                );
            }
        };

        // Build module identity lookup for enrichment
        let module_identity_map: HashMap<&str, serde_json::Value> = facts
            .context
            .modules
            .iter()
            .map(|m| {
                (
                    m.canonical_root_path.as_str(),
                    serde_json::json!({
                        "module_uid": m.module_candidate_uid,
                        "module_key": m.module_key,
                        "canonical_root_path": m.canonical_root_path,
                        "module_kind": m.module_kind,
                        "display_name": m.display_name,
                        "confidence": m.confidence,
                    }),
                )
            })
            .collect();

        // Load module evidence (Phase 3.2)
        let evidence_output: Vec<serde_json::Value> = repo_state
            .storage
            .get_module_candidate_evidence(&resolved_module.module_candidate_uid)
            .unwrap_or_default()
            .into_iter()
            .map(|e| {
                let (evidence_strength, build_files_present, dominant_language) =
                    if let Some(ref payload) = e.payload_json {
                        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(payload) {
                            let strength = parsed
                                .get("evidence_strength")
                                .and_then(|v| v.as_str())
                                .map(String::from);
                            let build_files = parsed
                                .get("build_files_present")
                                .and_then(|v| v.as_array())
                                .map(|arr| {
                                    arr.iter()
                                        .filter_map(|v| v.as_str().map(String::from))
                                        .collect::<Vec<_>>()
                                })
                                .unwrap_or_default();
                            let lang = parsed
                                .get("dominant_language")
                                .and_then(|v| v.as_str())
                                .map(String::from);
                            (strength, build_files, lang)
                        } else {
                            (None, vec![], None)
                        }
                    } else {
                        (None, vec![], None)
                    };

                let mut ev = serde_json::json!({
                    "source_type": e.source_type,
                    "source_path": e.source_path,
                    "evidence_kind": e.evidence_kind,
                });
                if let Some(strength) = evidence_strength {
                    ev["evidence_strength"] = serde_json::json!(strength);
                }
                if !build_files_present.is_empty() {
                    ev["build_files_present"] = serde_json::json!(build_files_present);
                }
                if let Some(lang) = dominant_language {
                    ev["dominant_language"] = serde_json::json!(lang);
                }
                ev
            })
            .collect();

        // Load dead nodes (SYMBOL kind only)
        let dead_nodes = repo_state
            .storage
            .find_dead_nodes(&snapshot.snapshot_uid, &repo_uid, Some("SYMBOL"))
            .unwrap_or_default();

        // Evaluate violations (advisory)
        let (violations_eval, violations_warning) =
            match evaluate_violations_from_facts(&repo_state.storage, &repo_uid, &facts) {
                Ok(r) => (Some(r.evaluation), None::<String>),
                Err(msg) => (
                    None,
                    Some(format!(
                        "discovered-module violation rollups unavailable: {}",
                        msg
                    )),
                ),
            };

        let violations_available = violations_eval.is_some();

        // Compute rollup for this module
        let violations_for_rollup = violations_eval
            .as_ref()
            .map(|e| e.violations.clone())
            .unwrap_or_default();

        let owned_file_facts: Vec<OwnedFileFact> = facts
            .context
            .owned_files
            .iter()
            .map(|f| OwnedFileFact {
                file_path: f.file_path.clone(),
                module_uid: f.module_candidate_uid.clone(),
                is_test: f.is_test,
            })
            .collect();

        let dead_node_facts: Vec<DeadNodeFact> = dead_nodes
            .into_iter()
            .filter_map(|d| {
                d.file.map(|file_path| DeadNodeFact {
                    file_path,
                    is_test: d.is_test,
                })
            })
            .collect();

        let rollup_input = ModuleRollupInput {
            modules: facts.module_refs.clone(),
            owned_files: owned_file_facts,
            edges: facts.edges.clone(),
            violations: violations_for_rollup,
            dead_nodes: dead_node_facts,
        };

        let rollups = match compute_module_rollups(&rollup_input) {
            Ok(r) => r,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(
                        ErrorCode::InternalError,
                        format!("failed to compute rollups: {}", e),
                    ),
                );
            }
        };

        // Find this module's rollup
        let module_rollup = rollups
            .iter()
            .find(|r| r.module_uid == resolved_module.module_candidate_uid);

        let rollups_output = serde_json::json!({
            "owned_file_count": module_rollup.map_or(0, |r| r.owned_file_count),
            "owned_test_file_count": module_rollup.map_or(0, |r| r.owned_test_file_count),
            "outbound_dependency_count": module_rollup.map_or(0, |r| r.outbound_dependency_count),
            "outbound_import_count": module_rollup.map_or(0, |r| r.outbound_import_count),
            "inbound_dependency_count": module_rollup.map_or(0, |r| r.inbound_dependency_count),
            "inbound_import_count": module_rollup.map_or(0, |r| r.inbound_import_count),
            "violation_count": if violations_available {
                serde_json::json!(module_rollup.map_or(0, |r| r.violation_count))
            } else {
                serde_json::Value::Null
            },
            "dead_symbol_count": module_rollup.map_or(0, |r| r.dead_symbol_count),
            "dead_test_symbol_count": module_rollup.map_or(0, |r| r.dead_test_symbol_count),
        });

        // Compute weighted neighbors
        let weighted =
            compute_weighted_neighbors(&resolved_module.module_candidate_uid, &facts.edges);

        // Enrich outbound neighbors
        let outbound_dependencies: Vec<serde_json::Value> = weighted
            .outbound
            .iter()
            .filter_map(|n| {
                let module_path = facts
                    .edges
                    .iter()
                    .find(|e| e.target_module_uid == n.module_uid)
                    .map(|e| e.target_canonical_path.as_str())?;
                let identity = module_identity_map.get(module_path)?;
                Some(serde_json::json!({
                    "module_uid": identity["module_uid"],
                    "module_key": identity["module_key"],
                    "canonical_root_path": identity["canonical_root_path"],
                    "module_kind": identity["module_kind"],
                    "import_count": n.import_count,
                    "source_file_count": n.source_file_count,
                }))
            })
            .collect();

        // Enrich inbound neighbors
        let inbound_dependencies: Vec<serde_json::Value> = weighted
            .inbound
            .iter()
            .filter_map(|n| {
                let module_path = facts
                    .edges
                    .iter()
                    .find(|e| e.source_module_uid == n.module_uid)
                    .map(|e| e.source_canonical_path.as_str())?;
                let identity = module_identity_map.get(module_path)?;
                Some(serde_json::json!({
                    "module_uid": identity["module_uid"],
                    "module_key": identity["module_key"],
                    "canonical_root_path": identity["canonical_root_path"],
                    "module_kind": identity["module_kind"],
                    "import_count": n.import_count,
                    "source_file_count": n.source_file_count,
                }))
            })
            .collect();

        // Filter violations for this module (source-side only)
        let violations_output: serde_json::Value = if violations_available {
            let source_violations: Vec<serde_json::Value> = violations_eval
                .as_ref()
                .unwrap()
                .violations
                .iter()
                .filter(|v| v.source_canonical_path == resolved_module.canonical_root_path)
                .filter_map(|v| {
                    let target_identity =
                        module_identity_map.get(v.target_canonical_path.as_str())?;
                    Some(serde_json::json!({
                        "declaration_uid": v.declaration_uid,
                        "target": {
                            "module_uid": target_identity["module_uid"],
                            "module_key": target_identity["module_key"],
                            "canonical_root_path": target_identity["canonical_root_path"],
                            "module_kind": target_identity["module_kind"],
                        },
                        "import_count": v.import_count,
                        "source_file_count": v.source_file_count,
                        "reason": v.reason,
                    }))
                })
                .collect();
            serde_json::json!(source_violations)
        } else {
            serde_json::Value::Null
        };

        // Build warnings
        let warnings: Vec<String> = violations_warning.into_iter().collect();

        // Build module identity
        let module_identity = serde_json::json!({
            "module_uid": resolved_module.module_candidate_uid,
            "module_key": resolved_module.module_key,
            "canonical_root_path": resolved_module.canonical_root_path,
            "module_kind": resolved_module.module_kind,
            "display_name": resolved_module.display_name,
            "confidence": resolved_module.confidence,
        });

        // Build response (no results array for show)
        let mut response = serde_json::json!({
            "command": "modules show",
            "repo": repo_uid,
            "snapshot": snapshot.snapshot_uid,
            "module": module_identity,
            "rollups": rollups_output,
            "outbound_dependencies": outbound_dependencies,
            "inbound_dependencies": inbound_dependencies,
            "violations": violations_output,
            "rollups_degraded": !violations_available,
            "warnings": warnings,
        });

        // Add evidence if non-empty
        if !evidence_output.is_empty() {
            response["evidence"] = serde_json::json!(evidence_output);
        }

        // Add trust overlay if degraded
        if let Some(trust) =
            compute_trust_overlay_for_snapshot(&repo_state.storage, &repo_uid, &snapshot, "IMPORTS")
        {
            if trust.has_degradation() || !trust.caveats.is_empty() {
                response["trust"] = serde_json::to_value(&trust).unwrap_or(serde_json::Value::Null);
            }
        }

        DispatchResult::success(&request.id, response)
    }

    fn handle_modules_list(&self, request: &Request) -> DispatchResult {
        use repo_graph_classification::module_rollup::{
            compute_module_rollups, DeadNodeFact, ModuleRollupInput, OwnedFileFact,
        };
        use repo_graph_module_queries::{evaluate_violations_from_facts, load_module_graph_facts};
        use std::collections::HashMap;

        // REG-1: resolve repo from path/alias and auto-load
        let (repo_state, repo_uid) = match self.resolve_and_load_repo(&request.params) {
            Ok(r) => r,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        // Acquire read lock
        let _read_guard = repo_state.coordinator.acquire_read();

        // Get latest snapshot
        let snapshot = match repo_state.storage.get_latest_snapshot(&repo_uid) {
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

        // Load module graph facts
        let facts = match load_module_graph_facts(&repo_state.storage, &snapshot.snapshot_uid) {
            Ok(f) => f,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(
                        ErrorCode::InternalError,
                        format!("failed to load module graph facts: {}", e),
                    ),
                );
            }
        };

        // Load dead nodes (SYMBOL kind only)
        let dead_nodes = repo_state
            .storage
            .find_dead_nodes(&snapshot.snapshot_uid, &repo_uid, Some("SYMBOL"))
            .unwrap_or_default();

        // Evaluate violations (advisory)
        let (violations_eval, violations_warning) =
            match evaluate_violations_from_facts(&repo_state.storage, &repo_uid, &facts) {
                Ok(r) => (Some(r.evaluation), None::<String>),
                Err(msg) => (
                    None,
                    Some(format!(
                        "discovered-module violation rollups unavailable: {}",
                        msg
                    )),
                ),
            };

        let violations_available = violations_eval.is_some();

        // Compute rollups
        let owned_file_facts: Vec<OwnedFileFact> = facts
            .context
            .owned_files
            .iter()
            .map(|f| OwnedFileFact {
                file_path: f.file_path.clone(),
                module_uid: f.module_candidate_uid.clone(),
                is_test: f.is_test,
            })
            .collect();

        let dead_node_facts: Vec<DeadNodeFact> = dead_nodes
            .into_iter()
            .filter_map(|d| {
                d.file.map(|file_path| DeadNodeFact {
                    file_path,
                    is_test: d.is_test,
                })
            })
            .collect();

        let violations_for_rollup = violations_eval
            .as_ref()
            .map(|e| e.violations.clone())
            .unwrap_or_default();

        let rollup_input = ModuleRollupInput {
            modules: facts.module_refs.clone(),
            owned_files: owned_file_facts.clone(),
            edges: facts.edges.clone(),
            violations: violations_for_rollup,
            dead_nodes: dead_node_facts,
        };

        let rollups = match compute_module_rollups(&rollup_input) {
            Ok(r) => r,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(
                        ErrorCode::InternalError,
                        format!("failed to compute rollups: {}", e),
                    ),
                );
            }
        };

        // Build rollup lookup by module_uid
        let rollup_map: HashMap<&str, _> =
            rollups.iter().map(|r| (r.module_uid.as_str(), r)).collect();

        // Build results with module identity + rollup stats
        let results: Vec<serde_json::Value> = facts
            .context
            .modules
            .iter()
            .map(|m| {
                let rollup = rollup_map.get(m.module_candidate_uid.as_str());
                serde_json::json!({
                    "module_uid": m.module_candidate_uid,
                    "module_key": m.module_key,
                    "canonical_root_path": m.canonical_root_path,
                    "module_kind": m.module_kind,
                    "display_name": m.display_name,
                    "confidence": m.confidence,
                    "owned_file_count": rollup.map_or(0, |r| r.owned_file_count),
                    "owned_test_file_count": rollup.map_or(0, |r| r.owned_test_file_count),
                    "outbound_dependency_count": rollup.map_or(0, |r| r.outbound_dependency_count),
                    "outbound_import_count": rollup.map_or(0, |r| r.outbound_import_count),
                    "inbound_dependency_count": rollup.map_or(0, |r| r.inbound_dependency_count),
                    "inbound_import_count": rollup.map_or(0, |r| r.inbound_import_count),
                    "violation_count": if violations_available {
                        serde_json::json!(rollup.map_or(0, |r| r.violation_count))
                    } else {
                        serde_json::Value::Null
                    },
                    "dead_symbol_count": rollup.map_or(0, |r| r.dead_symbol_count),
                    "dead_test_symbol_count": rollup.map_or(0, |r| r.dead_test_symbol_count),
                })
            })
            .collect();

        let count = results.len();

        // Compute sanity metrics (Phase 3.1)
        let sanity_metrics = compute_sanity_metrics_for_list(
            &results,
            &owned_file_facts,
            &facts,
            snapshot.files_total as u64,
            &repo_state.storage,
            &snapshot.snapshot_uid,
            &repo_uid,
        );

        // Build warnings
        let mut warnings: Vec<String> = violations_warning.into_iter().collect();

        if sanity_metrics
            .get("has_inferred_modules")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            warnings.push(
                "Module topology includes inferred modules (heuristic detection, not manifest-declared). \
                Some directories are intentionally excluded from module ownership. \
                Use `rmap modules unowned` to see classification of files without module assignment.".to_string()
            );
        }

        if let Some(breakdown) = sanity_metrics.get("unowned_breakdown") {
            if let Some(true_gap) = breakdown.get("true_gap_count").and_then(|v| v.as_u64()) {
                if true_gap > 0 {
                    warnings.push(format!(
                        "True heuristic gap: {} files could be owned but aren't. Run `rmap modules unowned` for details.",
                        true_gap
                    ));
                }
            }
        }

        // Build response
        let response = serde_json::json!({
            "command": "modules list",
            "repo": repo_uid,
            "snapshot": snapshot.snapshot_uid,
            "results": results,
            "count": count,
            "rollups_degraded": !violations_available,
            "sanity_metrics": sanity_metrics,
            "warnings": warnings,
        });

        DispatchResult::success(&request.id, response)
    }
}

/// Compute sanity metrics for modules list (Phase 3.1).
fn compute_sanity_metrics_for_list(
    results: &[serde_json::Value],
    owned_file_facts: &[repo_graph_classification::module_rollup::OwnedFileFact],
    facts: &repo_graph_module_queries::ModuleGraphFacts,
    total_files: u64,
    storage: &repo_graph_storage::StorageConnection,
    snapshot_uid: &str,
    repo_uid: &str,
) -> serde_json::Value {
    use std::collections::{HashMap, HashSet};

    // largest_module_ownership_pct
    let total_owned: u64 = results
        .iter()
        .filter_map(|r| r.get("owned_file_count").and_then(|v| v.as_u64()))
        .sum();
    let max_owned: u64 = results
        .iter()
        .filter_map(|r| r.get("owned_file_count").and_then(|v| v.as_u64()))
        .max()
        .unwrap_or(0);
    let largest_module_ownership_pct = if total_owned > 0 {
        (max_owned as f64 / total_owned as f64) * 100.0
    } else {
        0.0
    };

    // tiny_module_count (< 3 files)
    const TINY_THRESHOLD: u64 = 3;
    let tiny_module_count = results
        .iter()
        .filter(|r| {
            r.get("owned_file_count")
                .and_then(|v| v.as_u64())
                .unwrap_or(0)
                < TINY_THRESHOLD
        })
        .count() as u64;

    // root_fallback_used
    let root_fallback_used = results.iter().any(|r| {
        r.get("canonical_root_path")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            == "."
    });

    // has_inferred_modules
    let has_inferred_modules = results
        .iter()
        .any(|r| r.get("module_kind").and_then(|v| v.as_str()).unwrap_or("") == "inferred");

    // mixed_language_module_count
    let mut languages_per_module: HashMap<&str, HashSet<&str>> = HashMap::new();
    for file in owned_file_facts {
        let lang = infer_language_for_unowned(&file.file_path);
        languages_per_module
            .entry(file.module_uid.as_str())
            .or_default()
            .insert(lang);
    }
    let mixed_language_module_count = languages_per_module
        .values()
        .filter(|langs| langs.len() > 1)
        .count() as u64;

    // Compute unowned breakdown
    let unowned_breakdown = compute_unowned_breakdown_for_list(
        storage,
        snapshot_uid,
        repo_uid,
        facts,
        results,
        total_files,
    );

    serde_json::json!({
        "largest_module_ownership_pct": largest_module_ownership_pct,
        "tiny_module_count": tiny_module_count,
        "root_fallback_used": root_fallback_used,
        "mixed_language_module_count": mixed_language_module_count,
        "has_inferred_modules": has_inferred_modules,
        "unowned_breakdown": unowned_breakdown,
    })
}

/// Compute breakdown of unowned files for modules list.
fn compute_unowned_breakdown_for_list(
    storage: &repo_graph_storage::StorageConnection,
    snapshot_uid: &str,
    repo_uid: &str,
    facts: &repo_graph_module_queries::ModuleGraphFacts,
    results: &[serde_json::Value],
    total_files: u64,
) -> serde_json::Value {
    use std::collections::HashSet;

    // Get all file UIDs in snapshot
    let file_hashes = storage
        .query_file_version_hashes(snapshot_uid)
        .unwrap_or_default();

    // Build set of owned file UIDs
    let owned_uids: HashSet<&str> = facts
        .context
        .owned_files
        .iter()
        .map(|f| f.file_uid.as_str())
        .collect();

    // Build set of module root paths
    let module_roots: HashSet<&str> = results
        .iter()
        .filter_map(|r| r.get("canonical_root_path").and_then(|v| v.as_str()))
        .collect();

    let mut excluded_count: u64 = 0;
    let mut suppressed_test_count: u64 = 0;
    let mut true_gap_count: u64 = 0;

    for file_uid in file_hashes.keys() {
        if owned_uids.contains(file_uid.as_str()) {
            continue;
        }

        // Extract path from file_uid (format: repo_uid:path)
        let path = file_uid
            .strip_prefix(&format!("{}:", repo_uid))
            .unwrap_or(file_uid);

        // Only count source files
        if !is_source_file_for_unowned(path) {
            continue;
        }

        // Classify the unowned file
        let top_level = path.split('/').next().unwrap_or("");

        if is_excluded_directory(top_level) {
            excluded_count += 1;
        } else if is_test_directory(top_level) {
            suppressed_test_count += 1;
        } else if !path.contains('/') {
            // Root-level source file with no module
            true_gap_count += 1;
        } else if module_roots.contains(top_level) {
            // Under a module root but not owned - ownership bug
            true_gap_count += 1;
        } else {
            // Directory not recognized as module
            true_gap_count += 1;
        }
    }

    let true_gap_pct = if total_files > 0 {
        (true_gap_count as f64 / total_files as f64) * 100.0
    } else {
        0.0
    };

    serde_json::json!({
        "excluded_count": excluded_count,
        "suppressed_test_count": suppressed_test_count,
        "true_gap_count": true_gap_count,
        "true_gap_pct": true_gap_pct,
        "classified_pct": 100.0,
    })
}

// ── Unowned classification helpers (TECH DEBT: should be in shared module) ──

/// Classify why a file is unowned.
fn classify_unowned_reason(path: &str, module_roots: &std::collections::HashSet<&str>) -> String {
    // Check if file is at repo root (no directory)
    if !path.contains('/') {
        return "root_source_no_module".to_string();
    }

    // Get top-level directory
    let top_level = path.split('/').next().unwrap_or("");

    // Check if in an excluded directory
    if is_excluded_directory(top_level) {
        return format!("excluded_directory:{}", top_level);
    }

    // Check if parent would be a module root
    if module_roots.contains(top_level) {
        return "ownership_computation_gap".to_string();
    }

    // Check if it's a test directory that was suppressed
    if is_test_directory(top_level) {
        return "suppressed_test_directory".to_string();
    }

    format!("heuristic_gap:{}", top_level)
}

/// Check if a directory name is in the exclusion list.
fn is_excluded_directory(dir_name: &str) -> bool {
    let dir_lower = dir_name.to_lowercase();
    matches!(
        dir_lower.as_str(),
        "vendor"
            | "vendors"
            | "third_party"
            | "third-party"
            | "thirdparty"
            | "node_modules"
            | "bower_components"
            | "jspm_packages"
            | "external"
            | "externals"
            | "deps"
            | "dependencies"
            | "dist"
            | "build"
            | "builds"
            | "out"
            | "output"
            | "target"
            | "bin"
            | "obj"
            | "_build"
            | "generated"
            | "gen"
            | "codegen"
            | "auto"
            | "autogen"
            | "__generated__"
            | "docs"
            | "doc"
            | "documentation"
            | "man"
            | "manpages"
            | "javadoc"
            | "apidoc"
            | "apidocs"
            | "examples"
            | "example"
            | "samples"
            | "sample"
            | "demo"
            | "demos"
            | "tutorials"
            | "tutorial"
            | "benchmark"
            | "benchmarks"
            | "bench"
            | "benches"
            | "perf"
            | "performance"
    )
}

/// Check if a directory is a test directory.
fn is_test_directory(dir_name: &str) -> bool {
    let dir_lower = dir_name.to_lowercase();
    matches!(
        dir_lower.as_str(),
        "test" | "tests" | "testing" | "__tests__" | "spec" | "specs"
    )
}

/// Check if a file is a source file.
fn is_source_file_for_unowned(path: &str) -> bool {
    let ext = path.rsplit('.').next().unwrap_or("");
    matches!(
        ext.to_lowercase().as_str(),
        "c" | "h"
            | "cpp"
            | "hpp"
            | "cc"
            | "hh"
            | "cxx"
            | "hxx"
            | "java"
            | "kt"
            | "scala"
            | "py"
            | "rs"
            | "go"
            | "js"
            | "jsx"
            | "ts"
            | "tsx"
            | "mjs"
            | "cjs"
            | "rb"
            | "swift"
            | "m"
            | "mm"
    )
}

/// Infer language from file extension.
fn infer_language_for_unowned(path: &str) -> &'static str {
    let ext = path.rsplit('.').next().unwrap_or("");
    match ext.to_lowercase().as_str() {
        "c" | "h" => "c",
        "cpp" | "cc" | "cxx" | "hpp" | "hh" | "hxx" => "cpp",
        "java" => "java",
        "kt" | "kts" => "kotlin",
        "scala" => "scala",
        "py" | "pyi" => "python",
        "rs" => "rust",
        "go" => "go",
        "js" | "jsx" | "mjs" | "cjs" => "javascript",
        "ts" | "tsx" | "mts" | "cts" => "typescript",
        "rb" => "ruby",
        "swift" => "swift",
        "m" | "mm" => "objc",
        _ => "other",
    }
}

// ── Filter parsing helpers ──────────────────────────────────────────────

fn parse_channel_kind(s: &str) -> Option<repo_graph_boundary_interaction::ChannelKind> {
    use repo_graph_boundary_interaction::ChannelKind;
    match s.to_lowercase().as_str() {
        "unix_socket" | "unixsocket" | "unix" => Some(ChannelKind::UnixSocket),
        "named_pipe" | "namedpipe" | "fifo" => Some(ChannelKind::NamedPipe),
        "anonymous_pipe" | "anonymouspipe" | "pipe" => Some(ChannelKind::AnonymousPipe),
        "shared_memory" | "sharedmemory" | "shm" => Some(ChannelKind::SharedMemory),
        "message_queue" | "messagequeue" | "mq" | "mqueue" => Some(ChannelKind::MessageQueue),
        "semaphore" | "sem" => Some(ChannelKind::Semaphore),
        "process_signal" | "processsignal" | "signal" => Some(ChannelKind::ProcessSignal),
        "tcp_socket" | "tcpsocket" | "tcp" => Some(ChannelKind::TcpSocket),
        "udp_socket" | "udpsocket" | "udp" => Some(ChannelKind::UdpSocket),
        "shared_array_buffer" | "sharedarraybuffer" | "sab" | "atomics" => {
            Some(ChannelKind::SharedArrayBuffer)
        }
        "amqp_queue" | "amqpqueue" | "amqp" | "rabbitmq" => Some(ChannelKind::AmqpQueue),
        "kafka_topic" | "kafkatopic" | "kafka" => Some(ChannelKind::KafkaTopic),
        "nats_subject" | "natssubject" | "nats" => Some(ChannelKind::NatsSubject),
        "serial_port" | "serialport" | "serial" => Some(ChannelKind::SerialPort),
        "can_message" | "canmessage" | "can" => Some(ChannelKind::CanMessage),
        "inter_core_channel" | "intercorechannel" | "inter_core" => {
            Some(ChannelKind::InterCoreChannel)
        }
        _ => None,
    }
}

fn parse_boundary_scope(s: &str) -> Option<repo_graph_boundary_interaction::BoundaryScope> {
    use repo_graph_boundary_interaction::BoundaryScope;
    match s.to_lowercase().as_str() {
        "intra_process" | "intraprocess" | "thread" => Some(BoundaryScope::IntraProcess),
        "inter_process" | "interprocess" | "ipc" => Some(BoundaryScope::InterProcess),
        "inter_device" | "interdevice" | "device" => Some(BoundaryScope::InterDevice),
        "unknown" => Some(BoundaryScope::Unknown),
        _ => None,
    }
}

fn parse_direction(s: &str) -> Option<repo_graph_boundary_interaction::Direction> {
    use repo_graph_boundary_interaction::Direction;
    match s.to_lowercase().as_str() {
        "provider" | "server" | "listen" => Some(Direction::Provider),
        "consumer" | "client" | "connect" => Some(Direction::Consumer),
        "bidirectional" | "both" => Some(Direction::Bidirectional),
        _ => None,
    }
}

fn parse_protocol_family(s: &str) -> Option<repo_graph_boundary_interaction::ProtocolFamily> {
    use repo_graph_boundary_interaction::ProtocolFamily;
    match s.to_lowercase().as_str() {
        "socket" => Some(ProtocolFamily::Socket),
        "pipe" => Some(ProtocolFamily::Pipe),
        "shared_memory" | "sharedmemory" | "shm" => Some(ProtocolFamily::SharedMemory),
        "message_queue" | "messagequeue" | "mq" => Some(ProtocolFamily::MessageQueue),
        "signal" | "signals" | "process_signal" => Some(ProtocolFamily::Signal),
        "semaphore" | "sem" => Some(ProtocolFamily::Semaphore),
        "inter_core" | "intercore" => Some(ProtocolFamily::InterCore),
        "serial" => Some(ProtocolFamily::Serial),
        "bus" => Some(ProtocolFamily::Bus),
        "message_broker" | "messagebroker" | "broker" => Some(ProtocolFamily::MessageBroker),
        "usb" => Some(ProtocolFamily::Usb),
        "bluetooth" | "bt" | "ble" => Some(ProtocolFamily::Bluetooth),
        "custom" => Some(ProtocolFamily::Custom),
        _ => None,
    }
}

/// Map an ExtractedFact to a NewSemanticFact for storage.
fn map_extracted_to_storage(
    repo_uid: &str,
    fact: &repo_graph_doc_facts::ExtractedFact,
) -> repo_graph_storage::crud::semantic_facts::NewSemanticFact {
    repo_graph_storage::crud::semantic_facts::NewSemanticFact {
        repo_uid: repo_uid.to_string(),
        fact_kind: fact.fact_kind.as_str().to_string(),
        subject_ref: fact.subject_ref.clone(),
        subject_ref_kind: fact.subject_ref_kind.as_str().to_string(),
        object_ref: fact.object_ref.clone(),
        object_ref_kind: fact.object_ref_kind.map(|k| k.as_str().to_string()),
        source_file: fact.source_file.clone(),
        source_line_start: fact.line_start.map(|n| n as i64),
        source_line_end: fact.line_end.map(|n| n as i64),
        source_text_excerpt: fact.excerpt.clone(),
        content_hash: fact.content_hash.clone(),
        extraction_method: fact.extraction_method.as_str().to_string(),
        confidence: fact.confidence,
        generated: fact.generated,
        doc_kind: fact.doc_kind.as_str().to_string(),
    }
}
