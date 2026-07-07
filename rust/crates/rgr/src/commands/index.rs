//! Index command family.
//!
//! Initial indexing and incremental refresh of repository graphs.
//!
//! ## Daemon Requirement
//!
//! `index` and `refresh` are daemon-required operations (RMAPD-2 D4).
//! They mutate the database and require daemon coordination to prevent
//! concurrent writes. When daemon is unavailable, these commands fail
//! with an actionable error message.

use std::path::Path;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use crate::daemon_client::{DaemonClient, DaemonClientError};

/// Per-read stall timeout for a long index/refresh, in seconds. Progress frames reset this deadline
/// (contract C2), so it is effectively "abort if the daemon goes SILENT for this long", at which
/// point the still-running probe (contract C1) reports the truth. Matches the transport default.
const LONG_OP_READ_TIMEOUT_SECS: u64 = 300;

/// Environment override for [`LONG_OP_READ_TIMEOUT_SECS`].
///
/// Two concrete users: (1) the still-running E2E proof forces a short timeout so a real in-flight
/// index trips the contract-C path deterministically instead of waiting 5 minutes; (2) an operator on
/// a slow machine / flaky link can widen the stall window. Default (unset / unparsable) = 300s. A
/// value of 0 is clamped to 1 (the transport rejects a 0-duration read timeout).
fn long_op_read_timeout_secs() -> u64 {
    match std::env::var("RMAP_LONG_OP_READ_TIMEOUT_SECS") {
        Ok(v) => v
            .trim()
            .parse::<u64>()
            .map(|n| n.max(1))
            .unwrap_or(LONG_OP_READ_TIMEOUT_SECS),
        Err(_) => LONG_OP_READ_TIMEOUT_SECS,
    }
}

/// DAEMON-VISIBILITY-1 (contract C2): a throttled stderr renderer for the daemon's progress frames.
///
/// Renders on every phase change and at most once every 2s within a phase, so a long index surfaces
/// coarse progress ("extracting: 42000/160000") instead of blocking silently — without flooding the
/// terminal when the pipeline emits per-file events.
fn make_progress_renderer() -> impl FnMut(&serde_json::Value) {
    let mut last_render = Instant::now();
    let mut last_phase = String::new();
    let mut first = true;
    move |p: &serde_json::Value| {
        let phase = p
            .get("phase")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let current = p.get("current").and_then(|v| v.as_u64()).unwrap_or(0);
        let total = p.get("total").and_then(|v| v.as_u64()).unwrap_or(0);
        let phase_changed = phase != last_phase;
        if first || phase_changed || last_render.elapsed() >= Duration::from_secs(2) {
            if total > 0 {
                eprintln!("  {}: {}/{}", phase, current, total);
            } else if current > 0 {
                eprintln!("  {}: {}", phase, current);
            } else if !phase.is_empty() {
                eprintln!("  {}…", phase);
            }
            last_render = Instant::now();
            last_phase = phase;
            first = false;
        }
    }
}

