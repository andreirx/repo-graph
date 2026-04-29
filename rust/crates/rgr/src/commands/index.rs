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
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::from(2)
        }
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
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::from(2)
        }
    }
}
