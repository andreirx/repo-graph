//! Contracts command family.
//!
//! Contract schema discovery and inspection (CS-1+).
//!
//! # Boundary rules
//!
//! This module owns contracts command-family behavior:
//! - `run_contracts`, `run_contracts_list`, `run_contracts_show`,
//!   `run_contracts_elements` handlers
//! - contracts-family DTOs
//! - contracts-family argument parsing
//! - contracts-family output shaping
//!
//! This module does **not** own:
//! - shared infrastructure (lives in `crate::cli`)
//! - storage queries (belongs in storage crate)

use std::path::Path;
use std::process::ExitCode;

use crate::cli::{build_envelope, open_storage, resolve_repo_ref};

// ── contracts command ────────────────────────────────────────────

pub fn run_contracts(args: &[String]) -> ExitCode {
    if args.is_empty() {
        print_contracts_usage();
        return ExitCode::from(1);
    }

    match args[0].as_str() {
        "list" => run_contracts_list(&args[1..]),
        "show" => run_contracts_show(&args[1..]),
        "elements" => run_contracts_elements(&args[1..]),
        "usages" => run_contracts_usages(&args[1..]),
        other => {
            eprintln!("unknown contracts subcommand: {}", other);
            print_contracts_usage();
            ExitCode::from(1)
        }
    }
}

fn print_contracts_usage() {
    eprintln!("usage:");
    eprintln!("  rmap contracts list <db_path> <repo_uid> [--kind protobuf]");
    eprintln!("  rmap contracts show <db_path> <repo_uid> <file_path>");
    eprintln!("  rmap contracts elements <db_path> <repo_uid> [--kind message|enum|service|method|field] [--file <path>]");
    eprintln!("  rmap contracts usages <db_path> <repo_uid> [--element <element_uid>] [--min-confidence <0.0-1.0>]");
}

// ── contracts list command ───────────────────────────────────────

/// Output DTO for `contracts list` command.
#[derive(serde::Serialize)]
struct ContractSchemaEntry {
    schema_uid: String,
    file_path: String,
    schema_kind: String,
    package_name: Option<String>,
    syntax_version: Option<String>,
    parsed_at: String,
}

fn run_contracts_list(args: &[String]) -> ExitCode {
    // Parse args: <db_path> <repo_uid> [--kind <kind>]
    if args.len() < 2 {
        eprintln!("usage: rmap contracts list <db_path> <repo_uid> [--kind protobuf]");
        return ExitCode::from(1);
    }

    let db_path = Path::new(&args[0]);
    let repo_ref = &args[1];

    // Parse optional --kind filter
    let mut kind_filter: Option<String> = None;
    let mut i = 2;
    while i < args.len() {
        if args[i] == "--kind" {
            if i + 1 >= args.len() {
                eprintln!("--kind requires a value");
                return ExitCode::from(1);
            }
            kind_filter = Some(args[i + 1].clone());
            i += 2;
        } else {
            eprintln!("unknown option: {}", args[i]);
            return ExitCode::from(1);
        }
    }

    // Open storage
    let storage = match open_storage(db_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("storage error: {}", e);
            return ExitCode::from(2);
        }
    };

    // Resolve repo
    let repo_uid = match resolve_repo_ref(&storage, repo_ref) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{}", e);
            return ExitCode::from(2);
        }
    };

    // Get latest snapshot
    let snapshot = match storage.get_latest_snapshot(&repo_uid) {
        Ok(Some(s)) => s,
        Ok(None) => {
            eprintln!("no snapshot found for repo '{}'", repo_ref);
            return ExitCode::from(2);
        }
        Err(e) => {
            eprintln!("storage error: {}", e);
            return ExitCode::from(2);
        }
    };

    // Query schemas
    use repo_graph_storage::contract_schema_port::ContractSchemaStoragePort;
    let schemas =
        match storage.list_contract_schemas(&snapshot.snapshot_uid, kind_filter.as_deref()) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("query error: {}", e);
                return ExitCode::from(2);
            }
        };

    // Build output
    let results: Vec<ContractSchemaEntry> = schemas
        .into_iter()
        .map(|s| ContractSchemaEntry {
            schema_uid: s.schema_uid,
            file_path: s.file_path,
            schema_kind: s.schema_kind,
            package_name: s.package_name,
            syntax_version: s.syntax_version,
            parsed_at: s.parsed_at,
        })
        .collect();

    let count = results.len();
    let mut extra = serde_json::Map::new();
    if let Some(ref k) = kind_filter {
        extra.insert(
            "filter_kind".to_string(),
            serde_json::Value::String(k.clone()),
        );
    }

    let output = match build_envelope(
        &storage,
        "contracts list",
        &repo_uid,
        &snapshot,
        serde_json::to_value(&results).unwrap(),
        count,
        extra,
    ) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };

    match serde_json::to_string_pretty(&output) {
        Ok(json) => {
            println!("{}", json);
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::from(2)
        }
    }
}