/// Run the `rmap index` command (REG-1 contract).
///
/// Usage: `rmap index [repo_path] [--alias <name>] [--include-root <path>]...`
///
/// - `repo_path`: Optional. Path to repository root. Defaults to current directory.
/// - `--alias`: Optional. Human-friendly name for the repo.
/// - `--include-root`: Optional. C/C++ include paths.
///
/// The daemon:
/// 1. Registers repo in registry (or retrieves existing entry)
/// 2. Allocates db_path if new
/// 3. Generates stable repo_uid if new
/// 4. Indexes the repo
/// 5. Updates registry with last_indexed_at and last_snapshot_uid
///
/// This command requires the daemon to be running.
///
/// Exit codes:
/// - 0: success
/// - 1: usage error
/// - 2: runtime error (includes daemon unavailable)
pub fn run_index(args: &[String]) -> ExitCode {
    // Parse options and positional args.
    let mut include_roots: Vec<String> = Vec::new();
    let mut alias: Option<String> = None;
    let mut repo_path_arg: Option<String> = None;
    let mut i = 0;

    while i < args.len() {
        if args[i] == "--include-root" {
            if i + 1 >= args.len() {
                eprintln!("error: --include-root requires a path argument");
                return ExitCode::from(1);
            }
            include_roots.push(args[i + 1].clone());
            i += 2;
        } else if args[i] == "--alias" {
            if i + 1 >= args.len() {
                eprintln!("error: --alias requires a name argument");
                return ExitCode::from(1);
            }
            alias = Some(args[i + 1].clone());
            i += 2;
        } else if args[i].starts_with("--") {
            eprintln!("error: unknown option: {}", args[i]);
            return ExitCode::from(1);
        } else if repo_path_arg.is_none() {
            repo_path_arg = Some(args[i].clone());
            i += 1;
        } else {
            eprintln!("error: unexpected argument: {}", args[i]);
            eprintln!("usage: rmap index [repo_path] [--alias <name>] [--include-root <path>]...");
            return ExitCode::from(1);
        }
    }

    // Default to current directory if no repo_path provided
    let repo_path_str = repo_path_arg.unwrap_or_else(|| ".".to_string());
    let repo_path = Path::new(&repo_path_str);

    if !repo_path.is_dir() {
        eprintln!(
            "error: repo path does not exist or is not a directory: {}",
            repo_path.display()
        );
        return ExitCode::from(1);
    }

    // Canonicalize repo path for daemon
    let repo_path_canon = match repo_path.canonicalize() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: failed to canonicalize repo path: {}", e);
            return ExitCode::from(2);
        }
    };

    // Build daemon request params (REG-1: no db_path, daemon allocates)
    let mut params = serde_json::json!({
        "repo_path": repo_path_canon.to_string_lossy(),
    });

    if let Some(alias_str) = &alias {
        params["alias"] = serde_json::json!(alias_str);
    }

    if !include_roots.is_empty() {
        params["include_roots"] = serde_json::json!(include_roots);
    }

    // Execute via daemon (required for index)
    let mut client = match DaemonClient::new() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };

    // Transport selection (socket vs stdio) happens in request() via ensure_connected().
    // DAEMON-VISIBILITY-1 (C2): render the daemon's progress frames while attached.
    let mut on_progress = make_progress_renderer();
    match client.request_with_progress(
        "index",
        Some(params),
        long_op_read_timeout_secs(),
        &mut on_progress,
    ) {
        Ok(result) => {
            // Extract fields from daemon response
            let files_total = result["files_total"].as_u64().unwrap_or(0);
            let nodes_total = result["nodes_total"].as_u64().unwrap_or(0);
            let edges_total = result["edges_total"].as_u64().unwrap_or(0);
            let edges_unresolved = result["edges_unresolved"].as_u64().unwrap_or(0);
            let snapshot_uid = result["snapshot_uid"].as_str().unwrap_or("unknown");
            let repo_uid = result["repo_uid"].as_str().unwrap_or("unknown");

            // HONEST-DEGRADATION-IMPL-1 (D4): "nodes (all kinds)" label — rationale + unit test in
            // `format_index_summary`.
            eprintln!(
                "{}",
                format_index_summary(files_total, nodes_total, edges_total, edges_unresolved)
            );
            eprintln!("  repo: {}", repo_uid);
            eprintln!("  snapshot: {}", snapshot_uid);
            // ENRICH-LIFECYCLE-1 (D3): every completed index states whether the background enrichment
            // pass was QUEUED (auto-run, async — the result surfaces on `rmap doctor`) or is DISABLED
            // via `RMAP_AUTO_ENRICH`. It never fabricates the resolved/promoted numbers here (they do
            // not exist yet — same discipline as retention).
            if let Some(line) = format_enrichment_line(result.get("enrichment")) {
                eprintln!("{line}");
            }

            // SNAPSHOT-RETENTION-1: report the queued background cleanup pass (async; result on doctor).
            if let Some(line) = format_retention_line(result.get("retention")) {
                eprintln!("{line}");
            }

            // Print contract summary if present
            if let Some(contracts) = result.get("contracts") {
                print_contract_summary_from_daemon(contracts);
            }

            // Print mapping summary if present
            if let Some(mappings) = result.get("generated_code_mappings") {
                print_mapping_summary_from_daemon(mappings);
            }

            ExitCode::SUCCESS
        }
        // DAEMON-VISIBILITY-1 (contract C): a read timeout on a long index is NOT a failure. Probe
        // the daemon; if the index is still running, say so truthfully with a DISTINCT exit status.
        Err(DaemonClientError::Timeout { timeout_secs }) => {
            report_long_op_timeout(&repo_path_canon, "index", timeout_secs)
        }
        Err(DaemonClientError::DaemonError { code, message, .. }) => {
            eprintln!("error: daemon returned {}: {}", code, message);
            ExitCode::from(2)
        }
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::from(2)
        }
    }
}

/// DAEMON-VISIBILITY-1 (contract C): the client's read timed out on a long index/refresh. Probe the
/// daemon (a FRESH connection — the timed-out one is still mid-request daemon-side; the daemon is
/// concurrent so a second connection is served) and report the truth:
///
/// - daemon reachable + an active op for THIS repo → "still running" + a DISTINCT exit status
///   ([`EXIT_STILL_RUNNING`], not the failure code). The operation survived the client timeout.
/// - daemon reachable + no active op → it likely just completed between the timeout and the probe;
///   still not a failure.
/// - daemon unreachable → it stopped/crashed mid-operation → a genuine failure ([`EXIT_RUNTIME_ERROR`]).
///
/// This is the fix for the field bug where a live 160k-file index printed "timed out after 300s"
/// and exited as a failure while `rmapd` kept indexing.
fn report_long_op_timeout(repo_path_canon: &Path, op_label: &str, timeout_secs: u64) -> ExitCode {
    let repo_str = repo_path_canon.to_string_lossy();

    // A short-timeout `daemon_info` probe on a fresh connection.
    let probe = DaemonClient::new()
        .ok()
        .and_then(|mut c| c.request_with_timeout("daemon_info", None, 10).ok());

    let status = classify_long_op_timeout(probe.as_ref(), &repo_str);
    match status {
        LongOpStatus::StillRunning => {
            // The honest truth: the op is STILL RUNNING (the client's read timed out, not the op).
            eprintln!(
                "note: {op_label} of {repo_str} is STILL RUNNING on the daemon — the client's {timeout_secs}s read timed out, the operation did not."
            );
            if let Some(op) = probe
                .as_ref()
                .and_then(|info| find_active_op(info, &repo_str))
            {
                let phase = op.get("phase").and_then(|v| v.as_str());
                let current = op.get("current").and_then(|v| v.as_u64()).unwrap_or(0);
                let total = op.get("total").and_then(|v| v.as_u64()).unwrap_or(0);
                match (phase, total) {
                    (Some(ph), t) if t > 0 => eprintln!("      progress: {ph} {current}/{t}"),
                    (Some(ph), _) if current > 0 => eprintln!("      progress: {ph} {current}"),
                    (Some(ph), _) => eprintln!("      progress: {ph}"),
                    (None, _) => {}
                }
            }
            eprintln!("      it continues in the background — follow it with `rmap doctor`.");
        }
        LongOpStatus::ReachableNoOp => {
            eprintln!(
                "note: the client's {timeout_secs}s read timed out, but the daemon is reachable and reports no"
            );
            eprintln!(
                "      active {op_label} for {repo_str} — it may have just completed. Check with `rmap doctor`."
            );
        }
        LongOpStatus::Unreachable => {
            eprintln!(
                "error: {op_label} timed out after {timeout_secs}s and the daemon is no longer reachable —"
            );
            eprintln!(
                "       the operation may have failed. Run `rmap doctor` to check daemon health."
            );
        }
    }
    ExitCode::from(status.exit_code())
}

