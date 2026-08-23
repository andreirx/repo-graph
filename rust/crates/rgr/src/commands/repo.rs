//! Repo management commands (REG-1).
//!
//! Commands for managing the daemon's repo registry.
//!
//! ## Commands
//!
//! - `rmap repo list` — list all registered repos
//! - `rmap repo info [repo]` — show details for a repo
//! - `rmap repo alias <repo> <alias>` — set or change alias
//! - `rmap repo remove <repo> [--keep-db]` — forget repo (registry + database + `.rgr/`); FORGET-REPO-1

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
        eprintln!(
            "  remove <repo> [--keep-db]  Forget repo: registry + database + .rgr/ (destructive)"
        );
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

/// Run `rmap repo forget <repo> [--keep-db]` (aka `rmap repo remove`).
///
/// FORGET-REPO-1: FORGETS by default — removes the registry entry, evicts in-memory state, drops
/// the `db_runtimes` slot, and deletes `.db`/`-wal`/`-shm` + `<repo>/.rgr/`. Reports each artifact
/// `removed | absent | failed(<reason>)`; ANY `failed` → non-zero exit. `--keep-db` opts out and
/// keeps the DB file (printing where it stays). `--delete-db` is accepted as a no-op (deletion is
/// the default now). Refuses (nothing deleted) while an index/refresh is in flight.
/// Parsed `rmap repo remove` arguments (review-1 #1).
#[derive(Debug)]
struct RemoveArgs {
    repo_ref: String,
    /// `--keep-db`: opt out of the forget-by-default deletion (keep the `.db` file).
    keep_db: bool,
}

/// Parse `rmap repo remove` arguments STRICTLY (review-1 #1).
///
/// Accepts exactly one positional `<repo>`, the `--keep-db` opt-out, and the legacy `--delete-db`
/// (a no-op now that deletion is the default). Any other flag, a second positional, or a missing
/// repo is a hard error — so a typo can never silently perform the destructive default. `--keep-db`
/// together with the (contradictory) explicit `--delete-db` is rejected rather than silently picking
/// one.
fn parse_remove_args(args: &[String]) -> Result<RemoveArgs, String> {
    let mut repo: Option<String> = None;
    let mut keep_db = false;
    let mut delete_db = false;
    for arg in args {
        match arg.as_str() {
            "--keep-db" => keep_db = true,
            "--delete-db" => delete_db = true, // legacy muscle-memory flag; deletion is the default
            other if other.starts_with('-') => return Err(format!("unknown option: {other}")),
            positional => {
                if repo.is_some() {
                    return Err(format!(
                        "unexpected extra argument: {positional} (expected exactly one <repo>)"
                    ));
                }
                repo = Some(positional.to_string());
            }
        }
    }
    if keep_db && delete_db {
        return Err("--keep-db and --delete-db are contradictory; pass at most one".to_string());
    }
    let repo_ref = repo.ok_or_else(|| "missing <repo> argument".to_string())?;
    Ok(RemoveArgs { repo_ref, keep_db })
}

/// Usage for `rmap repo remove` (shared by the arg-error and `--help` paths).
fn print_remove_usage() {
    eprintln!("usage: rmap repo remove <repo> [--keep-db]");
    eprintln!("  Forgets repo X: removes the registry entry, in-memory state, the database");
    eprintln!("  (.db/-wal/-shm) and <repo>/.rgr/. --keep-db keeps the database file.");
    eprintln!("  --delete-db is accepted for muscle memory (deletion is the default).");
}

fn run_repo_remove(args: &[String]) -> ExitCode {
    if args.iter().any(|a| a == "-h" || a == "--help") {
        print_remove_usage();
        return ExitCode::SUCCESS;
    }
    // review-1 #1: parse strictly BEFORE any destructive action. An unrecognized flag or an extra
    // positional is a hard error — never a silent fall-through to the destructive default (e.g. a
    // `--keep-dbb` typo must NOT forget-and-delete the DB).
    let parsed = match parse_remove_args(args) {
        Ok(p) => p,
        Err(msg) => {
            eprintln!("error: {msg}");
            print_remove_usage();
            return ExitCode::from(1);
        }
    };
    let repo_ref = parsed.repo_ref;
    let keep_db = parsed.keep_db;

    let mut client = match DaemonClient::new() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };

    let params = serde_json::json!({
        "repo": repo_ref,
        "keep_db": keep_db,
    });

    match client.request("repo_remove", Some(params)) {
        Ok(result) => render_forget_result(&result),
        Err(DaemonClientError::DaemonError { code, message, .. }) => {
            // A refusal (in-flight write) comes back as a StateUnavailable error — surface it plainly.
            eprintln!("error: cannot forget repo: {}", message);
            let _ = code;
            ExitCode::from(2)
        }
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::from(2)
        }
    }
}

