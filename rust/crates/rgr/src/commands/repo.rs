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

use std::io::{IsTerminal, Write};
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
        eprintln!(
            "  rebuild <repo> [--yes]     Discard the store and reindex from scratch (destructive)"
        );
        return ExitCode::from(1);
    }

    match args[0].as_str() {
        "list" => run_repo_list(&args[1..]),
        "info" => run_repo_info(&args[1..]),
        "alias" => run_repo_alias(&args[1..]),
        "remove" => run_repo_remove(&args[1..]),
        "rebuild" => run_repo_rebuild(&args[1..]),
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

// ── repo rebuild (DAEMON-RESIDUALS-2C §7) ────────────────────────────────────────────────────────

/// Parsed `rmap repo rebuild` arguments.
#[derive(Debug)]
struct RebuildArgs {
    repo_ref: String,
    /// `--yes`: skip the interactive confirmation (required in a non-interactive session).
    yes: bool,
    /// `--json`: emit the daemon's rebuild result as JSON.
    json: bool,
    /// `--include-root <path>` (repeatable): C/C++ include roots for the reindex.
    include_roots: Vec<String>,
}

/// Parse `rmap repo rebuild` arguments STRICTLY: exactly one positional `<repo>`, plus the known
/// flags. An unknown flag or a second positional is a hard error so a typo can never slip past the
/// confirmation into the destructive default.
fn parse_rebuild_args(args: &[String]) -> Result<RebuildArgs, String> {
    let mut repo: Option<String> = None;
    let mut yes = false;
    let mut json = false;
    let mut include_roots: Vec<String> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--yes" | "-y" => yes = true,
            "--json" => json = true,
            "--include-root" => {
                if i + 1 >= args.len() {
                    return Err("--include-root requires a path argument".to_string());
                }
                include_roots.push(args[i + 1].clone());
                i += 1;
            }
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
        i += 1;
    }
    let repo_ref = repo.ok_or_else(|| "missing <repo> argument".to_string())?;
    Ok(RebuildArgs {
        repo_ref,
        yes,
        json,
        include_roots,
    })
}

fn print_rebuild_usage() {
    eprintln!("usage: rmap repo rebuild <repo> [--yes] [--json] [--include-root <path>]...");
    eprintln!("  Discards the repo's entire store and reindexes it from scratch.");
    eprintln!("  DISCARDED: every snapshot (incl. human baseline stamps), seed vectors,");
    eprintln!("             measurements, inferences, and the in-memory LiveGraph residency.");
    eprintln!("  KEPT:      the registry entry (path/alias/uid).");
    eprintln!(
        "  --yes skips the interactive confirmation (required in a non-interactive session)."
    );
}

/// The confirmation text — names exactly what is discarded and what is kept (§7). Returned as a
/// string so it renders identically in the prompt.
fn rebuild_confirmation_text(repo_ref: &str) -> String {
    format!(
        "About to REBUILD '{repo_ref}': this DISCARDS every snapshot (including human baseline \
         stamps), seed vectors, measurements, inferences, and the in-memory graph for this repo, \
         then reindexes it from scratch. The registry entry (path/alias) is KEPT."
    )
}

/// The required measurements a successful `repo_rebuild` reply must carry, for the human render.
/// Borrows the snapshot uid from the reply `Value` (lives for the render). `duration_secs` is the
/// verb's OWN timing (`rebuild.duration_secs`) and is genuinely optional — absent → the line is
/// omitted, never fabricated.
#[derive(Debug)]
struct RebuildReport<'a> {
    snapshot_uid: &'a str,
    files: u64,
    /// `nodes_total` = `COUNT(*)` over EVERY node kind (SYMBOL + FILE + MODULE …) — the superset
    /// `rmap index` renders as "nodes (all kinds)" (index.rs `format_index_summary`), NOT a symbol
    /// count. Kept as a required, honestly-labelled fact.
    nodes_total: u64,
    /// The REAL repo-level symbol `COUNT(*)` — `AgentStorageRead::compute_repo_summary().symbol_count`,
    /// the same figure `orient` shows — carried as the ADDITIVE `symbols_total` wire field (review-6
    /// item 2). Genuinely optional: the daemon OMITS it when the post-reindex repo summary could not be
    /// computed, and it is then rendered as a NAMED gap — never 0, and never `nodes_total` relabelled.
    symbols_total: Option<u64>,
    duration_secs: Option<u64>,
}