/// The three honest outcomes of a long-op read timeout (contract C). Only [`Unreachable`] is a
/// failure; a still-running (or just-completed) op is a DISTINCT non-failure status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LongOpStatus {
    /// Daemon reachable + an active op for this repo → still running in the background.
    StillRunning,
    /// Daemon reachable, no active op for this repo → likely just completed.
    ReachableNoOp,
    /// Daemon no longer reachable after our timeout → it stopped mid-operation.
    Unreachable,
}

impl LongOpStatus {
    fn exit_code(self) -> u8 {
        match self {
            // Both "still running" and "just completed" are NON-failures (the op was not lost).
            LongOpStatus::StillRunning | LongOpStatus::ReachableNoOp => {
                crate::daemon_command::EXIT_STILL_RUNNING
            }
            LongOpStatus::Unreachable => crate::daemon_command::EXIT_RUNTIME_ERROR,
        }
    }
}

/// Find the active daemon operation for `repo_str` in a `daemon_info` response, if any.
fn find_active_op<'a>(
    info: &'a serde_json::Value,
    repo_str: &str,
) -> Option<&'a serde_json::Value> {
    info.get("active_operations")
        .and_then(|v| v.as_array())
        .and_then(|ops| {
            ops.iter()
                .find(|op| op.get("repo").and_then(|v| v.as_str()) == Some(repo_str))
        })
}

/// Pure classifier for a long-op timeout given the `daemon_info` probe result (contract C).
///
/// `probe` is `None` when the daemon could not be reached after the timeout.
fn classify_long_op_timeout(probe: Option<&serde_json::Value>, repo_str: &str) -> LongOpStatus {
    match probe {
        None => LongOpStatus::Unreachable,
        Some(info) if find_active_op(info, repo_str).is_some() => LongOpStatus::StillRunning,
        Some(_) => LongOpStatus::ReachableNoOp,
    }
}

/// Canonicalize a database path, handling the case where the file doesn't exist yet.
///
/// For new databases, we canonicalize the parent directory and append the filename.
///
/// Note: This function is retained for the refresh command which still uses
/// explicit db_path until REG-1 CLI migration is complete.
#[allow(dead_code)]
fn canonicalize_db_path(db_path: &Path) -> Result<std::path::PathBuf, String> {
    if db_path.exists() {
        db_path
            .canonicalize()
            .map_err(|e| format!("failed to canonicalize db path: {}", e))
    } else {
        // DB doesn't exist yet - canonicalize parent and append filename
        let parent = db_path
            .parent()
            .ok_or_else(|| "db_path has no parent directory".to_string())?;

        // Create parent if needed, then canonicalize
        if !parent.exists() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("failed to create db parent directory: {}", e))?;
        }

        let parent_canon = parent
            .canonicalize()
            .map_err(|e| format!("failed to canonicalize db parent: {}", e))?;

        let filename = db_path
            .file_name()
            .ok_or_else(|| "db_path has no filename".to_string())?;

        Ok(parent_canon.join(filename))
    }
}

/// Print contract indexing summary to stderr (typed variant).
///
/// Used by direct library calls (e.g., tests). Daemon-backed CLI uses
/// `print_contract_summary_from_daemon` which parses the JSON response.
#[allow(dead_code)]
fn print_contract_summary(contracts: &Option<repo_graph_indexer::types::ContractIndexResult>) {
    for line in format_contract_summary(contracts) {
        eprintln!("{}", line);
    }
}

/// Format contract indexing summary as lines (testable).
#[allow(dead_code)]
fn format_contract_summary(
    contracts: &Option<repo_graph_indexer::types::ContractIndexResult>,
) -> Vec<String> {
    let Some(c) = contracts else {
        return Vec::new();
    };

    // Skip if no contract activity at all
    if c.schemas_indexed == 0 && c.parse_failures.is_empty() && c.storage_error.is_none() {
        return Vec::new();
    }

    let mut lines = Vec::new();
    let fail_count = c.parse_failures.len();

    // Build status suffix combining both conditions
    let status = match (&c.storage_error, fail_count) {
        (Some(err), 0) => format!(" (storage error: {})", err),
        (Some(err), n) => format!(" ({} failed, storage error: {})", n, err),
        (None, 0) => String::new(),
        (None, n) => format!(" ({} failed)", n),
    };

    lines.push(format!(
        "  contracts: {} schemas, {} elements{}",
        c.schemas_indexed, c.elements_indexed, status
    ));

    // Show parse failure details (first 5)
    if fail_count > 0 {
        for failure in c.parse_failures.iter().take(5) {
            lines.push(format!(
                "    FAILED: {}: {}",
                failure.file_path, failure.error
            ));
        }
        if fail_count > 5 {
            lines.push(format!("    ... and {} more failures", fail_count - 5));
        }
    }

    lines
}

/// Print generated code mapping summary to stderr (typed variant).
///
/// Used by direct library calls (e.g., tests). Daemon-backed CLI uses
/// `print_mapping_summary_from_daemon` which parses the JSON response.
#[allow(dead_code)]
fn print_mapping_summary(mappings: &Option<repo_graph_indexer::types::GeneratedCodeMappingResult>) {
    for line in format_mapping_summary(mappings) {
        eprintln!("{}", line);
    }
}

