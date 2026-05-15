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

use crate::daemon_client::{DaemonClient, DaemonClientError};

/// Run the `rmap index` command.
///
/// Usage: `rmap index <repo_path> <db_path> [--include-root <path>]...`
///
/// This command requires the daemon to be running. If the daemon is
/// unavailable, it fails with an actionable error message.
///
/// Exit codes:
/// - 0: success
/// - 1: usage error
/// - 2: runtime error (includes daemon unavailable)
pub fn run_index(args: &[String]) -> ExitCode {
    // Parse options and positional args.
    let mut include_roots: Vec<String> = Vec::new();
    let mut positional: Vec<&String> = Vec::new();
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
            positional.push(&args[i]);
            i += 1;
        }
    }

    if positional.len() != 2 {
        eprintln!("usage: rmap index <repo_path> <db_path> [--include-root <path>]...");
        return ExitCode::from(1);
    }

    let repo_path = Path::new(positional[0]);
    let db_path = Path::new(positional[1]);

    if !repo_path.is_dir() {
        eprintln!(
            "error: repo path does not exist or is not a directory: {}",
            repo_path.display()
        );
        return ExitCode::from(1);
    }

    // Canonicalize paths for daemon
    let repo_path_canon = match repo_path.canonicalize() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: failed to canonicalize repo path: {}", e);
            return ExitCode::from(2);
        }
    };

    // db_path may not exist yet (new index), so we canonicalize parent + filename
    let db_path_canon = match canonicalize_db_path(db_path) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };

    // Build daemon request params
    let mut params = serde_json::json!({
        "repo_path": repo_path_canon.to_string_lossy(),
        "db_path": db_path_canon.to_string_lossy(),
    });

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

    if !client.is_available() {
        eprintln!(
            "{}",
            crate::daemon_client::daemon_unavailable_message(client.socket_path(), "index")
        );
        return ExitCode::from(2);
    }

    match client.request("index", Some(params)) {
        Ok(result) => {
            // Extract fields from daemon response
            let files_total = result["files_total"].as_u64().unwrap_or(0);
            let nodes_total = result["nodes_total"].as_u64().unwrap_or(0);
            let edges_total = result["edges_total"].as_u64().unwrap_or(0);
            let edges_unresolved = result["edges_unresolved"].as_u64().unwrap_or(0);
            let snapshot_uid = result["snapshot_uid"].as_str().unwrap_or("unknown");

            eprintln!(
                "indexed {} files, {} nodes, {} edges ({} unresolved) → {}",
                files_total, nodes_total, edges_total, edges_unresolved, snapshot_uid,
            );

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
        Err(DaemonClientError::DaemonError { code, message }) => {
            eprintln!("error: daemon returned {}: {}", code, message);
            ExitCode::from(2)
        }
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::from(2)
        }
    }
}

/// Canonicalize a database path, handling the case where the file doesn't exist yet.
///
/// For new databases, we canonicalize the parent directory and append the filename.
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

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::*;
    use repo_graph_indexer::types::{
        ArtifactCopyForward, ContractIndexResult, ContractParseFailure, GeneratedCodeMappingResult,
    };

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
    // Parse options and positional args.
    let mut include_roots: Vec<String> = Vec::new();
    let mut positional: Vec<&String> = Vec::new();
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
            positional.push(&args[i]);
            i += 1;
        }
    }

    if positional.len() != 2 {
        eprintln!("usage: rmap refresh <db_path> <repo_uid> [--include-root <path>]...");
        return ExitCode::from(1);
    }

    let db_path = Path::new(positional[0]);
    let repo_uid = positional[1];

    if !db_path.exists() {
        eprintln!("error: database does not exist: {}", db_path.display());
        return ExitCode::from(1);
    }

    // Canonicalize db_path for daemon
    let db_path_canon = match db_path.canonicalize() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: failed to canonicalize db path: {}", e);
            return ExitCode::from(2);
        }
    };

    // Build daemon request params
    let mut params = serde_json::json!({
        "db_path": db_path_canon.to_string_lossy(),
        "repo_uid": repo_uid,
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

    if !client.is_available() {
        eprintln!(
            "{}",
            crate::daemon_client::daemon_unavailable_message(client.socket_path(), "refresh")
        );
        return ExitCode::from(2);
    }

    match client.request("refresh", Some(params)) {
        Ok(result) => {
            // Extract fields from daemon response
            let files_total = result["files_total"].as_u64().unwrap_or(0);
            let nodes_total = result["nodes_total"].as_u64().unwrap_or(0);
            let edges_total = result["edges_total"].as_u64().unwrap_or(0);
            let edges_unresolved = result["edges_unresolved"].as_u64().unwrap_or(0);
            let snapshot_uid = result["snapshot_uid"].as_str().unwrap_or("unknown");

            eprintln!(
                "refreshed {} files, {} nodes, {} edges ({} unresolved) → {}",
                files_total, nodes_total, edges_total, edges_unresolved, snapshot_uid,
            );

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
        Err(DaemonClientError::DaemonError { code, message }) => {
            eprintln!("error: daemon returned {}: {}", code, message);
            ExitCode::from(2)
        }
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::from(2)
        }
    }
}