// ── contracts show command ───────────────────────────────────────

/// Output DTO for `contracts show` command.
#[derive(serde::Serialize)]
struct ContractSchemaDetail {
    schema_uid: String,
    file_path: String,
    schema_kind: String,
    package_name: Option<String>,
    syntax_version: Option<String>,
    content_hash: String,
    extractor: String,
    parsed_at: String,
    elements: Vec<ContractElementEntry>,
}

/// Element entry for show command.
#[derive(serde::Serialize)]
struct ContractElementEntry {
    element_uid: String,
    element_kind: String,
    name: String,
    full_name: String,
    parent_element_uid: Option<String>,
    line_start: Option<u32>,
    line_end: Option<u32>,
    metadata: Option<serde_json::Value>,
}

fn run_contracts_show(args: &[String]) -> ExitCode {
    // Parse args: <db_path> <repo_uid> <file_path>
    if args.len() < 3 {
        eprintln!("usage: rmap contracts show <db_path> <repo_uid> <file_path>");
        return ExitCode::from(1);
    }

    let db_path = Path::new(&args[0]);
    let repo_ref = &args[1];
    let file_path = &args[2];

    // Open storage
    let storage = match open_storage(db_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("storage error: {}", e);
            return ExitCode::from(2);
        }
    };

    // Resolve repo
    let repo_uid = match resolve_repo_ref(&storage, repo_ref) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{}", e);
            return ExitCode::from(2);
        }
    };

    // Get latest snapshot
    let snapshot = match storage.get_latest_snapshot(&repo_uid) {
        Ok(Some(s)) => s,
        Ok(None) => {
            eprintln!("no snapshot found for repo '{}'", repo_ref);
            return ExitCode::from(2);
        }
        Err(e) => {
            eprintln!("storage error: {}", e);
            return ExitCode::from(2);
        }
    };

    // Query schema by file path
    use repo_graph_storage::contract_schema_port::ContractSchemaStoragePort;
    let schema = match storage.get_schema_by_file(&snapshot.snapshot_uid, file_path) {
        Ok(Some(s)) => s,
        Ok(None) => {
            eprintln!("schema not found: {}", file_path);
            return ExitCode::from(2);
        }
        Err(e) => {
            eprintln!("query error: {}", e);
            return ExitCode::from(2);
        }
    };

    // Query elements for this schema
    let elements = match storage.list_elements_for_schema(&schema.schema_uid, None) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("query error: {}", e);
            return ExitCode::from(2);
        }
    };

    // Build output
    let element_entries: Vec<ContractElementEntry> = elements
        .into_iter()
        .map(|e| ContractElementEntry {
            element_uid: e.element_uid,
            element_kind: e.element_kind,
            name: e.name,
            full_name: e.full_name,
            parent_element_uid: e.parent_element_uid,
            line_start: e.line_start,
            line_end: e.line_end,
            metadata: e.metadata_json.and_then(|s| serde_json::from_str(&s).ok()),
        })
        .collect();

    let detail = ContractSchemaDetail {
        schema_uid: schema.schema_uid,
        file_path: schema.file_path,
        schema_kind: schema.schema_kind,
        package_name: schema.package_name,
        syntax_version: schema.syntax_version,
        content_hash: schema.content_hash,
        extractor: schema.extractor,
        parsed_at: schema.parsed_at,
        elements: element_entries,
    };

    let output = match build_envelope(
        &storage,
        "contracts show",
        &repo_uid,
        &snapshot,
        serde_json::to_value(&detail).unwrap(),
        1,
        serde_json::Map::new(),
    ) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };

    match serde_json::to_string_pretty(&output) {
        Ok(json) => {
            println!("{}", json);
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::from(2)
        }
    }
}