/// Format generated code mapping summary as lines (testable).
#[allow(dead_code)]
fn format_mapping_summary(
    mappings: &Option<repo_graph_indexer::types::GeneratedCodeMappingResult>,
) -> Vec<String> {
    let Some(m) = mappings else {
        return Vec::new();
    };

    // Skip if no mapping activity and no errors
    if m.mappings_persisted == 0 && !m.has_error() {
        return Vec::new();
    }

    let mut lines = Vec::new();

    // Build error suffix
    let errors: Vec<&str> = [
        m.element_query_error
            .as_ref()
            .map(|_| "element query failed"),
        m.symbol_query_error.as_ref().map(|_| "symbol query failed"),
        m.storage_error.as_ref().map(|_| "storage failed"),
    ]
    .into_iter()
    .flatten()
    .collect();

    let status = if errors.is_empty() {
        String::new()
    } else {
        format!(" ({})", errors.join(", "))
    };

    lines.push(format!(
        "  mappings: {} persisted ({} high-confidence){}",
        m.mappings_persisted, m.high_confidence_count, status
    ));

    // Show error details if any
    if let Some(ref err) = m.element_query_error {
        lines.push(format!("    element query: {}", err));
    }
    if let Some(ref err) = m.symbol_query_error {
        lines.push(format!("    symbol query: {}", err));
    }
    if let Some(ref err) = m.storage_error {
        lines.push(format!("    storage: {}", err));
    }

    lines
}

/// Print artifact copy-forward summary to stderr (typed variant, refresh only).
///
/// Used by direct library calls (e.g., tests). Daemon-backed CLI uses
/// `print_copy_forward_summary_from_daemon` which parses the JSON response.
#[allow(dead_code)]
fn print_copy_forward_summary(
    copy_forward: &Option<repo_graph_indexer::types::ArtifactCopyForward>,
) {
    for line in format_copy_forward_summary(copy_forward) {
        eprintln!("{}", line);
    }
}

// ── Daemon response formatters ─────────────────────────────────────
//
// These functions parse JSON values from daemon responses and format them
// for CLI output, preserving parity with the direct-call formatters above.

/// Print contract summary from daemon JSON response.
fn print_contract_summary_from_daemon(contracts: &serde_json::Value) {
    let schemas_indexed = contracts["schemas_indexed"].as_u64().unwrap_or(0);
    let elements_indexed = contracts["elements_indexed"].as_u64().unwrap_or(0);
    let parse_failures = contracts["parse_failures"].as_array();
    let storage_error = contracts["storage_error"].as_str();

    let fail_count = parse_failures.map(|a| a.len()).unwrap_or(0);

    // Skip if no contract activity at all
    if schemas_indexed == 0 && fail_count == 0 && storage_error.is_none() {
        return;
    }

    // Build status suffix combining both conditions
    let status = match (storage_error, fail_count) {
        (Some(err), 0) => format!(" (storage error: {})", err),
        (Some(err), n) => format!(" ({} failed, storage error: {})", n, err),
        (None, 0) => String::new(),
        (None, n) => format!(" ({} failed)", n),
    };

    eprintln!(
        "  contracts: {} schemas, {} elements{}",
        schemas_indexed, elements_indexed, status
    );

    // Show parse failure details (first 5)
    if let Some(failures) = parse_failures {
        for failure in failures.iter().take(5) {
            let file_path = failure["file_path"].as_str().unwrap_or("unknown");
            let error = failure["error"].as_str().unwrap_or("unknown error");
            eprintln!("    FAILED: {}: {}", file_path, error);
        }
        if fail_count > 5 {
            eprintln!("    ... and {} more failures", fail_count - 5);
        }
    }
}

/// Print generated code mapping summary from daemon JSON response.
fn print_mapping_summary_from_daemon(mappings: &serde_json::Value) {
    let mappings_persisted = mappings["mappings_persisted"].as_u64().unwrap_or(0);
    let high_confidence_count = mappings["high_confidence_count"].as_u64().unwrap_or(0);
    let element_query_error = mappings["element_query_error"].as_str();
    let symbol_query_error = mappings["symbol_query_error"].as_str();
    let storage_error = mappings["storage_error"].as_str();

    let has_error =
        element_query_error.is_some() || symbol_query_error.is_some() || storage_error.is_some();

    // Skip if no mapping activity and no errors
    if mappings_persisted == 0 && !has_error {
        return;
    }

    // Build error suffix
    let mut errors: Vec<&str> = Vec::new();
    if element_query_error.is_some() {
        errors.push("element query failed");
    }
    if symbol_query_error.is_some() {
        errors.push("symbol query failed");
    }
    if storage_error.is_some() {
        errors.push("storage failed");
    }

    let status = if errors.is_empty() {
        String::new()
    } else {
        format!(" ({})", errors.join(", "))
    };

    eprintln!(
        "  mappings: {} persisted ({} high-confidence){}",
        mappings_persisted, high_confidence_count, status
    );

    // Show error details if any
    if let Some(err) = element_query_error {
        eprintln!("    element query: {}", err);
    }
    if let Some(err) = symbol_query_error {
        eprintln!("    symbol query: {}", err);
    }
    if let Some(err) = storage_error {
        eprintln!("    storage: {}", err);
    }
}

/// Print artifact copy-forward summary from daemon JSON response (refresh only).
fn print_copy_forward_summary_from_daemon(copy_forward: &serde_json::Value) {
    let measurements_copied = copy_forward["measurements_copied"].as_u64().unwrap_or(0);
    let inferences_copied = copy_forward["inferences_copied"].as_u64().unwrap_or(0);
    let boundary_surfaces_copied = copy_forward["boundary_surfaces_copied"]
        .as_u64()
        .unwrap_or(0);
    let boundary_channels_copied = copy_forward["boundary_channels_copied"]
        .as_u64()
        .unwrap_or(0);
    let contract_schemas_copied = copy_forward["contract_schemas_copied"]
        .as_u64()
        .unwrap_or(0);
    let contract_elements_copied = copy_forward["contract_elements_copied"]
        .as_u64()
        .unwrap_or(0);

    // Skip if nothing was copied
    let total = measurements_copied
        + inferences_copied
        + boundary_surfaces_copied
        + boundary_channels_copied
        + contract_schemas_copied
        + contract_elements_copied;

    if total == 0 {
        return;
    }

    let mut parts = Vec::new();
    if measurements_copied > 0 {
        parts.push(format!("{} measurements", measurements_copied));
    }
    if inferences_copied > 0 {
        parts.push(format!("{} inferences", inferences_copied));
    }
    if boundary_surfaces_copied > 0 {
        parts.push(format!("{} boundary surfaces", boundary_surfaces_copied));
    }
    if boundary_channels_copied > 0 {
        parts.push(format!("{} channels", boundary_channels_copied));
    }
    if contract_schemas_copied > 0 {
        parts.push(format!("{} schemas", contract_schemas_copied));
    }
    if contract_elements_copied > 0 {
        parts.push(format!("{} elements", contract_elements_copied));
    }

    eprintln!("  copy-forward: {}", parts.join(", "));
}

