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

    if !client.is_available() {
        eprintln!(
            "{}",
            crate::daemon_client::daemon_unavailable_message(client.socket_path(), "repo list")
        );
        return ExitCode::from(2);
    }

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

    if !client.is_available() {
        eprintln!(
            "{}",
            crate::daemon_client::daemon_unavailable_message(client.socket_path(), "repo info")
        );
        return ExitCode::from(2);
    }

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

            ExitCode::SUCCESS
        }
        Err(DaemonClientError::DaemonError { code, message }) => {
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

    if !client.is_available() {
        eprintln!(
            "{}",
            crate::daemon_client::daemon_unavailable_message(client.socket_path(), "repo alias")
        );
        return ExitCode::from(2);
    }

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

    if !client.is_available() {
        eprintln!(
            "{}",
            crate::daemon_client::daemon_unavailable_message(client.socket_path(), "repo remove")
        );
        return ExitCode::from(2);
    }

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