// ── contracts elements command ───────────────────────────────────

/// Output DTO for `contracts elements` command.
#[derive(serde::Serialize)]
struct ContractElementListEntry {
    element_uid: String,
    schema_uid: String,
    file_path: String,
    element_kind: String,
    name: String,
    full_name: String,
    line_start: Option<u32>,
}

fn run_contracts_elements(args: &[String]) -> ExitCode {
    // Parse args: <db_path> <repo_uid> [--kind <kind>] [--file <path>]
    if args.len() < 2 {
        eprintln!("usage: rmap contracts elements <db_path> <repo_uid> [--kind message|enum|service|method|field] [--file <path>]");
        return ExitCode::from(1);
    }

    let db_path = Path::new(&args[0]);
    let repo_ref = &args[1];

    // Parse optional filters
    let mut kind_filter: Option<String> = None;
    let mut file_filter: Option<String> = None;
    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--kind" => {
                if i + 1 >= args.len() {
                    eprintln!("--kind requires a value");
                    return ExitCode::from(1);
                }
                kind_filter = Some(args[i + 1].clone());
                i += 2;
            }
            "--file" => {
                if i + 1 >= args.len() {
                    eprintln!("--file requires a value");
                    return ExitCode::from(1);
                }
                file_filter = Some(args[i + 1].clone());
                i += 2;
            }
            other => {
                eprintln!("unknown option: {}", other);
                return ExitCode::from(1);
            }
        }
    }

    // Open storage
    let storage = match open_storage(db_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("storage error: {}", e);
            return ExitCode::from(2);
        }
    };

    // Resolve repo
    let repo_uid = match resolve_repo_ref(&storage, repo_ref) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{}", e);
            return ExitCode::from(2);
        }
    };

    // Get latest snapshot
    let snapshot = match storage.get_latest_snapshot(&repo_uid) {
        Ok(Some(s)) => s,
        Ok(None) => {
            eprintln!("no snapshot found for repo '{}'", repo_ref);
            return ExitCode::from(2);
        }
        Err(e) => {
            eprintln!("storage error: {}", e);
            return ExitCode::from(2);
        }
    };

    use repo_graph_storage::contract_schema_port::ContractSchemaStoragePort;

    // Get schemas (optionally filtered by file)
    let schemas = match &file_filter {
        Some(path) => match storage.get_schema_by_file(&snapshot.snapshot_uid, path) {
            Ok(Some(s)) => vec![s],
            Ok(None) => {
                eprintln!("schema not found: {}", path);
                return ExitCode::from(2);
            }
            Err(e) => {
                eprintln!("query error: {}", e);
                return ExitCode::from(2);
            }
        },
        None => match storage.list_contract_schemas(&snapshot.snapshot_uid, None) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("query error: {}", e);
                return ExitCode::from(2);
            }
        },
    };

    // Collect elements from all schemas
    let mut results: Vec<ContractElementListEntry> = Vec::new();
    for schema in schemas {
        let elements =
            match storage.list_elements_for_schema(&schema.schema_uid, kind_filter.as_deref()) {
                Ok(e) => e,
                Err(e) => {
                    eprintln!("query error: {}", e);
                    return ExitCode::from(2);
                }
            };

        for elem in elements {
            results.push(ContractElementListEntry {
                element_uid: elem.element_uid,
                schema_uid: schema.schema_uid.clone(),
                file_path: schema.file_path.clone(),
                element_kind: elem.element_kind,
                name: elem.name,
                full_name: elem.full_name,
                line_start: elem.line_start,
            });
        }
    }

    let count = results.len();
    let mut extra = serde_json::Map::new();
    if let Some(ref k) = kind_filter {
        extra.insert(
            "filter_kind".to_string(),
            serde_json::Value::String(k.clone()),
        );
    }
    if let Some(ref f) = file_filter {
        extra.insert(
            "filter_file".to_string(),
            serde_json::Value::String(f.clone()),
        );
    }

    let output = match build_envelope(
        &storage,
        "contracts elements",
        &repo_uid,
        &snapshot,
        serde_json::to_value(&results).unwrap(),
        count,
        extra,
    ) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };

    match serde_json::to_string_pretty(&output) {
        Ok(json) => {
            println!("{}", json);
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::from(2)
        }
    }
}

// ── contracts usages command ─────────────────────────────────────