/// Format artifact copy-forward summary as lines (testable).
#[allow(dead_code)]
fn format_copy_forward_summary(
    copy_forward: &Option<repo_graph_indexer::types::ArtifactCopyForward>,
) -> Vec<String> {
    let Some(cf) = copy_forward else {
        return Vec::new();
    };

    // Skip if nothing was copied
    let total = cf.measurements_copied
        + cf.inferences_copied
        + cf.boundary_surfaces_copied
        + cf.boundary_channels_copied
        + cf.contract_schemas_copied
        + cf.contract_elements_copied;

    if total == 0 {
        return Vec::new();
    }

    let mut parts = Vec::new();
    if cf.measurements_copied > 0 {
        parts.push(format!("{} measurements", cf.measurements_copied));
    }
    if cf.inferences_copied > 0 {
        parts.push(format!("{} inferences", cf.inferences_copied));
    }
    if cf.boundary_surfaces_copied > 0 {
        parts.push(format!("{} boundary surfaces", cf.boundary_surfaces_copied));
    }
    if cf.boundary_channels_copied > 0 {
        parts.push(format!("{} channels", cf.boundary_channels_copied));
    }
    if cf.contract_schemas_copied > 0 {
        parts.push(format!("{} schemas", cf.contract_schemas_copied));
    }
    if cf.contract_elements_copied > 0 {
        parts.push(format!("{} elements", cf.contract_elements_copied));
    }

    vec![format!("  copy-forward: {}", parts.join(", "))]
}

/// Format the one-line `index` completion summary written to stderr.
///
/// HONEST-DEGRADATION-IMPL-1 (D4): the node count is labelled **"nodes (all kinds)"** — it is
/// `nodes_total` = `COUNT(*)` over EVERY node kind (SYMBOL + FILE + MODULE …), a superset of
/// "symbols". Without the qualifier an agent can misread it as a symbol count (e.g. nginx: 4393 nodes
/// vs 3977 symbols), one of the three divergent "symbol"/"node" numbers this slice reconciles.
/// Extracted as a pure function so the label is unit-testable without capturing stderr; the command
/// only prints the returned line.
fn format_index_summary(
    files_total: u64,
    nodes_total: u64,
    edges_total: u64,
    edges_unresolved: u64,
) -> String {
    format!(
        "indexed {} files, {} nodes (all kinds), {} edges ({} unresolved)",
        files_total, nodes_total, edges_total, edges_unresolved,
    )
}

/// Format the one-line `refresh` completion summary written to stderr. Same "nodes (all kinds)"
/// labelling as [`format_index_summary`] (HONEST-DEGRADATION-IMPL-1 D4), plus the resulting snapshot.
fn format_refresh_summary(
    files_total: u64,
    nodes_total: u64,
    edges_total: u64,
    edges_unresolved: u64,
    snapshot_uid: &str,
) -> String {
    format!(
        "refreshed {} files, {} nodes (all kinds), {} edges ({} unresolved) → {}",
        files_total, nodes_total, edges_total, edges_unresolved, snapshot_uid,
    )
}

/// SNAPSHOT-RETENTION-1: the retention line on the index/refresh completion report.
///
/// The automatic cleanup pass is **asynchronous** — it never runs on the foreground request path
/// (REFRESH-HANG-1) — so this line reports that cleanup was QUEUED (with the current prunable backlog
/// the queued pass will clear) or is DISABLED via `RMAP_AUTO_RETENTION`. It NEVER fabricates
/// pruned/reclaimed numbers, because they do not exist yet when this reply is sent: the RESULT
/// surfaces on `rmap doctor` (which shows the pass running and its last outcome) and the daemon log.
/// Returns `None` when the daemon carried no retention block (older daemon) — nothing to print.
fn format_retention_line(retention: Option<&serde_json::Value>) -> Option<String> {
    let retention = retention?;
    let auto_pass = retention.get("auto_pass").and_then(|v| v.as_str())?;
    match auto_pass {
        "queued" => {
            let prunable = retention
                .get("prunable_count")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            Some(if prunable > 0 {
                format!("  retention: background cleanup queued ({prunable} snapshot(s) to reclaim) — see `rmap doctor`")
            } else {
                "  retention: background cleanup queued — see `rmap doctor`".to_string()
            })
        }
        "disabled" => Some("  retention: auto-cleanup disabled (RMAP_AUTO_RETENTION)".to_string()),
        _ => None,
    }
}