/// Validate that a `repo_rebuild` success reply carries the fields the verb reports. Returns a named
/// protocol error (no fabricated substitute) when a required field is missing or the wrong type — the
/// honest alternative to the prior `unwrap_or("unknown")`/`unwrap_or(0)` on a success path.
fn parse_rebuild_response(result: &serde_json::Value) -> Result<RebuildReport<'_>, String> {
    let malformed = |field: &str| {
        format!(
            "daemon returned a malformed rebuild response: missing or invalid `{field}` — the \
             rebuild's outcome cannot be reported (this is a protocol error, not a rebuild failure)"
        )
    };
    let snapshot_uid = result["snapshot_uid"]
        .as_str()
        .ok_or_else(|| malformed("snapshot_uid"))?;
    let files = result["files_total"]
        .as_u64()
        .ok_or_else(|| malformed("files_total"))?;
    let nodes_total = result["nodes_total"]
        .as_u64()
        .ok_or_else(|| malformed("nodes_total"))?;
    // Additive & genuinely optional (review-6 item 2): present ⇒ must be a number; absent ⇒ the daemon
    // could not compute the post-reindex repo summary → None (rendered as a NAMED gap, never fabricated
    // as 0 and never `nodes_total` relabelled "symbols").
    let symbols_total = match result.get("symbols_total") {
        None => None,
        Some(v) => Some(v.as_u64().ok_or_else(|| malformed("symbols_total"))?),
    };
    // Optional by contract: present ⇒ must be a number; absent ⇒ omit the duration line (not 0).
    let duration_secs = match result.get("rebuild").and_then(|r| r.get("duration_secs")) {
        None => None,
        Some(v) => Some(
            v.as_u64()
                .ok_or_else(|| malformed("rebuild.duration_secs"))?,
        ),
    };
    Ok(RebuildReport {
        snapshot_uid,
        files,
        nodes_total,
        symbols_total,
        duration_secs,
    })
}