/// Output DTO for `contracts usages` command.
#[derive(serde::Serialize)]
struct GeneratedCodeMappingEntry {
    mapping_uid: String,
    schema_element_uid: String,
    element_name: Option<String>,
    element_full_name: Option<String>,
    generated_symbol_key: String,
    language: String,
    generated_file: String,
    mapping_basis: String,
    confidence: f64,
    evidence: Option<serde_json::Value>,
}

fn run_contracts_usages(args: &[String]) -> ExitCode {
    // Parse args: <db_path> <repo_uid> [--element <element_uid>] [--min-confidence <value>]
    if args.len() < 2 {
        eprintln!("usage: rmap contracts usages <db_path> <repo_uid> [--element <element_uid>] [--min-confidence <0.0-1.0>]");
        return ExitCode::from(1);
    }

    let db_path = Path::new(&args[0]);
    let repo_ref = &args[1];

    // Parse optional filters
    let mut element_filter: Option<String> = None;
    let mut min_confidence: f64 = 0.0;
    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--element" => {
                if i + 1 >= args.len() {
                    eprintln!("--element requires a value");
                    return ExitCode::from(1);
                }
                element_filter = Some(args[i + 1].clone());
                i += 2;
            }
            "--min-confidence" => {
                if i + 1 >= args.len() {
                    eprintln!("--min-confidence requires a value");
                    return ExitCode::from(1);
                }
                match args[i + 1].parse::<f64>() {
                    Ok(v) if (0.0..=1.0).contains(&v) => min_confidence = v,
                    _ => {
                        eprintln!("--min-confidence must be a number between 0.0 and 1.0");
                        return ExitCode::from(1);
                    }
                }
                i += 2;
            }
            other => {
                eprintln!("unknown option: {}", other);
                return ExitCode::from(1);
            }
        }
    }

    // Open storage
    let storage = match open_storage(db_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("storage error: {}", e);
            return ExitCode::from(2);
        }
    };

    // Resolve repo
    let repo_uid = match resolve_repo_ref(&storage, repo_ref) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{}", e);
            return ExitCode::from(2);
        }
    };

    // Get latest snapshot
    let snapshot = match storage.get_latest_snapshot(&repo_uid) {
        Ok(Some(s)) => s,
        Ok(None) => {
            eprintln!("no snapshot found for repo '{}'", repo_ref);
            return ExitCode::from(2);
        }
        Err(e) => {
            eprintln!("storage error: {}", e);
            return ExitCode::from(2);
        }
    };

    use repo_graph_storage::contract_schema_port::ContractSchemaStoragePort;

    // Query mappings
    let mappings = match storage
        .list_generated_code_mappings(&snapshot.snapshot_uid, element_filter.as_deref())
    {
        Ok(m) => m,
        Err(e) => {
            eprintln!("query error: {}", e);
            return ExitCode::from(2);
        }
    };

    // Filter by min confidence and build results
    let mut results: Vec<GeneratedCodeMappingEntry> = Vec::new();
    for mapping in mappings {
        if mapping.confidence < min_confidence {
            continue;
        }

        results.push(GeneratedCodeMappingEntry {
            mapping_uid: mapping.mapping_uid,
            schema_element_uid: mapping.schema_element_uid,
            element_name: None, // Element lookup by UID deferred
            element_full_name: None,
            generated_symbol_key: mapping.generated_symbol_key,
            language: mapping.language,
            generated_file: mapping.generated_file,
            mapping_basis: mapping.mapping_basis,
            confidence: mapping.confidence,
            evidence: mapping
                .metadata_json
                .and_then(|s| serde_json::from_str(&s).ok()),
        });
    }

    let count = results.len();
    let mut extra = serde_json::Map::new();
    if let Some(ref e) = element_filter {
        extra.insert(
            "filter_element".to_string(),
            serde_json::Value::String(e.clone()),
        );
    }
    if min_confidence > 0.0 {
        extra.insert(
            "filter_min_confidence".to_string(),
            serde_json::json!(min_confidence),
        );
    }

    let output = match build_envelope(
        &storage,
        "contracts usages",
        &repo_uid,
        &snapshot,
        serde_json::to_value(&results).unwrap(),
        count,
        extra,
    ) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };

    match serde_json::to_string_pretty(&output) {
        Ok(json) => {
            println!("{}", json);
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::from(2)
        }
    }
}