/// ENRICH-LIFECYCLE-1: the enrichment line on the index/refresh completion report.
///
/// The automatic enrichment pass is **asynchronous** (never on the foreground request path), so this
/// line reports that enrichment was QUEUED (auto-run in the background — the resolved/promoted result
/// and any toolchain skip surface on `rmap doctor`) or is DISABLED via `RMAP_AUTO_ENRICH`. It NEVER
/// fabricates the resolved/promoted numbers here — they do not exist when this reply is sent (same
/// discipline as `format_retention_line`). Returns `None` when the daemon carried no enrichment block
/// (older daemon) — nothing to print.
fn format_enrichment_line(enrichment: Option<&serde_json::Value>) -> Option<String> {
    let auto_pass = enrichment?.get("auto_pass").and_then(|v| v.as_str())?;
    match auto_pass {
        "queued" => Some(
            "  enrichment: background pass queued (resolved call types) — see `rmap doctor`"
                .to_string(),
        ),
        "disabled" => Some("  enrichment: auto-enrichment disabled (RMAP_AUTO_ENRICH)".to_string()),
        _ => None,
    }
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::*;
    use crate::daemon_command::{EXIT_RUNTIME_ERROR, EXIT_STILL_RUNNING};
    use repo_graph_indexer::types::{
        ArtifactCopyForward, ContractIndexResult, ContractParseFailure, GeneratedCodeMappingResult,
    };

    // DAEMON-VISIBILITY-1 (contract C) — the still-running proof. A client read-timeout on a long op
    // is classified from a fresh `daemon_info` probe: a live op → "still running" with a DISTINCT
    // exit status (NOT the failure code); an unreachable daemon → the failure code. This is the fix
    // for a 160k-file index that printed "timed out after 300s" and exited as a failure while the
    // daemon kept indexing.
    #[test]
    fn still_running_timeout_yields_distinct_non_failure_exit_status() {
        let repo = "/repos/big";

        // Daemon reachable + an active index for THIS repo → still running.
        let with_op = serde_json::json!({
            "active_operations": [
                { "kind": "index", "repo": repo, "phase": "extracting",
                  "current": 42_000, "total": 160_000, "started_secs_ago": 360 }
            ]
        });
        let status = classify_long_op_timeout(Some(&with_op), repo);
        assert_eq!(status, LongOpStatus::StillRunning);
        assert_eq!(status.exit_code(), EXIT_STILL_RUNNING);

        // Reachable, but the op is for a DIFFERENT repo → not this repo's op (just-completed case).
        let other = serde_json::json!({
            "active_operations": [ { "kind": "index", "repo": "/repos/other" } ]
        });
        assert_eq!(
            classify_long_op_timeout(Some(&other), repo),
            LongOpStatus::ReachableNoOp
        );
        assert_eq!(
            classify_long_op_timeout(Some(&other), repo).exit_code(),
            EXIT_STILL_RUNNING,
            "just-completed is still NOT a failure"
        );

        // Daemon unreachable after the timeout → genuine failure.
        let unreachable = classify_long_op_timeout(None, repo);
        assert_eq!(unreachable, LongOpStatus::Unreachable);
        assert_eq!(unreachable.exit_code(), EXIT_RUNTIME_ERROR);

        // The "still running" status is DISTINCT from the failure status.
        assert_ne!(EXIT_STILL_RUNNING, EXIT_RUNTIME_ERROR);
    }

    // HONEST-DEGRADATION-IMPL-1 (D4): the `index`/`refresh` node count is labelled "nodes (all kinds)"
    // — never "symbols" — so an agent cannot misread the all-kinds COUNT(*) (e.g. nginx's 4393) as the
    // symbol count (3977). These assert the label on the exact line the commands print to stderr.
    #[test]
    fn index_summary_labels_nodes_all_kinds_not_symbols() {
        let line = format_index_summary(3, 28, 7, 0);
        assert_eq!(
            line, "indexed 3 files, 28 nodes (all kinds), 7 edges (0 unresolved)",
            "{line}"
        );
        assert!(line.contains("28 nodes (all kinds)"), "{line}");
        assert!(
            !line.contains("28 symbols") && !line.contains("nodes,"),
            "the node count must NOT be presented as symbols, nor a bare unqualified `nodes`: {line}"
        );
    }

    #[test]
    fn refresh_summary_labels_nodes_all_kinds_not_symbols() {
        let line = format_refresh_summary(3, 28, 7, 0, "snap_x");
        assert!(line.contains("28 nodes (all kinds)"), "{line}");
        assert!(
            line.contains("→ snap_x"),
            "refresh names the resulting snapshot: {line}"
        );
        assert!(
            !line.contains("28 symbols") && !line.contains("nodes,"),
            "the node count must NOT be presented as symbols, nor a bare unqualified `nodes`: {line}"
        );
    }

    // SNAPSHOT-RETENTION-1: the completion-report retention line reports the QUEUED (async) pass —
    // never fabricated pruned/reclaimed numbers (those surface on `rmap doctor`).
    #[test]
    fn retention_line_reports_queued_with_backlog() {
        let r = serde_json::json!({ "auto_pass": "queued", "prunable_count": 3 });
        let line = format_retention_line(Some(&r)).unwrap();
        assert!(line.contains("queued"), "{line}");
        assert!(line.contains("3 snapshot(s) to reclaim"), "{line}");
        assert!(line.contains("rmap doctor"), "{line}");
        assert!(
            !line.contains("reclaimed") && !line.contains("pruned "),
            "must NOT fabricate a result before the async pass ran: {line}"
        );
    }

    #[test]
    fn retention_line_queued_without_backlog() {
        let r = serde_json::json!({ "auto_pass": "queued", "prunable_count": 0 });
        let line = format_retention_line(Some(&r)).unwrap();
        assert!(
            line.contains("queued") && !line.contains("to reclaim"),
            "{line}"
        );
    }

    #[test]
    fn retention_line_disabled() {
        let r = serde_json::json!({ "auto_pass": "disabled" });
        let line = format_retention_line(Some(&r)).unwrap();
        assert!(
            line.contains("disabled") && line.contains("RMAP_AUTO_RETENTION"),
            "{line}"
        );
    }

    #[test]
    fn retention_line_absent_when_no_block_or_unknown() {
        assert!(format_retention_line(None).is_none());
        assert!(format_retention_line(Some(&serde_json::json!({}))).is_none());
        assert!(format_retention_line(Some(&serde_json::json!({ "auto_pass": "??" }))).is_none());
    }

    #[test]
    fn format_none_returns_empty() {
        let lines = format_contract_summary(&None);
        assert!(lines.is_empty());
    }

    #[test]
    fn format_zero_activity_returns_empty() {
        let result = ContractIndexResult {
            schemas_indexed: 0,
            elements_indexed: 0,
            parse_failures: Vec::new(),
            storage_error: None,
        };
        let lines = format_contract_summary(&Some(result));
        assert!(lines.is_empty());
    }

    #[test]
    fn format_success_no_suffix() {
        let result = ContractIndexResult {
            schemas_indexed: 5,
            elements_indexed: 42,
            parse_failures: Vec::new(),
            storage_error: None,
        };
        let lines = format_contract_summary(&Some(result));
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0], "  contracts: 5 schemas, 42 elements");
    }

    #[test]
    fn format_storage_error_only() {
        let result = ContractIndexResult {
            schemas_indexed: 5,
            elements_indexed: 42,
            parse_failures: Vec::new(),
            storage_error: Some("connection refused".to_string()),
        };
        let lines = format_contract_summary(&Some(result));
        assert_eq!(lines.len(), 1);
        assert_eq!(
            lines[0],
            "  contracts: 5 schemas, 42 elements (storage error: connection refused)"
        );
    }

    #[test]
    fn format_parse_failures_only() {
        let result = ContractIndexResult {
            schemas_indexed: 3,
            elements_indexed: 20,
            parse_failures: vec![
                ContractParseFailure {
                    file_path: "bad.proto".to_string(),
                    error: "syntax error".to_string(),
                },
                ContractParseFailure {
                    file_path: "other.proto".to_string(),
                    error: "unexpected token".to_string(),
                },
            ],
            storage_error: None,
        };
        let lines = format_contract_summary(&Some(result));
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], "  contracts: 3 schemas, 20 elements (2 failed)");
        assert_eq!(lines[1], "    FAILED: bad.proto: syntax error");
        assert_eq!(lines[2], "    FAILED: other.proto: unexpected token");
    }

    #[test]
    fn format_combined_storage_error_and_parse_failures() {
        let result = ContractIndexResult {
            schemas_indexed: 3,
            elements_indexed: 20,
            parse_failures: vec![ContractParseFailure {
                file_path: "bad.proto".to_string(),
                error: "syntax error".to_string(),
            }],
            storage_error: Some("disk full".to_string()),
        };
        let lines = format_contract_summary(&Some(result));
        assert_eq!(lines.len(), 2);
        assert_eq!(
            lines[0],
            "  contracts: 3 schemas, 20 elements (1 failed, storage error: disk full)"
        );
        assert_eq!(lines[1], "    FAILED: bad.proto: syntax error");
    }

    #[test]
    fn format_truncates_after_five_failures() {
        let result = ContractIndexResult {
            schemas_indexed: 1,
            elements_indexed: 5,
            parse_failures: (0..8)
                .map(|i| ContractParseFailure {
                    file_path: format!("file{}.proto", i),
                    error: "error".to_string(),
                })
                .collect(),
            storage_error: None,
        };
        let lines = format_contract_summary(&Some(result));
        assert_eq!(lines.len(), 7); // summary + 5 failures + truncation notice
        assert!(lines[0].contains("(8 failed)"));
        assert!(lines[6].contains("... and 3 more failures"));
    }

    // ── Generated code mapping summary tests ─────────────────────

    #[test]
    fn format_mapping_none_returns_empty() {
        let lines = format_mapping_summary(&None);
        assert!(lines.is_empty());
    }

    #[test]
    fn format_mapping_zero_activity_no_errors_returns_empty() {
        let result = GeneratedCodeMappingResult {
            mappings_persisted: 0,
            high_confidence_count: 0,
            element_query_error: None,
            symbol_query_error: None,
            storage_error: None,
        };
        let lines = format_mapping_summary(&Some(result));
        assert!(lines.is_empty());
    }

    #[test]
    fn format_mapping_success_no_errors() {
        let result = GeneratedCodeMappingResult {
            mappings_persisted: 10,
            high_confidence_count: 7,
            element_query_error: None,
            symbol_query_error: None,
            storage_error: None,
        };
        let lines = format_mapping_summary(&Some(result));
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0], "  mappings: 10 persisted (7 high-confidence)");
    }

    #[test]
    fn format_mapping_with_element_query_error() {
        let result = GeneratedCodeMappingResult {
            mappings_persisted: 0,
            high_confidence_count: 0,
            element_query_error: Some("no such table".to_string()),
            symbol_query_error: None,
            storage_error: None,
        };
        let lines = format_mapping_summary(&Some(result));
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("element query failed"));
        assert_eq!(lines[1], "    element query: no such table");
    }

    #[test]
    fn format_mapping_with_multiple_errors() {
        let result = GeneratedCodeMappingResult {
            mappings_persisted: 5,
            high_confidence_count: 3,
            element_query_error: None,
            symbol_query_error: Some("timeout".to_string()),
            storage_error: Some("disk full".to_string()),
        };
        let lines = format_mapping_summary(&Some(result));
        assert_eq!(lines.len(), 3);
        assert!(lines[0].contains("symbol query failed"));
        assert!(lines[0].contains("storage failed"));
        assert_eq!(lines[1], "    symbol query: timeout");
        assert_eq!(lines[2], "    storage: disk full");
    }

    #[test]
    fn format_copy_forward_none_returns_empty() {
        let lines = format_copy_forward_summary(&None);
        assert!(lines.is_empty());
    }

    #[test]
    fn format_copy_forward_zero_activity_returns_empty() {
        let result = ArtifactCopyForward {
            measurements_copied: 0,
            inferences_copied: 0,
            boundary_surfaces_copied: 0,
            boundary_channels_copied: 0,
            contract_schemas_copied: 0,
            contract_elements_copied: 0,
        };
        let lines = format_copy_forward_summary(&Some(result));
        assert!(lines.is_empty());
    }

    #[test]
    fn format_copy_forward_measurements_only() {
        let result = ArtifactCopyForward {
            measurements_copied: 42,
            inferences_copied: 0,
            boundary_surfaces_copied: 0,
            boundary_channels_copied: 0,
            contract_schemas_copied: 0,
            contract_elements_copied: 0,
        };
        let lines = format_copy_forward_summary(&Some(result));
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0], "  copy-forward: 42 measurements");
    }

    #[test]
    fn format_copy_forward_multiple_families() {
        let result = ArtifactCopyForward {
            measurements_copied: 10,
            inferences_copied: 5,
            boundary_surfaces_copied: 3,
            boundary_channels_copied: 7,
            contract_schemas_copied: 2,
            contract_elements_copied: 12,
        };
        let lines = format_copy_forward_summary(&Some(result));
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("10 measurements"));
        assert!(lines[0].contains("5 inferences"));
        assert!(lines[0].contains("3 boundary surfaces"));
        assert!(lines[0].contains("7 channels"));
        assert!(lines[0].contains("2 schemas"));
        assert!(lines[0].contains("12 elements"));
    }
}

