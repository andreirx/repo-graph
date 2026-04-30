//! Docs command family.
//!
//! Documentation discovery and semantic fact extraction:
//! - `list` — documentation inventory (primary surface)
//! - `extract` — semantic fact extraction (secondary hints)
//!
//! Docs are primary; semantic_facts are secondary derived hints.
//!
//! # Boundary rules
//!
//! This module owns docs command-family behavior:
//! - command handlers
//! - family-local helpers
//! - fact-to-storage mapping
//!
//! This module does **not** own:
//! - shared infrastructure (lives in `crate::cli`)
//! - doc discovery (lives in `repo-graph-doc-facts`)
//! - semantic fact storage (lives in `repo-graph-storage`)

use std::path::Path;
use std::process::ExitCode;

use crate::cli::open_storage;

/// Dispatcher for `rmap docs <subcommand>`.
pub fn run_docs(args: &[String]) -> ExitCode {
    if args.is_empty() {
        print_docs_usage();
        return ExitCode::from(1);
    }

    match args[0].as_str() {
        "list" => run_docs_list(&args[1..]),
        "extract" => run_docs_extract(&args[1..]),
        other => {
            eprintln!("unknown docs subcommand: {}", other);
            print_docs_usage();
            ExitCode::from(1)
        }
    }
}

fn print_docs_usage() {
    eprintln!("usage:");
    eprintln!("  rmap docs list    <db_path> <repo_uid>  — documentation inventory");
    eprintln!("  rmap docs extract <db_path> <repo_uid>  — extract semantic hints");
}

/// List documentation inventory (primary documentation surface).
///
/// Returns doc file paths, kinds, and generated flags. Does NOT
/// derive from semantic_facts — uses live filesystem discovery.
fn run_docs_list(args: &[String]) -> ExitCode {
    if args.len() != 2 {
        eprintln!("usage: rmap docs list <db_path> <repo_uid>");
        return ExitCode::from(1);
    }

    let db_path = Path::new(&args[0]);
    let repo_uid = &args[1];

    let storage = match open_storage(db_path) {
        Ok(s) => s,
        Err(msg) => {
            eprintln!("error: {}", msg);
            return ExitCode::from(2);
        }
    };

    // Get repo to find root_path
    use repo_graph_storage::types::RepoRef;
    let repo = match storage.get_repo(&RepoRef::Uid(repo_uid.to_string())) {
        Ok(Some(r)) => r,
        Ok(None) => {
            eprintln!("error: repo '{}' not found", repo_uid);
            return ExitCode::from(2);
        }
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };

    let repo_path = Path::new(&repo.root_path);

    // Discover documentation inventory (live filesystem, not semantic_facts)
    let inventory = match repo_graph_doc_facts::discover_doc_inventory(repo_path, true) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: discovery failed: {}", e);
            return ExitCode::from(2);
        }
    };

    // Build JSON output
    let output = serde_json::json!({
        "command": "docs list",
        "repo": repo_uid,
        "repo_path": repo.root_path,
        "entries": inventory.entries,
        "count": inventory.entries.len(),
        "counts_by_kind": inventory.counts_by_kind,
        "generated_count": inventory.generated_count
    });

    println!("{}", serde_json::to_string_pretty(&output).unwrap());
    ExitCode::from(0)
}

/// Extract semantic facts from documentation (secondary hints).
///
/// Populates semantic_facts table with derived hints for ranking
/// and filtering. The docs themselves remain the primary data.
fn run_docs_extract(args: &[String]) -> ExitCode {
    if args.len() != 2 {
        eprintln!("usage: rmap docs extract <db_path> <repo_uid>");
        return ExitCode::from(1);
    }

    let db_path = Path::new(&args[0]);
    let repo_uid = &args[1];

    let mut storage = match open_storage(db_path) {
        Ok(s) => s,
        Err(msg) => {
            eprintln!("error: {}", msg);
            return ExitCode::from(2);
        }
    };

    // Get repo to find root_path
    use repo_graph_storage::types::RepoRef;
    let repo = match storage.get_repo(&RepoRef::Uid(repo_uid.to_string())) {
        Ok(Some(r)) => r,
        Ok(None) => {
            eprintln!("error: repo '{}' not found", repo_uid);
            return ExitCode::from(2);
        }
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };

    let repo_path = Path::new(&repo.root_path);

    // Extract semantic facts from documentation
    let extraction_result = match repo_graph_doc_facts::extract_semantic_facts(repo_path) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: extraction failed: {}", e);
            return ExitCode::from(2);
        }
    };

    // Map ExtractedFact to NewSemanticFact
    let new_facts: Vec<repo_graph_storage::crud::semantic_facts::NewSemanticFact> =
        extraction_result
            .facts
            .iter()
            .map(|f| map_extracted_to_storage(repo_uid, f))
            .collect();

    // Replace facts in storage atomically
    let replace_result = match storage.replace_semantic_facts_for_repo(repo_uid, &new_facts) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: storage failed: {}", e);
            return ExitCode::from(2);
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

    // Build JSON output
    let output = serde_json::json!({
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
    });

    println!("{}", serde_json::to_string_pretty(&output).unwrap());
    ExitCode::from(0)
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
