//! Index command family.
//!
//! Initial indexing and incremental refresh of repository graphs.

use std::path::Path;
use std::process::ExitCode;

/// Run the `rmap index` command.
///
/// Usage: `rmap index <repo_path> <db_path> [--include-root <path>]...`
///
/// Exit codes:
/// - 0: success
/// - 1: usage error
/// - 2: runtime error
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

    let repo_uid = repo_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("repo");

    use repo_graph_repo_index::compose::{index_path, ComposeOptions};
    let options = ComposeOptions {
        c_include_roots: include_roots,
        ..ComposeOptions::default()
    };
    match index_path(repo_path, db_path, repo_uid, &options) {
        Ok(result) => {
            eprintln!(
                "indexed {} files, {} nodes, {} edges ({} unresolved) → {}",
                result.files_total,
                result.nodes_total,
                result.edges_total,
                result.edges_unresolved,
                result.snapshot_uid,
            );
            print_contract_summary(&result.contracts);
            print_mapping_summary(&result.generated_code_mappings);
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::from(2)
        }
    }
}

/// Print contract indexing summary to stderr.
fn print_contract_summary(contracts: &Option<repo_graph_indexer::types::ContractIndexResult>) {
    for line in format_contract_summary(contracts) {
        eprintln!("{}", line);
    }
}

/// Format contract indexing summary as lines (testable).
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
            lines.push(format!("    FAILED: {}: {}", failure.file_path, failure.error));
        }
        if fail_count > 5 {
            lines.push(format!("    ... and {} more failures", fail_count - 5));
        }
    }

    lines
}

/// Print generated code mapping summary to stderr.
fn print_mapping_summary(mappings: &Option<repo_graph_indexer::types::GeneratedCodeMappingResult>) {
    for line in format_mapping_summary(mappings) {
        eprintln!("{}", line);
    }
}

/// Format generated code mapping summary as lines (testable).
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
        m.element_query_error.as_ref().map(|_| "element query failed"),
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

#[cfg(test)]
mod tests {
    use super::*;
    use repo_graph_indexer::types::{ContractIndexResult, ContractParseFailure, GeneratedCodeMappingResult};

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
}

/// Run the `rmap refresh` command.
///
/// Usage: `rmap refresh <repo_path> <db_path> [--include-root <path>]...`
///
/// Exit codes:
/// - 0: success
/// - 1: usage error
/// - 2: runtime error
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
        eprintln!("usage: rmap refresh <repo_path> <db_path> [--include-root <path>]...");
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

    let repo_uid = repo_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("repo");

    use repo_graph_repo_index::compose::{refresh_path, ComposeOptions};
    let options = ComposeOptions {
        c_include_roots: include_roots,
        ..ComposeOptions::default()
    };
    match refresh_path(repo_path, db_path, repo_uid, &options) {
        Ok(result) => {
            eprintln!(
                "refreshed {} files, {} nodes, {} edges ({} unresolved) → {}",
                result.files_total,
                result.nodes_total,
                result.edges_total,
                result.edges_unresolved,
                result.snapshot_uid,
            );
            print_contract_summary(&result.contracts);
            print_mapping_summary(&result.generated_code_mappings);
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::from(2)
        }
    }
}