/// Run the `rmap refresh` command.
///
/// Usage: `rmap refresh <db_path> <repo_uid> [--include-root <path>]...`
///
/// This command requires the daemon to be running. If the daemon is
/// unavailable, it fails with an actionable error message.
///
/// Exit codes:
/// - 0: success
/// - 1: usage error
/// - 2: runtime error (includes daemon unavailable)
pub fn run_refresh(args: &[String]) -> ExitCode {
    // Parse options (REG-1: no positional args, repo from cwd)
    let mut include_roots: Vec<String> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--include-root" {
            if i + 1 >= args.len() {
                eprintln!("error: --include-root requires a path argument");
                return ExitCode::from(1);
            }
            include_roots.push(args[i + 1].clone());
            i += 2;
        } else if args[i].starts_with("--") {
            eprintln!("error: unknown option: {}", args[i]);
            return ExitCode::from(1);
        } else {
            eprintln!("error: unexpected argument: {}", args[i]);
            eprintln!("usage: rmap refresh [--include-root <path>]...");
            return ExitCode::from(1);
        }
    }

    // REG-1: resolve repo from cwd
    let cwd = match std::env::current_dir() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: cannot get current directory: {}", e);
            return ExitCode::from(2);
        }
    };

    let repo_path = match cwd.canonicalize() {
        Ok(p) => p.to_string_lossy().to_string(),
        Err(e) => {
            eprintln!("error: cannot canonicalize current directory: {}", e);
            return ExitCode::from(2);
        }
    };

    // Build daemon request params (REG-1: sends repo path, not db_path/repo_uid)
    let mut params = serde_json::json!({
        "repo": repo_path,
    });

    if !include_roots.is_empty() {
        params["include_roots"] = serde_json::json!(include_roots);
    }

    // Execute via daemon (required for refresh)
    let mut client = match DaemonClient::new() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };

    // Transport selection (socket vs stdio) happens in request() via ensure_connected().
    // DAEMON-VISIBILITY-1 (C2): render the daemon's progress frames while attached.
    let mut on_progress = make_progress_renderer();
    match client.request_with_progress(
        "refresh",
        Some(params),
        long_op_read_timeout_secs(),
        &mut on_progress,
    ) {
        Ok(result) => {
            // Extract fields from daemon response
            let files_total = result["files_total"].as_u64().unwrap_or(0);
            let nodes_total = result["nodes_total"].as_u64().unwrap_or(0);
            let edges_total = result["edges_total"].as_u64().unwrap_or(0);
            let edges_unresolved = result["edges_unresolved"].as_u64().unwrap_or(0);
            let snapshot_uid = result["snapshot_uid"].as_str().unwrap_or("unknown");

            // HONEST-DEGRADATION-IMPL-1 (D4): "nodes (all kinds)" label — see `format_refresh_summary`.
            eprintln!(
                "{}",
                format_refresh_summary(
                    files_total,
                    nodes_total,
                    edges_total,
                    edges_unresolved,
                    snapshot_uid
                )
            );
            // ENRICH-LIFECYCLE-1 (D3): enrichment lifecycle on the refresh completion report too.
            if let Some(line) = format_enrichment_line(result.get("enrichment")) {
                eprintln!("{line}");
            }

            // SNAPSHOT-RETENTION-1: report the queued background cleanup pass (async; result on doctor).
            if let Some(line) = format_retention_line(result.get("retention")) {
                eprintln!("{line}");
            }

            // Print copy-forward summary if present (refresh-specific)
            if let Some(copy_forward) = result.get("artifact_copy_forward") {
                print_copy_forward_summary_from_daemon(copy_forward);
            }

            // Print contract summary if present
            if let Some(contracts) = result.get("contracts") {
                print_contract_summary_from_daemon(contracts);
            }

            // Print mapping summary if present
            if let Some(mappings) = result.get("generated_code_mappings") {
                print_mapping_summary_from_daemon(mappings);
            }

            ExitCode::SUCCESS
        }
        // DAEMON-VISIBILITY-1 (contract C): a read timeout on a long refresh is NOT a failure.
        Err(DaemonClientError::Timeout { timeout_secs }) => {
            report_long_op_timeout(Path::new(&repo_path), "refresh", timeout_secs)
        }
        Err(DaemonClientError::DaemonError { code, message, .. }) => {
            if code == "RepoNotFound" {
                eprintln!("error: repo not indexed");
                eprintln!("hint: run 'rmap index .' to index this repo first");
            } else {
                eprintln!("error: daemon returned {}: {}", code, message);
            }
            ExitCode::from(2)
        }
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::from(2)
        }
    }
}
