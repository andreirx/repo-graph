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

/// Seed dispatch surface (EMBED-SEED-IMPL-1 §8/§8B): the `find` handler, the semantic
/// fallback tier, and the `next.cwd` canonical-root resolver. A CHILD module (extracted
/// for the 500-line guardrail, review-6 #2) so the seed responsibility does not grow the
/// already-oversized dispatcher; named `seed_dispatch` — NOT `seed` — so it never reads
/// as the top-level `crate::seed` domain module. Its `pub(super)` items are wired below.
#[path = "dispatch_seed.rs"]
mod seed_dispatch;

/// INDEX-BASIS-1: attach a computed serving fact (`index_drift`, `parse_status`)
/// onto the serialized envelope's `value` object, so rgr's response structs capture
/// it via one `#[serde(default)]` field. Additive: if `value` is somehow not an
/// object, or serialization fails, the field is simply absent (logged) — the rest of
/// the response is untouched. Two concrete callers: `index_drift` (orient + explain)
/// and `parse_status` (orient); axis = per-field additive `value` enrichment; the
/// rejected simpler alternative (one bespoke fn per field) duplicated this
/// object-get/insert/log logic.
pub(crate) fn inject_value_field<T: serde::Serialize>(
    output: &mut serde_json::Value,
    key: &str,
    value: &T,
    repo_uid: &str,
) {
    match serde_json::to_value(value) {
        Ok(v) => {
            if let Some(value_obj) = output.get_mut("value").and_then(|v| v.as_object_mut()) {
                value_obj.insert(key.to_string(), v);
            } else {
                eprintln!(
                    "warning: {key} not attached (envelope `value` missing/not object) for {repo_uid}"
                );
            }
        }
        Err(e) => eprintln!("warning: could not serialize {key} for {repo_uid}: {e}"),
    }
}

/// INDEX-BASIS-1 (review-0 fix #2): the honest `parse` footer axis for orient,
/// computed from the SAME `get_stale_files` read that drives `check`'s
/// `UNPARSED_FILES` condition. A FAILED read is `Unknown` WITH the reason — never
/// `Ok`/zero (standing honesty rule: a rendered fallible read is unknown, not zero).
pub(crate) fn compute_parse_status(
    storage: &StorageConnection,
    snapshot_uid: &str,
) -> repo_graph_agent::dto::parse_status::ParseStatus {
    use repo_graph_agent::dto::parse_status::ParseStatus;
    match repo_graph_agent::AgentStorageRead::get_stale_files(storage, snapshot_uid) {
        Ok(files) if files.is_empty() => ParseStatus::Ok,
        Ok(files) => ParseStatus::Unparsed {
            count: files.len() as u64,
        },
        Err(e) => ParseStatus::Unknown {
            reason: e.to_string(),
        },
    }
}

