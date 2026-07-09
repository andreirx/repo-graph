//! Repo management commands (REG-1).
//!
//! Commands for managing the daemon's repo registry.
//!
//! ## Commands
//!
//! - `rmap repo list` — list all registered repos
//! - `rmap repo info [repo]` — show details for a repo
//! - `rmap repo alias <repo> <alias>` — set or change alias
//! - `rmap repo remove <repo> [--delete-db]` — remove repo from registry

use std::path::Path;
use std::process::ExitCode;

use crate::daemon_client::{DaemonClient, DaemonClientError};

/// Run the `rmap repo` command family.
pub fn run_repo(args: &[String]) -> ExitCode {
    if args.is_empty() {
        eprintln!("usage: rmap repo <subcommand> [args]");
        eprintln!();
        eprintln!("subcommands:");
        eprintln!("  list              List all registered repos");
        eprintln!("  info [repo]       Show details for a repo (default: cwd)");
        eprintln!("  alias <repo> <name>  Set or change alias");
        eprintln!("  remove <repo> [--delete-db]  Remove repo from registry");
        return ExitCode::from(1);
    }

    match args[0].as_str() {
        "list" => run_repo_list(&args[1..]),
        "info" => run_repo_info(&args[1..]),
        "alias" => run_repo_alias(&args[1..]),
        "remove" => run_repo_remove(&args[1..]),
        other => {
            eprintln!("error: unknown repo subcommand: {}", other);
            ExitCode::from(1)
        }
    }
}

/// Run `rmap repo list`.
fn run_repo_list(args: &[String]) -> ExitCode {
    let json_output = args.iter().any(|a| a == "--json");

    let mut client = match DaemonClient::new() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };

    match client.request("list_repos", None) {
        Ok(result) => {
            if json_output {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&result["repos"]).unwrap_or_default()
                );
                return ExitCode::SUCCESS;
            }

            let repos = match result["repos"].as_array() {
                Some(r) => r,
                None => {
                    eprintln!("no repos registered");
                    return ExitCode::SUCCESS;
                }
            };

            if repos.is_empty() {
                eprintln!("no repos registered");
                return ExitCode::SUCCESS;
            }

            // Print header
            println!("{:<20} {:<50} LAST INDEXED", "ALIAS", "PATH");
            println!("{}", "-".repeat(90));

            for repo in repos {
                let alias = repo["alias"].as_str().unwrap_or("-");
                let path = repo["canonical_path"].as_str().unwrap_or("?");
                let last_indexed = repo["last_indexed_at"]
                    .as_str()
                    .map(|s| &s[..19]) // Truncate to datetime without timezone
                    .unwrap_or("never");

                println!("{:<20} {:<50} {}", alias, path, last_indexed);
            }

            ExitCode::SUCCESS
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