/// Render the per-artifact forget report and pick the exit code (non-zero on any `failed`).
fn render_forget_result(result: &serde_json::Value) -> ExitCode {
    let path = result["canonical_path"].as_str().unwrap_or("?");
    let db_path = result["db_path"].as_str().unwrap_or("?");
    let kept_db = result["kept_db"].as_bool().unwrap_or(false);
    let ok = result["ok"].as_bool().unwrap_or(false);

    eprintln!("Forgot repo: {}", path);
    if let Some(artifacts) = result["artifacts"].as_array() {
        for a in artifacts {
            let kind = a["kind"].as_str().unwrap_or("?");
            let status = a["status"].as_str().unwrap_or("?");
            let artifact = a["artifact"].as_str().unwrap_or("");
            match status {
                "removed" => {
                    // `bytes: null` = size unknown (a sizing fault, named in size_error) — render
                    // it as unknown, NEVER as 0 or as nothing (unknown is never zero).
                    match a["bytes"].as_u64() {
                        Some(bytes) if bytes > 0 => {
                            eprintln!("  removed {kind}: {artifact} ({})", format_bytes(bytes))
                        }
                        Some(_) => eprintln!("  removed {kind}: {artifact}"),
                        None => {
                            let why = a["size_error"].as_str().unwrap_or("sizing failed");
                            eprintln!("  removed {kind}: {artifact} (size unknown — {why})")
                        }
                    }
                }
                "absent" => eprintln!("  absent  {kind}: {artifact} (nothing to remove)"),
                "failed" => {
                    let reason = a["reason"].as_str().unwrap_or("unknown error");
                    eprintln!("  FAILED  {kind}: {artifact} — {reason}");
                }
                other => eprintln!("  {other}  {kind}: {artifact}"),
            }
        }
    }
    if kept_db {
        eprintln!("Database retained (--keep-db): {}", db_path);
    }

    if ok {
        ExitCode::SUCCESS
    } else {
        eprintln!("error: one or more artifacts could not be removed (see FAILED lines above)");
        ExitCode::from(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    // review-1 #1: the happy paths — one repo, optional --keep-db, legacy --delete-db no-op.
    #[test]
    fn parse_remove_accepts_repo_and_known_flags() {
        let p = parse_remove_args(&args(&["/repo"])).unwrap();
        assert_eq!(p.repo_ref, "/repo");
        assert!(!p.keep_db, "forget-by-default: no --keep-db → delete");

        let p = parse_remove_args(&args(&["/repo", "--keep-db"])).unwrap();
        assert!(p.keep_db);
        // Order-independent.
        let p = parse_remove_args(&args(&["--keep-db", "/repo"])).unwrap();
        assert!(p.keep_db && p.repo_ref == "/repo");

        // Legacy --delete-db is a no-op (deletion is the default) — accepted, keep_db stays false.
        let p = parse_remove_args(&args(&["/repo", "--delete-db"])).unwrap();
        assert!(!p.keep_db);
    }

    // review-1 #1: THE bug — an unknown/typo flag must be REJECTED, never silently ignored so the
    // destructive default runs anyway.
    #[test]
    fn parse_remove_rejects_unknown_flag() {
        let err = parse_remove_args(&args(&["/repo", "--keep-dbb"])).unwrap_err();
        assert!(err.contains("unknown option"), "{err}");
        assert!(err.contains("--keep-dbb"), "{err}");
        // Any unrelated flag likewise.
        assert!(parse_remove_args(&args(&["/repo", "--force"])).is_err());
    }

    #[test]
    fn parse_remove_rejects_extra_positional() {
        let err = parse_remove_args(&args(&["/repo", "/other"])).unwrap_err();
        assert!(err.contains("extra argument"), "{err}");
    }

    #[test]
    fn parse_remove_requires_a_repo() {
        assert!(parse_remove_args(&args(&[]))
            .unwrap_err()
            .contains("missing"));
        assert!(parse_remove_args(&args(&["--keep-db"]))
            .unwrap_err()
            .contains("missing"));
    }

    #[test]
    fn parse_remove_rejects_contradictory_keep_and_delete() {
        let err = parse_remove_args(&args(&["/repo", "--keep-db", "--delete-db"])).unwrap_err();
        assert!(err.contains("contradictory"), "{err}");
    }
}