fn run_repo_rebuild(args: &[String]) -> ExitCode {
    if args.iter().any(|a| a == "-h" || a == "--help") {
        print_rebuild_usage();
        return ExitCode::SUCCESS;
    }
    let parsed = match parse_rebuild_args(args) {
        Ok(p) => p,
        Err(msg) => {
            eprintln!("error: {msg}");
            print_rebuild_usage();
            return ExitCode::from(1);
        }
    };

    // Explicit intent. `--yes` skips the prompt; otherwise confirm interactively. In a
    // non-interactive session (no TTY) with no `--yes`, REFUSE rather than wipe silently.
    if !parsed.yes {
        if !std::io::stdin().is_terminal() {
            eprintln!(
                "error: refusing to rebuild without confirmation in a non-interactive session"
            );
            eprintln!(
                "hint: pass --yes to '{}' to confirm the destructive rebuild",
                parsed.repo_ref
            );
            return ExitCode::from(1);
        }
        eprintln!("{}", rebuild_confirmation_text(&parsed.repo_ref));
        eprint!("Type 'yes' to proceed: ");
        let _ = std::io::stderr().flush();
        let mut line = String::new();
        if std::io::stdin().read_line(&mut line).is_err() || line.trim() != "yes" {
            eprintln!("aborted: nothing was discarded");
            return ExitCode::from(1);
        }
    }

    // Resolve the ref: canonicalize a real on-disk path (so "." / relative paths match the registry's
    // canonical key), else pass it through as an alias.
    let repo_arg = match Path::new(&parsed.repo_ref).canonicalize() {
        Ok(p) => p.to_string_lossy().to_string(),
        Err(_) => parsed.repo_ref.clone(),
    };

    let mut client = match DaemonClient::new() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };

    let mut params = serde_json::json!({
        "repo": repo_arg,
        "confirm": true,
    });
    if !parsed.include_roots.is_empty() {
        params["include_roots"] = serde_json::json!(parsed.include_roots);
    }

    eprintln!(
        "rebuilding {} (progress: run `rmap doctor`)…",
        parsed.repo_ref
    );

    // Rebuild reindexes → a long op. Progress frames are consumed SILENTLY: the transport read loop
    // reads each frame purely to reset the stall deadline (the same long-op read timeout +
    // still-running classification as `rmap index`); nothing is rendered inline (review-6 item 3 removed
    // the no-op `--progress` flag — live progress is `rmap doctor`, per the message above).
    let mut on_progress = |_p: &serde_json::Value| {};
    match client.request_with_progress(
        "repo_rebuild",
        Some(params),
        crate::commands::index::long_op_read_timeout_secs(),
        &mut on_progress,
    ) {
        Ok(result) => {
            // The daemon returned success, but the reply must actually CARRY the measurements the
            // verb reports (the new snapshot uid, files, symbols). Validate them BEFORE claiming
            // success either way — never substitute a fabricated "unknown"/0 for a missing/malformed
            // field (STANDING HONESTY RULE 2). A contract-violating reply is a NAMED protocol error.
            let report = match parse_rebuild_response(&result) {
                Ok(r) => r,
                Err(msg) => {
                    eprintln!("error: {msg}");
                    return ExitCode::from(2);
                }
            };
            if parsed.json {
                // Emit the daemon's (validated) reply verbatim. A serialization failure is itself a
                // protocol error — never an empty body reported as success.
                match serde_json::to_string_pretty(&result) {
                    Ok(s) => {
                        println!("{s}");
                        return ExitCode::SUCCESS;
                    }
                    Err(e) => {
                        eprintln!("error: could not serialize the rebuild response: {e}");
                        return ExitCode::from(2);
                    }
                }
            }
            eprintln!("Rebuilt {}: reindexed from scratch", parsed.repo_ref);
            eprintln!("  discarded: all snapshots, baseline stamps, seed vectors, measurements, inferences, LiveGraph residency (registry entry kept)");
            eprintln!("  new snapshot: {}", report.snapshot_uid);
            eprintln!(
                "  files: {}, nodes (all kinds): {}",
                report.files, report.nodes_total
            );
            // The REAL symbol count (review-6 item 2), never `nodes_total` relabelled. A genuinely
            // uncomputed count is NAMED, never fabricated as 0.
            match report.symbols_total {
                Some(s) => eprintln!("  symbols: {s}"),
                None => eprintln!(
                    "  symbols: unavailable (the repo summary could not be computed after the reindex)"
                ),
            }
            if let Some(secs) = report.duration_secs {
                eprintln!("  duration: {secs}s");
            }
            ExitCode::SUCCESS
        }
        Err(DaemonClientError::Timeout { timeout_secs }) => {
            // A long rebuild that outlived the client read timeout is STILL RUNNING on the daemon —
            // not a failure (same contract-C classification as `rmap index`). Reuse its reporter.
            let canon = Path::new(&repo_arg).to_path_buf();
            crate::commands::index::report_long_op_timeout(&canon, "rebuild", timeout_secs)
        }
        Err(DaemonClientError::DaemonError { code, message, .. }) => {
            if code == "Busy" {
                eprintln!("error: cannot rebuild right now: {message}");
            } else if code == "RepoNotFound" {
                eprintln!("error: {message}");
            } else {
                eprintln!("error: daemon returned {code}: {message}");
            }
            ExitCode::from(2)
        }
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::from(2)
        }
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

    // ── repo rebuild ────────────────────────────────────────────────────────

    #[test]
    fn parse_rebuild_accepts_repo_and_flags() {
        let p = parse_rebuild_args(&args(&["/repo"])).unwrap();
        assert_eq!(p.repo_ref, "/repo");
        assert!(!p.yes && !p.json && p.include_roots.is_empty());

        let p = parse_rebuild_args(&args(&["/repo", "--yes", "--json"])).unwrap();
        assert!(p.yes && p.json);
        // Order-independent, `-y` alias, include-root repeatable.
        let p = parse_rebuild_args(&args(&["--yes", "/repo", "--include-root", "/inc"])).unwrap();
        assert!(p.yes && p.repo_ref == "/repo" && p.include_roots == vec!["/inc".to_string()]);
        let p = parse_rebuild_args(&args(&["-y", "/repo"])).unwrap();
        assert!(p.yes);
    }

    #[test]
    fn parse_rebuild_rejects_unknown_flag_and_extra_positional() {
        assert!(parse_rebuild_args(&args(&["/repo", "--force"]))
            .unwrap_err()
            .contains("unknown option"));
        assert!(parse_rebuild_args(&args(&["/a", "/b"]))
            .unwrap_err()
            .contains("extra argument"));
    }

    #[test]
    fn parse_rebuild_requires_a_repo_and_include_root_needs_value() {
        assert!(parse_rebuild_args(&args(&[]))
            .unwrap_err()
            .contains("missing"));
        assert!(parse_rebuild_args(&args(&["--yes"]))
            .unwrap_err()
            .contains("missing"));
        assert!(parse_rebuild_args(&args(&["/repo", "--include-root"]))
            .unwrap_err()
            .contains("requires a path"));
    }

    // The confirmation text must NAME what is discarded and that the registry entry is kept (§7).
    #[test]
    fn rebuild_confirmation_names_what_is_discarded_and_kept() {
        let t = rebuild_confirmation_text("my-repo");
        for needle in [
            "my-repo",
            "snapshot",
            "baseline",
            "seed vectors",
            "measurements",
            "inferences",
            "registry entry",
            "KEPT",
        ] {
            assert!(
                t.contains(needle),
                "confirmation text missing {needle:?}: {t}"
            );
        }
    }

    // review-0 item 2 + review-6 item 2: a complete reply parses; the human render reads the REAL symbol
    // count (`symbols_total`), NOT `nodes_total` relabelled. Both are carried and distinct here.
    #[test]
    fn parse_rebuild_response_accepts_a_complete_reply() {
        let v = serde_json::json!({
            "snapshot_uid": "snap-abc",
            "files_total": 133,
            "nodes_total": 2255,
            "symbols_total": 1977,
            "rebuild": { "duration_secs": 7 },
        });
        let r = parse_rebuild_response(&v).expect("complete reply parses");
        assert_eq!(r.snapshot_uid, "snap-abc");
        assert_eq!(r.files, 133);
        assert_eq!(r.nodes_total, 2255);
        assert_eq!(
            r.symbols_total,
            Some(1977),
            "the REAL symbol count is read from symbols_total, not nodes_total"
        );
        assert_eq!(r.duration_secs, Some(7));
    }

    // review-6 item 2: an ABSENT `symbols_total` (daemon could not compute the post-reindex summary) is
    // genuinely optional → None (rendered as a NAMED gap, never 0 and never nodes_total relabelled).
    #[test]
    fn parse_rebuild_response_treats_absent_symbols_total_as_none() {
        let v = serde_json::json!({
            "snapshot_uid": "s", "files_total": 1, "nodes_total": 2,
        });
        assert_eq!(parse_rebuild_response(&v).unwrap().symbols_total, None);
    }

    // A present-but-malformed `symbols_total` is a NAMED protocol error, never silently dropped to None
    // (honesty rule 2) — an additive field, when present, must still be a number.
    #[test]
    fn parse_rebuild_response_rejects_malformed_symbols_total() {
        let v = serde_json::json!({
            "snapshot_uid": "s", "files_total": 1, "nodes_total": 2, "symbols_total": "lots",
        });
        assert!(parse_rebuild_response(&v)
            .unwrap_err()
            .contains("symbols_total"));
    }

    // Absent duration is genuinely optional → None (the line is omitted, NOT rendered as 0s).
    #[test]
    fn parse_rebuild_response_treats_absent_duration_as_none() {
        let v = serde_json::json!({
            "snapshot_uid": "s", "files_total": 1, "nodes_total": 2,
        });
        assert_eq!(parse_rebuild_response(&v).unwrap().duration_secs, None);
    }

    // review-0 item 2: a reply missing/malforming a REQUIRED measurement is a NAMED protocol error —
    // never SUCCESS with a fabricated "unknown"/0.
    #[test]
    fn parse_rebuild_response_rejects_missing_or_malformed_required_fields() {
        // snapshot_uid absent.
        let v = serde_json::json!({ "files_total": 1, "nodes_total": 2 });
        let e = parse_rebuild_response(&v).unwrap_err();
        assert!(
            e.contains("snapshot_uid") && e.contains("protocol error"),
            "{e}"
        );

        // files_total wrong type.
        let v = serde_json::json!({ "snapshot_uid": "s", "files_total": "oops", "nodes_total": 2 });
        assert!(parse_rebuild_response(&v)
            .unwrap_err()
            .contains("files_total"));

        // nodes_total absent.
        let v = serde_json::json!({ "snapshot_uid": "s", "files_total": 1 });
        assert!(parse_rebuild_response(&v)
            .unwrap_err()
            .contains("nodes_total"));

        // duration present but wrong type → malformed (not silently dropped to None).
        let v = serde_json::json!({
            "snapshot_uid": "s", "files_total": 1, "nodes_total": 2,
            "rebuild": { "duration_secs": "later" },
        });
        assert!(parse_rebuild_response(&v)
            .unwrap_err()
            .contains("duration_secs"));
    }
}