/// Run `rmap repo info [repo] [--json]`.
fn run_repo_info(args: &[String]) -> ExitCode {
    // Parse args: optional --json flag and repo reference
    let json_output = args.iter().any(|a| a == "--json");
    let repo_ref = args
        .iter()
        .find(|a| !a.starts_with("--"))
        .cloned()
        .unwrap_or_else(|| {
            std::env::current_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| ".".to_string())
        });

    if repo_ref == "." {
        if let Err(e) = std::env::current_dir() {
            eprintln!("error: cannot get current directory: {}", e);
            return ExitCode::from(2);
        }
    }

    let mut client = match DaemonClient::new() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };

    let params = serde_json::json!({"repo": repo_ref});

    match client.request("repo_info", Some(params)) {
        Ok(result) => {
            if json_output {
                // JSON mode: output full result for machine consumption
                println!(
                    "{}",
                    serde_json::to_string_pretty(&result).unwrap_or_default()
                );
                return ExitCode::SUCCESS;
            }

            // Human mode: show user-facing information only
            // Internal storage identifiers (repo_uid, db_path, snapshot_uid) are hidden
            let path = result["canonical_path"].as_str().unwrap_or("?");
            let alias = result["alias"].as_str();
            let last_indexed = result["last_indexed_at"].as_str().unwrap_or("never");
            let loaded = result["loaded"].as_bool().unwrap_or(false);

            println!("Repo: {}", path);
            if let Some(a) = alias {
                println!("Alias: {}", a);
            }
            println!("Last indexed: {}", last_indexed);
            println!("Loaded: {}", if loaded { "yes" } else { "no" });

            // DAEMON-VISIBILITY-1 (F): per-snapshot state + outcome + repo storage size. Internal
            // identifiers (snapshot_uid) stay hidden; STATE/OUTCOME are first-class facts.
            if let Some(storage) = result.get("storage") {
                print_repo_storage(storage);
            }

            ExitCode::SUCCESS
        }
        Err(DaemonClientError::DaemonError { code, message, .. }) => {
            if code == "RepoNotFound" {
                eprintln!("error: repo not indexed: {}", repo_ref);
                eprintln!("hint: run 'rmap index {}' to index this repo", repo_ref);
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

/// DAEMON-VISIBILITY-1 (F): render the per-repo storage/snapshot facts for `rmap repo info`.
///
/// Shows the repo's on-disk size and each snapshot's reader-frame STATE + OUTCOME (READY /
/// interrupted). Internal identifiers (`snapshot_uid`) stay hidden per the REG-1 human-mode
/// convention. Short-circuits to an "in use by daemon" note during an active index (contract E).
fn print_repo_storage(storage: &serde_json::Value) {
    let size = storage
        .get("db_size_bytes")
        .and_then(|v| v.as_u64())
        .map(format_bytes);

    if storage
        .get("in_use_by_daemon")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        let verb = match storage
            .get("operation")
            .and_then(|o| o.get("kind"))
            .and_then(|v| v.as_str())
        {
            Some("index") => "indexing",
            Some("refresh") => "refreshing",
            Some("enrich") => "enriching",
            _ => "using",
        };
        println!(
            "Storage: {} (daemon is {} this repo now — snapshot detail available after it completes)",
            size.as_deref().unwrap_or("?"),
            verb
        );
        return;
    }

    if let Some(reason) = storage.get("read_error").and_then(|v| v.as_str()) {
        println!(
            "Storage: {} (cannot read snapshots: {})",
            size.as_deref().unwrap_or("?"),
            reason
        );
        return;
    }

    if let Some(s) = &size {
        println!("Storage: {s}");
    }
    let snapshots = storage
        .get("snapshots")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    if snapshots.is_empty() {
        println!("Snapshots: none");
        return;
    }
    println!("Snapshots ({}):", snapshots.len());
    for snap in &snapshots {
        let state = snap.get("state").and_then(|v| v.as_str()).unwrap_or("?");
        let outcome = snap.get("outcome").and_then(|v| v.as_str()).unwrap_or("");
        let created = snap
            .get("created_at")
            .and_then(|v| v.as_str())
            .unwrap_or("?");
        println!("  - {state}: {outcome} (created {created})");
    }

    // PERSIST-RECURSION-1: honest degradation from the latest index — files skipped for
    // pathological AST nesting, or an isolated postpass failure. The reader-language lines are
    // computed daemon-side (snapshot_facts) and printed verbatim (same facts `rmap doctor` shows).
    if let Some(lines) = storage
        .get("extraction_degradations")
        .and_then(|d| d.get("lines"))
        .and_then(|v| v.as_array())
    {
        for line in lines.iter().filter_map(|l| l.as_str()) {
            println!("  ! {line}");
        }
    }
}

/// Humanise a byte count (GB/MB/KB) for `repo info` storage lines.
fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}

/// Run `rmap repo alias <repo> <alias>`.
fn run_repo_alias(args: &[String]) -> ExitCode {
    if args.len() != 2 {
        eprintln!("usage: rmap repo alias <repo_path> <alias>");
        return ExitCode::from(1);
    }

    let repo_path = &args[0];
    let alias = &args[1];

    // Canonicalize repo path
    let canonical = match Path::new(repo_path).canonicalize() {
        Ok(p) => p.to_string_lossy().to_string(),
        Err(e) => {
            eprintln!("error: cannot resolve path '{}': {}", repo_path, e);
            return ExitCode::from(2);
        }
    };

    let mut client = match DaemonClient::new() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };

    let params = serde_json::json!({
        "repo": canonical,
        "alias": alias,
    });

    match client.request("repo_alias", Some(params)) {
        Ok(result) => {
            let path = result["canonical_path"].as_str().unwrap_or("?");
            let set_alias = result["alias"].as_str().unwrap_or("?");
            eprintln!("Alias set: {} -> {}", set_alias, path);
            ExitCode::SUCCESS
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

/// Run `rmap repo remove <repo> [--delete-db]`.
fn run_repo_remove(args: &[String]) -> ExitCode {
    if args.is_empty() {
        eprintln!("usage: rmap repo remove <repo> [--delete-db]");
        return ExitCode::from(1);
    }

    let repo_ref = &args[0];
    let delete_db = args.iter().any(|a| a == "--delete-db");

    let mut client = match DaemonClient::new() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };

    let params = serde_json::json!({
        "repo": repo_ref,
        "delete_db": delete_db,
    });

    match client.request("repo_remove", Some(params)) {
        Ok(result) => {
            let path = result["canonical_path"].as_str().unwrap_or("?");
            let db_path = result["db_path"].as_str().unwrap_or("?");
            let db_deleted = result["db_deleted"].as_bool().unwrap_or(false);

            eprintln!("Removed from registry: {}", path);
            if db_deleted {
                eprintln!("Database deleted: {}", db_path);
            } else {
                eprintln!("Database retained: {}", db_path);
            }
            ExitCode::SUCCESS
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