/// RMAPD-PERF-1 / PERF-INSTRUMENTATION-1: performance tracing macro.
///
/// Emits to stderr (the daemon log) when EITHER the compile-time `perf-trace`
/// feature is built (force-on, unchanged legacy behavior) OR the RUNTIME gate
/// `RMAP_PERF` is at level >= 1 — so an already-installed daemon binary can emit
/// `[PERF]` markers without a `--features` rebuild, just by relaunching with
/// `RMAP_PERF=1` in its environment.
///
/// When off, the only cost is a single relaxed atomic load (`perf_enabled`); the
/// `cfg!` constant short-circuits so a force-on build pays nothing. The shared
/// process-global gate lives in `repo-graph-repo-index` (the lowest crate both
/// this crate's `perf_trace!` and repo-index's `perf_log!` already reach).
macro_rules! perf_trace {
    ($($arg:tt)*) => {
        if cfg!(feature = "perf-trace") || repo_graph_repo_index::perf::perf_enabled() {
            eprintln!($($arg)*);
        }
    };
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

    /// FOREGROUND-LOCK-1 (spec 2.2): open a repo's storage for a FOREGROUND request with bounded
    /// lock patience, re-coding an exhausted transient lock as `Busy` + holder naming (never
    /// `InternalError`). The single choke the foreground read handlers use in place of the bare
    /// `repo_state.storage()` + `ErrorDetail::new(ErrorCode::InternalError, e)` wrap. Delegates to
    /// `crate::foreground_open`, passing the daemon's activity registry so the exhausted-patience
    /// message can name the holder class from a stored fact.
    fn open_storage(
        &self,
        repo_state: &crate::state::RepoState,
    ) -> Result<StorageConnection, ErrorDetail> {
        self.state.open_repo_storage_for_request(repo_state)
    }

    /// FOREGROUND-LOCK-1 (§2.2/§2.3): the SPLIT peer of [`Self::open_storage`], for the write
    /// handlers whose SECONDARY open (`handle_enrich`, `handle_docs_extract`) has a DISTINCT
    /// pre-existing non-lock error message. Same bounded patience + honest `Busy` re-code, but a
    /// genuine non-lock fault comes back RAW so the caller preserves its own §2.3 message verbatim
    /// (never the shared "failed to open storage connection: …"). Delegates to
    /// `crate::foreground_open` with the daemon's activity registry.
    fn open_storage_split(
        &self,
        repo_state: &crate::state::RepoState,
    ) -> Result<StorageConnection, crate::foreground_open::ForegroundOpenFault> {
        self.state.open_repo_storage_for_request_split(repo_state)
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

    /// PERF-INSTRUMENTATION-1: best-effort repo label for the per-request marker.
    ///
    /// Read methods usually resolve from cwd and may carry no repo in params
    /// (`-`); write/index methods carry `repo_path`/`alias`. This is a log label
    /// only — never a trust, freshness, or ownership signal.
    fn perf_repo_label(params: &Value) -> &str {
        params
            .get("repo_path")
            .or_else(|| params.get("alias"))
            .or_else(|| params.get("repo_uid"))
            .and_then(|v| v.as_str())
            .unwrap_or("-")
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

    /// DAEMON-VISIBILITY-1 (F2): the shared "no READY snapshot" error for EVERY READY-requiring
    /// dispatch surface (orient/explain/callers/callees/imports/stats/cycles/path/trust/gate/resource/
    /// contracts/inferences/deps/surfaces/boundaries/modules and the cycle-completeness audit).
    ///
    /// # Why this helper exists (abstraction ledger)
    ///
    /// - **What:** builds ONE `SnapshotNotFound` `ErrorDetail` whose message is the honest F2 text
    ///   (`snapshot_facts::no_ready_snapshot_message`) — NAMES an existing non-READY partial (state,
    ///   when, on-disk size) + both next actions, or the plain "index it first" when the repo was
    ///   genuinely never indexed.
    /// - **Concrete current users:** the 34 READY-requiring surfaces above (32 that formerly emitted the
    ///   bare `"no snapshot found"` + orient + explain, which previously each hand-built the message).
    /// - **Named axis of variation:** none — this is DEDUP of one identical `(code, honest-message)`
    ///   pairing across 34 sites. The force is the reviewer's consistency requirement: the bare message
    ///   was the exact copy-paste that reintroduced the day-2 gaslighting; a single definition removes
    ///   the vector.
    /// - **Rejected simpler alternative:** inline `ErrorDetail::new(SnapshotNotFound,
    ///   no_ready_snapshot_message(..))` at all 34 sites — rejected: 34× duplication of a non-trivial
    ///   (query-any-state + stat + format) pairing, and a fresh copy-paste site could silently reinstate
    ///   the bare message.
    ///
    /// `SnapshotNotFound` (not `InternalError`) is the honest code: a missing READY snapshot is an
    /// expected user-facing state, not an internal fault. Every non-`RepoNotFound` code renders on the
    /// client as `error: <code>: <message>` (verified in `daemon_command::print_daemon_error` and the
    /// orient/explain inline handlers), so the honest message reaches the reader verbatim.
    ///
    /// Pure w.r.t. `self` (associated fn): callers pass the `storage`/`db_path`/`repo_uid` already in
    /// scope. Runs only on the cold no-READY-snapshot error path.
    fn no_ready_snapshot_detail(
        storage: &StorageConnection,
        db_path: &Path,
        repo_uid: &str,
    ) -> ErrorDetail {
        ErrorDetail::new(
            ErrorCode::SnapshotNotFound,
            crate::snapshot_facts::no_ready_snapshot_message(storage, db_path, repo_uid),
        )
    }
}

impl Dispatcher for ServiceDispatcher {
    fn dispatch(&self, request: &Request, emitter: &mut dyn ProgressEmitter) -> DispatchResult {
        // PERF-INSTRUMENTATION-1: per-request wall-clock. dispatch() is the single
        // choke point every request (socket + stdio) flows through, so timing it
        // here makes the SERIAL daemon's per-request cost — and any slow op like a
        // kernel-scale `index` — self-identify in the log instead of timing out
        // silently. When RMAP_PERF is off this is one relaxed atomic load + branch.
        let req_start = Instant::now();
        let result = match request.method.as_str() {
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
            "cycle_completeness_audit" => self.handle_cycle_completeness_audit(request, emitter),
            "imports" => self.handle_imports(request),
            // RMAPD-PERF-1: These operations emit heartbeat for long queries
            "stats" => self.handle_stats(request, emitter),
            "cycles" => self.handle_cycles(request, emitter),
            // DAEMON-CANCEL-1: path now receives the emitter so its LiveGraph BFS can
            // checkpoint mid-search (it previously dispatched without one).
            "path" => self.handle_path(request, emitter),

            // ── Agent services ──────────────────────────────────────
            // RMAPD-PERF-1: These operations emit heartbeat for long queries
            "orient" => self.handle_orient(request, emitter),
            "check" => self.handle_check(request, emitter),
            "explain" => self.handle_explain(request, emitter),
            // EMBED-SEED-IMPL-1 (spec §8B): affirmative concept search.
            "find" => self.handle_find(request, emitter),

            // ── Deterministic MAP.md facts (MAP-FROM-INDEX-1) ────────
            // Flat extracted facts for the `rmap map` renderer; no model call.
            "map" => crate::handlers::map::handle_map(&self.state, request),

            // ── Trust and governance ────────────────────────────────
            // RMAPD-PERF-1: trust emits heartbeat for long queries
            "trust" => self.handle_trust(request, emitter),
            // RESOLUTION-BREAKDOWN-CLI-1: per-language/per-module call-resolution
            // breakdown — the decomposition of the aggregate reliability figure.
            // Handler extracted to handlers/reliability.rs (dispatch stays wiring).
            "reliability" => crate::handlers::reliability::handle_reliability(&self.state, request),
            "gate" => self.handle_gate(request),

            // ── Quality queries (LEGACY-CONTRACT-MIGRATION-1B) ──────
            // Handlers extracted to handlers/quality.rs
            "churn" => crate::handlers::quality::handle_churn(&self.state, request),
            "hotspots" => crate::handlers::quality::handle_hotspots(&self.state, request),
            "risk" => crate::handlers::quality::handle_risk(&self.state, request),
            "coverage" => crate::handlers::quality::handle_coverage(&self.state, request),
            // DEAD-CAUSES-1: derived facts for `rmap dead`'s refusal "Root causes" (READ-only).
            "dead_causes" => crate::handlers::quality::handle_dead_causes(&self.state, request),

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
            // FORGET-REPO-1 §2.3: reclaim orphan DB files + stray sidecars, list dead-path entries.
            "maintenance_gc" => self.handle_maintenance_gc(request),

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
        };

        perf_trace!(
            "[PERF] req {} {}: {}ms",
            request.method,
            Self::perf_repo_label(&request.params),
            req_start.elapsed().as_millis()
        );
        result
    }
}

// ── Method handlers ─────────────────────────────────────────────────

impl ServiceDispatcher {
    /// Return daemon-level diagnostic information.
    ///
    /// STATE-ROOT-SEPARATION-1: Reports state root mode and authority write policy.
    /// DOCTOR-RESOURCE-REPORT: Reports the daemon's own resident memory (RSS) and the
    /// total on-disk size of its `databases/` state root across all repos.
    ///
    /// Request: `{"method": "daemon_info", "params": {}}`
    ///
    /// Response:
    /// ```json
    /// {
    ///   "state_root": "/path/to/state",
    ///   "state_root_mode": "global" | "sandbox-local",
    ///   "authority_writes_allowed": true | false,
    ///   "rss_bytes": 47448064,            // current RSS (live footprint), or null
    ///   "rss_peak_bytes": 81788928,       // peak RSS high-water mark, or null
    ///   "databases_total_bytes": 31244288,// sum of databases/ (all repos), or null
    ///   "repo_count": 3                   // registered repos
    /// }
    /// ```
    ///
    /// The three byte metrics are `null` (UNKNOWN) when the platform/filesystem read
    /// is genuinely unavailable; `databases_total_bytes` is `0` (known-zero) for an
    /// empty-but-readable dir. `repo_count` is always known. `rmap doctor` renders
    /// these and must keep the health verdict green when a metric is unavailable.
    fn handle_daemon_info(&self, request: &Request) -> DispatchResult {
        let state_root = self
            .state
            .registry()
            .state_root()
            .to_string_lossy()
            .to_string();
        let mode = self.state.state_root_mode();

        // DOCTOR-RESOURCE-REPORT: the daemon measures ITSELF — its live resident memory
        // (did the in-memory LiveGraph substrate balloon?) and the total disk its
        // warm state occupies. Mechanism lives in `resource_metrics` (kept out of this
        // oversized file per the structural guardrail); each read degrades to `None`.
        let rss_bytes = crate::resource_metrics::current_rss_bytes();
        let rss_peak_bytes = crate::resource_metrics::peak_rss_bytes();
        let registry = self.state.registry();
        let repo_count = registry.list().len() as u64;
        let db_dir = registry.db_dir().to_path_buf();
        // FORGET-REPO-1 §2.2: snapshot the registry entries for the orphan scan (below, after the
        // registry guard is dropped — the scan does I/O and must not hold the registry Mutex).
        let orphan_entries: Vec<crate::registry::RegistryEntry> =
            registry.list().into_iter().cloned().collect();
        // DAEMON-VISIBILITY-1 (D2): the most-recently-completed snapshot across all repos, for the
        // idle "idle; last snapshot <repo> @ <time>" doctor line. Sourced from the registry's
        // `last_indexed_at` (set ONLY on a successful index — `record_index` runs in `handle_index`'s
        // Ok arm), so it is an honest "last completed" fact with NO DB open / no lock. ISO8601
        // (`toISOString`) sorts lexicographically = chronologically, so `max_by_key` on the timestamp
        // picks the latest. `repo` is the reader-frame display name (alias, else path basename) — never
        // the internal `repo_uid`. `None` (no repo ever indexed) renders as a bare "idle".
        let last_snapshot = registry
            .list()
            .iter()
            .filter(|e| e.last_indexed_at.is_some())
            .max_by_key(|e| e.last_indexed_at.clone().unwrap_or_default())
            .map(|e| {
                let repo = e.alias.clone().unwrap_or_else(|| {
                    e.canonical_path
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or(&e.repo_uid)
                        .to_string()
                });
                serde_json::json!({ "repo": repo, "at": e.last_indexed_at })
            });
        drop(registry);
        let databases_total_bytes = crate::resource_metrics::directory_size_bytes(&db_dir);

        // FORGET-REPO-1 §2.2: orphan-storage reconciliation for `rmap doctor` — the three classes
        // (orphan DB files + bytes, dead-path registry entries, stray sidecars). Cheap directory
        // listing; a listing failure is reported as `scan_error` (unknown, never rendered as zero).
        let orphans = crate::reclaim::scan_orphans(&db_dir, &orphan_entries).to_json();

        // DAEMON-VISIBILITY-1 (D): the daemon's current activity. Lock-light (a brief Vec mutex,
        // no repo read guard) so this stays responsive DURING an index — which is exactly when
        // `rmap doctor` and the still-running client probe (C) ask "what are you doing?". Empty
        // array = idle (known-empty, never a false "nothing indexed").
        let active_operations: Vec<serde_json::Value> = self
            .state
            .activity()
            .snapshot()
            .iter()
            .map(|op| op.to_json())
            .collect();

        DispatchResult::success(
            &request.id,
            serde_json::json!({
                "state_root": state_root,
                "state_root_mode": mode.as_str(),
                "authority_writes_allowed": mode.allows_authority_writes(),
                // DOCTOR-RESOURCE-REPORT additive fields (Option -> number | null):
                "rss_bytes": rss_bytes,
                "rss_peak_bytes": rss_peak_bytes,
                "databases_total_bytes": databases_total_bytes,
                "repo_count": repo_count,
                // DAEMON-VISIBILITY-1 (D) additive fields:
                "active_operations": active_operations,
                // Idle "last snapshot <repo> @ <time>" fact (null when no repo ever indexed).
                "last_snapshot": last_snapshot,
                // SNAPSHOT-RETENTION-1: the most-recent background retention pass outcome (null until
                // the daemon has run one) — the honesty surface for "pruned N / reclaimed X".
                "last_retention": self.state.last_retention_json(),
                // ENRICH-LIFECYCLE-1: the most-recent background enrichment pass outcome (null until
                // one completes) — the lifecycle surface for "enriched N / promoted P / skipped: …".
                "last_enrichment": self.state.last_enrichment_json(),
                // Whether auto-enrichment is enabled on this daemon (RMAP_AUTO_ENRICH) — lets the
                // doctor render the "disabled" lifecycle state honestly (slice §3.7).
                "enrichment_enabled": crate::enrich_pass::auto_enrich_enabled(),
                // ENRICH-LIFECYCLE-1 (slice §3.7): whether an auto pass is currently queued/running,
                // so doctor tells "queued" from the false "none yet — runs after the next index"
                // (review-0 item 1). "idle" | "queued" | "running".
                "enrichment_activity": self.state.enrich_coord().activity_state(),
                // FORGET-REPO-1 §2.2: orphan-storage classes (orphan DB files + bytes, dead-path
                // registry entries, stray sidecars) — the `rmap doctor` orphan-storage line.
                "orphans": orphans,
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

                // DAEMON-VISIBILITY-1 (F): per-snapshot state/outcome/size for `rmap repo info`.
                // Nested under `storage` (additive — existing fields unchanged). Short-circuits to
                // "in use by daemon" during an active index rather than erroring on the busy open.
                let storage_facts = crate::snapshot_facts::collect_snapshot_facts(
                    &self.state,
                    Path::new(&entry.db_path),
                    &entry.repo_uid,
                );

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
                        "storage": storage_facts,
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

    /// FORGET-REPO-1 §2.1: forget everything repo-graph created for a repo — registry entry +
    /// in-memory state + `db_runtimes` slot + `.db`/`-wal`/`-shm` + `<repo>/.rgr/`. FORGETS by
    /// default (operator-ratified 2026-08-23; supersedes REG-1's keep-by-default). `--keep-db`
    /// (`keep_db: true`) leaves the DB file; the legacy `delete_db` param is accepted and ignored (it
    /// is now the default). REFUSES (deletes nothing) while a write op is in flight.
    ///
    /// The mechanism lives in `reclaim::forget_repo`; this handler is thin wiring. Transport shape:
    /// - repo not found → `RepoNotFound` error.
    /// - refused (in-flight write) → `StateUnavailable` error with the reason (no partial deletion).
    /// - otherwise → success with the full per-artifact `removed | absent | failed` report and an
    ///   `ok` flag; the CLI picks the exit code (non-zero on any `failed`).
    ///
    /// Request: `{"method": "repo_remove", "params": {"repo": "<alias_or_path>", "keep_db": false}}`
    fn handle_repo_remove(&self, request: &Request) -> DispatchResult {
        let repo_ref = match Self::get_string_param(&request.params, "repo") {
            Ok(r) => r,
            Err(e) => return DispatchResult::error(&request.id, e),
        };
        // `--keep-db` opts out of forget-by-default. `delete_db` is accepted as a no-op (muscle
        // memory) — deletion is the default now, so a `delete_db` request needs no handling.
        let keep_db = request
            .params
            .get("keep_db")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let entry = match self.state.resolve_alias_or_path(repo_ref) {
            Some(entry) => entry,
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

        let report = crate::reclaim::forget_repo(&self.state, &entry, keep_db);
        if let Some(reason) = &report.refused {
            // No partial deletion happened — a clear error the CLI surfaces as a non-zero exit.
            return DispatchResult::error(
                &request.id,
                ErrorDetail::new(ErrorCode::StateUnavailable, reason.clone()),
            );
        }
        DispatchResult::success(&request.id, report.to_json())
    }

    /// FORGET-REPO-1 §2.3: reclaim orphan DB files + stray sidecars (classes A + C), reporting bytes;
    /// LIST dead-path registry entries (class B) with their `rmap repo remove` next action — never
    /// auto-remove them (a path may be a temporarily-unmounted volume). `dry_run` lists without
    /// deleting. Daemon-global (not repo-scoped): reads the registry + `databases/` dir; the
    /// mechanism lives in `reclaim::run_gc`.
    ///
    /// Request: `{"method": "maintenance_gc", "params": {"dry_run": false}}`
    fn handle_maintenance_gc(&self, request: &Request) -> DispatchResult {
        let dry_run = request
            .params
            .get("dry_run")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        // `run_gc` reads the registry + `databases/` itself and guards each unlink with the target
        // DB's write slot + a live registry recheck (operator ruling 2), so it must own `&self.state`.
        let outcome = crate::reclaim::run_gc(&self.state, dry_run);
        DispatchResult::success(&request.id, outcome.to_json())
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
        // DAEMON-CONCURRENCY-IMPL-1 (D-W = W-A + the #2b coordination fix): preload SWAPS the in-memory
        // LiveGraph, so under concurrent accept it MUST hold the repo coordinator as a writer — otherwise it
        // could swap the graph under a live reader (the read-guard exclusion W-A relies on is defeated by an
        // uncoordinated LiveGraph writer). Mirror handle_refresh: DB write lock, then repo refresh lock.
        let db_runtime = match self.state.get_or_create_db_runtime(repo_state.db_path()) {
            Ok(r) => r,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e),
                )
            }
        };
        let _db_write_guard = db_runtime.acquire_write();
        let _refresh_guard = repo_state.coordinator.acquire_refresh();
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
        // DAEMON-CONCURRENCY-IMPL-1 (D-W = W-A + the #2b coordination fix): refresh REBUILDS + SWAPS the
        // in-memory LiveGraph, so under concurrent accept it MUST hold the repo coordinator as a writer (same
        // reason as preload; mirror handle_refresh). Held across ALL refresh branches below so no branch can
        // swap the LiveGraph under a live reader.
        let db_runtime = match self.state.get_or_create_db_runtime(repo_state.db_path()) {
            Ok(r) => r,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e),
                )
            }
        };
        let _db_write_guard = db_runtime.acquire_write();
        let _refresh_guard = repo_state.coordinator.acquire_refresh();
        // CYCLES-COMPLETENESS-ENUMERATION-1 (D2): `--all-discovered` loads the SHARED discovery's INCLUDED
        // roots (the SAME function the read-only audit's EXPECTED set uses -> they cannot drift). Best-effort
        // multi-refresh; the load step that lets the audit advance past IncompleteMissingPartitions. The
        // mutation lives HERE (refresh), never in the audit. Reports what it excluded (fixtures) + included.
        let all_discovered = request
            .params
            .get("all_discovered")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if all_discovered {
            let include_fixtures = request
                .params
                .get("include_fixtures")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let discovered =
                crate::partition_discovery::discover_partition_roots(&repo_path, include_fixtures);
            let mut body = crate::livegraph_refresh::run_refresh_multi(
                &repo_state,
                &repo_uid,
                &repo_path,
                &discovered.included,
            );
            if let Some(obj) = body.as_object_mut() {
                obj.insert(
                    "discovered_included".to_string(),
                    serde_json::json!(discovered.included),
                );
                obj.insert(
                    "excluded_fixture_partitions".to_string(),
                    serde_json::json!(discovered
                        .excluded
                        .iter()
                        .map(|(d, r)| serde_json::json!({ "dir": d, "reason": r }))
                        .collect::<Vec<_>>()),
                );
            }
            return DispatchResult::success(&request.id, body);
        }
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

    /// CYCLES-COMPLETENESS-AUDIT-1 (dev-only, READ-ONLY): build the module-cycle completeness BASELINE
    /// (filesystem tsconfig discovery + the SQLite language inventory, AT THE AUDIT BOUNDARY) and report the
    /// SQLite-free certificate for the CURRENT in-memory LiveGraph. Does NOT refresh/load partitions (the
    /// caller loads them first via `livegraph_refresh`) and changes NO default. Mirrors the ratified
    /// boundary: the audit reads SQLite; the certificate evaluator never does.
    fn handle_cycle_completeness_audit(
        &self,
        request: &Request,
        emitter: &mut dyn ProgressEmitter,
    ) -> DispatchResult {
        let (repo_state, repo_uid) = match self.resolve_and_load_repo(&request.params) {
            Ok(r) => r,
            Err(e) => return DispatchResult::error(&request.id, e),
        };
        let repo_root = Self::get_optional_string_param(&request.params, "repo")
            .unwrap_or("")
            .to_string();
        // ENUMERATION-1 (D3): opt-in to certify a fixture corpus (disables the fixture-segment exclusion).
        let include_fixtures = request
            .params
            .get("include_fixtures")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        // DAEMON-CONCURRENCY-IMPL-1 + W-B-EPOCH-IMPL-2D: this dev-only mixed-read handler reads SQLite
        // (snapshot-pinned `find_cycles` + the snapshot-pinned language inventory) AND the LiveGraph (module
        // cycles). Under W-A the read guard excludes a concurrent refresh/preload for the request's whole
        // duration; this slice ALSO captures a RAW resident-LiveGraph epoch-IDENTITY witness so the audit
        // stays coherent once IMPL-3 relaxes W-B — it verifies the identity did not move across the whole
        // audit, else honest-degrades (never a false incompleteness from SQLite@N vs LiveGraph@N+1).
        let _read_guard = repo_state.coordinator.acquire_read();
        let storage = match self.open_storage(&repo_state) {
            Ok(s) => s,
            Err(e) => return DispatchResult::error(&request.id, e),
        };
        let snapshot = match storage.get_latest_snapshot(&repo_uid) {
            Ok(Some(snap)) => snap,
            Ok(None) => {
                return DispatchResult::error(
                    &request.id,
                    Self::no_ready_snapshot_detail(&storage, repo_state.db_path(), &repo_uid),
                );
            }
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                );
            }
        };
        // Capture the audit's single-coherent-epoch IDENTITY ONCE under the read guard: the pinned snapshot uid
        // (which pins BOTH SQLite reads — `find_cycles` AND the snapshot-scoped language baseline) + the RAW
        // resident LiveGraph fingerprint (GREEN-or-RED — an epoch-identity witness, NOT the GREEN
        // serve-eligibility one; the audit MUST run on RED repos). INFALLIBLE: it reads only the in-memory
        // LiveGraph. Mirrors the other handlers' capture-under-guard, with a raw-identity witness distinct from
        // `RequestEpoch.fingerprint`.
        let epoch = crate::cycle_completeness_audit::AuditEpoch::capture(
            &repo_state,
            &snapshot.snapshot_uid,
        );
        // DAEMON-CANCEL-1: thread a cooperative checkpoint into the audit's two Tarjan SCC traversals (the
        // LiveGraph module cycles + the SQLite module cycles), so a peer disconnect surfaces as Cancelled
        // (mapped to `ErrorCode::Cancelled`, never `InternalError`) — like the other Tarjan-running handlers.
        let mut checkpoint = crate::cancel::loop_checkpoint(emitter, "cycle_completeness_audit");
        match crate::cycle_completeness_audit::cycle_completeness_audit_response(
            &repo_state,
            &storage,
            &repo_uid,
            &epoch,
            &repo_root,
            include_fixtures,
            &mut checkpoint,
        ) {
            Ok(v) => DispatchResult::success(&request.id, v),
            Err(repo_graph_storage::error::StorageError::Cancelled) => DispatchResult::error(
                &request.id,
                ErrorDetail::new(
                    ErrorCode::Cancelled,
                    "cycle completeness audit cancelled (client disconnected during traversal)",
                ),
            ),
            Err(e) => DispatchResult::error(
                &request.id,
                ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
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
        let storage = match self.open_storage(&repo_state) {
            Ok(s) => s,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        // RECON-M-R2 (flag-gated, non-default): union serving rides the `Auto` engine arm ONLY.
        // The engine parse moves ABOVE the epoch capture (pure param parsing, no side effects) so
        // the capture function can be chosen per arm: flag-ON `Auto` captures the
        // LEDGER-validity-gated `callgraph_union_eligibility` (verdict-independent — §4.2); every
        // other combination keeps the GREEN-gated capture + serve byte-exact.
        let engine = crate::livegraph_feed::Engine::parse(Self::get_optional_string_param(
            &request.params,
            "engine",
        ));
        let union_serving = crate::union_serve::union_serving_enabled()
            && engine == crate::livegraph_feed::Engine::Auto;

        // W-B-EPOCH-IMPL-1: capture the request epoch ONCE (the pinned `AgentSnapshot` via the
        // `AgentStorageRead` trait + the BUILD-THEN-PEEK CALLGRAPH-cert LG-serve eligibility). The callers
        // handler already resolved the snapshot exactly once and threaded its uid to every SQLite read, so
        // there is NO double-resolve here — the epoch just adds the EV-A serve gate (`epoch.fingerprint`).
        let epoch =
            match repo_graph_agent::AgentStorageRead::get_latest_snapshot(&storage, &repo_uid) {
                Ok(Some(snapshot)) => {
                    let fingerprint = if union_serving {
                        crate::callgraph_cert::callgraph_union_eligibility(
                            &repo_state,
                            &snapshot.snapshot_uid,
                        )
                    } else {
                        crate::callgraph_cert::callgraph_cert_eligibility(
                            &repo_state,
                            &snapshot.snapshot_uid,
                        )
                    };
                    crate::livegraph_feed::RequestEpoch {
                        snapshot,
                        fingerprint,
                    }
                }
                Ok(None) => {
                    return DispatchResult::error(
                        &request.id,
                        Self::no_ready_snapshot_detail(&storage, repo_state.db_path(), &repo_uid),
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
        let target = match storage.resolve_symbol(epoch.snapshot_uid(), symbol) {
            Ok(sym) => sym,
            Err(SymbolResolveError::NotFound) => {
                // EMBED-SEED-IMPL-1 (spec §8, Group B): fire the semantic tier on the
                // deterministic-zero NotFound — additive `data` (candidates + hint) on
                // the UNCHANGED `symbol not found` error (code/message/exit byte-stable).
                return DispatchResult::error(
                    &request.id,
                    self.symbol_not_found_with_semantic(
                        &storage,
                        epoch.snapshot_uid(),
                        &repo_uid,
                        repo_state.db_path(),
                        &request.params,
                        "callers",
                        symbol,
                    ),
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

        // QUERY-AUTO-LAZY-SQLITE-1: the SQLite read is now LAZY -- a closure the engine_response calls ONLY
        // when LiveGraph cannot serve (or for --engine sqlite/compare). The LiveGraph-served default SKIPS it.
        let edge_types = ["CALLS"];
        let repo_root = Self::get_optional_string_param(&request.params, "repo")
            .unwrap_or("")
            .to_string();
        let value = if union_serving {
            // RECON-M-R2: the flag-ON `Auto` arm — union rows in W-BOTH activation; today's exact
            // fallback bytes everywhere else (the shared `callers_auto_or_sqlite` builder).
            crate::union_serve::callers_union_response(&repo_state, &epoch, &target, || {
                storage.find_direct_callers(epoch.snapshot_uid(), &target.stable_key, &edge_types)
            })
        } else {
            crate::livegraph_feed::callers_engine_response(
                engine,
                &repo_state,
                &epoch,
                &target,
                || {
                    storage.find_direct_callers(
                        epoch.snapshot_uid(),
                        &target.stable_key,
                        &edge_types,
                    )
                },
                symbol,
                &repo_root,
            )
        };
        match value {
            Ok(mut v) => {
                // RECON-M-R3b: attach the INCOMING reference tier ("which symbols reference this")
                // — data-driven, W-BOTH only; additive beside the call rows (never touches `count`
                // or the call multiset). No-op outside a current measured ledger (R-0/R-1).
                Self::attach_reference_tier(
                    &repo_state,
                    epoch.snapshot_uid(),
                    &target.stable_key,
                    crate::witness_projection::ReferenceDirection::Incoming,
                    &mut v,
                );
                DispatchResult::success(&request.id, v)
            }
            Err(e) => DispatchResult::error(
                &request.id,
                ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
            ),
        }
    }

    /// RECON-M-R3b: attach the reference tier to a callers/callees response value (the shared
    /// path both drilldown arms use; explain uses [`WitnessProjection::attach_explain_reference_tier`]).
    /// Additive `references` block when W-BOTH has a current measured ledger with ≥1 non-withheld
    /// reference; a no-op (byte-identical) otherwise — the R-0/R-1 absence.
    fn attach_reference_tier(
        repo_state: &crate::state::RepoState,
        snapshot_uid: &str,
        target_key: &str,
        direction: crate::witness_projection::ReferenceDirection,
        value: &mut serde_json::Value,
    ) {
        if let Some(block) = crate::witness_projection::WitnessProjection::reference_tier_block(
            repo_state,
            snapshot_uid,
            target_key,
            direction,
        ) {
            if let Some(obj) = value.as_object_mut() {
                obj.insert("references".to_string(), block);
            }
        }
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
        let storage = match self.open_storage(&repo_state) {
            Ok(s) => s,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        // RECON-M-R2 (flag-gated): engine parse above the capture; the flag-ON `Auto` arm captures
        // the LEDGER-validity-gated eligibility and serves the union (see handle_callers).
        let engine = crate::livegraph_feed::Engine::parse(Self::get_optional_string_param(
            &request.params,
            "engine",
        ));
        let union_serving = crate::union_serve::union_serving_enabled()
            && engine == crate::livegraph_feed::Engine::Auto;

        // W-B-EPOCH-IMPL-1: capture the request epoch ONCE (pinned `AgentSnapshot` + the BUILD-THEN-PEEK
        // CALLGRAPH-cert eligibility). Like callers, callees already resolved once — the epoch adds the EV-A
        // serve gate (`epoch.fingerprint`).
        let epoch =
            match repo_graph_agent::AgentStorageRead::get_latest_snapshot(&storage, &repo_uid) {
                Ok(Some(snapshot)) => {
                    let fingerprint = if union_serving {
                        crate::callgraph_cert::callgraph_union_eligibility(
                            &repo_state,
                            &snapshot.snapshot_uid,
                        )
                    } else {
                        crate::callgraph_cert::callgraph_cert_eligibility(
                            &repo_state,
                            &snapshot.snapshot_uid,
                        )
                    };
                    crate::livegraph_feed::RequestEpoch {
                        snapshot,
                        fingerprint,
                    }
                }
                Ok(None) => {
                    return DispatchResult::error(
                        &request.id,
                        Self::no_ready_snapshot_detail(&storage, repo_state.db_path(), &repo_uid),
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
        let target = match storage.resolve_symbol(epoch.snapshot_uid(), symbol) {
            Ok(sym) => sym,
            Err(SymbolResolveError::NotFound) => {
                // EMBED-SEED-IMPL-1 (spec §8, Group B): fire the semantic tier on the
                // deterministic-zero NotFound (additive `data`; error otherwise unchanged).
                return DispatchResult::error(
                    &request.id,
                    self.symbol_not_found_with_semantic(
                        &storage,
                        epoch.snapshot_uid(),
                        &repo_uid,
                        repo_state.db_path(),
                        &request.params,
                        "callees",
                        symbol,
                    ),
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

        // QUERY-AUTO-LAZY-SQLITE-1: LAZY SQLite read -- the closure runs ONLY when LiveGraph cannot serve (or
        // for --engine sqlite/compare). The LiveGraph-served default SKIPS it.
        let edge_types = ["CALLS"];
        let repo_root = Self::get_optional_string_param(&request.params, "repo")
            .unwrap_or("")
            .to_string();
        let value = if union_serving {
            crate::union_serve::callees_union_response(&repo_state, &epoch, &target, || {
                storage.find_direct_callees(epoch.snapshot_uid(), &target.stable_key, &edge_types)
            })
        } else {
            crate::livegraph_feed::callees_engine_response(
                engine,
                &repo_state,
                &epoch,
                &target,
                || {
                    storage.find_direct_callees(
                        epoch.snapshot_uid(),
                        &target.stable_key,
                        &edge_types,
                    )
                },
                symbol,
                &repo_root,
            )
        };
        match value {
            Ok(mut v) => {
                // RECON-M-R3b: attach the OUTGOING reference tier ("which symbols this references")
                // — data-driven, W-BOTH only; additive beside the call rows. No-op outside a
                // current measured ledger (R-0/R-1).
                Self::attach_reference_tier(
                    &repo_state,
                    epoch.snapshot_uid(),
                    &target.stable_key,
                    crate::witness_projection::ReferenceDirection::Outgoing,
                    &mut v,
                );
                DispatchResult::success(&request.id, v)
            }
            Err(e) => DispatchResult::error(
                &request.id,
                ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
            ),
        }
    }

    fn handle_imports(&self, request: &Request) -> DispatchResult {
        // REG-1: resolve repo from path/alias and auto-load (display_name for the livegraph feed).
        let (repo_state, repo_uid, display_name) =
            match self.resolve_and_load_repo_with_display_name(&request.params) {
                Ok(r) => r,
                Err(e) => return DispatchResult::error(&request.id, e),
            };
        // IMPORTS-LIVEGRAPH-DEFAULT-1 (D2=B): engine routing. The DEFAULT is now `auto` -- LiveGraph-first with
        // a per-call directional no-loss compare + a labelled SQLite fallback. `sqlite` (EXPLICIT) = the
        // unchanged single-file listing (the escape hatch); `livegraph` / `compare` = the read-model / compare
        // surfaces (unchanged, D6).
        let engine = Self::get_optional_string_param(&request.params, "engine").unwrap_or("auto");

        // Acquire read lock
        let _read_guard = repo_state.coordinator.acquire_read();
        let storage = match self.open_storage(&repo_state) {
            Ok(s) => s,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        // Get latest snapshot. W-B-EPOCH-IMPL-2A: resolve the AGENT `AgentSnapshot` DTO (the
        // `AgentStorageRead` trait — same READY-row selection as the inherent method) so the `auto` path can
        // wrap it in a `RequestEpoch`. The explicit engines below use only `snapshot.snapshot_uid` (a &str),
        // unchanged.
        let snapshot =
            match repo_graph_agent::AgentStorageRead::get_latest_snapshot(&storage, &repo_uid) {
                Ok(Some(snap)) => snap,
                Ok(None) => {
                    return DispatchResult::error(
                        &request.id,
                        Self::no_ready_snapshot_detail(&storage, repo_state.db_path(), &repo_uid),
                    );
                }
                Err(e) => {
                    return DispatchResult::error(
                        &request.id,
                        ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                    );
                }
            };

        if engine == "livegraph" {
            // D6: file filter OPTIONAL (None -> repo-wide). repo_root + include_fixtures feed the
            // module-cycle completeness certificate (the named `module_cycle_*` trust fields).
            let repo_root = Self::get_optional_string_param(&request.params, "repo")
                .unwrap_or("")
                .to_string();
            let include_fixtures = request
                .params
                .get("include_fixtures")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let file_filter = Self::get_optional_string_param(&request.params, "file");
            return DispatchResult::success(
                &request.id,
                crate::livegraph_feed::imports_view_response(
                    &repo_state,
                    &repo_uid,
                    &display_name,
                    &snapshot.snapshot_uid,
                    &repo_root,
                    include_fixtures,
                    file_filter,
                ),
            );
        }
        if engine == "compare" {
            // IMPORTS-LIVEGRAPH-DEFAULT-READINESS-1 (D6) + REPOWIDE-1 (D6): NO file -> the repo-wide readiness
            // aggregate; WITH file -> the per-file SQLite-vs-LiveGraph compare. SQLite primary; no default flip.
            match Self::get_optional_string_param(&request.params, "file") {
                Some(file_path) => {
                    return DispatchResult::success(
                        &request.id,
                        crate::livegraph_feed::imports_compare_response(
                            &repo_state,
                            &repo_uid,
                            &snapshot.snapshot_uid,
                            file_path,
                        ),
                    );
                }
                None => {
                    return DispatchResult::success(
                        &request.id,
                        crate::livegraph_feed::imports_readiness_response(
                            &repo_state,
                            &repo_uid,
                            &display_name,
                            &snapshot.snapshot_uid,
                        ),
                    );
                }
            }
        }
        if engine != "sqlite" && engine != "auto" {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::invalid_request(format!(
                    "unsupported imports engine: {engine} (expected auto|sqlite|livegraph|compare)"
                )),
            );
        }

        // ---- engine = auto (DEFAULT) or sqlite (explicit): single-file (file REQUIRED + must exist). ----
        let file_path = match Self::get_string_param(&request.params, "file") {
            Ok(f) => f,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        // Construct FILE stable key
        let file_stable_key = format!("{}:{}:FILE", repo_uid, file_path);

        // Verify file exists (the existing "file not found" contract is preserved for BOTH auto + sqlite).
        match storage.node_exists(&snapshot.snapshot_uid, &file_stable_key) {
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

        if engine == "auto" {
            // IMPORTS-LIVEGRAPH-DEFAULT-1 (D2=B): LiveGraph-first with the per-call no-loss compare + a labelled
            // SQLite fallback (backend_used / fallback_reason are JSON-only; the human render strips them).
            // W-B-EPOCH-IMPL-2A (SC-B): capture the request epoch ONCE — the pinned snapshot + the BUILD-THEN-PEEK
            // import-cert eligibility witness (`import_cert_eligibility`) — and serve under it. The EV-A gate in
            // `imports_auto_response` fails soft to the pinned SQLite snapshot on a fingerprint mismatch, closing
            // the capture-view-then-lazy-cert-build TOCTOU the decision-review found.
            let fingerprint = crate::livegraph_feed::import_cert_eligibility(
                &repo_state,
                &repo_uid,
                &snapshot.snapshot_uid,
            );
            let epoch = crate::livegraph_feed::RequestEpoch {
                snapshot,
                fingerprint,
            };
            return DispatchResult::success(
                &request.id,
                crate::livegraph_feed::imports_auto_response(
                    &repo_state,
                    &repo_uid,
                    &epoch,
                    file_path,
                ),
            );
        }

        // ---- engine = sqlite (EXPLICIT escape hatch): the existing listing, UNCHANGED (no backend metadata). --
        let imports = match storage.find_imports(&snapshot.snapshot_uid, &file_stable_key) {
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
        let storage = match self.open_storage(&repo_state) {
            Ok(s) => s,
            Err(e) => return DispatchResult::error(&request.id, e),
        };
        let lock_ms = lock_start.elapsed().as_millis();

        // Get latest snapshot. W-B-EPOCH-IMPL-2B: resolve the AGENT `AgentSnapshot` DTO (the
        // `AgentStorageRead` trait — same READY-row selection as the inherent method) so the `auto` path can
        // wrap it in a `RequestEpoch`. The explicit engines below use only `snapshot.snapshot_uid` (a &str),
        // unchanged.
        let snapshot_start = Instant::now();
        let snapshot =
            match repo_graph_agent::AgentStorageRead::get_latest_snapshot(&storage, &repo_uid) {
                Ok(Some(snap)) => snap,
                Ok(None) => {
                    return DispatchResult::error(
                        &request.id,
                        Self::no_ready_snapshot_detail(&storage, repo_state.db_path(), &repo_uid),
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

        // ── HONEST-DEGRADATION-IMPL-1 (D1 + D4): snapshot-level honesty signals ──────────────────────
        // Computed ONCE here and injected into EVERY engine path's response, because both are
        // engine-independent — they describe the snapshot, not the backend. Reusing the SAME
        // `compute_repo_summary` / `compute_trust_overlay_for_snapshot` that `orient`/`trust` already
        // consume makes the three surfaces report ONE coherent symbol count and ONE coherent
        // import-graph posture (the cross-surface incoherence this slice exists to remove).
        //
        // D4 — `total_symbols`: the repo-level all-SYMBOL COUNT(*) (the number `orient` shows), NOT a
        // per-module row-sum (rows are module-owned; a sum loses symbols in unowned files). On a
        // summary error we OMIT the field rather than inject 0 — a false zero is the very bug we fix.
        // D4 + D5 (IMPL-2) share the repo-level summary: D4 needs the all-SYMBOL count, D5 the languages.
        let repo_summary = repo_graph_agent::AgentStorageRead::compute_repo_summary(
            &storage,
            &snapshot.snapshot_uid,
        )
        .ok();
        let total_symbols_field: Option<u64> = repo_summary.as_ref().map(|s| s.symbol_count);

        // D1 + D5 (IMPL-2) share the snapshot reliability overlay (the SAME axis `trust`/`orient`
        // consume), computed ONCE here. Overlay-assembly failure -> None -> no caveat / no next-action: we
        // never fabricate a posture we could not compute (honest degradation, not silence).
        let overlay = match storage.get_latest_snapshot(&repo_uid) {
            Ok(Some(snap)) => {
                compute_trust_overlay_for_snapshot(&storage, &repo_uid, &snap, "CALLS+IMPORTS")
            }
            _ => None,
        };
        // D1 — the import-graph reliability axis (level + reasons), serialized to JSON HERE so the
        // response sites need not name the trust type. The renderer renders a reason-specific caveat
        // above the dependency sections when level != HIGH.
        let import_graph_reliability_field: Option<serde_json::Value> = overlay
            .as_ref()
            .and_then(|o| serde_json::to_value(&o.reliability.import_graph).ok());
        // D5 (HONEST-DEGRADATION-IMPL-2) — the toolchain-aware next-action line, keyed on the daemon's
        // CONFIGURED resolvers (`configured_resolver_languages_from_env` — the SAME source `handle_enrich`
        // registers from) × the repo's languages. `None` unless a relationship axis is LOW (no noise on a
        // resolved repo). The SAME helper backs `orient`, so the two surfaces render ONE coherent line.
        // CONTRADICTION-SWEEP-1 §5: the next-action is now keyed on per-language FILE COUNTS (the ≥10%
        // material-share gate), not the DISTINCT-language list — so an incidental tooling language (e.g.
        // django's ~3.7% JS) cannot trip a false enrich CTA. Counts are read ONLY when a relationship
        // axis is LOW (guarding the query off the healthy hot path). review-1 #1: a counts-read FAILURE
        // renders an UNKNOWN-WITH-REASON next-action (via `relationship_next_action_line_or_read_error`),
        // NOT a silent omission — the reader is owed a remedy on a LOW axis, and a dropped read would
        // misclassify the repo as "nothing to say". The read is only issued on the LOW branch.
        let relationship_next_action: Option<String> = overlay.as_ref().and_then(|o| {
            if !relationship_reliability_is_low(&o.reliability) {
                return None;
            }
            let language_counts = repo_graph_agent::AgentStorageRead::query_file_count_by_language(
                &storage,
                &snapshot.snapshot_uid,
            )
            .map_err(|e| e.to_string());
            relationship_next_action_line_or_read_error(
                &o.reliability,
                language_counts,
                &configured_resolver_languages_from_env(),
            )
        });

        // MODULE-MODEL-2 §13 D4/D7: per-toolchain manifest roots (already-stored
        // module_candidates ⋈ evidence.source_type), read ONCE here — BEFORE
        // `storage` may move into the SQLite worker below — and folded into the
        // COMPLETE package-group set inside `inject_stats_summary_fields`. The SAME
        // shared `rollup_package_groups` + manifest facts `orient` uses, so the two
        // surfaces cannot report divergent topology.
        //
        // MODULE-MODEL-2 review-0 #2 — "couldn't read" is NOT "doesn't exist": a
        // READ FAILURE must PROPAGATE as a request error, exactly like the sibling
        // `storage`/`snapshot` reads above (and exactly like `orient`, whose
        // `module_summary` aggregator reaches the same `list_manifest_roots` via `?`
        // and whose handler maps that `Err` to `DispatchResult::error`). Silently
        // swallowing it to an empty set would render directory grouping as though
        // manifests were genuinely absent — a false honest-degradation claim that
        // could also diverge from `orient` (which errors on the same failure). A
        // GENUINELY-EMPTY `Ok(vec![])` (no manifest facts indexed — a C/manifest-less
        // tree, or the raw-indexer path) still degrades HONESTLY to directory/JVM
        // grouping inside the fold; only the read-failure path errors.
        let manifest_roots = match repo_graph_agent::AgentStorageRead::list_manifest_roots(
            &storage,
            &snapshot.snapshot_uid,
        ) {
            Ok(roots) => roots,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                );
            }
        };

        // RECON-M-R3a (g1u, §5.3.2): the ADDITIVE union-accounting call block from the shared
        // witness projection, computed ONCE and injected on EVERY engine path below (the four
        // bodies share `inject_stats_summary_fields`, so fastpath and SQLite carry the identical
        // additive field — the stats cert's byte-freeze covers only the `stats` rows, which this
        // never touches). `None` outside W-BOTH-with-current-ledger → field absent (R-0).
        let witnesses_field: Option<serde_json::Value> =
            crate::witness_projection::WitnessProjection::compute(
                &repo_state,
                &snapshot.snapshot_uid,
            )
            .and_then(|p| p.g1u_block());

        // STATS-LIVEGRAPH-IMPL-1: engine routing. DEFAULT (no flags == `auto`) = the cert-gated LiveGraph
        // module-stats FASTPATH (`stats_auto_response`): serves the LiveGraph stats WITHOUT
        // `compute_module_stats` on a GREEN repo cert at the current fingerprint, else a labelled SQLite
        // fallback (byte-identical human output). EXPLICIT `--engine sqlite` = the forced SQLite path below
        // (rule 7: UNCHANGED escape hatch, no `backend_used`). `livegraph` = the forced LiveGraph diagnostic
        // (no fallback); `compare` = SQLite primary + a field-exact divergence report + sidecar.
        let engine = Self::get_optional_string_param(&request.params, "engine").unwrap_or("auto");
        match engine {
            "auto" => {
                // DAEMON-CANCEL-2: the DEFAULT path. On a GREEN cert it serves the LiveGraph stats
                // (no SQL). On the SQLite fallback (non-resident / non-`Exact` LiveGraph, or a RED/stale
                // cert) it runs `compute_module_stats` under the supervisor, so a peer-disconnect
                // mid-aggregation aborts the in-flight SELECT and returns `Cancelled`.
                //
                // W-B-EPOCH-IMPL-2B (SC-B): capture the request epoch ONCE — the pinned snapshot + the
                // BUILD-THEN-PEEK stats-cert eligibility witness (`stats_cert_eligibility`, which runs the
                // cert build under the SAME supervisor so it still cancels mid-aggregation) — and serve
                // under it. The EV-A gate in `stats_auto_response` fails soft to the pinned SQLite snapshot
                // on a fingerprint mismatch, closing the capture-LG-stats-then-lazy-cert-build TOCTOU.
                let fingerprint = match crate::livegraph_feed::stats_cert_eligibility(
                    emitter,
                    &repo_state,
                    &snapshot.snapshot_uid,
                ) {
                    crate::livegraph_feed::StatsEligibility::Witness(fp) => fp,
                    crate::livegraph_feed::StatsEligibility::Cancelled => {
                        return DispatchResult::error(
                            &request.id,
                            ErrorDetail::new(
                                ErrorCode::Cancelled,
                                "stats query cancelled (client disconnected during aggregation)",
                            ),
                        );
                    }
                };
                let epoch = crate::livegraph_feed::RequestEpoch {
                    snapshot,
                    fingerprint,
                };
                return match crate::livegraph_feed::stats_auto_response(
                    emitter,
                    &repo_state,
                    &repo_uid,
                    &display_name,
                    &epoch,
                ) {
                    Ok(crate::livegraph_feed::StatsOutcome::Ready(mut v)) => {
                        Self::inject_stats_summary_fields(
                            &mut v,
                            total_symbols_field,
                            import_graph_reliability_field.as_ref(),
                            relationship_next_action.as_deref(),
                            &manifest_roots,
                            witnesses_field.as_ref(),
                        );
                        DispatchResult::success(&request.id, v)
                    }
                    Ok(crate::livegraph_feed::StatsOutcome::Cancelled) => DispatchResult::error(
                        &request.id,
                        ErrorDetail::new(
                            ErrorCode::Cancelled,
                            "stats query cancelled (client disconnected during aggregation)",
                        ),
                    ),
                    Err(e) => DispatchResult::error(
                        &request.id,
                        ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                    ),
                };
            }
            "livegraph" => {
                let mut v = crate::livegraph_feed::stats_livegraph_response(
                    &repo_state,
                    &repo_uid,
                    &display_name,
                    &snapshot.snapshot_uid,
                );
                Self::inject_stats_summary_fields(
                    &mut v,
                    total_symbols_field,
                    import_graph_reliability_field.as_ref(),
                    relationship_next_action.as_deref(),
                    &manifest_roots,
                    witnesses_field.as_ref(),
                );
                return DispatchResult::success(&request.id, v);
            }
            "compare" => {
                let repo_root = Self::get_optional_string_param(&request.params, "repo")
                    .unwrap_or("")
                    .to_string();
                return match crate::livegraph_feed::stats_compare_response(
                    emitter,
                    &repo_state,
                    &repo_uid,
                    &display_name,
                    &snapshot.snapshot_uid,
                    &repo_root,
                ) {
                    Ok(crate::livegraph_feed::StatsOutcome::Ready(mut v)) => {
                        Self::inject_stats_summary_fields(
                            &mut v,
                            total_symbols_field,
                            import_graph_reliability_field.as_ref(),
                            relationship_next_action.as_deref(),
                            &manifest_roots,
                            witnesses_field.as_ref(),
                        );
                        DispatchResult::success(&request.id, v)
                    }
                    Ok(crate::livegraph_feed::StatsOutcome::Cancelled) => DispatchResult::error(
                        &request.id,
                        ErrorDetail::new(
                            ErrorCode::Cancelled,
                            "stats query cancelled (client disconnected during aggregation)",
                        ),
                    ),
                    Err(e) => DispatchResult::error(
                        &request.id,
                        ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                    ),
                };
            }
            // EXPLICIT `--engine sqlite` (or any non-routed value) -> the forced SQLite path below (UNCHANGED).
            _ => {}
        }

        // EXPLICIT `--engine sqlite` ONLY (the DEFAULT `auto` returned via the fastpath arm above). The
        // forced SQLite module-stats answer. SUCCESS output is UNCHANGED (rule 7): the canonical body
        // with NO `backend_used`.
        //
        // DAEMON-CANCEL-2: `compute_module_stats` is a SINGLE heavy SQL aggregation the worker blocks
        // INSIDE, with no Rust frame to poll a cooperative cancel flag (unlike cycles/path, whose heavy
        // work is a Rust loop the transport thread checkpoints; CANCEL-1). It cancels via
        // `sqlite3_interrupt` through the SHARED `cancellable_module_stats` chokepoint — the SAME
        // mechanism the default `auto`/`compare` paths use, so "stats cancels mid-execution" is one
        // wiring, not three. `storage` is THIS handler's own owned per-op connection (B1 D-S=S-A); the
        // helper hoists its interrupt handle out BEFORE moving the connection into the worker (the
        // slice's key design point). Read-only ⇒ a cancelled query has no partial state to roll back.
        let query_start = Instant::now();

        // Cheap handler-boundary layer (mirrors cycles/path): if the peer is already gone, skip the
        // worker entirely and report "before" (vs the "during" the supervisor reports mid-aggregation).
        if crate::cancel::pre_work_check(emitter, "computing_module_stats").is_break() {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::new(
                    ErrorCode::Cancelled,
                    "stats query cancelled (client disconnected before aggregation)",
                ),
            );
        }

        let stats = match crate::livegraph_feed::cancellable_module_stats(
            emitter,
            storage,
            &snapshot.snapshot_uid,
        ) {
            Ok(crate::livegraph_feed::SqlStats::Stats(s)) => s,
            Ok(crate::livegraph_feed::SqlStats::Cancelled) => {
                // Peer disconnected mid-aggregation; the interrupt aborted the in-flight statement.
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(
                        ErrorCode::Cancelled,
                        "stats query cancelled (client disconnected during aggregation)",
                    ),
                );
            }
            // A genuine SQL/storage failure, OR a vanished worker (internal teardown) — both INTERNAL
            // failures, classified as such (CANCEL-1 deliverable #2), NEVER masqueraded as a client
            // cancel. (`cancellable_module_stats` maps `WorkerVanished` to a storage error.)
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

        let mut body = serde_json::json!({
            "repo_uid": repo_uid,
            "snapshot_uid": snapshot.snapshot_uid,
            "display_name": display_name,
            "stats": stats,
            "count": stats.len(),
        });
        Self::inject_stats_summary_fields(
            &mut body,
            total_symbols_field,
            import_graph_reliability_field.as_ref(),
            relationship_next_action.as_deref(),
            &manifest_roots,
            witnesses_field.as_ref(),
        );
        DispatchResult::success(&request.id, body)
    }

    /// HONEST-DEGRADATION-IMPL-1 (D1 + D4): attach the snapshot-level honesty fields to a `stats`
    /// response body. Shared across every engine path so the human/JSON output is coherent regardless
    /// of backend. `total_symbols` (D4) is the repo-level all-SYMBOL count (== `orient`); it is
    /// OMITTED when unavailable — never injected as `0` (a false zero is the bug we fix).
    /// `import_graph_reliability` (D1) is the axis the renderer gates its dependency caveat on; it is
    /// OMITTED when the overlay could not be assembled (no fabricated posture). Both are additive; the
    /// human renderer strips them, surfacing only the derived Summary total + the caveat.
    ///
    /// `relationship_next_action` (D5, IMPL-2) is the toolchain-aware honest next-action line; OMITTED
    /// when absent (no LOW relationship axis, or no honest statement applies). The human renderer renders
    /// it beneath the dependency caveat.
    ///
    /// `package_groups` (MODULE-MODEL-2 §13 D4/D7) is the COMPLETE folded topology: the per-directory
    /// `stats` rows this body already carries, folded through the SAME shared `rollup_package_groups` +
    /// `manifest_roots` `orient` uses — so the two surfaces cannot diverge. Folded here (one point, all
    /// four engine paths) reading `body["stats"]`. The JSON carries the WHOLE set; the human renderer
    /// bounds it (top-N + omission). Empty `manifest_roots` → directory/JVM grouping (honest degradation).
    fn inject_stats_summary_fields(
        body: &mut serde_json::Value,
        total_symbols: Option<u64>,
        import_graph_reliability: Option<&serde_json::Value>,
        relationship_next_action: Option<&str>,
        manifest_roots: &[repo_graph_agent::ManifestRoot],
        witnesses: Option<&serde_json::Value>,
    ) {
        let Some(obj) = body.as_object_mut() else {
            return;
        };
        if let Some(total) = total_symbols {
            obj.insert("total_symbols".to_string(), serde_json::json!(total));
        }
        // RECON-M-R3a (g1u): the additive, coverage-labeled union call block — OMITTED when
        // absent (never an empty/zero placeholder; R-0 byte-identity outside W-BOTH).
        if let Some(block) = witnesses {
            obj.insert("witnesses".to_string(), block.clone());
        }
        if let Some(axis) = import_graph_reliability {
            obj.insert("import_graph_reliability".to_string(), axis.clone());
        }
        if let Some(line) = relationship_next_action {
            obj.insert(
                "relationship_next_action".to_string(),
                serde_json::json!(line),
            );
        }
        // Fold the per-directory topology into the COMPLETE package-group set. The
        // `stats` array (present on every engine path) is the authoritative
        // population (§13 D2); mapping it to `DirGroup` here keeps the fold
        // daemon-side + shared with orient. `file_count` is clamped non-negative.
        let dirs: Vec<repo_graph_agent::DirGroup> = obj
            .get("stats")
            .and_then(|s| s.as_array())
            .map(|rows| {
                rows.iter()
                    .filter_map(|r| {
                        let path = r.get("module")?.as_str()?.to_string();
                        let file_count = r
                            .get("file_count")
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0)
                            .max(0) as u64;
                        Some(repo_graph_agent::DirGroup { path, file_count })
                    })
                    .collect()
            })
            .unwrap_or_default();
        let groups = repo_graph_agent::rollup_package_groups(&dirs, manifest_roots);
        let groups_json: Vec<serde_json::Value> = groups
            .iter()
            .map(|g| {
                serde_json::json!({
                    "name": g.name,
                    "file_count": g.file_count,
                    "test_file_count": g.test_file_count,
                })
            })
            .collect();
        obj.insert("package_groups".to_string(), serde_json::json!(groups_json));
        // MODULE-MODEL-2 (ROOT-MANIFEST-POLYGLOT, ratified 2026-07-12): surface the
        // one-line limitation marker when a repo-root manifest was suppressed by the
        // conservative rule (nested roots coexist) — the SAME shared
        // `root_manifest_limitation` orient's aggregator uses, so the two surfaces
        // carry the identical line. Inserted only when Some (a genuine single-package
        // or manifest-less repo carries no marker); the human renderer + JSON both
        // read it.
        if let Some(line) = repo_graph_agent::root_manifest_limitation(manifest_roots) {
            obj.insert(
                "root_manifest_limitation".to_string(),
                serde_json::json!(line),
            );
        }
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
        let storage = match self.open_storage(&repo_state) {
            Ok(s) => s,
            Err(e) => return DispatchResult::error(&request.id, e),
        };
        let lock_ms = lock_start.elapsed().as_millis();

        // Get latest snapshot. W-B-EPOCH-IMPL-2B: resolve the AGENT `AgentSnapshot` DTO (the
        // `AgentStorageRead` trait — same READY-row selection as the inherent method) so the `auto` path can
        // wrap it in a `RequestEpoch`. The explicit engines below use only `snapshot.snapshot_uid` (a &str),
        // unchanged.
        let snapshot_start = Instant::now();
        let snapshot =
            match repo_graph_agent::AgentStorageRead::get_latest_snapshot(&storage, &repo_uid) {
                Ok(Some(snap)) => snap,
                Ok(None) => {
                    return DispatchResult::error(
                        &request.id,
                        Self::no_ready_snapshot_detail(&storage, repo_state.db_path(), &repo_uid),
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

        // CYCLES-LIVEGRAPH-CLI-1 + CYCLES-LIVEGRAPH-DEFAULT-FASTPATH-1: engine/kind routing. DEFAULT
        // (no flags == `auto`) = the cert-gated LiveGraph MODULE-cycle fastpath (`cycles_auto_response`).
        // EXPLICIT `--engine sqlite` = the forced SQLite MODULE-import path below (rule 7: UNCHANGED escape
        // hatch). `livegraph` + `file-import` = the LiveGraph captured FILE import-cycle graph (a DIFFERENT
        // question; NO SQLite fallback — D7). The daemon rejects unsupported combos defensively (D2/D6).
        let engine = Self::get_optional_string_param(&request.params, "engine").unwrap_or("auto");
        let kind = Self::get_optional_string_param(&request.params, "kind").unwrap_or("");

        // DAEMON-CANCEL-1: map a cancellable cycles route's Result onto the dispatch
        // envelope. A checkpoint Break surfaces as `StorageError::Cancelled` →
        // `ErrorCode::Cancelled` with a "during" message (proving the cancel fired
        // IN-LOOP, not at the handler boundary); any other storage error is
        // `InternalError` (worker/storage failure is NEVER mislabelled as a client
        // cancel). Shared by all four Tarjan-running cycles routes below.
        let cyc_result = |r: Result<Value, repo_graph_storage::error::StorageError>| match r {
            Ok(v) => DispatchResult::success(&request.id, v),
            Err(repo_graph_storage::error::StorageError::Cancelled) => DispatchResult::error(
                &request.id,
                ErrorDetail::new(
                    ErrorCode::Cancelled,
                    "cycles query cancelled (client disconnected during traversal)",
                ),
            ),
            Err(e) => DispatchResult::error(
                &request.id,
                ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
            ),
        };

        match (engine, kind) {
            ("livegraph", "file-import") => {
                // DAEMON-CANCEL-1: thread the in-loop checkpoint into the file-import Tarjan.
                let mut checkpoint = crate::cancel::loop_checkpoint(emitter, "finding_cycles");
                return cyc_result(crate::livegraph_feed::file_import_cycles_response(
                    &repo_state,
                    &repo_uid,
                    &display_name,
                    &snapshot.snapshot_uid,
                    &mut checkpoint,
                ));
            }
            // MODULE-CYCLES-CLI-1 (D2): LiveGraph directory-aggregated MODULE cycles (no SQLite fallback).
            ("livegraph", "module-import") => {
                // DAEMON-CANCEL-1: thread the in-loop checkpoint into the module-import Tarjan.
                let mut checkpoint = crate::cancel::loop_checkpoint(emitter, "finding_cycles");
                return cyc_result(crate::livegraph_feed::module_import_cycles_response(
                    &repo_state,
                    &repo_uid,
                    &display_name,
                    &snapshot.snapshot_uid,
                    &mut checkpoint,
                ));
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
                // DAEMON-CANCEL-1: thread the in-loop checkpoint into the compare's two Tarjan loops.
                let mut checkpoint = crate::cancel::loop_checkpoint(emitter, "finding_cycles");
                return cyc_result(crate::livegraph_feed::module_cycle_compare_response(
                    &repo_state,
                    &repo_uid,
                    &display_name,
                    &snapshot.snapshot_uid,
                    &repo_root,
                    &mut checkpoint,
                ));
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
            // CYCLES-LIVEGRAPH-DEFAULT-FASTPATH-1: the DEFAULT (`auto`, no kind or module-import) -> the
            // cert-gated LiveGraph fastpath. Serves the LiveGraph module cycles WITHOUT `find_cycles` when a
            // valid GREEN repo no-loss certificate holds at the current fingerprint; ELSE the canonical SQLite
            // answer (byte-identical, CYCLES-OUTPUT-CONTRACT-1) with a labelled `fallback_reason`.
            ("auto", "") | ("auto", "module-import") => {
                // DAEMON-CANCEL-1: thread the in-loop checkpoint into the DEFAULT route's Tarjan
                // loops — the LiveGraph module-cycle SCC and (on fallback) the SQLite SCC.
                let mut checkpoint = crate::cancel::loop_checkpoint(emitter, "finding_cycles");
                // W-B-EPOCH-IMPL-2B (SC-B): capture the request epoch ONCE — the pinned snapshot + the
                // BUILD-THEN-PEEK cycles-cert eligibility witness (`cycles_cert_eligibility`, which threads
                // the SAME checkpoint so the first-call cert build still cancels mid-Tarjan) — and serve
                // under it. The EV-A gate in `cycles_auto_response` fails soft to the pinned SQLite snapshot
                // on a fingerprint mismatch, closing the capture-LG-cycles-then-lazy-cert-build TOCTOU.
                let fingerprint = match crate::livegraph_feed::cycles_cert_eligibility(
                    &repo_state,
                    &snapshot.snapshot_uid,
                    &mut checkpoint,
                ) {
                    Ok(fp) => fp,
                    Err(e) => return cyc_result(Err(e)),
                };
                let epoch = crate::livegraph_feed::RequestEpoch {
                    snapshot,
                    fingerprint,
                };
                return cyc_result(crate::livegraph_feed::cycles_auto_response(
                    &repo_state,
                    &repo_uid,
                    &display_name,
                    &epoch,
                    &mut checkpoint,
                ));
            }
            // EXPLICIT `--engine sqlite` (no kind or module-import, D6) -> the forced SQLite MODULE-import path
            // below (rule 7: UNCHANGED -- the canonical SQLite answer, no fastpath, no `backend_used`).
            _ => {}
        }

        // RMAPD-PERF-1 + DAEMON-CANCEL-1 (handler-boundary layer): a heartbeat before
        // the (potentially long) Tarjan SCC. Unlike the prior fire-and-forget emit,
        // this OBSERVES the result: if the peer is already gone, skip the heavy work
        // and return Cancelled (read-only ⇒ nothing to discard yet).
        if crate::cancel::pre_work_check(emitter, "finding_cycles").is_break() {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::new(
                    ErrorCode::Cancelled,
                    "cycles query cancelled (client disconnected before traversal)",
                ),
            );
        }

        // EXPLICIT `--engine sqlite` ONLY (every other engine/kind returned from the match above). The forced
        // SQLite MODULE-import answer: canonical, qualified, deterministically-ordered (CYCLES-OUTPUT-
        // CONTRACT-1). DAEMON-CANCEL-1 in-loop layer: `find_cycles` is an in-memory Tarjan SCC (a Rust/CPU
        // loop, NOT a single SQL statement), so it takes a cooperative `loop_checkpoint` threaded into the
        // traversal — a disconnect DURING the SCC pass cancels mid-flight. This is the LAST of the cycles
        // routes: every Tarjan-running route is now checkpointed (the DEFAULT `auto` route — both its LiveGraph
        // module-cycle Tarjan and its SQLite fallback — and the `livegraph`/`compare` routes were wired in the
        // match arms above; the iteration-0 review flagged the `auto` route as the gap). Worker-panic vs
        // client-cancel are NOT conflated: a real storage error stays InternalError; only
        // `StorageError::Cancelled` (the checkpoint-`Break` channel) maps to Cancelled.
        let query_start = Instant::now();
        let cycles = {
            let mut checkpoint = crate::cancel::loop_checkpoint(emitter, "finding_cycles");
            match storage.find_cycles_cancellable(&snapshot.snapshot_uid, "module", &mut checkpoint)
            {
                Ok(c) => c,
                Err(repo_graph_storage::error::StorageError::Cancelled) => {
                    return DispatchResult::error(
                        &request.id,
                        ErrorDetail::new(
                            ErrorCode::Cancelled,
                            "cycles query cancelled (client disconnected during traversal)",
                        ),
                    );
                }
                Err(e) => {
                    return DispatchResult::error(
                        &request.id,
                        ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                    );
                }
            }
        };
        let qualified = match storage.module_qualified_names(&snapshot.snapshot_uid) {
            Ok(q) => q,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                );
            }
        };
        // CYCLE-HONESTY-1 (§2.1): the forced SQLite escape hatch also carries the REAL intra-SCC edges so it
        // renders a verified walk (never a fabricated ring). Same edge set `find_cycles` loaded above.
        let module_edges = match storage.module_import_edges(&snapshot.snapshot_uid) {
            Ok(e) => e,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                );
            }
        };
        let mut canonical_cycles = crate::cycle_output::sqlite_module_cycles_json_with_edges(
            &cycles,
            &qualified,
            &module_edges,
        );
        // FIXTURE-POLLUTION-1 §2.2/§2.3: the forced `--engine sqlite` route reaches the
        // stored `is_test` fact, so it classifies test-only cycles (the renderer demotes
        // them below the real cycles). Conservative aggregation: a cycle is test-only iff
        // every member module is wholly test-owned; an unclassifiable member ⇒ unknown (not
        // demoted). CLASSIFIED read → a genuine error PROPAGATES (never a silent cycle).
        let tracked_files = match storage.get_files_by_repo(&repo_uid) {
            Ok(f) => f,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                );
            }
        };
        let files: Vec<(&str, bool)> = tracked_files
            .iter()
            .map(|f| (f.path.as_str(), f.is_test))
            .collect();
        crate::cycle_output::label_test_only_cycles(&mut canonical_cycles, &files);
        // TYPE-ONLY-IMPORTS-1: this forced `--engine sqlite` MODULE-cycle route reaches the stored
        // per-module-edge `is_type_only` fact, so it attaches the PER-CYCLE type-only verdict (the precise
        // successor of the blanket caveat) — only on TS/JS-member cycles (§5). Reuses `tracked_files`/
        // `qualified` (already-CLASSIFIED reads whose errors propagated above) — no new fallible read.
        let all_module_dirs: Vec<String> = qualified.values().cloned().collect();
        let files_by_lang: Vec<(&str, Option<&str>)> = tracked_files
            .iter()
            .map(|f| (f.path.as_str(), f.language.as_deref()))
            .collect();
        crate::cycle_output::attach_type_only_labels(
            &mut canonical_cycles,
            &module_edges,
            &files_by_lang,
            &all_module_dirs,
        );
        let cycle_count = canonical_cycles.len();
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

        DispatchResult::success(
            &request.id,
            serde_json::json!({
                "repo_uid": repo_uid,
                "display_name": display_name,
                "snapshot_uid": snapshot.snapshot_uid,
                "cycles": canonical_cycles,
                "count": cycle_count,
                // Blanket caveat RETIRED on the SQLite route — the fact is now per-cycle (`type_only`);
                // the renderer derives any residual hedge from those verdicts.
                "ts_type_only_caveat": false,
            }),
        )
    }

    fn handle_path(&self, request: &Request, emitter: &mut dyn ProgressEmitter) -> DispatchResult {
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
        let storage = match self.open_storage(&repo_state) {
            Ok(s) => s,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        // Get latest snapshot. W-B-EPOCH-IMPL-2A (§14 D-CC refined): capture the request epoch — the pinned
        // `AgentSnapshot` (resolved ONCE; the request's atomic SQLite pin) with NO LG-serve eligibility
        // (`fingerprint: None`). `path` has NO cert: there is no CALLS∪IMPORTS no-loss cert to license a
        // LiveGraph path serve, so `path` serves the pinned SQLite snapshot under the epoch (the `Engine::Auto`
        // arm of `path_engine_response`). `fingerprint: None` is the honest, uniform "no eligibility" witness
        // (a future CALLS∪IMPORTS parity cert makes it `Some`, re-enabling the LG fastpath). Every read +
        // the response stamp use `epoch.snapshot_uid()`.
        let snapshot =
            match repo_graph_agent::AgentStorageRead::get_latest_snapshot(&storage, &repo_uid) {
                Ok(Some(snap)) => snap,
                Ok(None) => {
                    return DispatchResult::error(
                        &request.id,
                        Self::no_ready_snapshot_detail(&storage, repo_state.db_path(), &repo_uid),
                    );
                }
                Err(e) => {
                    return DispatchResult::error(
                        &request.id,
                        ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                    );
                }
            };
        let epoch = crate::livegraph_feed::RequestEpoch {
            snapshot,
            fingerprint: None,
        };

        // Resolve symbols
        use repo_graph_storage::queries::SymbolResolveError;

        let from_sym = match storage.resolve_symbol(epoch.snapshot_uid(), from_query) {
            Ok(sym) => sym,
            Err(SymbolResolveError::NotFound) => {
                // EMBED-SEED-IMPL-1 (spec §8, Group B): fire the tier on the `from`
                // endpoint's deterministic-zero NotFound (additive `data`; error
                // otherwise unchanged).
                return DispatchResult::error(
                    &request.id,
                    self.symbol_not_found_with_semantic(
                        &storage,
                        epoch.snapshot_uid(),
                        &repo_uid,
                        repo_state.db_path(),
                        &request.params,
                        "path",
                        from_query,
                    ),
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

        let to_sym = match storage.resolve_symbol(epoch.snapshot_uid(), to_query) {
            Ok(sym) => sym,
            Err(SymbolResolveError::NotFound) => {
                // EMBED-SEED-IMPL-1 (spec §8, Group B): fire the tier on the `to`
                // endpoint's deterministic-zero NotFound (additive `data`; error
                // otherwise unchanged).
                return DispatchResult::error(
                    &request.id,
                    self.symbol_not_found_with_semantic(
                        &storage,
                        epoch.snapshot_uid(),
                        &repo_uid,
                        repo_state.db_path(),
                        &request.params,
                        "path",
                        to_query,
                    ),
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
        // Auto serves the PINNED SQLite snapshot -- the RATIFIED posture (D-EC-6-C,
        // engine-consolidation-1 §8, re-affirming the W-B D-CC refinement; EC-1 M-4 fixed this
        // comment, which stale-claimed a LiveGraph-first Auto). The authoritative arm is
        // `path_engine_response` (livegraph_feed.rs): the SQLite closure below ALWAYS runs on
        // Auto/`--engine sqlite`; it is lazy only for the explicit LiveGraph/compare arms. Under the
        // ratified reconciliation frame (recon-design-1 D-R5) CALLS rows persist as the pipeline
        // witness, so this posture has no scheduled flip; a change requires a D-EC-6 re-ratification.
        let engine = crate::livegraph_feed::Engine::parse(Self::get_optional_string_param(
            &request.params,
            "engine",
        ));
        let repo_root =
            Self::get_optional_string_param(&request.params, "repo").unwrap_or_default();

        // DAEMON-CANCEL-1 (handler-boundary layer): bail before the BFS if the peer
        // is already gone. Symbol resolution above is cheap; the heavy work is the
        // LiveGraph BFS inside `path_engine_response`.
        if crate::cancel::pre_work_check(emitter, "finding_path").is_break() {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::new(
                    ErrorCode::Cancelled,
                    "path query cancelled (client disconnected before search)",
                ),
            );
        }

        // DAEMON-CANCEL-1 in-loop layer: thread a cooperative checkpoint INTO the
        // LiveGraph BFS (`path_engine_response` → `LiveGraph::path_cancellable`) so a
        // disconnect DURING the search cancels mid-traversal. The SQLite fallback
        // (`find_shortest_path`, a recursive CTE) is NOT checkpointed here — that is
        // the `sqlite3_interrupt` path, DAEMON-CANCEL-2. Read-only ⇒ no rollback.
        let mut checkpoint = crate::cancel::loop_checkpoint(emitter, "finding_path");
        let response = crate::livegraph_feed::path_engine_response(
            engine,
            &repo_state,
            &from_sym.stable_key,
            &to_sym.stable_key,
            &repo_uid,
            epoch.snapshot_uid(),
            || {
                let path_result = storage.find_shortest_path(
                    epoch.snapshot_uid(),
                    &from_sym.stable_key,
                    &to_sym.stable_key,
                    8,
                )?;
                let found = path_result.found;
                Ok(serde_json::json!({
                    "repo_uid": repo_uid,
                    "snapshot_uid": epoch.snapshot_uid(),
                    "path": path_result,
                    "found": found,
                }))
            },
            repo_root,
            &mut checkpoint,
        );
        match response {
            Ok(v) => DispatchResult::success(&request.id, v),
            Err(repo_graph_storage::error::StorageError::Cancelled) => DispatchResult::error(
                &request.id,
                ErrorDetail::new(
                    ErrorCode::Cancelled,
                    "path query cancelled (client disconnected during search)",
                ),
            ),
            Err(e) => DispatchResult::error(
                &request.id,
                ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
            ),
        }
    }

    // ── Write operations ────────────────────────────────────────────

    /// ENRICH-LIFECYCLE-1 + SNAPSHOT-RETENTION-1: queue the background maintenance passes for a
    /// just-completed write op, then annotate + return the success response. NEITHER pass runs on this
    /// foreground path.
    ///
    /// **Sequencing (slice §3, verified):** the enrichment pass CHAINS the retention pass on its own
    /// completion (`enrich_pass::run_auto_enrich`), so the two never contend for the write lock —
    /// retention's bounded requeue is never starved by a long enrichment. When enrichment is opted
    /// out (`RMAP_AUTO_ENRICH=off`), retention is triggered DIRECTLY here (its SNAPSHOT-RETENTION-1
    /// site, unchanged) so cleanup still runs. Both passes yield to any live op via the two-gate
    /// discipline and report their async results on `rmap doctor` + the daemon log; the synchronous
    /// reply therefore only states `queued` / `disabled` (final numbers surface on doctor — the
    /// ratified never-on-foreground invariant). `db_path` MUST be the SAME path this handler stamped
    /// into the activity registry, so each pass's gate-1 `active_for_db` check matches.
    fn finish_write_with_maintenance(
        &self,
        request_id: &str,
        mut response: serde_json::Value,
        db_path: &Path,
        repo_uid: &str,
        repo_display: String,
    ) -> DispatchResult {
        let enrichment_state = if crate::enrich_pass::auto_enrich_enabled() {
            // Enrichment chains the tail (enrich → seed → retention, spec §5) on
            // completion — one queued call covers all three passes in order.
            crate::enrich_pass::spawn_auto_enrich(
                Arc::clone(&self.state),
                db_path.to_path_buf(),
                repo_uid.to_string(),
                repo_display,
            );
            "queued"
        } else {
            // Enrichment off → it will NOT chain the tail, so trigger the
            // seed → retention tail directly here (EMBED-SEED-IMPL-1, spec §5).
            crate::seed_pass::chain_seed_then_retention(
                &self.state,
                db_path,
                repo_uid,
                &repo_display,
            );
            "disabled"
        };
        // Seed runs (chained after enrich, or directly above) iff seeding is enabled.
        let seed_state = if crate::seed::seed_enabled() {
            "queued"
        } else {
            "disabled"
        };
        // Retention runs (chained after enrich/seed, or directly above) iff retention itself is enabled.
        let retention_state = if crate::retention_pass::auto_retention_enabled() {
            "queued"
        } else {
            "disabled"
        };
        Self::annotate_auto_pass(&mut response, "enrichment", enrichment_state);
        Self::annotate_auto_pass(&mut response, "seed", seed_state);
        Self::annotate_auto_pass(&mut response, "retention", retention_state);
        DispatchResult::success(request_id, response)
    }

    /// Merge an `auto_pass` state token into a named reply block, preserving any fields the block
    /// already carries (e.g. retention's foreground `classify_retention_only` counts). The CLI
    /// completion report reads `<block>.auto_pass` (see `rgr::commands::index::format_*_line`).
    fn annotate_auto_pass(response: &mut serde_json::Value, block: &str, state: &str) {
        match response.get_mut(block).and_then(|v| v.as_object_mut()) {
            Some(obj) => {
                obj.insert("auto_pass".to_string(), serde_json::json!(state));
            }
            None => {
                response[block] = serde_json::json!({ "auto_pass": state });
            }
        }
    }

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

        // Register in registry (or get existing entry). `repo_uid` is NOT captured here: it is
        // re-captured from the live entry AFTER we hold the DB write lock (see the re-confirm block
        // below) so a concurrent forget cannot leave us indexing under a forgotten identity
        // (FORGET-REPO-1 review-3 #1). db_path is a deterministic hash of the path, so it is stable
        // across a forget+re-register and is the correct key to coordinate the write lock on.
        let (canonical_path, db_path) = {
            let mut registry = self.state.registry_mut();
            // Scope the `&RegistryEntry` borrow so it is released before `registry.save()`
            // (register* return an immutable borrow; save needs `&mut registry`).
            let resolved = {
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
                (entry.canonical_path.clone(), entry.db_path.clone())
            };
            // INDEX-DISCONNECT-1 (contract item 2): persist the registration UP-FRONT, before
            // indexing. The repo must exist in the on-disk registry even if the index later fails or
            // the client disconnects — otherwise a failed index leaves the repo unregistered (the F5
            // field bug: `repo info` reported "repo not indexed" by path AND uid because both
            // `record_index` and `save` lived only in the success branch). `record_index` + `save`
            // still run on success (below); this save guarantees the identity survives a failure.
            // review-0 change #1: the up-front save IS the durability guarantee ("registration
            // persists up-front"), so a failure here is a REAL failure — do NOT proceed to index a
            // repo whose registration did not persist. Proceeding would recreate the exact F5 window
            // this slice closes: the identity would survive only if the index reached its success
            // branch (which also saves). Fail fast, BEFORE any indexing work. (The success-branch save
            // stays best-effort: by the time it runs the repo is already registered here and the
            // snapshot already durable, so it only refreshes `last_snapshot_uid`.) `register()` above
            // is idempotent, so a retried index re-registers and re-saves — the in-memory entry left
            // behind on this error path is harmless and self-heals on the next successful save.
            if let Err(e) = registry.save() {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(
                        ErrorCode::InternalError,
                        format!("failed to persist repo registration before indexing: {e}"),
                    ),
                );
            }
            resolved
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
        // ENRICH-LIFECYCLE-1 (running-yield, slice §3.4): explicit writes win against a RUNNING
        // background enrichment. Ask any enrichment pass on this DB to yield BEFORE we block on the
        // write lock — it polls this flag at its next batch boundary and releases the lock, and we
        // proceed. Must be BEFORE acquire_write (a lock-holding pass never sees our still-unstamped
        // activity op). No-op if none is running. See enrich_pass::EnrichCoordinator.
        self.state.enrich_coord().request_yield_for_db(&db_path);
        let _db_write_guard = db_runtime.acquire_write();
        // Now that we own the write lock, drop any PENDING yield marker for this DB. While we hold the
        // lock no enrichment pass can be mid-registration (mutual exclusion on the same lock), so a
        // marker is stale — and must not make the pass THIS index is about to queue yield spuriously
        // (ENRICH-LIFECYCLE-1 review-1: close the acquire→register window without a self-inflicted yield).
        self.state.enrich_coord().clear_pending_yield(&db_path);

        // FORGET-REPO-1 (review-3 #1): re-confirm the registration NOW THAT WE HOLD THE DB WRITE LOCK,
        // and re-capture `repo_uid` from the live entry. The up-front register+save above
        // (INDEX-DISCONNECT-1) runs BEFORE `acquire_write`; a concurrent `reclaim::forget_repo` holds
        // this SAME write lock across its whole registry-entry + DB-file deletion, so while we blocked
        // on the lock it may have removed our entry. Indexing with the `repo_uid` captured up-front
        // would then write rows under an identity the registry no longer knows — an unregistered/orphan
        // DB, the exact failure forget exists to prevent (the operator-ratified "late writer
        // re-registers fresh", 2026-08-23). `register` is idempotent: in the common no-forget case it
        // returns our surviving entry unchanged (same `repo_uid`, alias intact); if forget removed the
        // entry it MINTS A FRESH `repo_uid` here, under the lock, before any write. We hold the write
        // lock across this and the whole index, so no forget can race between the re-confirm and the
        // write. (Alias note: in the rare forget-race the re-minted entry has no alias — a fresh
        // registration by definition; the safety invariant, a registered identity, still holds.)
        let repo_uid = {
            let mut registry = self.state.registry_mut();
            let uid = match registry.register(&canonical_path) {
                Ok(e) => e.repo_uid.clone(),
                Err(e) => {
                    return DispatchResult::error(
                        &request.id,
                        ErrorDetail::new(
                            ErrorCode::InternalError,
                            format!(
                                "failed to re-confirm repo registration under the write lock: {e}"
                            ),
                        ),
                    );
                }
            };
            if let Err(e) = registry.save() {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(
                        ErrorCode::InternalError,
                        format!("failed to persist repo re-registration before indexing: {e}"),
                    ),
                );
            }
            uid
        };

        // DAEMON-VISIBILITY-1 (D): record this index as in-flight so `rmap doctor` and the
        // still-running client probe can see it (op kind, repo, started-at, live phase/counters
        // teed below). The guard deregisters on every exit path (RAII). Repo identity is already
        // resolved above (registry.register) — the coordinator never records it (index coordinates
        // on the DB `Mutex<()>`, not `RepoCoordinator`), which is why this record exists.
        let _activity = self.state.activity().begin(
            crate::activity::OpKind::Index,
            canonical_path.to_string_lossy().to_string(),
            Some(repo_uid.clone()),
            db_path.clone(),
        );

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

        // INDEX-BASIS-1 (operator RULING 3): classify the git HEAD this index is being
        // built FROM into a `BasisOutcome`, and hand it to compose on `ComposeOptions`.
        // The daemon is the composition root that owns the git probe; compose PERSISTS the
        // outcome (the `basis_commit` column for a read HEAD, plus a self-describing
        // `index_basis` diagnostic for the two NULL-basis outcomes) IN THE SAME WRITE FLOW
        // that writes the snapshot row — deterministic, propagates on failure. This replaces
        // the deleted best-effort post-compose daemon write whose loss could impersonate
        // pre-slice history (review-5). Three outcomes: Ok(Some)→Basis (stamp the sha);
        // Ok(None)→NonGit (recorded NULL, so a THIS-slice non-git is distinct from pre-slice);
        // Err→Failure (git repo, HEAD unreadable — unborn or generic, classified from git's
        // own error here at write time, the freshest evidence).
        let basis_probe = crate::index_drift::basis_at_index(&canonical_path);
        if let Err(e) = &basis_probe {
            eprintln!(
                "warning: could not read git HEAD to stamp index basis for {}: {e}",
                canonical_path.display()
            );
        }
        // Pass the repo path so a HEAD-read failure can be classified via the POSITIVE
        // unborn probe (`git rev-list -n 1 --all`), never from stderr text (review-9 #1).
        let basis_outcome =
            crate::index_drift::basis_outcome_from_probe(&canonical_path, basis_probe);
        let basis_commit = basis_outcome.basis_commit();

        let options = ComposeOptions {
            c_include_roots,
            storage_root_path,
            basis_commit,
            basis_outcome: Some(basis_outcome),
            ..ComposeOptions::default()
        };

        // Create progress callback that maps repo-index events to daemon protocol.
        // DAEMON-VISIBILITY-1 (D): also tee the phase + counters into the activity record so
        // `rmap doctor` renders live progress ("extraction 42k/160k files") for an ATTACHED-less
        // observer — the same event already sent to the attached client, no new instrumentation.
        //
        // INDEX-DISCONNECT-1 (contract item 1): progress emission is BEST-EFFORT. An index is a
        // durable mutation; a dead client's closed socket (read timeout, closed terminal, machine
        // sleep) must NEVER abort it — the pre-fix `Err(_) => Break` turned a broken pipe into an
        // aborted index (TECH-DEBT F5). On the FIRST emit failure we log ONE reader-frame line, mark
        // the client gone, and thereafter skip the (doomed) emit entirely so the index runs to
        // completion detached. The activity tee keeps running so `rmap doctor` still renders live
        // progress for other observers. This callback therefore NEVER returns `Break` on transport
        // failure; the orchestrator's `Break`→abort seam is untouched, so an explicit cancel remains
        // the one deliberate way to stop a write op (contract item 4).
        let mut client_gone = false;
        let mut last_phase: Option<String> = None;
        let mut progress_callback = |event: &ProgressEvent| -> ControlFlow<()> {
            _activity.update(&event.phase, event.current, event.total);
            // DAEMON-CRASH-RECOVERY-1 (F8): log COARSE phase transitions (deduped — a handful of
            // phases, never per-file spam), so a crash's daemon log shows which phase was in flight
            // (the field crash was mid-postpass).
            if last_phase.as_deref() != Some(event.phase.as_str()) {
                crate::oplog::log_op_phase("index", &repo_uid, &event.phase);
                last_phase = Some(event.phase.clone());
            }
            if client_gone {
                // Client already disconnected: skip the emit cheaply, keep indexing.
                return ControlFlow::Continue(());
            }
            if emitter
                .emit(ProgressDetail {
                    phase: event.phase.clone(),
                    current: event.current,
                    total: event.total,
                })
                .is_err()
            {
                client_gone = true;
                // review-0 change #3: emit the ONE reader-frame line via the shared helper, which
                // also feeds the parallel-safe test-capture seam so the named F5 test observes the
                // actual logged line (not just the emit-call count).
                crate::detached::log_detached_continuation("index", &repo_uid);
            }
            ControlFlow::Continue(())
        };

        // DAEMON-CRASH-RECOVERY-1 (F8): the op-START line lands in the daemon LOG the moment the index
        // actually begins (after the pre-op validation/registration returns above, so those rejections
        // never log a spurious start). The snapshot_uid is created INSIDE the extractor, so it is not
        // known here — the outcome line below carries it. If the daemon dies between here and an
        // outcome line, the next boot's reconciliation line supplies the missing "interrupted" outcome.
        crate::oplog::log_op_start("index", &repo_uid, None);

        // Execute index under DB write lock (with progress)
        match index_path_with_progress(
            &canonical_path,
            &db_path,
            &repo_uid,
            &options,
            Some(&mut progress_callback),
        ) {
            Ok(result) => {
                // DAEMON-CRASH-RECOVERY-1 (F8): the op-OUTCOME line, now that the snapshot exists.
                crate::oplog::log_op_outcome(
                    "index",
                    &repo_uid,
                    Some(&result.snapshot_uid),
                    "completed",
                );
                // INDEX-BASIS-1 (operator RULING 3): the index-basis outcome is now recorded
                // BY COMPOSE, in the same write flow that wrote the snapshot row (see
                // `ComposeOptions.basis_outcome` above) — no best-effort post-compose write.
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
                        // D-S = S-A: open a fresh per-op connection for the retention read. The index
                        // succeeded already; a storage-open failure only skips best-effort retention
                        // classification (the success response is still returned).
                        let storage = match repo_state.storage() {
                            Ok(s) => s,
                            Err(e) => {
                                eprintln!(
                                    "warning: retention classification skipped (storage open failed) for {}: {}",
                                    repo_uid, e
                                );
                                // Retention still queues: the background pass opens its own storage
                                // connection, so this per-op read failure does not block cleanup.
                                return self.finish_write_with_maintenance(
                                    &request.id,
                                    response,
                                    &db_path,
                                    &repo_uid,
                                    canonical_path.to_string_lossy().to_string(),
                                );
                            }
                        };
                        match classify_retention_only(&storage, &repo_uid) {
                            Ok(lifecycle) => {
                                response["retention"] = serde_json::json!({
                                    "pruned_count": lifecycle.pruned_count,
                                    "prunable_count": lifecycle.prunable_count,
                                    "current": lifecycle.stats.current,
                                    "parent": lifecycle.stats.parent,
                                    "baseline_auto": lifecycle.stats.baseline_auto,
                                    "baseline_user": lifecycle.stats.baseline_user,
                                    "baseline_stamp": lifecycle.stats.baseline_stamp,
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

                // SNAPSHOT-RETENTION-1: queue the background retention pass (never foreground).
                self.finish_write_with_maintenance(
                    &request.id,
                    response,
                    &db_path,
                    &repo_uid,
                    canonical_path.to_string_lossy().to_string(),
                )
            }
            // INDEX-DISCONNECT-1: after the best-effort callback change above, transport failure no
            // longer produces `Aborted` — this arm is now only reachable if the orchestrator's
            // `Break`→abort seam is driven by a deliberate explicit cancel. Retained (not removed) so
            // that mapping stays correct; on the daemon path it is unreachable.
            Err(repo_graph_repo_index::compose::ComposeError::Aborted) => {
                // DAEMON-CRASH-RECOVERY-1 (F8): a deliberate cancel is a terminal outcome too.
                crate::oplog::log_op_outcome("index", &repo_uid, None, "interrupted (aborted)");
                DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(
                        ErrorCode::ProgressDeliveryFailed,
                        "operation aborted: progress delivery failed",
                    ),
                )
            }
            Err(e) => {
                // DAEMON-CRASH-RECOVERY-1 (F8): a genuine index failure, named in the LOG.
                crate::oplog::log_op_outcome("index", &repo_uid, None, &format!("failed: {e}"));
                DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                )
            }
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
        // ENRICH-LIFECYCLE-1 (running-yield, slice §3.4): a refresh is an explicit write; make any
        // RUNNING background enrichment on this DB yield before we block on the write lock (same
        // rationale as handle_index — the pass can't see our not-yet-stamped activity op).
        self.state.enrich_coord().request_yield_for_db(db_path);
        let _db_write_guard = db_runtime.acquire_write();
        // Drop any stale PENDING yield marker now that we own the write lock (see handle_index): it
        // must not make the next enrichment pass yield spuriously (ENRICH-LIFECYCLE-1 review-1).
        self.state.enrich_coord().clear_pending_yield(db_path);

        // FORGET-REPO-1 (review-3 #1, same defect class as handle_index): re-confirm the repo is STILL
        // registered now that we hold the DB write lock. A concurrent `reclaim::forget_repo` holds this
        // SAME lock across its deletion, so while we blocked on it the repo may have been forgotten.
        // `repo_state.storage()` now opens NO-CREATE (`StorageConnection::open_existing`, operator
        // ruling 2), so a forgotten refresh can no longer resurrect the deleted DB as an orphan — the
        // open would fail honestly. This recheck stays as the belt-and-suspenders that turns that raw
        // open failure into a clear "was forgotten" error: unlike an index, a forgotten refresh has no
        // fresh-index intent to re-register under, so abort with a precise error and create NO file.
        let still_registered = {
            let reg = self.state.registry();
            reg.list().iter().any(|e| e.repo_uid == repo_uid)
        };
        if !still_registered {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::new(
                    ErrorCode::StateUnavailable,
                    "this repo was forgotten (`rmap repo remove`) while the refresh waited for the write lock; nothing was refreshed".to_string(),
                ),
            );
        }

        // Then acquire repo refresh lock (blocks new readers, waits for active readers)
        let _refresh_guard = repo_state.coordinator.acquire_refresh();
        let storage = match self.open_storage(&repo_state) {
            Ok(s) => s,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        // Resolve repo_path from stored root_path
        let canonical_db_path = repo_state.db_path();
        let repo_info = match storage.get_repo(&RepoRef::Uid(repo_uid.clone())) {
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

        // DAEMON-VISIBILITY-1 (D): record this refresh as in-flight (RAII-cleared on exit). The
        // activity record's `repo_display` is documented "canonical repo path — what the operator
        // recognises". The reconstructed `repo_path` above is DB-relative (db_parent + stored
        // `root_path`, so it carries `../..`); canonicalize it for the status line so refresh MATCHES
        // `handle_index` (which stamps the already-canonical registry path) — otherwise `rmap doctor`
        // shows `…/databases/../../repo` for a refresh but a clean path for an index. Display only: the
        // refresh work below still uses `repo_path`. Falls back to the raw path if canonicalization
        // fails (never blocks — the dir existence was just checked). Surfaced by the review-6 in-flight
        // refresh status proof.
        let _activity = self.state.activity().begin(
            crate::activity::OpKind::Refresh,
            std::fs::canonicalize(&repo_path)
                .unwrap_or_else(|_| repo_path.clone())
                .to_string_lossy()
                .to_string(),
            Some(repo_uid.clone()),
            canonical_db_path.to_path_buf(),
        );

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

        // INDEX-BASIS-1 (operator RULING 3): re-classify the git basis at refresh start (an
        // explicit refresh re-anchors the facts to the current HEAD). `repo_path` is the
        // on-disk repo root resolved above. Same outcome policy as the index arm — compose
        // persists the `BasisOutcome` (column sha for a read HEAD; self-describing
        // `index_basis` diagnostic for the NULL-basis outcomes) in the refresh write flow.
        let basis_probe = crate::index_drift::basis_at_index(&repo_path);
        if let Err(e) = &basis_probe {
            eprintln!(
                "warning: could not read git HEAD to stamp index basis for {}: {e}",
                repo_path.display()
            );
        }
        // Pass the repo path so a HEAD-read failure can be classified via the POSITIVE
        // unborn probe (`git rev-list -n 1 --all`), never from stderr text (review-9 #1).
        let basis_outcome = crate::index_drift::basis_outcome_from_probe(&repo_path, basis_probe);
        let basis_commit = basis_outcome.basis_commit();

        let options = ComposeOptions {
            c_include_roots,
            storage_root_path,
            basis_commit,
            basis_outcome: Some(basis_outcome),
            ..ComposeOptions::default()
        };

        // Create progress callback that maps repo-index events to daemon protocol.
        // DAEMON-VISIBILITY-1 (C/D, review-6): also tee the phase + counters into the activity record
        // — the SAME event already sent to the attached client — so `rmap doctor` and the still-running
        // client probe render live refresh progress ("refreshing <repo>: <phase> …") for an
        // ATTACHED-less observer, exactly as `handle_index` does. Exposure, not instrumentation: no new
        // bookkeeping, the coordinator / W-B epoch are untouched. Without this tee an in-flight refresh
        // reported a null phase on the status surface (the review-6 gap).
        //
        // INDEX-DISCONNECT-1 (contract item 1): refresh shares `handle_index`'s emitter pattern and is
        // a WRITE op, so its progress emission is BEST-EFFORT too. A dead client's closed socket must
        // NEVER abort a refresh: on the first emit failure log ONCE, mark the client gone, then skip
        // subsequent emits and run to completion detached. NEVER returns `Break` on transport failure
        // (the previous `Err(_) => Break` aborted the refresh mid-flight, same F5 class as index).
        let mut client_gone = false;
        let mut last_phase: Option<String> = None;
        let mut progress_callback = |event: &ProgressEvent| -> ControlFlow<()> {
            _activity.update(&event.phase, event.current, event.total);
            // DAEMON-CRASH-RECOVERY-1 (F8): coarse phase transitions, deduped (see handle_index).
            if last_phase.as_deref() != Some(event.phase.as_str()) {
                crate::oplog::log_op_phase("refresh", &repo_uid, &event.phase);
                last_phase = Some(event.phase.clone());
            }
            if client_gone {
                return ControlFlow::Continue(());
            }
            if emitter
                .emit(ProgressDetail {
                    phase: event.phase.clone(),
                    current: event.current,
                    total: event.total,
                })
                .is_err()
            {
                client_gone = true;
                // review-0 change #3: same shared helper as handle_index (op label "refresh"); the
                // capture seam lets the refresh named test observe the actual logged line.
                crate::detached::log_detached_continuation("refresh", &repo_uid);
            }
            ControlFlow::Continue(())
        };

        // DAEMON-CRASH-RECOVERY-1 (F8): op-START line (see handle_index for the snapshot_uid note).
        crate::oplog::log_op_start("refresh", &repo_uid, None);

        // Execute refresh under both locks (with progress)
        match refresh_path_with_progress(
            &repo_path,
            canonical_db_path,
            &repo_uid,
            &options,
            Some(&mut progress_callback),
        ) {
            Ok(result) => {
                // DAEMON-CRASH-RECOVERY-1 (F8): op-OUTCOME line, now that the snapshot exists.
                crate::oplog::log_op_outcome(
                    "refresh",
                    &repo_uid,
                    Some(&result.snapshot_uid),
                    "completed",
                );
                // INDEX-BASIS-1 (operator RULING 3): the refresh-start basis outcome is
                // recorded BY COMPOSE in the refresh write flow (see
                // `ComposeOptions.basis_outcome` above) — no best-effort post-compose write.
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
                match classify_retention_only(&storage, &repo_uid) {
                    Ok(lifecycle) => {
                        response["retention"] = serde_json::json!({
                            "pruned_count": lifecycle.pruned_count,
                            "prunable_count": lifecycle.prunable_count,
                            "current": lifecycle.stats.current,
                            "parent": lifecycle.stats.parent,
                            "baseline_auto": lifecycle.stats.baseline_auto,
                            "baseline_user": lifecycle.stats.baseline_user,
                            "baseline_stamp": lifecycle.stats.baseline_stamp,
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

                // SNAPSHOT-RETENTION-1: queue the background retention pass for the refreshed repo
                // (never foreground). `canonical_db_path` is the SAME path stamped into the activity
                // registry above, so the pass's gate-1 contention check matches.
                self.finish_write_with_maintenance(
                    &request.id,
                    response,
                    canonical_db_path,
                    &repo_uid,
                    repo_path.display().to_string(),
                )
            }
            // INDEX-DISCONNECT-1: as in `handle_index`, transport failure no longer yields `Aborted`
            // (best-effort callback above); this arm is now only reachable via a deliberate explicit
            // cancel driving the orchestrator's abort seam. Unreachable on the daemon path, retained
            // so the mapping stays correct.
            Err(repo_graph_repo_index::compose::ComposeError::Aborted) => {
                crate::oplog::log_op_outcome("refresh", &repo_uid, None, "interrupted (aborted)");
                DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(
                        ErrorCode::ProgressDeliveryFailed,
                        "operation aborted: progress delivery failed",
                    ),
                )
            }
            Err(e) => {
                crate::oplog::log_op_outcome("refresh", &repo_uid, None, &format!("failed: {e}"));
                DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                )
            }
        }
        // Guards drop here: _refresh_guard then _db_write_guard
    }

    fn handle_enrich(
        &self,
        request: &Request,
        emitter: &mut dyn ProgressEmitter,
    ) -> DispatchResult {
        // ENRICH-LIFECYCLE-1 (REG-1 closure, slice §3.6): prefer the registry-resolved `repo` param
        // (cwd/alias, like every other command — `resolve_and_load_repo` auto-loads it); fall back to
        // the legacy positional `db_path` + `repo_uid` for compatibility (kept working, dropped from
        // `--help`). The two forms converge on `(repo_state, repo_uid, db_path)` for the rest of the
        // handler.
        let (repo_state, repo_uid_owned, db_path_buf) = if request.params.get("repo").is_some() {
            match self.resolve_and_load_repo(&request.params) {
                Ok((state, uid)) => {
                    let db = state.db_path().to_path_buf();
                    (state, uid, db)
                }
                Err(e) => return DispatchResult::error(&request.id, e),
            }
        } else {
            let db_path_str = match Self::get_string_param(&request.params, "db_path") {
                Ok(p) => p,
                Err(_) => {
                    return DispatchResult::error(
                        &request.id,
                        ErrorDetail::invalid_request(
                            "enrich requires a `repo` (registry-resolved) or the legacy `db_path` + `repo_uid`",
                        ),
                    )
                }
            };
            let repo_uid = match Self::get_string_param(&request.params, "repo_uid") {
                Ok(r) => r,
                Err(e) => return DispatchResult::error(&request.id, e),
            };
            let db_path = Path::new(db_path_str);
            let key = match RepoKey::new(db_path, repo_uid) {
                Ok(k) => k,
                Err(e) => {
                    return DispatchResult::error(&request.id, ErrorDetail::invalid_request(e));
                }
            };
            // Legacy contract preserved: the repo must already be loaded (unchanged error).
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
            (repo_state, repo_uid.to_string(), db_path.to_path_buf())
        };
        let repo_uid: &str = &repo_uid_owned;
        let db_path: &Path = &db_path_buf;

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

        // DAEMON-VISIBILITY-1 (D): record this enrich as in-flight (RAII-cleared on exit).
        // Enrich's pipeline does not stream per-phase progress, so the record carries op kind +
        // repo + started-at (no live counters) — enough for `rmap doctor` to say "enriching <repo>".
        // Keyed by repo_uid (enrich is uid-addressed).
        //
        // ORIENT-FACT-COHERENCE-1 (operator ruling review-3(b)): stamp the CANONICAL db path
        // (`repo_state.db_path()`), NOT the raw `db_path` the caller supplied. `ActivityRegistry`'s
        // `active_for_db` matches by EXACT path equality against "the canonical DB path the write handler
        // stamped", and every reader (`DaemonState::enrichment_in_flight_for_db`, orient/check/reliability)
        // queries `repo_state.db_path()`. On the legacy `db_path`+`repo_uid` form, the raw `db_path` may be
        // relative or symlinked (`RepoKey::new` canonicalizes only for the lookup, so `repo_state.db_path()`
        // is canonical while `db_path` is not) — stamping the raw spelling would leave a concurrent reader
        // unable to see the in-flight enrich and still handing the stale "run `rmap enrich`" CTA. Stamp the
        // canonical path so the write matches the read and the in-flight suppression holds for BOTH the
        // registry-resolved and the legacy form.
        let _activity = self.state.activity().begin(
            crate::activity::OpKind::Enrich,
            repo_uid.to_string(),
            Some(repo_uid.to_string()),
            repo_state.db_path().to_path_buf(),
        );

        let storage = match self.open_storage(&repo_state) {
            Ok(s) => s,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

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
                match storage.get_snapshot(uid) {
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
                match storage.get_latest_snapshot(repo_uid) {
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
                        // DAEMON-VISIBILITY-1 (F2, review-4): enrich is a READY-requiring surface
                        // too — never a bare "no snapshot found" when a non-READY partial exists.
                        // Route through the SAME shared helper as orient/explain and the
                        // quality/governance handlers: it NAMES the interrupted partial (state +
                        // when + on-disk size) and BOTH next actions (`rmap index` /
                        // `rmap maintenance prune`) under the honest `SnapshotNotFound` code.
                        // `get_latest_snapshot` is READY-only (documented parity-critical), so a
                        // lingering non-READY snapshot lands here — the exact day-2 gaslighting path
                        // for enrich, previously the last bare "no snapshot found" in this crate.
                        return DispatchResult::error(
                            &request.id,
                            Self::no_ready_snapshot_detail(
                                &storage,
                                repo_state.db_path(),
                                repo_uid,
                            ),
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

        // ENRICH-LIFECYCLE-1: enrich is a write op with detached completion (INDEX-DISCONNECT-1
        // semantics) — a client disconnect must NOT abort it. Progress emission is best-effort: on
        // the first emit failure log ONE reader-frame line and continue under the write lock; the
        // pass runs to completion regardless of whether the client is still attached.
        let mut client_gone = false;

        // Emit initial progress (best-effort).
        if emitter
            .emit(ProgressDetail {
                phase: "initializing".to_string(),
                current: 0,
                total: 1,
            })
            .is_err()
            && !client_gone
        {
            crate::detached::log_detached_continuation("enrich", repo_uid);
            client_gone = true;
        }

        // Build resolver registry. The set of languages with a CONFIGURED resolver is the SINGLE source
        // `configured_resolver_languages` (HONEST-DEGRADATION-IMPL-2 D5 keys on the SAME value): we
        // register exactly these, intersected with any explicit `languages` filter. Tying registration
        // to that source is what guarantees the D5 honest next-action can never promise a remedy this
        // path cannot deliver.
        let configured = configured_resolver_languages(jdtls_path.as_deref());
        let requested =
            |lang: EnrichmentLanguage| languages.is_empty() || languages.contains(&lang);
        let mut registry = ResolverRegistry::new();
        let mut available_languages = Vec::new();

        if configured.contains(&EnrichmentLanguage::Rust) && requested(EnrichmentLanguage::Rust) {
            registry.register(Box::new(RustAnalyzerResolver::new()));
            available_languages.push("rust".to_string());
        }
        if configured.contains(&EnrichmentLanguage::TypeScript)
            && requested(EnrichmentLanguage::TypeScript)
        {
            registry.register(Box::new(TsServerResolver::new()));
            available_languages.push("typescript".to_string());
        }
        if requested(EnrichmentLanguage::Java) {
            if configured.contains(&EnrichmentLanguage::Java) {
                // Java in the configured set ⇒ a jdtls_path is present (source invariant).
                let path = jdtls_path
                    .as_ref()
                    .expect("Java configured ⇒ jdtls_path present");
                let config = JdtlsConfig {
                    jdtls_path: Some(path.clone()),
                    ..Default::default()
                };
                registry.register(Box::new(JdtlsResolver::with_config(config)));
                available_languages.push("java".to_string());
            } else if languages.contains(&EnrichmentLanguage::Java) {
                // User explicitly requested Java but no jdtls path.
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::invalid_request(
                        "language 'java' requires jdtls_path parameter or JDTLS_PATH env var",
                    ),
                );
            }
        }

        // TEST SEAM (ORIENT-FACT-COHERENCE-1, operator ruling review-3(b)): if a hermetic resolver
        // backend is installed (`enrich_pass::set_test_registry_builder`, the SAME seam the auto pass
        // honors), use it instead of the real LSP resolvers, so the explicit-enrich real-handler coherence
        // test can park this handler in flight without a live toolchain. Inert in production — the builder
        // has no production caller — so the configured-resolver path above is unchanged for real enrich.
        if let Some(test_registry) = crate::enrich_pass::test_enrich_registry(&languages) {
            registry = test_registry;
            if available_languages.is_empty() {
                available_languages = languages
                    .iter()
                    .map(|l| match l {
                        EnrichmentLanguage::Rust => "rust".to_string(),
                        EnrichmentLanguage::TypeScript => "typescript".to_string(),
                        EnrichmentLanguage::Java => "java".to_string(),
                    })
                    .collect();
            }
        }

        if available_languages.is_empty() {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::invalid_request("no resolvers available for requested languages"),
            );
        }

        // Emit resolving progress (best-effort — detached completion, see above).
        if emitter
            .emit(ProgressDetail {
                phase: "resolving".to_string(),
                current: 0,
                total: 0,
            })
            .is_err()
            && !client_gone
        {
            crate::detached::log_detached_continuation("enrich", repo_uid);
            client_gone = true;
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

        // Open fresh storage connection for pipeline (EnrichmentPipeline takes ownership).
        // We acquire a separate connection since the pipeline consumes it. This is safe under
        // the coordinator's refresh lock. NO-CREATE (FORGET-REPO-1): enrich writes an EXISTING,
        // already-indexed DB; it must never create — a stale enrich after a forget would otherwise
        // resurrect the removed DB as an unregistered orphan (the SPLIT choke wraps the same
        // NO-CREATE `open_existing`).
        // FOREGROUND-LOCK-1 (§2.2/§2.3): route this second foreground open through the bounded-
        // patience choke so a transient lock is the honest `Busy` transient (never `InternalError`),
        // while a genuine non-lock fault keeps this handler's pre-existing message verbatim.
        let storage = match self.open_storage_split(&repo_state) {
            Ok(s) => s,
            Err(crate::foreground_open::ForegroundOpenFault::Busy(detail)) => {
                return DispatchResult::error(&request.id, detail);
            }
            Err(crate::foreground_open::ForegroundOpenFault::Other(e)) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(
                        ErrorCode::InternalError,
                        format!("failed to open storage for enrichment: {}", e),
                    ),
                );
            }
        };

        // Run enrichment pipeline.
        // DAEMON-CRASH-RECOVERY-1 (F8, review-2 item 2): the manual `rmap enrich` command is a write
        // op, so — exactly like index/refresh — log an op-START the moment real work begins (AFTER the
        // pre-op validation/registration returns above, so a rejected request never logs a spurious
        // start) and a terminal OUTCOME on every arm of the write. Unlike index, the snapshot is
        // already resolved here, so the start line NAMES it. Observability only — enrich is unchanged.
        crate::oplog::log_op_start("enrich", repo_uid, Some(&snapshot_uid));
        let mut pipeline = EnrichmentPipeline::with_registry(storage, registry);
        let report = match pipeline.run(repo_uid, &snapshot_uid, &config) {
            Ok(r) => r,
            Err(e) => {
                crate::oplog::log_op_outcome(
                    "enrich",
                    repo_uid,
                    Some(&snapshot_uid),
                    &format!("failed: {e}"),
                );
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(
                        ErrorCode::InternalError,
                        format!("enrichment failed: {}", e),
                    ),
                );
            }
        };
        // F8: the terminal OUTCOME line for the completed manual enrich (reader-frame summary mirrors
        // the auto pass's wording: "nothing to enrich" | "enriched N/M edges").
        let summary = if report.eligible_count == 0 {
            "nothing to enrich".to_string()
        } else {
            format!(
                "enriched {}/{} edges",
                report.enriched_count, report.eligible_count
            )
        };
        crate::oplog::log_op_outcome(
            "enrich",
            repo_uid,
            Some(&snapshot_uid),
            &format!("completed ({summary})"),
        );

        // Emit completion progress (best-effort; skip if the client already departed).
        if !client_gone {
            let _ = emitter.emit(ProgressDetail {
                phase: "complete".to_string(),
                current: 1,
                total: 1,
            });
        }

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
                // ENRICH-YIELD-1: the reader-frame, per-gate first-rejection breakdown of the
                // candidates that did NOT promote — silently dropped before this slice. Additive to
                // the existing object (no new command/flag); {candidates, promoted, rejected,
                // rejections:[{reason, gate, label, count}]}.
                "funnel": serde_json::to_value(p.funnel()).unwrap_or(serde_json::Value::Null),
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
                // ENRICH-ROOT-1 §2: edges NOT attempted (project context lacked a toolchain), with
                // the per-context breakdown. Additive; `eligible = enriched + failed + not_attempted`.
                "not_attempted_count": report.not_attempted_count,
                "skipped_contexts": report.skipped_contexts.iter().map(|c| serde_json::json!({
                    "context_path": c.context_path,
                    "reason": c.reason,
                    "edge_count": c.edge_count,
                })).collect::<Vec<_>>(),
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
    /// INDEX-BASIS-1: compute working-tree drift for a query handler
    /// (orient/check/explain). Resolves the on-disk repo root (the same
    /// `resolve_root_path` pattern the churn/hotspots/risk handlers use to reach
    /// git) and hands git + the indexed-file/module facts to
    /// [`crate::index_drift::compute_index_drift`]. A failure to resolve the repo
    /// path is rendered as an honest `Unknown`/`BasisUnknown`, never a false
    /// "clean".
    fn compute_query_drift(
        &self,
        storage: &StorageConnection,
        repo_state: &crate::state::RepoState,
        repo_uid: &str,
        snapshot: &repo_graph_agent::storage_port::AgentSnapshot,
    ) -> repo_graph_agent::dto::index_drift::IndexDrift {
        let repo_path = match storage.get_repo(&RepoRef::Uid(repo_uid.to_string())) {
            Ok(Some(r)) => crate::handlers::quality::support::resolve_root_path(
                repo_state.db_path(),
                &r.root_path,
            ),
            // A storage MISS or READ ERROR means git was never reached → drift is
            // genuinely UNKNOWN (never `BasisUnknown`, which would falsely claim the
            // snapshot "predates basis tracking"; never a false clean). `Err` preserves
            // the actual `StorageError` in the reason instead of discarding it.
            Ok(None) => {
                return crate::index_drift::unresolved_repo_drift(
                    snapshot.basis_commit.clone(),
                    "repo metadata not found in storage; cannot resolve repo path to compute \
                     drift"
                        .to_string(),
                );
            }
            Err(e) => {
                return crate::index_drift::unresolved_repo_drift(
                    snapshot.basis_commit.clone(),
                    format!("repo metadata could not be read from storage to compute drift ({e})"),
                );
            }
        };
        // INDEX-BASIS-1 (operator RULING 3): read the WRITE-time `index_basis` outcome
        // record (if any) so a no-basis snapshot renders the TRUE state (non-git / unborn /
        // HEAD-unreadable / pre-slice) from a recorded fact, never a live HEAD re-probe.
        // `storage` is a concrete `StorageConnection` (`TrustStorageRead`), which the narrow
        // agent port lacks — so the read lives here and the outcome is passed into the generic
        // computation. A read/parse failure is carried as `Err` → rendered Unknown-with-reason,
        // never a false BasisUnknown (honesty rule #1). Consulted only on the no-basis branch.
        let basis_outcome = crate::index_drift::read_basis_outcome(storage, &snapshot.snapshot_uid);
        crate::index_drift::compute_index_drift(
            storage,
            &repo_path,
            &snapshot.snapshot_uid,
            snapshot.basis_commit.as_deref(),
            basis_outcome,
        )
    }

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

        // Parse optional budget (default: small).
        // TRUNCATION-AUDIT-1: "full" (the `--full` escape hatch) maps to the uncapped tier.
        let budget = match request.params.get("budget").and_then(|v| v.as_str()) {
            None | Some("small") => Budget::Small,
            Some("medium") => Budget::Medium,
            Some("large") => Budget::Large,
            Some("full") => Budget::Full,
            Some(other) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::invalid_request(format!(
                        "invalid budget value: {} (expected small|medium|large|full)",
                        other
                    )),
                );
            }
        };

        // Acquire read lock
        let lock_start = Instant::now();
        let _read_guard = repo_state.coordinator.acquire_read();
        let storage = match self.open_storage(&repo_state) {
            Ok(s) => s,
            Err(e) => return DispatchResult::error(&request.id, e),
        };
        let lock_ms = lock_start.elapsed().as_millis();

        // Get wall-clock timestamp for waiver expiry evaluation
        let now = utc_now_iso8601();

        // DAEMON-CANCEL-3: cheap handler-boundary cancel check (the "before" layer,
        // replacing the prior fire-and-forget heartbeat). If the peer is already gone,
        // skip the whole orient computation and report "before" — distinct from the
        // in-loop "during" cancellation that the cycle Tarjan / complexity loop raise.
        if crate::cancel::pre_work_check(emitter, "computing_orient").is_break() {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::new(
                    ErrorCode::Cancelled,
                    "orient query cancelled (client disconnected before computation)",
                ),
            );
        }

        // COHERENCE-LEAF-SERVE-IMPL-1 + EC-M2-LEAF-SERVE-1: orient bounded (b)-leaf
        // SERVE-THEN-FALLBACK. Resolve the latest snapshot uid first (a cheap `snapshots` read —
        // NOT a `nodes`/`edges` read) so the serve WITNESS — the bounded FOCUS-RESOLUTION ∧
        // CALLGRAPH cert PLUS the per-leaf M-2 decisions — can be evaluated BEFORE the use case
        // runs. GREEN -> run orient through the StoragePort decorator: focus resolution +
        // callers/callees are served from the CURRENT-STATE LiveGraph with ZERO eager
        // `nodes`/`edges` reads for those leaves; cycle VALUES additionally serve from the
        // LiveGraph module-cycle SCC when the cycles cert's VALUES verdict is GREEN (EC-M2 /
        // CYCLES-B — the canonical agent shapes proven byte-equal at cert build; supersedes the
        // CYCLES-A delegate-always posture), and MODULE_SUMMARY structural counts serve from the
        // LiveGraph structural inventory when the module-summary identity-reconciliation cert is
        // GREEN (EC-M2 / DR-2; divergence ⇒ RED ⇒ SQLite). The (c) trust contributor stays SQLite
        // FOREVER (Contract Clause 3) — delegated and SQLite-LABELLED. The IMPORT_CYCLES leaf
        // LABEL keeps the shipped cert-gated `orient_cycles_outcome` semantics; the MODULE_SUMMARY
        // leaf LABEL follows the ACTUAL M-2 serve (a new decision; absent = today's plain sqlite
        // leaf, byte-identical). `build_orient_envelope`'s CALLGRAPH leaf LABEL follows this SAME
        // `serve_from_lg` decision (review-3 item 1): on green the served callers/callees outcomes
        // peek the GREEN callgraph cert so the full served path is zero per-call read for the callgraph leaf
        // (not just the decorator's value serve); on RED they are SQLite-LABELLED. Each leaf degrades
        // independently (review-0 #1); ALL leaves RED / non-resident / non-TS / no-snapshot -> the
        // unchanged eager SQLite path. Every cert build reads SQLite ONCE per fingerprint (the drilldown
        // invariant).
        //
        // W-B-EPOCH-IMPL-1: capture the request epoch ONCE here. This single resolve REPLACES both the
        // prior serve-decision resolve AND the orient use case's internal `get_latest_snapshot` (the
        // double-resolve, now deleted in `orient/repo.rs`): the pinned `&AgentSnapshot` is threaded into
        // `orient_cancellable`. `epoch.fingerprint` is the BUILD-THEN-PEEK LG-serve witness pin (EC-M2
        // review-0 #1: `Some` iff AT LEAST ONE of the three independent leaf decisions is GREEN at the
        // resident fingerprint); `serve_from_lg = serve_witness.bounded` is the SAME bounded (fr∧cg)
        // serve decision the prior `orient_bounded_cert_is_green(...)` produced under W-A — now ONE of
        // the three, gating only the six (b) methods + the callgraph leaf label. No READY snapshot ->
        // the prior `OrientError::NoSnapshot` (raised by the use case before) is raised HERE.
        // The agent-DTO snapshot (`AgentSnapshot`) via the `AgentStorageRead` trait — NOT the inherent
        // `StorageConnection::get_latest_snapshot` (which returns the storage `Snapshot`); the use case and
        // the `RequestEpoch` both speak the agent DTO. Same READY-row selection (the trait impl delegates to
        // the inherent method).
        // EC-M2-LEAF-SERVE-1 (review-0 #1): the eligibility capture is now the FULL serve witness —
        // the EV-A pin fingerprint PLUS three INDEPENDENT leaf decisions peeked at it: the bounded
        // (fr∧cg) fold for the six (b) methods, and the per-leaf M-2 decisions (cycle VALUES /
        // MODULE_SUMMARY). The M-2 leaf certs are warmed whenever a fingerprint is computable
        // (once per fingerprint, pre-use-case) — INDEPENDENT of the bounded outcome, so a GREEN
        // M-2 leaf serves even when an unrelated bounded sub-cert is RED. Only when NO leaf serves
        // (fingerprint None) is the path byte-identical bare SQLite (the pre-M-2 daemon).
        let (epoch, serve_witness) =
            match repo_graph_agent::AgentStorageRead::get_latest_snapshot(&storage, &repo_uid) {
                Ok(Some(snapshot)) => {
                    let witness = crate::orient_serve::orient_serve_witness(
                        &repo_state,
                        &snapshot.snapshot_uid,
                    );
                    (
                        crate::livegraph_feed::RequestEpoch {
                            snapshot,
                            fingerprint: witness.fingerprint.clone(),
                        },
                        witness,
                    )
                }
                Ok(None) => {
                    // DAEMON-VISIBILITY-1 (F2): never a bare "index the repo first" when a partial
                    // snapshot exists. The shared helper NAMES the interrupted snapshot (state, when,
                    // size) + both next actions under the honest `SnapshotNotFound` code (the daemon,
                    // unlike the pure agent use case, has the DB path for size).
                    return DispatchResult::error(
                        &request.id,
                        Self::no_ready_snapshot_detail(&storage, repo_state.db_path(), &repo_uid),
                    );
                }
                Err(e) => {
                    return DispatchResult::error(
                        &request.id,
                        ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                    );
                }
            };
        // review-0 #1: `serve_from_lg` is the BOUNDED (fr∧cg) decision ONLY — it gates the six (b)
        // methods' serve AND the callgraph leaf LABEL below (the review-3 item-1 honesty gate:
        // labelling from `fingerprint.is_some()` would mint false `livegraph` callgraph provenance
        // on an M-2-only serve). The decorator itself is constructed whenever ANY leaf serves.
        let serve_from_lg = serve_witness.bounded;

        // ORIENT-FACT-COHERENCE-1 (operator ruling D-then-B; review-1 F1): is ANY enrichment pass — the
        // AUTO background pass OR an explicit `rmap enrich` — queued/running for THIS repo right now? If
        // so, orient renders the in-flight enrichment truth and suppresses the stale "run `rmap enrich`"
        // CTA — the FRAKTAG divergence was this exact window (orient captured pre-pass, check post-pass).
        // The composed `DaemonState` predicate unions the coordinator (auto) with the activity registry
        // (explicit enrich); under the W-B epoch a reader is ADMITTED alongside a `Refreshing` enrich, so
        // this window is real for both kinds. Repo-scoped so a second repo's concurrent pass never
        // mislabels this one. Threaded into the trust aggregator (the enrichment posture / CTA-suppression
        // source) and the envelope CTA.
        // ORIENT-SMALL-ENRICH-1 (§1a/§2.1): the in-flight fact is repo-scoped but capability-BLIND — a pass
        // is entered-in-flight for a repo before the per-language skip decision, so on a repo the pass will
        // skip it is a true-but-irrelevant daemon fact that must NOT render "figures may rise". GATE it on
        // `in_flight_pass_can_apply`: apply the in-flight posture ONLY when ≥1 materially-present language is
        // ENRICHABLE NOW (has a CONFIGURED resolver — the SAME predicate the D5 CTA reads), which the running
        // pass can actually raise. The count read runs ONLY while a pass is actually in flight (the `if`
        // guards it off the hot path). reviewer review-1 F1: a FAILED count read is NOT collapsed to `false`
        // (that would classify "pass does not apply" from a read that never happened, hiding the reason and
        // silently rendering the persisted posture). Orient has no unknown-enrichment render channel (the
        // frozen `EnrichmentState` sum carries no `Unknown`), so the honest surface is the established
        // structured handler error — the SAME `InternalError` orient already returns for an internal read
        // failure — naming the reason. This only fires in the rare window where a pass IS in flight AND a
        // genuine storage read fails (empty repos read `Ok(vec![])`, never `Err`).
        let enrich_in_flight = if self.state.enrichment_in_flight_for_db(repo_state.db_path()) {
            match crate::reader_context::in_flight_pass_can_apply(
                repo_graph_agent::AgentStorageRead::query_file_count_by_language(
                    &storage,
                    &epoch.snapshot.snapshot_uid,
                )
                .map_err(|e| e.to_string()),
                &configured_resolver_languages_from_env(),
            ) {
                Ok(applies) => applies,
                Err(reason) => {
                    return DispatchResult::error(
                        &request.id,
                        ErrorDetail::new(
                            ErrorCode::InternalError,
                            format!(
                                "orient: an enrichment pass is in flight but the per-language \
                                 file-count read failed, so whether it can raise this repo's \
                                 resolution figures is unknown: {reason}"
                            ),
                        ),
                    );
                }
            }
        } else {
            false
        };
        // ORIENT-FACT-COHERENCE-1 (operator ruling review-3 = Option 2): lift the (now capability-gated)
        // in-flight bool into the enum-typed lifecycle override the agent use case takes. `Some(InFlight)`
        // is the authoritative daemon truth for a queued/running pass that CAN apply here; `None` means "no
        // override — derive from storage" (the agent reads the persisted enrichment state exactly as
        // before). The daemon injects only `InFlight` today; the enum is the single representation of the
        // enrichment state end-to-end.
        let enrich_state_override =
            enrich_in_flight.then_some(repo_graph_agent::EnrichmentState::InFlight);

        // Call the agent orient use case.
        //
        // DAEMON-CANCEL-3: run it through `orient_cancellable` with a cooperative
        // checkpoint built from THIS request's emitter (`loop_checkpoint`). orient
        // runs ON the transport thread (which owns the emitter), so the checkpoint
        // emits a heartbeat at each bounded interval inside the heavy module-cycle
        // Tarjan and complexity FETCH_ALL materialization; a peer disconnect makes
        // that write fail, breaking the loop so a disconnected client's in-flight
        // orient abandons the heavy work instead of running it to completion. The
        // `checkpoint` borrows `emitter` for the duration of the call, so it is scoped
        // to this block and dropped before the cancellation-classification probe.
        let orient_start = Instant::now();
        let orient_outcome = {
            let mut checkpoint = crate::cancel::loop_checkpoint(emitter, "computing_orient");
            if epoch.fingerprint.is_some() {
                // EC-M2-LEAF-SERVE-1 (review-0 #1): construct the decorator whenever ANY leaf
                // decision is GREEN, carrying ALL THREE independently — the bounded fold gates the
                // six (b) methods; cycle VALUES + MODULE_SUMMARY serve iff their own certs were
                // GREEN at the captured fingerprint (each leaf degrades independently to SQLite).
                let decorator = crate::orient_serve::OrientServeDecorator::with_leaf_serves(
                    &repo_state.livegraph,
                    &storage,
                    &epoch,
                    serve_witness.bounded,
                    serve_witness.m2,
                );
                repo_graph_agent::orient_cancellable(
                    &decorator,
                    &repo_uid,
                    &epoch.snapshot,
                    focus,
                    budget,
                    &now,
                    enrich_state_override,
                    &mut checkpoint,
                )
            } else {
                repo_graph_agent::orient_cancellable(
                    &storage,
                    &repo_uid,
                    &epoch.snapshot,
                    focus,
                    budget,
                    &now,
                    enrich_state_override,
                    &mut checkpoint,
                )
            }
        };
        let mut result = match orient_outcome {
            Ok(r) => r,
            Err(e) => {
                // Classify: the cancellable read paths only fail with a cancellation
                // when their checkpoint broke, which happens iff the emitter write
                // failed (peer gone). Probe the emitter once: a failing write means the
                // peer disconnected mid-computation -> Cancelled; otherwise it is a
                // genuine internal failure (read-only ⇒ nothing to roll back either way).
                if crate::cancel::pre_work_check(emitter, "orient_cancelled").is_break() {
                    return DispatchResult::error(
                        &request.id,
                        ErrorDetail::new(
                            ErrorCode::Cancelled,
                            "orient query cancelled (client disconnected during computation)",
                        ),
                    );
                }
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                );
            }
        };
        let orient_ms = orient_start.elapsed().as_millis();

        // CLI-OUT-2B: Inject display_name for human renderers
        result.display_name = Some(display_name);

        // EMBED-SEED-IMPL-1 (spec §8, Group A): the semantic fallback tier. Fires
        // ONLY on the deterministic-zero no-match branch (byte-unchanged for every
        // resolved/ambiguous result), filling the previously-empty
        // `focus.candidates` with labeled Layer-3 embedding hints + a `Limit`.
        {
            // Canonical registry root for `next.cwd` (review-2 #2), not the raw
            // `repo` param (may be an alias / relative path). `None` when the registry
            // lookup is unavailable ⇒ `next.cwd` omitted with a reason (operator
            // ruling 2 — never a fabricated empty cwd).
            let repo_root = self.canonical_root(&request.params);
            seed_dispatch::apply_semantic_fallback(
                &mut result,
                &storage,
                &epoch.snapshot.snapshot_uid,
                &repo_uid,
                repo_state.db_path(),
                repo_root.as_deref(),
                seed_dispatch::SEMANTIC_FALLBACK_CAP,
            );
        }

        // ORIENT-LIVEGRAPH-IMPL: assemble the `CoherenceEnvelope<CoherentOrientResult>` response. This
        // REPLACES the prior post-serialize top-level `trust` overlay injection: the degraded-state
        // briefing now rides on `value.trust_briefing` (D-ORIENT-6 = O2), and the wrapper adds per-signal
        // provenance/trust/freshness + the root MEET. The FOUR LG-first leaves (IMPORT_CYCLES /
        // HIGH_COMPLEXITY / CALLERS_SUMMARY / CALLEES_SUMMARY) are each labelled by a daemon-side NO-LOSS
        // proof (the cycles / complexity no-loss certs + the callers/callees `Auto` ladder with a per-symbol
        // no-loss key compare); everything else is SQLite/Authority/FS.
        let _ = emitter.emit(ProgressDetail {
            phase: "assembling_coherence_envelope".to_string(),
            current: 0,
            total: 1,
        });
        let envelope_start = Instant::now();
        // COHERENCE-LEAF-SERVE-IMPL-1 (review-3 item 1): pass the bounded SERVE DECISION (`serve_from_lg`,
        // computed above) into envelope assembly so the CALLERS/CALLEES callgraph leaf LABEL follows the
        // ACTUAL serve. On `serve_from_lg == false` the agent ran over BARE SQLite, so those leaves are
        // SQLite-LABELLED — never re-certified `livegraph` from the callgraph cert state alone (which could
        // be GREEN even when a DIFFERENT bounded contributor, e.g. focus-resolution, forced the fallback).
        let envelope = crate::orient_coherence::build_orient_envelope(
            &repo_state,
            &repo_uid,
            result,
            serve_from_lg,
            // EC-M2-LEAF-SERVE-1: the MODULE_SUMMARY leaf label follows the ACTUAL serve — the
            // decorator served the counts from the LiveGraph iff the module-summary cert was
            // GREEN at the captured fingerprint (review-0 #1: INDEPENDENT of the bounded fold —
            // a GREEN witness always constructs the decorator) ∧ the epoch is STILL resident
            // after the use case ran (a mid-request swap made the decorator's EV-A gate delegate
            // to SQLite — the post-serve revalidation keeps the label from minting false
            // `{livegraph}` provenance on that race; under-claim only, never over-claim).
            serve_witness.m2.module_summary
                && crate::orient_serve::epoch_still_resident(&repo_state.livegraph, &epoch),
            // ORIENT-FACT-COHERENCE-1: suppress the enrich CTA + render the in-flight truth on the
            // relationship_next_action when a pass is in flight for this repo.
            enrich_in_flight,
        );
        let mut output = match serde_json::to_value(&envelope) {
            Ok(v) => v,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                );
            }
        };

        // INDEX-BASIS-1 + ORIENT-SEGMENT-2: compute the query-time drift (needs
        // `&self`), then hand ALL post-serialize additive `value` fields — drift,
        // parse, the §2.1 collapse fallback, the §2.5 HTTP headline — to the orient
        // orchestrator (kept out of dispatch; see `orient_additive_fields`).
        let index_drift =
            self.compute_query_drift(&storage, &repo_state, &repo_uid, &epoch.snapshot);
        crate::orient_additive_fields::inject(
            &mut output,
            &index_drift,
            &repo_state,
            emitter,
            &storage,
            &repo_uid,
            &epoch.snapshot.snapshot_uid,
        );

        let envelope_ms = envelope_start.elapsed().as_millis();

        let total_ms = handler_start.elapsed().as_millis();

        // RMAPD-PERF-1: Timing instrumentation (enable with --features perf-trace)
        perf_trace!(
            "[PERF] orient: total={}ms resolve={}ms lock={}ms orient={}ms envelope={}ms",
            total_ms,
            resolve_ms,
            lock_ms,
            orient_ms,
            envelope_ms
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
        let storage = match self.open_storage(&repo_state) {
            Ok(s) => s,
            Err(e) => return DispatchResult::error(&request.id, e),
        };
        let lock_ms = lock_start.elapsed().as_millis();

        // Get wall-clock timestamp for waiver expiry evaluation
        let now = utc_now_iso8601();

        // DAEMON-CANCEL-3: handler-boundary cancel check (replaces the fire-and-forget
        // heartbeat). If the peer is already gone, skip the whole check computation.
        if crate::cancel::pre_work_check(emitter, "running_check").is_break() {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::new(
                    ErrorCode::Cancelled,
                    "check query cancelled (client disconnected before computation)",
                ),
            );
        }

        // Run check on a WORKER thread, supervised here (DAEMON-CANCEL-3). check
        // INHERITS trust's heavy work (`get_trust_summary` → `compute_module_stats` SQL
        // + the unresolved-sample loop) and the gate complexity-measurement load — all
        // reached through this `storage` connection. On peer-disconnect `on_disconnect`
        // fires `sqlite3_interrupt` to abort whichever opaque `SELECT` is in flight (the
        // trust stats or the gate complexity load), while the cooperative `CancelFlag`
        // — threaded via `run_check_cancellable` → `get_trust_summary_cancellable` —
        // breaks the pure trust sample loop. So fixing #2 (complexity, SQL-interrupted)
        // + #3 (trust) and routing check through them gives check in-flight cancel too.
        // INDEX-BASIS-1: compute working-tree drift (git + storage) BEFORE `storage`
        // is moved into the worker below. Drift becomes check's `INDEX_DRIFT`
        // condition (Incomplete when the tree has moved past the basis; Pass when
        // clean / not-a-git-repo). `None` only when there is no snapshot to anchor
        // to — the reducer then omits the condition rather than fabricate it.
        // CHECK-SIGNAL-1: compute BOTH the index-drift fact AND the permanent-ceiling capability
        // fact from the ONE `get_latest_snapshot` read. The ceiling fact = every materially-present
        // code language has no resolver on any build (the SAME materiality × resolver facts the D5
        // CTA reads, via `call_graph_ceiling_languages`); it lets check reclassify a permanent LOW /
        // "did not run" as a passing stated limitation instead of a false Fail (§2.1/§2.2).
        //
        // HONEST DEGRADATION (operator ruling 2026-08-31 `ceiling-read-unknown`, superseding
        // build-1's `Option<ResolutionCeiling>` whose `None` conflated no-ceiling with a failed
        // read): the read yields the exhaustive capability sum `CeilingFact`. A successful read →
        // `Ceiling { languages }` (permanent) or `NoCeiling` (actionable); a FAILED read →
        // `Unknown { reason }` — carried in-band as the RECORD (never a stderr-only log), rendering
        // unknown-with-reason on the affected condition and contributing to the verdict exactly as
        // NoCeiling (failing) — a read failure may never mint a false passing ceiling.
        let (index_drift, ceiling_fact, pass_can_apply, reliability_by_language) =
            match repo_graph_agent::AgentStorageRead::get_latest_snapshot(&storage, &repo_uid) {
                Ok(Some(snap)) => {
                    let drift = self.compute_query_drift(&storage, &repo_state, &repo_uid, &snap);
                    // `reader_context` owns the WHICH-languages computation (one source, never
                    // re-derived); one count read feeds BOTH the reducer's ceiling verdict AND the
                    // in-flight applicability gate. The fallible read maps into the exhaustive capability
                    // sum `CeilingFact` INLINE at this composition root — the same daemon→agent injected-fact
                    // pattern as `IndexDrift` (successful read → `Ceiling`/`NoCeiling`; FAILED read →
                    // `Unknown { reason }`, unknown-with-reason on the affected condition, never swallowed to
                    // a sentinel, never a false Pass). The mapping is inlined (not a `reader_context` helper)
                    // because this is its SOLE production caller — the pre-slice shape, restored per reviewer
                    // review-1 F2. ORIENT-SMALL-ENRICH-1 (§1a/§2.1): `in_flight_pass_can_apply` — the STRICTER
                    // pass-applicability fact (≥1 materially-present CONFIGURED-enrichable language) distinct
                    // from `NoCeiling`, computed from the SAME counts (no second read) so the override below
                    // renders the in-flight posture ONLY where the running pass can raise figures.
                    let counts = repo_graph_agent::AgentStorageRead::query_file_count_by_language(
                        &storage,
                        &snap.snapshot_uid,
                    )
                    .map_err(|e| e.to_string());
                    let ceiling_fact = Some(match &counts {
                        Ok(counts) => {
                            match crate::reader_context::call_graph_ceiling_languages(counts) {
                                Some(languages) => {
                                    repo_graph_agent::dto::ceiling_fact::CeilingFact::Ceiling {
                                        languages,
                                    }
                                }
                                None => repo_graph_agent::dto::ceiling_fact::CeilingFact::NoCeiling,
                            }
                        }
                        Err(reason) => repo_graph_agent::dto::ceiling_fact::CeilingFact::Unknown {
                            reason: reason.clone(),
                        },
                    });
                    // ORIENT-SMALL-ENRICH-1 (reviewer review-1 F1): the gate now PRESERVES a count-read
                    // failure instead of collapsing it to `false`. The in-flight override applies ONLY on an
                    // affirmative `Ok(true)`; a gate `Err` does NOT apply it. That is honest here because the
                    // SAME read's failure is ALREADY rendered as `ceiling_fact = Unknown { reason }` above
                    // (CHECK-SIGNAL-1 ratified `ceiling-read-unknown` — this read degrades to
                    // unknown-with-reason, NEVER a hard error), so check renders its persisted posture
                    // ALONGSIDE the unknown ceiling: the reason is surfaced, not swallowed. `matches!` is a
                    // pattern match, NOT the `unwrap_or*`/`.ok()` collapse the STANDING HONESTY RULE forbids —
                    // the failure reason is retained (in `ceiling_fact`), only the override decision is bool.
                    // CHECK-LANG-SPLIT-1 (§2): for a MIXED repo, compute the per-language breakdown line
                    // from the SAME `reader_context` materiality gate × the `reliability` per-language read
                    // the `reliability` handler serves. Gate on mixed-ness FIRST (over the file counts we
                    // already hold) so a single-language repo issues NO extra read and stays byte-identical
                    // (§2.4). A FAILED per-language read renders unknown-with-reason (STANDING HONESTY RULE
                    // 1). On a failed COUNT read mixed-ness is undecidable, so no breakdown — the count
                    // failure is already surfaced with its reason via `ceiling_fact = Unknown`. Computed
                    // BEFORE the `pass_can_apply` line below, which MOVES `counts`.
                    let reliability_by_language = match &counts {
                        Ok(c)
                            if crate::reliability_breakdown_line::is_mixed_material_code_repo(
                                c,
                            ) =>
                        {
                            let by_lang = storage
                                .query_call_resolution_by_language(&snap.snapshot_uid)
                                .map_err(|e| e.to_string());
                            crate::reliability_breakdown_line::reliability_by_language_line_or_read_error(
                                c, by_lang,
                            )
                        }
                        _ => None,
                    };
                    let pass_can_apply = matches!(
                        crate::reader_context::in_flight_pass_can_apply(
                            counts,
                            &configured_resolver_languages_from_env(),
                        ),
                        Ok(true)
                    );
                    (
                        Some(drift),
                        ceiling_fact,
                        pass_can_apply,
                        reliability_by_language,
                    )
                }
                // No snapshot → the call-graph condition is not evaluated; no ceiling analysis, and no
                // pass can apply (nothing to raise).
                Ok(None) => (None, None, false, None),
                Err(e) => {
                    // Snapshot read failed here; the check reducer re-reads it and will
                    // surface the failure. Omit the drift condition (never a false value); no
                    // ceiling analysis is attempted without a snapshot to anchor to.
                    eprintln!("warning: index-drift snapshot read failed for {repo_uid}: {e}");
                    (None, None, false, None)
                }
            };

        // ORIENT-FACT-COHERENCE-1: the SAME repo-scoped in-flight fact orient uses, so check renders the
        // honest non-failing in-flight ENRICHMENT_STATE (never "did not run" during a running pass) and
        // the two surfaces tell ONE story for one snapshot. Computed before the move-closure.
        // ORIENT-FACT-COHERENCE-1 (operator ruling review-3 = Option 2): lifted into the enum-typed
        // lifecycle override `run_check_cancellable` now takes — `Some(InFlight)` = authoritative daemon
        // truth, `None` = derive from storage. `Option<EnrichmentState>` is `Copy`, so it moves into the
        // worker closure like the bool it replaces.
        // ORIENT-SMALL-ENRICH-1 (§1a/§2): GATE the override on `pass_can_apply` (the STRICTER
        // pass-applicability fact computed above from the same count read) — apply the in-flight posture
        // ONLY when the running pass can raise THIS repo's figures (≥1 materially-present CONFIGURED-enrichable
        // language). On a permanent ceiling, a config-only repo, a Java-without-JDTLS repo, or an `Unknown`
        // capability the persisted state stands, so check renders its honest ceiling/no-eligible-edges posture
        // instead of "figures may rise", and orient/check/reliability agree for one snapshot.
        let enrich_state_override = (self.state.enrichment_in_flight_for_db(repo_state.db_path())
            && pass_can_apply)
            .then_some(repo_graph_agent::EnrichmentState::InFlight);

        let check_start = Instant::now();
        let mut check_result = {
            // Hoist the interrupt handle BEFORE moving the connection into the worker
            // (S-A; safe no-op if fired after the worker drops the connection).
            let interrupt = storage.interrupt_handle();
            let repo_uid_w = repo_uid.clone();
            let now_w = now.clone();
            let drift_w = index_drift.clone();
            let ceiling_fact_w = ceiling_fact.clone();
            let reliability_by_language_w = reliability_by_language.clone();
            match crate::cancel::run_interruptible(
                emitter,
                "running_check",
                move || interrupt.interrupt(),
                move |flag| {
                    let mut checkpoint = flag.checkpoint();
                    repo_graph_agent::run_check_cancellable(
                        &storage,
                        &repo_uid_w,
                        &now_w,
                        drift_w.clone(),
                        enrich_state_override,
                        ceiling_fact_w.clone(),
                        reliability_by_language_w.clone(),
                        &mut checkpoint,
                    )
                    .map_err(|e| e.to_string())
                },
            ) {
                crate::cancel::Supervised::Completed(Ok(result)) => result,
                // A genuine storage/gate failure while the peer stayed connected.
                crate::cancel::Supervised::Completed(Err(msg)) => {
                    return DispatchResult::error(
                        &request.id,
                        ErrorDetail::new(ErrorCode::InternalError, msg),
                    );
                }
                crate::cancel::Supervised::Cancelled => {
                    return DispatchResult::error(
                        &request.id,
                        ErrorDetail::new(
                            ErrorCode::Cancelled,
                            "check query cancelled (client disconnected during computation)",
                        ),
                    );
                }
                // WorkerVanished ≠ Cancelled (CANCEL-1 deliverable #2).
                crate::cancel::Supervised::WorkerVanished => {
                    return DispatchResult::error(
                        &request.id,
                        ErrorDetail::new(
                            ErrorCode::InternalError,
                            "check worker vanished (internal failure during computation)",
                        ),
                    );
                }
            }
        };
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

        // CLI-OUT-2B: Inject display_name for human renderers. (`check_result` is the
        // worker's already-unwrapped success value; failures returned early above.)
        check_result.display_name = Some(display_name);

        // CHECK-LIVEGRAPH-IMPL: assemble the `CoherenceEnvelope<CoherentOrientResult>` response,
        // mirroring `handle_orient`. check has NO LiveGraph leaf, NO cert, and NO trust overlay
        // (D-CHECK-2/4), so this is a THIN stale-read + delegate: the adapter reads the
        // AUTHORITATIVE stale-index flag (`get_stale_files`) and labels the verdict with honest
        // MEET freshness + the multi-source verdict provenance. The verdict VALUE is byte-identical
        // to before; only the wrapper adds labels.
        let envelope = crate::check_coherence::build_check_envelope(&repo_state, check_result);
        match serde_json::to_value(&envelope) {
            Ok(v) => DispatchResult::success(&request.id, v),
            Err(e) => DispatchResult::error(
                &request.id,
                ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
            ),
        }
    }

    fn handle_explain(
        &self,
        request: &Request,
        emitter: &mut dyn ProgressEmitter,
    ) -> DispatchResult {
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
        // CLI contract: explain accepts medium|large, plus "full" (TRUNCATION-AUDIT-1, the
        // `--full` uncapped escape hatch); not small.
        let budget = match request.params.get("budget").and_then(|v| v.as_str()) {
            None | Some("medium") => Budget::Medium,
            Some("large") => Budget::Large,
            Some("full") => Budget::Full,
            Some(other) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::invalid_request(format!(
                        "invalid budget value: {} (expected medium|large|full)",
                        other
                    )),
                );
            }
        };

        // Acquire read lock
        let _read_guard = repo_state.coordinator.acquire_read();
        let storage = match self.open_storage(&repo_state) {
            Ok(s) => s,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        // Get wall-clock timestamp for waiver expiry evaluation
        let now = utc_now_iso8601();

        // DAEMON-CANCEL-3: cheap handler-boundary cancel check (the "before" layer).
        // explain previously took no emitter at all; now it gets one, so it can skip
        // the whole computation if the peer is already gone (reported "before",
        // distinct from the in-loop "during" cancellation in the cycle Tarjan).
        if crate::cancel::pre_work_check(emitter, "computing_explain").is_break() {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::new(
                    ErrorCode::Cancelled,
                    "explain query cancelled (client disconnected before computation)",
                ),
            );
        }

        // COHERENCE-LEAF-SERVE-IMPL-2 + EC-M2-LEAF-SERVE-1: explain bounded (b)-leaf
        // SERVE-THEN-FALLBACK (the EXPLAIN consumer of the focus-resolution producer; sibling of
        // handle_orient's wiring). Resolve the latest snapshot uid first (a cheap `snapshots` read —
        // NOT a `nodes`/`edges` read) so the SHARED serve witness (bounded FOCUS-RESOLUTION ∧
        // CALLGRAPH + the per-leaf M-2 decisions) can be evaluated BEFORE the use case runs.
        // GREEN -> run explain through the SAME `OrientServeDecorator`: focus resolution
        // (`resolve_path_focus`/`resolve_stable_key_focus`/`get_symbol_context`/
        // `resolve_symbol_name`) is served from the CURRENT-STATE LiveGraph with ZERO eager `nodes`
        // reads, so explain SYMBOL-focus is `nodes`-FREE on green (the `explain_symbol` pipeline
        // emits no MODULE_SUMMARY; its only `nodes` reads ARE those four focus-resolution methods,
        // all decorator-served); cycle VALUES (`find_cycles_involving_*`) additionally serve from
        // the LiveGraph SCC when the cycles cert's VALUES verdict is GREEN (EC-M2 / CYCLES-B), and
        // the FILE/PATH identity structural counts (`compute_{path,file}_summary`) serve from the
        // LiveGraph inventory when the module-summary identity-reconciliation cert is GREEN
        // (EC-M2 / DR-E3). The (c) trust contributor (`get_trust_summary`,
        // edges+unresolved_edges) stays SQLite FOREVER (Contract Clause 3); explain is NEVER
        // `edges`-free on repos where those certs are RED, and the `list_symbols_in_file` /
        // `list_files_in_path` per-item LISTINGS keep their `nodes` reads (no LiveGraph home —
        // the DR-E3 listing half; NOT in M-2's scope).
        // RED / non-resident / non-TS / no-snapshot -> the unchanged eager bare-SQLite path. The cert build
        // reads SQLite ONCE per fingerprint (the drilldown invariant); a cached GREEN/RED reads none.
        //
        // W-B-EPOCH-IMPL-1: capture the request epoch (the pinned snapshot + the BUILD-THEN-PEEK bounded-cert
        // eligibility witness). explain IS epoch-pinned (review-0 #1): whenever a READY snapshot is captured,
        // the handler wraps the SHARED `OrientServeDecorator` — on GREEN and RED alike — and the decorator's
        // `get_latest_snapshot` returns the PINNED snapshot (`storage_port_impl`). `run_explain` derives its
        // `snapshot_uid` SOLELY from that call and threads it into every SQLite read + the response stamp, so
        // the whole explain request resolves to ONE epoch with NO mid-request "latest" re-read (the explain
        // analogue of orient's double-resolve removal — no agent-crate change needed). The fingerprint is
        // the EV-A pin; each leaf's OWN witness decision (bounded / cycle-values / module-summary) gates
        // its serve at that pin, and a `false` (or an all-off fingerprint-None witness) delegates that
        // leaf to SQLite at the pinned uid. A missing READY snapshot keeps the bare-SQLite path, where
        // `run_explain` raises `ExplainError::NoSnapshot` exactly as before.
        // EC-M2-LEAF-SERVE-1 (review-0 #1): the SAME full serve witness orient captures — the
        // EV-A pin plus the THREE INDEPENDENT leaf decisions (bounded fr∧cg for the six (b)
        // methods; cycle VALUES; MODULE_SUMMARY counts) — so explain's decorator serves each
        // GREEN leaf even when an unrelated leaf's cert is RED.
        let mut serve_witness = crate::orient_serve::OrientServeWitness::default();
        let epoch = repo_graph_agent::AgentStorageRead::get_latest_snapshot(&storage, &repo_uid)
            .ok()
            .flatten()
            .map(|snapshot| {
                serve_witness =
                    crate::orient_serve::orient_serve_witness(&repo_state, &snapshot.snapshot_uid);
                crate::livegraph_feed::RequestEpoch {
                    snapshot,
                    fingerprint: serve_witness.fingerprint.clone(),
                }
            });

        // Call the agent explain use case.
        //
        // DAEMON-CANCEL-3: explain now receives this request's emitter (it previously
        // took none — `dispatch` passes it as of this slice) and runs through
        // `run_explain_cancellable` with a cooperative checkpoint, so a peer disconnect
        // mid-Tarjan on the path/symbol-focus pipelines abandons the heavy cycle work.
        // Same transport-thread `loop_checkpoint` seam + scoped-borrow shape as
        // `handle_orient`.
        let explain_outcome = {
            let mut checkpoint = crate::cancel::loop_checkpoint(emitter, "computing_explain");
            if let Some(epoch) = epoch.as_ref() {
                // W-B-EPOCH-IMPL-1: wrap the epoch-pinned decorator whenever a READY snapshot was captured —
                // serving and non-serving witnesses alike. Each GREEN leaf decision serves from the LiveGraph
                // (EV-A gate); every other leaf delegates to SQLite at the pinned uid (review-0 #1: the
                // bounded fold and the M-2 leaves degrade independently; all-off ⇒ `epoch_resident` false ⇒
                // byte-transparent). EITHER WAY the decorator's `get_latest_snapshot` returns the PINNED
                // snapshot, so `run_explain` resolves the snapshot ONCE (no mid-request "latest" re-read)
                // and the whole explain request is coherent at epoch N.
                let decorator = crate::orient_serve::OrientServeDecorator::with_leaf_serves(
                    &repo_state.livegraph,
                    &storage,
                    epoch,
                    serve_witness.bounded,
                    serve_witness.m2,
                );
                repo_graph_agent::run_explain_cancellable(
                    &decorator,
                    &repo_uid,
                    target,
                    budget,
                    &now,
                    &mut checkpoint,
                )
            } else {
                // No READY snapshot captured -> bare SQLite; `run_explain` raises `ExplainError::NoSnapshot`
                // exactly as before (nothing to pin).
                repo_graph_agent::run_explain_cancellable(
                    &storage,
                    &repo_uid,
                    target,
                    budget,
                    &now,
                    &mut checkpoint,
                )
            }
        };
        let mut result = match explain_outcome {
            Ok(r) => r,
            Err(e) => {
                // Same cancellation classification as orient: a failing emitter probe
                // means the peer disconnected mid-computation -> Cancelled.
                if crate::cancel::pre_work_check(emitter, "explain_cancelled").is_break() {
                    return DispatchResult::error(
                        &request.id,
                        ErrorDetail::new(
                            ErrorCode::Cancelled,
                            "explain query cancelled (client disconnected during computation)",
                        ),
                    );
                }
                // DAEMON-VISIBILITY-1 (F2): explain is also a READY-requiring surface — its NoSnapshot
                // gets the SAME honest partial-naming detail as orient (shared helper, `SnapshotNotFound`
                // code); a genuine internal explain failure stays `InternalError`.
                let detail = match &e {
                    repo_graph_agent::ExplainError::NoSnapshot { .. } => {
                        Self::no_ready_snapshot_detail(&storage, repo_state.db_path(), &repo_uid)
                    }
                    _ => ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                };
                return DispatchResult::error(&request.id, detail);
            }
        };

        // CLI-OUT-3: Inject display_name for human renderers.
        result.display_name = Some(display_name);

        // EMBED-SEED-IMPL-1 (spec §8, Group A): the semantic fallback tier on
        // explain's no-match branch (identical contract to orient). Only runs when
        // a READY snapshot epoch was captured (else candidate stable_keys cannot be
        // resolved) — a no-snapshot explain keeps today's output.
        if let Some(ep) = epoch.as_ref() {
            // Canonical registry root for `next.cwd` (review-2 #2), not the raw
            // `repo` param (may be an alias / relative path). `None` when the registry
            // lookup is unavailable ⇒ `next.cwd` omitted with a reason (operator
            // ruling 2 — never a fabricated empty cwd).
            let repo_root = self.canonical_root(&request.params);
            seed_dispatch::apply_semantic_fallback(
                &mut result,
                &storage,
                &ep.snapshot.snapshot_uid,
                &repo_uid,
                repo_state.db_path(),
                repo_root.as_deref(),
                seed_dispatch::SEMANTIC_FALLBACK_CAP,
            );
        }

        // EXPLAIN-LIVEGRAPH-IMPL (operator 2026-06-12): assemble the `CoherenceEnvelope<CoherentOrientResult>`
        // response, mirroring `handle_orient`/`handle_check`. The adapter GENUINELY SERVES each green LG-first
        // leaf's VALUE from the LiveGraph — it rebuilds EXPLAIN_IMPORTS / EXPLAIN_CYCLES from
        // `live_import_view` / `module_import_cycles` and the EXPLAIN_IDENTITY anchor from `node_display`, and
        // gates EXPLAIN_CALLERS / EXPLAIN_CALLEES by the live caller/callee key-set no-loss compare — with a
        // labelled SQLite fallback per leaf when not green. It then folds the honest MEET freshness/provenance
        // and `value.trust_briefing` (the SAME degraded-only `"CALLS+IMPORTS"` overlay this handler injected
        // before, now on the shared container — explain is the SECOND populator after orient). The LG-first
        // values come from the LiveGraph (or the proven SQLite primary on fallback), NOT a relabelled SQLite
        // result.
        let envelope = crate::explain_coherence::build_explain_envelope(
            &repo_state,
            &repo_uid,
            result,
            matches!(budget, Budget::Large),
            // EC-M2-LEAF-SERVE-1: the FILE/PATH identity structural counts were decorator-served
            // from the LiveGraph iff the module-summary cert was GREEN at the captured
            // fingerprint (review-0 #1: INDEPENDENT of the bounded fold; a GREEN M-2 decision
            // implies the witness minted a fingerprint) ∧ the epoch is STILL resident after the
            // use case ran (the post-serve revalidation — a mid-request swap made the decorator
            // delegate to SQLite, so labelling from the pre-captured decision alone would mint
            // false `{livegraph}` provenance; under-claim only, mirroring the orient site).
            epoch.as_ref().is_some_and(|e| {
                serve_witness.m2.module_summary
                    && crate::orient_serve::epoch_still_resident(&repo_state.livegraph, e)
            }),
        );
        match serde_json::to_value(&envelope) {
            Ok(mut v) => {
                // RECON-M-R3a (g2u-b, §5.3.3b): on a SYMBOL focus, add the union-degree second
                // figure to the callers/callees evidence WHERE it differs — additive `union`
                // object with its accounting/coverage label; nothing else changes (zero-SCIP /
                // ledger-absent / no pinned snapshot → no-op, byte-identical, R-0).
                if let Some(epoch) = epoch.as_ref() {
                    crate::witness_projection::WitnessProjection::attach_explain_union_degrees(
                        &repo_state,
                        epoch.snapshot_uid(),
                        &mut v,
                    );
                    // RECON-M-R3b: the incoming reference tier ("which symbols reference this")
                    // on SYMBOL focus — additive, W-BOTH only; R-0/R-1 no-op.
                    crate::witness_projection::WitnessProjection::attach_explain_reference_tier(
                        &repo_state,
                        epoch.snapshot_uid(),
                        &mut v,
                    );
                    // RECON-M-R4 (§5.5): the Layer-2 landing on SYMBOL focus — "this call likely
                    // resolves to X" hints + contested signals for the focus caller. Reads the
                    // focus caller's unresolved CALL sites (the RED floor — SQLite only) at the
                    // pinned snapshot; the projection stays SQLite-free (sites passed in).
                    // Additive, W-BOTH only, no counter touched (R-0/R-1 no-op).
                    use repo_graph_trust::storage_port::TrustStorageRead as _;
                    let layer2_sites = v["value"]["focus"]["resolved_key"]
                        .as_str()
                        .filter(|_| v["value"]["focus"]["resolved_kind"].as_str() == Some("symbol"))
                        .and_then(|key| {
                            repo_state
                                .storage()
                                .ok()?
                                .unresolved_call_sites(epoch.snapshot_uid(), Some(key))
                                .ok()
                        })
                        .unwrap_or_default();
                    crate::witness_projection::WitnessProjection::attach_explain_layer2(
                        &repo_state,
                        epoch.snapshot_uid(),
                        &layer2_sites,
                        &mut v,
                    );

                    // INDEX-BASIS-1: additive working-tree drift on `value`, same
                    // pattern as orient. rgr renders it as explain's "index basis /
                    // drift" footer line.
                    let index_drift =
                        self.compute_query_drift(&storage, &repo_state, &repo_uid, &epoch.snapshot);
                    inject_value_field(&mut v, "index_drift", &index_drift, &repo_uid);
                }
                DispatchResult::success(&request.id, v)
            }
            Err(e) => DispatchResult::error(
                &request.id,
                ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
            ),
        }
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
        let storage = match self.open_storage(&repo_state) {
            Ok(s) => s,
            Err(e) => return DispatchResult::error(&request.id, e),
        };
        let lock_ms = lock_start.elapsed().as_millis();

        // Get latest snapshot
        let snapshot_start = Instant::now();
        let snapshot = match storage.get_latest_snapshot(&repo_uid) {
            Ok(Some(snap)) => snap,
            Ok(None) => {
                return DispatchResult::error(
                    &request.id,
                    Self::no_ready_snapshot_detail(&storage, repo_state.db_path(), &repo_uid),
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

        // DAEMON-CANCEL-3: cheap handler-boundary cancel check (the "before" layer,
        // replacing the prior fire-and-forget heartbeat). If the peer is already gone,
        // skip the whole trust assembly and report "before".
        if crate::cancel::pre_work_check(emitter, "assembling_trust_report").is_break() {
            return DispatchResult::error(
                &request.id,
                ErrorDetail::new(
                    ErrorCode::Cancelled,
                    "trust query cancelled (client disconnected before computation)",
                ),
            );
        }

        // W-B-EPOCH-IMPL-2C: capture the request epoch ONCE — the pinned snapshot + the BUILD-THEN-PEEK
        // stats-cert eligibility witness (`stats_cert_eligibility`), identically to the other 8 mixed-read
        // handlers. Trust's Half-A current-state posture is a PROJECTION of the LiveGraph `module_stats()`
        // answer (trust_coherence.rs), whose no-loss proof is the STATS cert — the SAME cert `stats` gates on,
        // reused (no new cert). The EV-A gate in `build_posture_leaf` fails soft to the Unavailable posture on a
        // fingerprint mismatch, so the LiveGraph posture is NEVER computed from an epoch incoherent with the
        // pinned v1 report (Half B) it ships beside (the cross-epoch split-brain this arc prevents). The cert
        // build runs under the SAME supervisor, so a peer-disconnect mid-aggregation aborts the in-flight
        // SELECT and returns Cancelled rather than a stale witness.
        let fingerprint = match crate::livegraph_feed::stats_cert_eligibility(
            emitter,
            &repo_state,
            &snapshot.snapshot_uid,
        ) {
            crate::livegraph_feed::StatsEligibility::Witness(fp) => fp,
            crate::livegraph_feed::StatsEligibility::Cancelled => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(
                        ErrorCode::Cancelled,
                        "trust query cancelled (client disconnected during stats-cert eligibility)",
                    ),
                );
            }
        };
        // The epoch's `AgentSnapshot` is built from the SAME resolved storage `Snapshot` above — NO second
        // "latest" resolve (the epoch's whole point: pin once, never re-resolve). The mapping mirrors the
        // canonical `AgentStorageRead` impl (storage/src/agent_impl.rs:121-132): storage `kind` -> agent
        // `scope`. `snapshot` itself is retained for the v1 worker below, which needs `toolchain_json` (a field
        // `AgentSnapshot` does not carry). Trust reads only `epoch.snapshot_uid()` (the pinned SQLite identity);
        // it has no orient-style `snapshot::aggregate`, so the other `AgentSnapshot` fields are inert here.
        let epoch = crate::livegraph_feed::RequestEpoch {
            snapshot: repo_graph_agent::AgentSnapshot {
                snapshot_uid: snapshot.snapshot_uid.clone(),
                repo_uid: snapshot.repo_uid.clone(),
                scope: snapshot.kind.clone(),
                basis_commit: snapshot.basis_commit.clone(),
                created_at: snapshot.created_at.clone(),
                files_total: snapshot.files_total.max(0) as u64,
                nodes_total: snapshot.nodes_total.max(0) as u64,
                edges_total: snapshot.edges_total.max(0) as u64,
            },
            fingerprint,
        };

        // COHERENCE-POLISH-1 §2 (operator ruling 2026-09-02, answers review-0): trust CONSUMES the same
        // capability fact `check` does. The daemon constructs the exhaustive capability sum `CeilingFact`
        // INLINE at this composition root — byte-for-byte the same route as `handle_check` (one
        // `query_file_count_by_language` → `reader_context::call_graph_ceiling_languages` → `CeilingFact`,
        // the ratified daemon→agent injected-fact precedent) — and then converts it to the serializable
        // wire mirror via the ONE production `From<&CeilingFact> for CeilingReport` impl. There is no
        // second derivation of the ceiling classification: `CeilingReport` is a pure mechanical mirror of
        // the `CeilingFact` this build already produced, never an independent re-classification.
        // Read HERE, before `storage` is moved into the worker below. The fallible read maps to
        // `Unknown { reason }` (STANDING HONESTY RULE 1: a classified fallible read is never swallowed to
        // a sentinel), not a false "no ceiling". Trust renders the ceiling posture + suppresses
        // "below N% target" for an at-ceiling repo.
        let ceiling_fact: repo_graph_agent::dto::ceiling_fact::CeilingFact =
            match repo_graph_agent::AgentStorageRead::query_file_count_by_language(
                &storage,
                &snapshot.snapshot_uid,
            ) {
                Ok(counts) => match crate::reader_context::call_graph_ceiling_languages(&counts) {
                    Some(languages) => {
                        repo_graph_agent::dto::ceiling_fact::CeilingFact::Ceiling { languages }
                    }
                    None => repo_graph_agent::dto::ceiling_fact::CeilingFact::NoCeiling,
                },
                Err(e) => repo_graph_agent::dto::ceiling_fact::CeilingFact::Unknown {
                    reason: e.to_string(),
                },
            };
        let call_graph_ceiling =
            repo_graph_agent::dto::ceiling_fact::CeilingReport::from(&ceiling_fact);

        // Compute the trust report on a WORKER thread, supervised from this (transport)
        // thread (DAEMON-CANCEL-3, reusing CANCEL-2's `run_interruptible`). Trust's
        // heavy work is two shapes that need two cancel mechanisms, both fired on
        // peer-disconnect: (1) opaque SQL — `compute_module_stats` + the up-to-100k
        // `query_unresolved_edges` — has no Rust frame to poll, so `on_disconnect` fires
        // `sqlite3_interrupt` to abort the in-flight `SELECT`; (2) the pure
        // unresolved-sample loop polls the cooperative `CancelFlag` the supervisor
        // latches. A disconnected peer's in-flight trust therefore abandons its work
        // instead of running to completion with no consumer.
        let trust_start = Instant::now();
        let mut report = {
            use repo_graph_trust::service::{
                assemble_trust_report_cancellable, TrustReportOutcome,
            };
            // Hoist the interrupt handle BEFORE moving the connection into the worker
            // (B1 D-S = S-A: this is the leaf's OWN per-op connection, so the handle is
            // from the exact connection the worker blocks inside; firing it after the
            // worker drops the connection is a safe no-op).
            let interrupt = storage.interrupt_handle();
            let repo_uid_w = repo_uid.clone();
            let snap_uid_w = snapshot.snapshot_uid.clone();
            let basis_w = snapshot.basis_commit.clone();
            let toolchain_w = snapshot.toolchain_json.clone();
            match crate::cancel::run_interruptible(
                emitter,
                "assembling_trust_report",
                move || interrupt.interrupt(),
                move |flag| {
                    let mut checkpoint = flag.checkpoint();
                    assemble_trust_report_cancellable(
                        &storage,
                        &repo_uid_w,
                        &snap_uid_w,
                        basis_w.as_deref(),
                        toolchain_w.as_deref(),
                        &mut checkpoint,
                    )
                    .map_err(|e| e.to_string())
                },
            ) {
                crate::cancel::Supervised::Completed(Ok(TrustReportOutcome::Ready(r))) => *r,
                // The cooperative checkpoint broke (peer gone). The supervisor returns
                // Cancelled before the worker can reach here, but classify honestly.
                crate::cancel::Supervised::Completed(Ok(TrustReportOutcome::Cancelled)) => {
                    return DispatchResult::error(
                        &request.id,
                        ErrorDetail::new(
                            ErrorCode::Cancelled,
                            "trust query cancelled (client disconnected during computation)",
                        ),
                    );
                }
                // A genuine storage/JSON failure while the peer stayed connected.
                crate::cancel::Supervised::Completed(Err(msg)) => {
                    return DispatchResult::error(
                        &request.id,
                        ErrorDetail::new(ErrorCode::InternalError, msg),
                    );
                }
                crate::cancel::Supervised::Cancelled => {
                    return DispatchResult::error(
                        &request.id,
                        ErrorDetail::new(
                            ErrorCode::Cancelled,
                            "trust query cancelled (client disconnected during computation)",
                        ),
                    );
                }
                // WorkerVanished ≠ Cancelled (CANCEL-1 deliverable #2): an internal
                // failure (worker panic), NEVER masqueraded as a client disconnect.
                crate::cancel::Supervised::WorkerVanished => {
                    return DispatchResult::error(
                        &request.id,
                        ErrorDetail::new(
                            ErrorCode::InternalError,
                            "trust worker vanished (internal failure during assembly)",
                        ),
                    );
                }
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

        // TRUST-LIVEGRAPH-IMPL: assemble the `CoherenceEnvelope<CoherentTrustReport>` response (the ratified
        // hybrid), mirroring handle_orient/handle_check/handle_explain. The adapter adds the Half-A
        // current-state posture leaf — GENUINELY SERVED from the LiveGraph (residency / per-partition
        // freshness / language / producer / migrated-answer capability, projected from `live_partitions()` +
        // the repo-wide `module_stats()` answer) — BESIDE the RETAINED v1 report (Half B, source=sqlite,
        // payloads byte-identical, LABELLED outgoing-extractor), folds the honest MEET freshness/provenance,
        // and labels the multi-source downgrade leaf `{sqlite, declaration}`. The v1 computation above is
        // UNCHANGED; the wrapper adds labels + the posture, it never re-judges (F5: no axis is presented as
        // current-state unless its leaf is source=livegraph).
        let mut envelope =
            crate::trust_coherence::build_trust_envelope(&repo_state, &epoch, report);
        // COHERENCE-POLISH-1 §2: attach the ceiling capability fact AFTER the pure fold (a capability
        // posture never downgrades the v1 report's freshness), exactly like `witnesses` /
        // `layer2_resolution`. `to_value` over a plain 3-variant enum of String/Vec<String> is
        // infallible — the `expect` documents an unreachable serialization error, not a swallowed read.
        envelope.value.call_graph_ceiling = Some(
            serde_json::to_value(&call_graph_ceiling)
                .expect("CeilingReport (plain enum) always serializes"),
        );
        match serde_json::to_value(&envelope) {
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
        let storage = match self.open_storage(&repo_state) {
            Ok(s) => s,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        // Get latest snapshot
        let snapshot = match storage.get_latest_snapshot(&repo_uid) {
            Ok(Some(snap)) => snap,
            Ok(None) => {
                return DispatchResult::error(
                    &request.id,
                    Self::no_ready_snapshot_detail(&storage, repo_state.db_path(), &repo_uid),
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
            &storage,
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
        let repo_name = storage
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
            // GOV-ARMED-1: additive configuration-presence fact. Frozen exit
            // codes and existing fields are untouched.
            "armed": report.armed,
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
        let storage = match self.open_storage(&repo_state) {
            Ok(s) => s,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        // Get repo to find root_path
        let repo = match storage.get_repo(&RepoRef::Uid(repo_uid.clone())) {
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
        let mut inventory = match repo_graph_doc_facts::discover_doc_inventory(&repo_path, true) {
            Ok(r) => r,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, format!("discovery failed: {}", e)),
                );
            }
        };

        // DOCS-LIST-2 §2: overlay the `vendored` kind using the index's EXISTING vendor
        // classification fact (`is_vendored_path` / `VENDORED_SEGMENTS`, quality/support.rs) — the
        // same predicate `hotspots --exclude-vendored` reads, never a second definition. This is a
        // DAEMON-layer overlay (not in `doc-facts`) because the vendor fact lives in daemon-runtime
        // and the dependency rule forbids `doc-facts`, a lower crate, from reaching up to it.
        // Vendored takes PRECEDENCE over every content/location kind: a vendored release-note or
        // license is still demoted as vendored (the reader's docs are what the headline is for), and
        // the overlay CLEARS `release_family` on the overridden entry so the DTO invariant holds
        // (review-3 finding 2). Extracted to `docs_list_overlay` for a unit-test seam.
        // `counts_by_kind` / `generated_count` are recomputed AFTER the overlay so the daemon's
        // complete JSON stays internally consistent; the human-render demotion is presentation's job.
        crate::docs_list_overlay::overlay_vendored_kind(&mut inventory.entries);
        let mut counts_by_kind: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for entry in &inventory.entries {
            *counts_by_kind.entry(entry.kind.clone()).or_insert(0) += 1;
        }
        let generated_count = inventory.entries.iter().filter(|e| e.generated).count();

        let mut payload = serde_json::json!({
            "command": "docs list",
            "repo": repo_uid,
            "repo_path": repo.root_path,
            "entries": inventory.entries,
            "count": inventory.entries.len(),
            "counts_by_kind": counts_by_kind,
            "generated_count": generated_count
        });
        // SELF-POLLUTION-1 / operator RULING 3: surface sidecar-named files we could
        // not read to check the marker ("unreadable" — UNKNOWN, admitted but not
        // asserted authored). Emitted ONLY when > 0 so the common case stays
        // byte-identical to the pre-slice payload (no `unreadable` key), preserving
        // `docs list --json` parity on repos with nothing unreadable.
        if inventory.unreadable_count > 0 {
            payload["unreadable"] = serde_json::json!(inventory.unreadable_count);
        }
        DispatchResult::success(&request.id, payload)
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
        let storage = match self.open_storage(&repo_state) {
            Ok(s) => s,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        // Get repo to find root_path
        let repo = match storage.get_repo(&RepoRef::Uid(repo_uid.clone())) {
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

        // Open fresh storage connection for write (under coordination). NO-CREATE
        // (FORGET-REPO-1): this replaces semantic facts on an EXISTING, already-indexed repo; it
        // must never create the DB — a stale request after a forget would otherwise resurrect the
        // removed DB as an unregistered orphan (the SPLIT choke wraps the same NO-CREATE
        // `open_existing`).
        // FOREGROUND-LOCK-1 (§2.2/§2.3): route this second foreground open through the bounded-
        // patience choke — a transient lock is the honest `Busy` transient (never `InternalError`),
        // while a genuine non-lock fault keeps this handler's pre-existing message verbatim.
        let mut storage = match self.open_storage_split(&repo_state) {
            Ok(s) => s,
            Err(crate::foreground_open::ForegroundOpenFault::Busy(detail)) => {
                return DispatchResult::error(&request.id, detail);
            }
            Err(crate::foreground_open::ForegroundOpenFault::Other(e)) => {
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
        let storage = match self.open_storage(&repo_state) {
            Ok(s) => s,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        // Get latest snapshot
        let snapshot = match storage.get_latest_snapshot(&repo_uid) {
            Ok(Some(snap)) => snap,
            Ok(None) => {
                return DispatchResult::error(
                    &request.id,
                    Self::no_ready_snapshot_detail(&storage, repo_state.db_path(), &repo_uid),
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
        let resources = match storage.list_resources(&snapshot.snapshot_uid, kind_filter) {
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

        // RESOURCE-HONESTY-1: the detector-coverage statement. `detected_languages` is the
        // build-static list of languages this build's resource-access detection covers, from the ONE
        // detector registry (never a hardcoded list — a registry change propagates here untouched).
        // `material_gap` names THIS repo's materially-present languages that have NO detector, so the
        // zero-state stops blaming the codebase and a lone result stops posing as an inventory. The
        // per-language read is fallible and CLASSIFIED (it decides which languages are named) → a
        // failed read renders `unknown`-with-reason, NEVER a silent empty (STANDING HONESTY RULE 1).
        let detected_languages =
            repo_graph_repo_index::resource_coverage::resource_detector_language_names();
        // RESOURCE-CPP-INERT-1 (§2.3): the per-language MECHANISM (specific access calls), so the
        // coverage line describes what the detector DOES, not the language it parses. Additive — an
        // older rgr ignores it and falls back to `detected_languages`; a current rgr renders it.
        let detected_mechanisms: Vec<serde_json::Value> =
            repo_graph_repo_index::resource_coverage::resource_detector_mechanisms()
                .into_iter()
                .map(|(language, mechanism)| {
                    serde_json::json!({ "language": language, "mechanism": mechanism })
                })
                .collect();
        let material_gap = match repo_graph_agent::AgentStorageRead::query_file_count_by_language(
            &storage,
            &snapshot.snapshot_uid,
        ) {
            Ok(counts) => serde_json::json!({
                "status": "known",
                "uncovered_languages": resource_uncovered_material_languages(&counts, |t| {
                    repo_graph_repo_index::resource_coverage::resource_detection_covers(t)
                }),
            }),
            Err(e) => serde_json::json!({
                "status": "unknown",
                "reason": e.to_string(),
            }),
        };

        let mut response = serde_json::json!({
            "command": "resource list",
            "repo": repo_uid,
            "snapshot": snapshot.snapshot_uid,
            "results": resources,
            "count": count,
            "total_reads": total_reads,
            "total_writes": total_writes,
            "coverage": {
                "detected_languages": detected_languages,
                "detected_mechanisms": detected_mechanisms,
                "material_gap": material_gap,
            },
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
        let storage = match self.open_storage(&repo_state) {
            Ok(s) => s,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        // Get latest snapshot
        let snapshot = match storage.get_latest_snapshot(&repo_uid) {
            Ok(Some(snap)) => snap,
            Ok(None) => {
                return DispatchResult::error(
                    &request.id,
                    Self::no_ready_snapshot_detail(&storage, repo_state.db_path(), &repo_uid),
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
        let target = match storage.resolve_resource(&snapshot.snapshot_uid, resource_key) {
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
        let readers =
            match storage.find_resource_readers(&snapshot.snapshot_uid, &target.stable_key) {
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
        let storage = match self.open_storage(&repo_state) {
            Ok(s) => s,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        // Get latest snapshot
        let snapshot = match storage.get_latest_snapshot(&repo_uid) {
            Ok(Some(snap)) => snap,
            Ok(None) => {
                return DispatchResult::error(
                    &request.id,
                    Self::no_ready_snapshot_detail(&storage, repo_state.db_path(), &repo_uid),
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
        let target = match storage.resolve_resource(&snapshot.snapshot_uid, resource_key) {
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
        let writers =
            match storage.find_resource_writers(&snapshot.snapshot_uid, &target.stable_key) {
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
        let storage = match self.open_storage(&repo_state) {
            Ok(s) => s,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        // Get latest snapshot
        let snapshot = match storage.get_latest_snapshot(&repo_uid) {
            Ok(Some(snap)) => snap,
            Ok(None) => {
                return DispatchResult::error(
                    &request.id,
                    Self::no_ready_snapshot_detail(&storage, repo_state.db_path(), &repo_uid),
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
        let schemas = match storage.list_contract_schemas(&snapshot.snapshot_uid, kind_filter) {
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
        let storage = match self.open_storage(&repo_state) {
            Ok(s) => s,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        // Get latest snapshot
        let snapshot = match storage.get_latest_snapshot(&repo_uid) {
            Ok(Some(snap)) => snap,
            Ok(None) => {
                return DispatchResult::error(
                    &request.id,
                    Self::no_ready_snapshot_detail(&storage, repo_state.db_path(), &repo_uid),
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
        let schema = match storage.get_schema_by_file(&snapshot.snapshot_uid, file_path) {
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
        let elements = match storage.list_elements_for_schema(&schema.schema_uid, None) {
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
        let storage = match self.open_storage(&repo_state) {
            Ok(s) => s,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        // Get latest snapshot
        let snapshot = match storage.get_latest_snapshot(&repo_uid) {
            Ok(Some(snap)) => snap,
            Ok(None) => {
                return DispatchResult::error(
                    &request.id,
                    Self::no_ready_snapshot_detail(&storage, repo_state.db_path(), &repo_uid),
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
            Some(path) => match storage.get_schema_by_file(&snapshot.snapshot_uid, path) {
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
            None => match storage.list_contract_schemas(&snapshot.snapshot_uid, None) {
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
            let elements = match storage.list_elements_for_schema(&schema.schema_uid, kind_filter) {
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
        let storage = match self.open_storage(&repo_state) {
            Ok(s) => s,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        // Get latest snapshot
        let snapshot = match storage.get_latest_snapshot(&repo_uid) {
            Ok(Some(snap)) => snap,
            Ok(None) => {
                return DispatchResult::error(
                    &request.id,
                    Self::no_ready_snapshot_detail(&storage, repo_state.db_path(), &repo_uid),
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
        let mappings =
            match storage.list_generated_code_mappings(&snapshot.snapshot_uid, element_filter) {
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

        // INFERENCES-SURFACE-1: optional `--limit N` — caps only the RECORDS in this
        // payload (see `inferences_serve` for the truncation contract).
        let limit = request.params.get("limit").and_then(|v| v.as_u64());

        // Acquire read lock
        let _read_guard = repo_state.coordinator.acquire_read();
        let storage = match self.open_storage(&repo_state) {
            Ok(s) => s,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        // Get latest snapshot
        let snapshot = match storage.get_latest_snapshot(&repo_uid) {
            Ok(Some(snap)) => snap,
            Ok(None) => {
                return DispatchResult::error(
                    &request.id,
                    Self::no_ready_snapshot_detail(&storage, repo_state.db_path(), &repo_uid),
                );
            }
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                );
            }
        };

        // Load ALL inferences UNFILTERED (`None`, not `kind_filter`): the read is
        // faithful (no LIMIT) and the detector inventory must be built from the true
        // per-kind totals — a `--kind` filter changes what is SHOWN (applied in
        // `build_response`), never what a detector produced (operator ruling §2).
        let inferences = match storage.list_inferences_for_snapshot(&snapshot.snapshot_uid, None) {
            Ok(i) => i,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                );
            }
        };

        // Snapshot language mix — drives detector-applicability + empty-state honesty.
        // RENDERED/CLASSIFIED, so a read failure is surfaced, never treated as "no
        // languages". Lowercased to match the catalog's lowercase language labels.
        let languages: std::collections::BTreeSet<String> =
            match storage.distinct_file_languages_for_snapshot(&snapshot.snapshot_uid) {
                Ok(v) => v.into_iter().map(|l| l.to_lowercase()).collect(),
                Err(e) => {
                    return DispatchResult::error(
                        &request.id,
                        ErrorDetail::new(
                            ErrorCode::InternalError,
                            format!("read snapshot languages: {e}"),
                        ),
                    );
                }
            };

        // Assemble the additive, self-declaring response in the crate-private module.
        let response = crate::inferences_serve::build_response(
            &repo_uid,
            &snapshot.snapshot_uid,
            inferences,
            &languages,
            kind_filter,
            limit,
        );

        DispatchResult::success(&request.id, response)
    }

    // ── Dependency handlers ──────────────────────────────────────────

    /// List package dependencies for a repo (REG-1 pattern).
    ///
    /// Request: `{"method": "deps_list", "params": {"repo": "<path_or_alias>", "module": "<optional>", "ecosystem": "<optional: npm|cargo>"}}`
    fn handle_deps_list(&self, request: &Request) -> DispatchResult {
        use repo_graph_module_queries::{
            compose_dependency_summaries, deps_runtime_builtins, ComposeDependenciesInput,
        };

        // REG-1: resolve repo from path/alias and auto-load
        let (repo_state, repo_uid) = match self.resolve_and_load_repo(&request.params) {
            Ok(r) => r,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        // Parse optional filters
        let module_filter = Self::get_optional_string_param(&request.params, "module");
        // HONEST-DEGRADATION-IMPL-2 (D2): an explicit caller override (npm|cargo) is honored; absent it,
        // the ecosystem is DERIVED from the repo's extracted languages below (NOT the old hardcoded "npm"
        // default), so a C/Java/Python repo is never falsely labelled an evaluated npm graph.
        let ecosystem_param =
            Self::get_optional_string_param(&request.params, "ecosystem").map(|s| s.to_string());

        // Acquire read lock
        let _read_guard = repo_state.coordinator.acquire_read();
        let storage = match self.open_storage(&repo_state) {
            Ok(s) => s,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        // Get latest snapshot
        let snapshot = match storage.get_latest_snapshot(&repo_uid) {
            Ok(Some(snap)) => snap,
            Ok(None) => {
                return DispatchResult::error(
                    &request.id,
                    Self::no_ready_snapshot_detail(&storage, repo_state.db_path(), &repo_uid),
                );
            }
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                );
            }
        };

        // DEPS-LIST-REWRITE-1 (§2.2): ecosystem is selected by the DOMINANT indexed language
        // (files-table plurality), not "any TS/JS file present". Per the STANDING HONESTY RULE the
        // fallible language-count read that DRIVES classification is surfaced as an error, never
        // silently defaulted to a wrong ecosystem.
        // Read via the `AgentStorageRead` port (operator ruling 2) — not a new public inherent
        // storage API. A read failure is surfaced, never silently defaulted to a wrong ecosystem.
        let language_counts = match repo_graph_agent::AgentStorageRead::query_file_count_by_language(
            &storage,
            &snapshot.snapshot_uid,
        ) {
            Ok(c) => c,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                );
            }
        };
        let repo_languages: Vec<String> = language_counts.iter().map(|(l, _)| l.clone()).collect();
        // Whether the caller pinned an ecosystem. The §2.4 secondary-ecosystem truth (below) is a
        // property of the DEFAULT whole-repo view only — an explicit `--ecosystem` is a targeted view.
        let ecosystem_explicit = ecosystem_param.is_some();
        let ecosystem = match ecosystem_param {
            Some(e) => e,
            None => dominant_deps_ecosystem(&language_counts).to_string(),
        };

        // Runtime-builtin vocabulary for the ecosystem (§2.1): globals/stdlib classify as builtins,
        // never packages. `none-detected` / unknown → empty set (shape-only rejection).
        let runtime_builtins = deps_runtime_builtins(&ecosystem);

        // §2.2 manifest provenance (operator rulings 2026-08-26 + ruling 3 item 2): the
        // parsed-manifest records persisted at index time, as a quad-state that keeps the "no exact
        // file" causes distinct (predates tracking vs. read failure vs. corruption) — never one
        // collapsed label and never a fabricated path.
        let manifest_provenance =
            crate::deps_headline::read_manifest_provenance(&storage, &snapshot.snapshot_uid);
        // Workspace-coverage denominator (ruling 3 item 4; review-3): how many manifests of this
        // ecosystem are PRESENT (scanned) — read from the SEPARATE `deps_manifests_present` key, NOT
        // the parsed record, so unparsed workspace manifests count toward the denominator without
        // being laundered into the parsed provenance. Returns a `PresentDenominator` sum type, NOT
        // `Option<usize>`: a FAILURE of this (independent) read surfaces as `Unavailable`-with-reason
        // and the coverage line renders unknown — it is NEVER collapsed to `None` and fabricated over
        // by `build_deps_list_response` (review-3 / operator ruling 2026-08-31; STANDING HONESTY RULE #1).
        let manifests_present = crate::deps_headline::read_manifests_present(
            &storage,
            &snapshot.snapshot_uid,
            &ecosystem,
        );

        // DEPS-ATTRIB-2 §2.3: the HONEST "govern no indexed source" split — computed per PARSED
        // manifest from file containment (file paths ⋈ manifest dir), NOT from module attribution.
        // Two INDEPENDENT reads feed the split, BOTH fallible AND rendered/classified, so EITHER
        // failure is surfaced as unknown-with-reason (`CoverageStatus::Unknown`) — NEVER `None`/silent
        // omission (operator binding 2026-08-31, STANDING HONESTY RULE #1):
        //   - `get_owned_files_for_rollup` = the `module_file_ownership`⋈`files` read → the ATTRIBUTED
        //     (module-owned) file universe.
        //   - `map_files_in_path(snapshot, "")` = every indexed file in the snapshot → the INDEXED
        //     SOURCE universe (review-4 blocker 2: an indexed source file with no matching module
        //     prefix stays UNOWNED, so owned files alone would misclassify indexed-but-unowned source
        //     as "no indexed source" — a false §2.3 excuse). Filtered to CODE files via the SAME
        //     `language_display_name` code-file predicate the materiality gate uses (config tokens
        //     like `json` for a `package.json` are not source), never a second definition.
        // A provenance-blob failure is caught inside `compute_manifest_coverage`
        // (`ProvenanceRead::Unavailable` → `Unknown`). A `None` return stays the genuine
        // not-applicable case (old snapshot / no manifests of this ecosystem).
        let manifest_coverage: Option<crate::deps_coverage::CoverageStatus> = {
            let owned = storage.get_owned_files_for_rollup(&snapshot.snapshot_uid);
            let indexed = storage.map_files_in_path(&snapshot.snapshot_uid, "");
            match (owned, indexed) {
                (Ok(owned_files), Ok(indexed_rows)) => {
                    let owned_paths: Vec<String> =
                        owned_files.into_iter().map(|f| f.file_path).collect();
                    let indexed_paths: Vec<String> = indexed_rows
                        .into_iter()
                        .filter(|r| match r.language.as_deref() {
                            Some(tok) => {
                                crate::reader_context::language_display_name(tok).is_some()
                            }
                            None => false, // no language token → not indexed SOURCE
                        })
                        .map(|r| r.path)
                        .collect();
                    crate::deps_coverage::compute_manifest_coverage(
                        &manifest_provenance,
                        &ecosystem,
                        &owned_paths,
                        &indexed_paths,
                    )
                }
                (Err(e), _) => Some(crate::deps_coverage::CoverageStatus::Unknown {
                    reason: format!("could not read owned files: {e}"),
                }),
                (_, Err(e)) => Some(crate::deps_coverage::CoverageStatus::Unknown {
                    reason: format!("could not read indexed files: {e}"),
                }),
            }
        };

        // DEPS-ATTRIB-2 §2.4 (ruling DR-JAVA-NOREADER = Option 2): the DEFAULT view states the truth
        // of every materially-present, reader-bearing ecosystem OTHER than the rendered dominant one,
        // so a materially-present ecosystem (glamCRM's Java half) is never SILENTLY absent. Each
        // secondary ecosystem's real attribution comes from ITS OWN compose (consistent with
        // `--ecosystem <e>`). Empty for a single-ecosystem repo, or when the caller pinned an
        // ecosystem / a module (targeted views, not the default). The provenance is cloned (cheap
        // Vec-of-strings DTO) so the primary `input` still moves the original below.
        let other_ecosystems: Vec<crate::deps_ecosystem_presence::EcosystemPresence> =
            if module_filter.is_none() && !ecosystem_explicit {
                crate::deps_ecosystem_presence::secondary_material_ecosystems(
                    &language_counts,
                    &ecosystem,
                )
                .into_iter()
                .map(|(eco, source_files)| {
                    let compose_outcome = compose_dependency_summaries(
                        &storage,
                        &ComposeDependenciesInput {
                            snapshot_uid: &snapshot.snapshot_uid,
                            runtime_builtins: deps_runtime_builtins(&eco),
                            ecosystem: eco.clone(),
                            manifest_provenance: manifest_provenance.clone(),
                        },
                    )
                    .map_err(|e| e.to_string());
                    let state = crate::deps_ecosystem_presence::classify_ecosystem_presence(
                        &eco,
                        source_files,
                        &manifest_provenance,
                        compose_outcome.as_ref().map_err(|e| e.clone()),
                    );
                    crate::deps_ecosystem_presence::EcosystemPresence {
                        ecosystem: eco,
                        state,
                    }
                })
                .collect()
            } else {
                Vec::new()
            };

        let input = ComposeDependenciesInput {
            snapshot_uid: &snapshot.snapshot_uid,
            runtime_builtins,
            ecosystem: ecosystem.clone(),
            manifest_provenance,
        };

        let result = match compose_dependency_summaries(&storage, &input) {
            Ok(r) => r,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                );
            }
        };

        // §2.3 unattributed headline — pure helper over the WHOLE repo (before any module filter),
        // so a per-module view never hides repo-level unattributed imports (glamCRM's false `0`).
        let unattributed =
            crate::deps_headline::compute_unattributed(&result, &ecosystem, &repo_languages);
        let total_rejected = crate::deps_headline::total_rejected(&result);

        // §2.4 resolution-state honesty (operator ruling 2, 2026-08-26 — Option B): reuse the SAME
        // query-time trust overlay `orient`/`trust` already assemble
        // (`compute_trust_overlay_for_snapshot`) — one shared function, two callers, no persistence,
        // no new machinery, computed once here. `alias_resolution_suspicion` is the downgrade the
        // audit named for FRAKTAG's `@fraktag/engine` (a workspace/alias import restated as
        // certainty); there is no separate persisted "workspace-package-as-library" flag, this one
        // covers that case. When active, a `declared_but_unobserved` row may be an UNRESOLVED import
        // rather than a truly-unused dep, so it renders "declared — imports not resolved on this
        // index" with capped confidence (below), never restated 1.0 certainty. Basis "IMPORTS" — deps
        // is an import-graph surface (matches the trust-overlay basis the import surfaces use).
        // Ruling 3 item 1 / review-5 item 3: the trust-overlay read is fallible; a failure is
        // UNKNOWN-with-the-ACTUAL-reason, NOT `unwrap_or(false)` (silent certainty — the audit's
        // false-1.0 case) and NOT a generic "could not be assembled" string that hides the cause.
        // `try_trust_overlay_for_snapshot` is the same single assembly path as
        // `compute_trust_overlay_for_snapshot` (which delegates to it) — it just preserves the
        // underlying error instead of collapsing it to `None`.
        let resolution = match crate::util::trust::try_trust_overlay_for_snapshot(
            &storage, &repo_uid, &snapshot, "IMPORTS",
        ) {
            Ok(o) => {
                if o.degradation_flags
                    .iter()
                    .any(|f| f == "alias_resolution_suspicion")
                {
                    crate::deps_headline::ResolutionState::Downgraded
                } else {
                    crate::deps_headline::ResolutionState::Clean
                }
            }
            Err(e) => crate::deps_headline::ResolutionState::Unknown {
                reason: format!("overlay read failed: {e}"),
            },
        };

        // §2.2/§2.3/§2.4/§2.5: assemble the full JSON payload (module filter, per-entry downgrade
        // labels + capped confidence, exact `manifest_path` or unknown-with-reason, headline-first
        // envelope, reader-context note). Extracted to the crate-private `deps_headline` module so
        // this dispatch arm stays wiring, not a 140-line JSON builder (guardrail: dispatch.rs is not
        // grown by this slice).
        let response = crate::deps_headline::build_deps_list_response(
            &repo_uid,
            &snapshot.snapshot_uid,
            &ecosystem,
            &repo_languages,
            result,
            module_filter,
            &resolution,
            &unattributed,
            total_rejected,
            &manifests_present,
            manifest_coverage,
            &other_ecosystems,
        );

        DispatchResult::success(&request.id, response)
    }

    /// Explain why a package is used (REG-1 pattern).
    ///
    /// Request: `{"method": "deps_why", "params": {"repo": "<path_or_alias>", "package": "<name>", "ecosystem": "<optional: npm|cargo>"}}`
    fn handle_deps_why(&self, request: &Request) -> DispatchResult {
        use repo_graph_module_queries::{
            build_identifier_resolution_map, compose_dependency_summaries, deps_runtime_builtins,
            normalize_cargo_specifier, normalize_npm_specifier, resolve_import_specifier,
            ComposeDependenciesInput, DependencyCategory,
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
        let storage = match self.open_storage(&repo_state) {
            Ok(s) => s,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        // Get latest snapshot
        let snapshot = match storage.get_latest_snapshot(&repo_uid) {
            Ok(Some(snap)) => snap,
            Ok(None) => {
                return DispatchResult::error(
                    &request.id,
                    Self::no_ready_snapshot_detail(&storage, repo_state.db_path(), &repo_uid),
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
                DependencyCategory::FirstPartySelf => "first_party_self",
                DependencyCategory::RuntimeBuiltin => "runtime_builtin",
                DependencyCategory::UnknownExternalLike => "unknown_external_like",
            }
        }

        // Load module_candidates for file → module mapping
        let modules = match storage.get_module_candidates_for_snapshot(&snapshot.snapshot_uid) {
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
        let ownership = match storage.get_file_ownership_for_snapshot(&snapshot.snapshot_uid) {
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
        let imports_with_locations =
            match storage.get_external_imports_with_locations(&snapshot.snapshot_uid) {
                Ok(i) => i,
                Err(e) => {
                    return DispatchResult::error(
                        &request.id,
                        ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                    );
                }
            };

        // Load import bindings for identifier → specifier resolution
        let import_bindings =
            match storage.get_external_import_bindings_for_snapshot(&snapshot.snapshot_uid) {
                Ok(b) => b,
                Err(e) => {
                    return DispatchResult::error(
                        &request.id,
                        ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                    );
                }
            };
        let identifier_to_specifier = build_identifier_resolution_map(&import_bindings);

        // Per-ecosystem builtin set (all four ecosystems — a bare cargo/npm match
        // here previously classified python/java against npm builtins).
        let runtime_builtins = deps_runtime_builtins(&ecosystem);
        let compose_input = ComposeDependenciesInput {
            snapshot_uid: &snapshot.snapshot_uid,
            runtime_builtins,
            ecosystem: ecosystem.clone(),
            // `deps why` reports declared/observed status, not the manifest file; still read the
            // real provenance (never a fabricated `Absent`) so the shared assembly stays consistent.
            manifest_provenance: crate::deps_headline::read_manifest_provenance(
                &storage,
                &snapshot.snapshot_uid,
            ),
        };
        let reconciled = match compose_dependency_summaries(&storage, &compose_input) {
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
            compose_dependency_summaries, deps_runtime_builtins, ComposeDependenciesInput,
            DependencyCategory,
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
        let storage = match self.open_storage(&repo_state) {
            Ok(s) => s,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        // Get latest snapshot
        let snapshot = match storage.get_latest_snapshot(&repo_uid) {
            Ok(Some(snap)) => snap,
            Ok(None) => {
                return DispatchResult::error(
                    &request.id,
                    Self::no_ready_snapshot_detail(&storage, repo_state.db_path(), &repo_uid),
                );
            }
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                );
            }
        };

        // Per-ecosystem builtin set (all four ecosystems; see the sibling arm).
        let runtime_builtins = deps_runtime_builtins(&ecosystem);

        let input = ComposeDependenciesInput {
            snapshot_uid: &snapshot.snapshot_uid,
            runtime_builtins,
            ecosystem: ecosystem.clone(),
            // `deps drift` reports usage anomalies, not manifest files; still read the real
            // provenance (never a fabricated `Absent`) so the shared assembly stays consistent.
            manifest_provenance: crate::deps_headline::read_manifest_provenance(
                &storage,
                &snapshot.snapshot_uid,
            ),
        };

        let result = match compose_dependency_summaries(&storage, &input) {
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
        let storage = match self.open_storage(&repo_state) {
            Ok(s) => s,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        // Get latest snapshot
        let snapshot = match storage.get_latest_snapshot(&repo_uid) {
            Ok(Some(snap)) => snap,
            Ok(None) => {
                return DispatchResult::error(
                    &request.id,
                    Self::no_ready_snapshot_detail(&storage, repo_state.db_path(), &repo_uid),
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
        let surfaces =
            match storage.get_project_surfaces_for_snapshot(&snapshot.snapshot_uid, &filter) {
                Ok(s) => s,
                Err(e) => {
                    return DispatchResult::error(
                        &request.id,
                        ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                    );
                }
            };

        // Load module candidates for enrichment
        let modules = match storage.get_module_candidates_for_snapshot(&snapshot.snapshot_uid) {
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
        let evidence_counts = match storage.count_evidence_by_surface(&snapshot.snapshot_uid) {
            Ok(c) => c,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                );
            }
        };

        // Build results. §2.3: HTTP-kind project surfaces are lifted OUT of the
        // catalog — the read-time union renders them ONCE in the richer HTTP/REST
        // section below (see `unified_http_surfaces_json`).
        let results: Vec<serde_json::Value> = surfaces
            .into_iter()
            .filter(|s| !matches!(s.surface_kind.as_str(), "http_provider" | "http_consumer"))
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

        // §2.3 / Option B: the UNIFIED HTTP surfaces (boundary family ⋈ legacy
        // `project_surfaces` HTTP family, deduped) feed this ONE section; the
        // renderer counts off these exact rows (headline == footer == rows). A
        // failed read is UNKNOWN, never an empty map (review-4 item 2). See
        // `unified_http_surfaces_json` (off this 8.9k-line file).
        let (http_boundary_surfaces, http_boundary_surfaces_degraded) =
            match crate::http_boundary_read::unified_http_surfaces_json(
                &storage,
                &repo_uid,
                &snapshot.snapshot_uid,
            ) {
                Ok((list, _providers, _consumers)) => (list, None),
                Err(reason) => (Vec::new(), Some(reason)),
            };

        // ZEROSTATE-SCOPE-1 §2.1: the HTTP surface-detector coverage statement (additive).
        // The shipped detector families are build-static; the "no detector for X" clause is
        // now PER-REPO (`surface_coverage_read`) — it names only THIS repo's materially-
        // present languages/frameworks the HTTP surface detectors cannot see, so leveldb
        // says its C/C++ truth and django keeps URLconf. No repo wears another's sentence.
        // The presenter renders it only in the empty case. A failed per-language read
        // becomes an unknown-with-reason gap arm, never a silent full-coverage claim.
        let surface_coverage =
            crate::surface_coverage_read::surface_coverage_json(&storage, &snapshot.snapshot_uid);

        let mut response = serde_json::json!({
            "command": "surfaces list",
            "repo": repo_uid,
            "snapshot": snapshot.snapshot_uid,
            "results": results,
            "count": count,
            "http_boundary_surfaces": http_boundary_surfaces,
            "surface_coverage": surface_coverage,
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

            // HTTP-BOUNDARY-1 (review-4 item 2): a failed HTTP-surface read is
            // surfaced as a labelled degradation, never a silent empty map.
            if let Some(reason) = &http_boundary_surfaces_degraded {
                map.insert(
                    "http_boundary_surfaces_degraded".to_string(),
                    serde_json::json!(reason),
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
        let storage = match self.open_storage(&repo_state) {
            Ok(s) => s,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        // Get latest snapshot
        let snapshot = match storage.get_latest_snapshot(&repo_uid) {
            Ok(Some(snap)) => snap,
            Ok(None) => {
                return DispatchResult::error(
                    &request.id,
                    Self::no_ready_snapshot_detail(&storage, repo_state.db_path(), &repo_uid),
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
        let surface = match storage.get_project_surface_by_ref(&snapshot.snapshot_uid, &surface_ref)
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
        let module = match storage.get_module_by_uid(&surface.module_candidate_uid) {
            Ok(m) => m,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                );
            }
        };

        // Load evidence
        let evidence_rows = match storage.get_project_surface_evidence(&surface.project_surface_uid)
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
        let storage = match self.open_storage(&repo_state) {
            Ok(s) => s,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        // Get latest snapshot
        let snapshot = match storage.get_latest_snapshot(&repo_uid) {
            Ok(Some(snap)) => snap,
            Ok(None) => {
                return DispatchResult::error(
                    &request.id,
                    Self::no_ready_snapshot_detail(&storage, repo_state.db_path(), &repo_uid),
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
        let items = match storage.list_boundary_interactions(&snapshot.snapshot_uid, &filter) {
            Ok(i) => i,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                );
            }
        };

        // §2.3/§2.4: the response (results = non-HTTP boundary rows ⋈ the ONE
        // unified HTTP set the surfaces footer/summary also count, grouped
        // file×direction; filter echo; count) is assembled off this 8.9k-line file
        // in `boundaries_list_read`. A failed read degrades honestly, never a
        // partial/false count.
        let response = match crate::boundaries_list_read::boundaries_list_response_json(
            &repo_uid,
            &snapshot.snapshot_uid,
            &filter,
            items,
            &storage,
        ) {
            Ok(v) => v,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e),
                )
            }
        };

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
        let storage = match self.open_storage(&repo_state) {
            Ok(s) => s,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        // Get latest snapshot for envelope
        let snapshot = match storage.get_latest_snapshot(&repo_uid) {
            Ok(Some(snap)) => snap,
            Ok(None) => {
                return DispatchResult::error(
                    &request.id,
                    Self::no_ready_snapshot_detail(&storage, repo_state.db_path(), &repo_uid),
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
        let detail = match storage.get_boundary_interaction_detail(surface_uid) {
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
        let storage = match self.open_storage(&repo_state) {
            Ok(s) => s,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        // Get latest snapshot
        let snapshot = match storage.get_latest_snapshot(&repo_uid) {
            Ok(Some(snap)) => snap,
            Ok(None) => {
                return DispatchResult::error(
                    &request.id,
                    Self::no_ready_snapshot_detail(&storage, repo_state.db_path(), &repo_uid),
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
        let summary = match storage.get_boundary_interaction_summary(&snapshot.snapshot_uid) {
            Ok(s) => s,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                );
            }
        };

        // §2.3: the summary's COUNT breakdowns are reconciled to the ONE unified
        // HTTP aggregation (same rows the surfaces footer and boundaries list
        // count), off this 8.9k-line file in `boundaries_summary_read`. A failed
        // read degrades honestly, never a partial/contradictory count.
        let response = match crate::boundaries_summary_read::summary_response_json(
            &repo_uid,
            &snapshot.snapshot_uid,
            &storage,
            &summary,
        ) {
            Ok(v) => v,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e),
                )
            }
        };

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
        let storage = match self.open_storage(&repo_state) {
            Ok(s) => s,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        // Get latest snapshot
        let snapshot = match storage.get_latest_snapshot(&repo_uid) {
            Ok(Some(snap)) => snap,
            Ok(None) => {
                return DispatchResult::error(
                    &request.id,
                    Self::no_ready_snapshot_detail(&storage, repo_state.db_path(), &repo_uid),
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
        let items = match storage.list_boundary_interaction_links(&snapshot.snapshot_uid, &filter) {
            Ok(i) => i,
            Err(e) => {
                return DispatchResult::error(
                    &request.id,
                    ErrorDetail::new(ErrorCode::InternalError, e.to_string()),
                );
            }
        };

        let count = items.len();

        // HTTP-BOUNDARY-1 (review-0 item 4): surface the per-consumer UNLINKED
        // reasons (ambiguous / unmatched / dynamic). These are recomputed
        // honestly at read time from the persisted `channel_kind='http'`
        // surfaces — via the SAME pure matcher `run_http_link_detection` applies
        // at index time (now in the boundary-interaction policy crate) — so no
        // extra storage write is needed and no `daemon-runtime -> indexer` edge.
        // A consumer whose route matched >1 provider or none is UNLINKED WITH a
        // reason, never guessed. See the crate-private `http_boundary_read`.
        let http_unlinked =
            crate::http_boundary_read::http_unlinked_json(&storage, &snapshot.snapshot_uid);

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
            // A failed HTTP-surface read is UNKNOWN, never a silent (absent)
            // footer (review-4 item 2): `Err` emits a labelled degradation the
            // renderer prints instead of "0 unlinked".
            match http_unlinked {
                Ok(unlinked) => {
                    map.insert("httpUnlinked".to_string(), unlinked);
                }
                Err(reason) => {
                    map.insert(
                        "httpUnlinkedDegraded".to_string(),
                        serde_json::json!(reason),
                    );
                }
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
        let storage = match self.open_storage(&repo_state) {
            Ok(s) => s,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        // Get latest snapshot
        let snapshot = match storage.get_latest_snapshot(&repo_uid) {
            Ok(Some(snap)) => snap,
            Ok(None) => {
                return DispatchResult::error(
                    &request.id,
                    Self::no_ready_snapshot_detail(&storage, repo_state.db_path(), &repo_uid),
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
        let ctx = match ModuleQueryContext::load(&storage, &snapshot.snapshot_uid) {
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
        let files = match storage.get_files_for_module(
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
        let storage = match self.open_storage(&repo_state) {
            Ok(s) => s,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        // Get latest snapshot
        let snapshot = match storage.get_latest_snapshot(&repo_uid) {
            Ok(Some(snap)) => snap,
            Ok(None) => {
                return DispatchResult::error(
                    &request.id,
                    Self::no_ready_snapshot_detail(&storage, repo_state.db_path(), &repo_uid),
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
        let facts = match load_module_graph_facts(&storage, &snapshot.snapshot_uid) {
            Ok(f) => f,
            Err(e) => {
                // MODULE-OWNERSHIP-DUPLICATE-1: duplicate ownership → labeled
                // degradation, not a bare InternalError.
                return crate::module_degradation::module_facts_error_result(
                    &request.id,
                    "modules deps",
                    e,
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
        let storage = match self.open_storage(&repo_state) {
            Ok(s) => s,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        // Get latest snapshot
        let snapshot = match storage.get_latest_snapshot(&repo_uid) {
            Ok(Some(snap)) => snap,
            Ok(None) => {
                return DispatchResult::error(
                    &request.id,
                    Self::no_ready_snapshot_detail(&storage, repo_state.db_path(), &repo_uid),
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
        let facts = match load_module_graph_facts(&storage, &snapshot.snapshot_uid) {
            Ok(f) => f,
            Err(e) => {
                // MODULE-OWNERSHIP-DUPLICATE-1: duplicate ownership → labeled
                // degradation, not a bare InternalError.
                return crate::module_degradation::module_facts_error_result(
                    &request.id,
                    "modules violations",
                    e,
                );
            }
        };

        // Evaluate violations using preloaded facts (service layer)
        let result = match evaluate_violations_from_facts(&storage, &repo_uid, &facts) {
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

        // GOV-ARMED-1: armed iff the repo has any active boundary declaration.
        // Config-presence fact from the loaded declarations, NOT inferred from
        // `count == 0`.
        let declarations_evaluated = result.declarations_evaluated;
        let armed = declarations_evaluated > 0;

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
            // GOV-ARMED-1: additive configuration-presence facts.
            "armed": armed,
            "declarations_checked": declarations_evaluated,
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
        let storage = match self.open_storage(&repo_state) {
            Ok(s) => s,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        // Get latest snapshot
        let snapshot = match storage.get_latest_snapshot(&repo_uid) {
            Ok(Some(snap)) => snap,
            Ok(None) => {
                return DispatchResult::error(
                    &request.id,
                    Self::no_ready_snapshot_detail(&storage, repo_state.db_path(), &repo_uid),
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
        let file_version_hashes = match storage.query_file_version_hashes(&snapshot.snapshot_uid) {
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
        let ownership = match storage.get_file_ownership_for_snapshot(&snapshot.snapshot_uid) {
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
        let modules = match storage.get_module_candidates_for_snapshot(&snapshot.snapshot_uid) {
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
        let storage = match self.open_storage(&repo_state) {
            Ok(s) => s,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        // Get latest snapshot
        let snapshot = match storage.get_latest_snapshot(&repo_uid) {
            Ok(Some(snap)) => snap,
            Ok(None) => {
                return DispatchResult::error(
                    &request.id,
                    Self::no_ready_snapshot_detail(&storage, repo_state.db_path(), &repo_uid),
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
        let facts = match load_module_graph_facts(&storage, &snapshot.snapshot_uid) {
            Ok(f) => f,
            Err(e) => {
                // MODULE-OWNERSHIP-DUPLICATE-1: duplicate ownership → labeled
                // degradation, not a bare InternalError.
                return crate::module_degradation::module_facts_error_result(
                    &request.id,
                    "modules show",
                    e,
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
        let evidence_output: Vec<serde_json::Value> = storage
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
        let dead_nodes = storage
            .find_dead_nodes(&snapshot.snapshot_uid, &repo_uid, Some("SYMBOL"))
            .unwrap_or_default();

        // Evaluate violations (advisory)
        let (violations_eval, violations_warning) =
            match evaluate_violations_from_facts(&storage, &repo_uid, &facts) {
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

        // RECON-M-R3a (g2u-a, §5.3.3a): the REDUCTION-ONLY unref overlay for THIS module — the
        // flagged symbols (same dead_nodes rows the rollup consumes, attributed by owned file)
        // tested against the ledger's compiler-witnessed incoming set. Ledger absent / not
        // current → `None` → exactly today's rollup (strict generalization); can only REMOVE
        // flags from the reader's view, never add (the pipeline count itself is untouched).
        let unref_reduction_block = {
            let owned: std::collections::BTreeSet<&str> = facts
                .context
                .owned_files
                .iter()
                .filter(|f| f.module_candidate_uid == resolved_module.module_candidate_uid)
                .map(|f| f.file_path.as_str())
                .collect();
            let flagged: Vec<&str> = dead_nodes
                .iter()
                .filter(|d| d.file.as_deref().is_some_and(|f| owned.contains(f)))
                .map(|d| d.stable_key.as_str())
                .collect();
            crate::witness_projection::WitnessProjection::compute(
                &repo_state,
                &snapshot.snapshot_uid,
            )
            .and_then(|p| p.unref_reduction_block(flagged))
        };

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

        let mut rollups_output = serde_json::json!({
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
        // RECON-M-R3a (g2u-a): additive, labeled, reduction-only — beside the pipeline count,
        // never replacing it; absent when nothing measured or the reduction is 0 (R-0).
        if let Some(block) = unref_reduction_block {
            rollups_output["unref_reduction"] = block;
        }

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
            compute_trust_overlay_for_snapshot(&storage, &repo_uid, &snapshot, "IMPORTS")
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
        let storage = match self.open_storage(&repo_state) {
            Ok(s) => s,
            Err(e) => return DispatchResult::error(&request.id, e),
        };

        // Get latest snapshot
        let snapshot = match storage.get_latest_snapshot(&repo_uid) {
            Ok(Some(snap)) => snap,
            Ok(None) => {
                return DispatchResult::error(
                    &request.id,
                    Self::no_ready_snapshot_detail(&storage, repo_state.db_path(), &repo_uid),
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
        let facts = match load_module_graph_facts(&storage, &snapshot.snapshot_uid) {
            Ok(f) => f,
            Err(e) => {
                // MODULE-OWNERSHIP-DUPLICATE-1: duplicate ownership → labeled
                // degradation, not a bare InternalError.
                return crate::module_degradation::module_facts_error_result(
                    &request.id,
                    "modules list",
                    e,
                );
            }
        };

        // Load dead nodes (SYMBOL kind only)
        let dead_nodes = storage
            .find_dead_nodes(&snapshot.snapshot_uid, &repo_uid, Some("SYMBOL"))
            .unwrap_or_default();

        // Evaluate violations (advisory)
        let (violations_eval, violations_warning) =
            match evaluate_violations_from_facts(&storage, &repo_uid, &facts) {
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

        // RECON-M-R3a (g2u-a, §5.3.3a): per-module flagged symbol keys (file→module join over
        // the SAME dead_nodes rows the rollup consumes) for the REDUCTION-ONLY unref overlay;
        // one shared-projection compute for the whole list. Ledger absent → `None` → today's
        // exact rows (strict generalization).
        let witness_projection = crate::witness_projection::WitnessProjection::compute(
            &repo_state,
            &snapshot.snapshot_uid,
        );
        let flagged_by_module: HashMap<String, Vec<String>> = {
            let file_to_module: HashMap<&str, &str> = owned_file_facts
                .iter()
                .map(|f| (f.file_path.as_str(), f.module_uid.as_str()))
                .collect();
            let mut map: HashMap<String, Vec<String>> = HashMap::new();
            for d in &dead_nodes {
                if let Some(module) = d.file.as_deref().and_then(|f| file_to_module.get(f)) {
                    map.entry((*module).to_string())
                        .or_default()
                        .push(d.stable_key.clone());
                }
            }
            map
        };

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
                let mut row = serde_json::json!({
                    "module_uid": m.module_candidate_uid,
                    "module_key": m.module_key,
                    "canonical_root_path": m.canonical_root_path,
                    "module_kind": m.module_kind,
                    "display_name": m.display_name,
                    // MODULES-IDENTITY-2 §2.1: the owning manifest filename, derived from
                    // the module_key source prefix via the SAME shared helper `orient`'s
                    // data path uses (`repo_graph_storage::manifest_for_module_key`) — so
                    // the presenter can disambiguate twin display names (django's two
                    // `Django` modules, both rooted at `.`) without a second derivation.
                    // `None` (→ JSON null) for inferred/directory modules — honest, never
                    // a guessed file.
                    "manifest": repo_graph_storage::manifest_for_module_key(&m.module_key),
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
                });
                // RECON-M-R3a (g2u-a): additive reduction-only overlay beside the pipeline
                // count (absent when unmeasured or zero — R-0).
                let flagged: &[String] = flagged_by_module
                    .get(m.module_candidate_uid.as_str())
                    .map(Vec::as_slice)
                    .unwrap_or(&[]);
                if let Some(block) = witness_projection
                    .as_ref()
                    .and_then(|p| p.unref_reduction_block(flagged.iter().map(String::as_str)))
                {
                    row["unref_reduction"] = block;
                }
                row
            })
            .collect();

        let count = results.len();

        // MODULE-EDGES-1 §2.1: project the SAME module dependency graph the rollups
        // above were computed from (`facts.edges` — the ONE `load_module_graph_facts`
        // read) into an additive `edges` array. The presenter renders the edge list AND
        // derives its count from this SAME array, so the count can never disagree with
        // its own list (the acid test). No new computation, no new fact class — the
        // exact projection `modules deps` already serves. `facts` succeeded above (a
        // failed load returned a labeled degradation before here), so `edges` is
        // authoritative, never a false-zero from a failed read.
        let edges: Vec<serde_json::Value> = facts
            .edges
            .iter()
            .map(|e| {
                // §2.1 row = `source → target (N imports)`; the presenter models exactly
                // these three scalars as REQUIRED (no serde default → a missing field
                // fails the parse with its reason, never a fabricated blank/zero —
                // review-0 item 3). `source_file_count` is not part of the row, so it is
                // deliberately not projected here.
                serde_json::json!({
                    "source": e.source_canonical_path,
                    "target": e.target_canonical_path,
                    "import_count": e.import_count,
                })
            })
            .collect();

        // Compute sanity metrics (Phase 3.1)
        let sanity_metrics = compute_sanity_metrics_for_list(
            &results,
            &owned_file_facts,
            &facts,
            snapshot.files_total as u64,
            &storage,
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

        // HTTP-BOUNDARY-1 (review-0 item 2): count persisted HTTP provider↔consumer
        // links so the presentation does not claim "boundaries may not be
        // meaningful" when modules demonstrably talk over HTTP/REST. Import-graph
        // silence (intra-module imports only) is not the same as boundary
        // silence: cross-subsystem calls travel over HTTP, not imports.
        // A failed link-count read is UNKNOWN, never 0 (review-4 item 2): 0 would
        // restore the "boundaries may not be meaningful" claim off a read error.
        // `Err` → `null` count PLUS a labelled degradation the renderer reads to
        // suppress that claim.
        let (http_boundary_link_count, http_boundary_link_degraded) =
            match crate::http_boundary_read::unified_http_link_count(
                &storage,
                &repo_uid,
                &snapshot.snapshot_uid,
            ) {
                Ok(n) => (Some(n), None::<String>),
                Err(reason) => (None, Some(reason)),
            };

        // Build response
        let mut response = serde_json::json!({
            "command": "modules list",
            "repo": repo_uid,
            "snapshot": snapshot.snapshot_uid,
            "results": results,
            "count": count,
            // MODULE-EDGES-1 §2.1: additive cross-module edge list (see above). Always
            // emitted when `facts` loaded (possibly empty = KNOWN zero cross-module deps);
            // the presenter treats its presence as authoritative and its absence (older
            // daemon) as UNKNOWN, never a false zero.
            "edges": edges,
            "rollups_degraded": !violations_available,
            "sanity_metrics": sanity_metrics,
            "warnings": warnings,
            "http_boundary_link_count": http_boundary_link_count,
        });
        if let (serde_json::Value::Object(ref mut map), Some(reason)) =
            (&mut response, &http_boundary_link_degraded)
        {
            map.insert(
                "http_boundary_link_degraded".to_string(),
                serde_json::json!(reason),
            );
        }

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
        // HTTP-BOUNDARY-1 review-2 item 2: without this arm `--kind http`
        // parsed to None and silently CLEARED the filter (returning all
        // surfaces) instead of restricting to HTTP.
        "http" | "rest" => Some(ChannelKind::Http),
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
        // HTTP-BOUNDARY-1 review-2 item 2: mirror the channel-kind arm so
        // `--family http` restricts to HTTP instead of silently clearing.
        "http" | "rest" => Some(ProtocolFamily::Http),
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

// ─────────────────────────────────────────────────────────────────────────────
// HONEST-DEGRADATION-IMPL-2 (D2 + D5) — configured-resolver source
//
// `docs/slices/honest-degradation-1.md` §12 (RATIFIED). `configured_resolver_languages` is the SINGLE
// source of which enrichment languages have a CONFIGURED resolver on this daemon; `handle_enrich`
// registers from it and the D5 next-action line keys on it — so no surface promises an enrichment remedy
// the enrich path cannot deliver. It lives here beside `handle_enrich`, its primary caller.
//
// The pure reader-context LABEL helpers that CONSUME this value (D2 ecosystem/note, D5 next-action line)
// were extracted to `reader_context.rs` (HONEST-DEGRADATION-IMPL-2-REFACTOR) to stop appending to this
// oversized file (agent_docs/architecture.md — Prohibited Patterns); their behavior is unchanged. They
// are re-exported `pub(crate)` just below so the call sites in this module — and the `crate::dispatch::…`
// references in `orient_coherence` — resolve exactly as before.
// ─────────────────────────────────────────────────────────────────────────────

/// SINGLE source of truth: which enrichment languages have a CONFIGURED resolver on this daemon, given
/// the resolved `jdtls_path` (the `jdtls_path` request param OR the `JDTLS_PATH` env). `handle_enrich`
/// registers EXACTLY these (intersected with any `--languages` filter), and the D5 honest next-action
/// line ([`relationship_next_action_line`]) keys on EXACTLY these — so no surface can promise an
/// enrichment remedy the enrich path cannot deliver (the false-trust mode D5 exists to prevent).
///
/// Rust + TypeScript resolvers are compiled into the binary (unconditional); the Java (JDTLS) resolver
/// is configured only when a `jdtls_path` is present. `Some("")` counts as present, matching
/// `handle_enrich`'s `if let Some(path) = &jdtls_path` (faithfulness to the source over a nicety).
///
/// `pub(crate)` so the extracted `reader_context` unit tests import it as `crate::dispatch::…`
/// (HONEST-DEGRADATION-IMPL-2-REFACTOR — visibility-only widening, no behavior change).
pub(crate) fn configured_resolver_languages(jdtls_path: Option<&str>) -> Vec<EnrichmentLanguage> {
    let mut langs = vec![EnrichmentLanguage::Rust, EnrichmentLanguage::TypeScript];
    if jdtls_path.is_some() {
        langs.push(EnrichmentLanguage::Java);
    }
    langs
}

/// The configured resolver languages for a surface that carries NO `jdtls_path` request param (`stats`,
/// `orient`): the `JDTLS_PATH` env is the only jdtls source, exactly as `handle_enrich` falls back to it
/// when the param is absent. The convenience both posture-bearing surfaces call so the env-read site is
/// not duplicated.
pub(crate) fn configured_resolver_languages_from_env() -> Vec<EnrichmentLanguage> {
    configured_resolver_languages(std::env::var("JDTLS_PATH").ok().as_deref())
}

// HONEST-DEGRADATION-IMPL-2-REFACTOR: the pure reader-context label helpers (D2 ecosystem, D5
// next-action line) and their unit tests moved to `reader_context.rs` — behavior unchanged. Re-exported
// `pub(crate)` so this module's call sites (`dominant_deps_ecosystem`) AND `orient_coherence`'s
// `crate::dispatch::relationship_reliability_is_low` / `…::relationship_next_action_line_or_read_error`
// resolve as before (a compile-time path alias only). CONTRADICTION-SWEEP-1 review-1 #1: both surfaces
// now route the counts-read RESULT through the shared `relationship_next_action_line_or_read_error`
// wrapper (which owns the LOW-axis check and the unknown-with-reason failure line); the bare
// `relationship_next_action_line` is called only inside `reader_context` (by that wrapper), so it no
// longer needs re-exporting here. (`deps_reader_context_note` is consumed directly by `deps_headline`.)
// ORIENT-SMALL-ENRICH-1: `call_graph_ceiling_languages` is no longer re-exported here — `handle_check`
// maps counts into `CeilingFact` INLINE (its sole production caller; reviewer review-1 F2) calling the
// bare `reader_context::call_graph_ceiling_languages` by its full path, and the helper's other callers
// live inside `reader_context`, so it no longer needs a `crate::dispatch::…` alias.
pub(crate) use crate::reader_context::{
    dominant_deps_ecosystem, relationship_next_action_line_or_read_error,
    relationship_reliability_is_low, resource_uncovered_material_languages,
};

#[cfg(test)]
mod http_boundary_filter_tests {
    //! HTTP-BOUNDARY-1 review-2 item 2: prove the `boundaries list` filters
    //! recognize HTTP. Before this arm existed, `parse_channel_kind("http")`
    //! and `parse_protocol_family("http")` returned `None`, which
    //! `handle_boundaries_list` assigns to the OPTIONAL filter — silently
    //! clearing it and returning ALL surfaces instead of HTTP-only.
    use super::{parse_channel_kind, parse_protocol_family};
    use repo_graph_boundary_interaction::{ChannelKind, ProtocolFamily};

    #[test]
    fn channel_kind_http_parses() {
        assert_eq!(parse_channel_kind("http"), Some(ChannelKind::Http));
        assert_eq!(parse_channel_kind("HTTP"), Some(ChannelKind::Http));
        assert_eq!(parse_channel_kind("rest"), Some(ChannelKind::Http));
    }

    #[test]
    fn protocol_family_http_parses() {
        assert_eq!(parse_protocol_family("http"), Some(ProtocolFamily::Http));
        assert_eq!(parse_protocol_family("HTTP"), Some(ProtocolFamily::Http));
        assert_eq!(parse_protocol_family("rest"), Some(ProtocolFamily::Http));
    }

    /// The filter must be SET (Some) for http — a `None` here is the exact bug
    /// that made `--kind http` / `--family http` return every surface.
    #[test]
    fn http_filters_are_not_silently_cleared() {
        assert!(parse_channel_kind("http").is_some());
        assert!(parse_protocol_family("http").is_some());
    }
}
