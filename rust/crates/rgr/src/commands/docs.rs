//! Docs command family.
//!
//! Documentation discovery and semantic fact extraction:
//! - `list` — documentation inventory (primary surface)
//! - `extract` — semantic fact extraction (secondary hints)
//!
//! Docs are primary; semantic_facts are secondary derived hints.
//!
//! # REG-1 Contract
//!
//! Both subcommands resolve the repo from cwd via daemon registry.
//! No explicit db_path or repo_uid arguments.
//!
//! # CLI-OUT-5 Output Contract
//!
//! - Human output by default
//! - `--json` for machine mode (raw daemon response)
//! - Deterministic ordering
//! - Full output, no truncation
//!
//! # Boundary rules
//!
//! This module owns docs command-family behavior:
//! - command handlers
//! - daemon request dispatch
//!
//! This module does **not** own:
//! - doc discovery (lives in `repo-graph-doc-facts` via daemon)
//! - semantic fact storage (lives in `repo-graph-storage` via daemon)

use std::process::ExitCode;

use crate::daemon_client::DaemonClient;
use crate::presentation::docs::{DocsExtractResponse, DocsListResponse};

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
    eprintln!(
        "  rmap docs list [--json] [--include-generated]  — documentation inventory (run from repo)"
    );
    eprintln!(
        "  rmap docs extract [--json]                     — extract semantic hints (run from repo)"
    );
}

/// Parse --json flag from args.
fn parse_json_flag(args: &[String]) -> (bool, Vec<&String>) {
    let mut json_mode = false;
    let mut remaining = Vec::new();

    for arg in args {
        if arg == "--json" {
            json_mode = true;
        } else {
            remaining.push(arg);
        }
    }

    (json_mode, remaining)
}

/// Parse `docs list` flags: `--json` and `--include-generated` (SELF-POLLUTION-1
/// §3). Returns `(json_mode, include_generated, unrecognized_args)`; any leftover
/// arg is a usage error at the call site.
fn parse_docs_list_flags(args: &[String]) -> (bool, bool, Vec<&String>) {
    let mut json_mode = false;
    let mut include_generated = false;
    let mut remaining = Vec::new();

    for arg in args {
        match arg.as_str() {
            "--json" => json_mode = true,
            "--include-generated" => include_generated = true,
            _ => remaining.push(arg),
        }
    }

    (json_mode, include_generated, remaining)
}

/// List documentation inventory (primary documentation surface).
///
/// REG-1 contract: resolves repo from cwd via daemon.
///
/// SELF-POLLUTION-1 §3: `--include-generated` opts rmap's OWN `map` sidecars back
/// into the listing (they are excluded by default so `docs list` shows the reader's
/// docs, not rmap's exhaust). The daemon always returns the FULL inventory; the
/// default-exclusion filter is applied in the presentation layer to BOTH surfaces —
/// `render_human` for text, `filtered_json_view` for `--json` (which reports the
/// excluded count as a machine-readable `excluded_generated` field, or passes the raw
/// daemon value through unchanged when nothing is filtered — byte-parity).
fn run_docs_list(args: &[String]) -> ExitCode {
    let (json_mode, include_generated, remaining) = parse_docs_list_flags(args);

    if !remaining.is_empty() {
        eprintln!("usage: rmap docs list [--json] [--include-generated]");
        eprintln!("       (run from within a repo directory)");
        return ExitCode::from(1);
    }

    // Get cwd for repo resolution
    let cwd = match std::env::current_dir() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: cannot determine current directory: {}", e);
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

    // Connect to daemon
    let mut client = match DaemonClient::new() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };

    // Send docs_list request with repo path
    let params = serde_json::json!({ "repo": repo_path });
    match client.request("docs_list", Some(params)) {
        Ok(result) => {
            // Parse the FULL daemon inventory, then apply the SAME §2.3 default-
            // exclusion filter to whichever surface we render. For `--json`, when
            // nothing is filtered (no generated maps, or --include-generated) we print
            // the RAW daemon result UNCHANGED — byte-parity with the pre-slice output
            // (review-5 finding 1). Only when rmap's maps are actually excluded do we
            // emit the filtered view + machine-readable excluded/unreadable counts, so a
            // machine consumer gets the same honesty the human does.
            match serde_json::from_value::<DocsListResponse>(result.clone()) {
                Ok(response) => {
                    if json_mode {
                        let out_value = response
                            .filtered_json_view(include_generated)
                            .unwrap_or(result);
                        match serde_json::to_string_pretty(&out_value) {
                            Ok(json) => {
                                println!("{}", json);
                                ExitCode::SUCCESS
                            }
                            Err(e) => {
                                eprintln!("error: failed to serialize result: {}", e);
                                ExitCode::from(2)
                            }
                        }
                    } else {
                        print!("{}", response.render_human(include_generated));
                        ExitCode::SUCCESS
                    }
                }
                Err(e) => {
                    eprintln!("error: failed to parse response: {}", e);
                    ExitCode::from(2)
                }
            }
        }
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::from(2)
        }
    }
}

/// Extract semantic facts from documentation (secondary hints).
///
/// REG-1 contract: resolves repo from cwd via daemon.
fn run_docs_extract(args: &[String]) -> ExitCode {
    let (json_mode, remaining) = parse_json_flag(args);

    if !remaining.is_empty() {
        eprintln!("usage: rmap docs extract [--json]");
        eprintln!("       (run from within a repo directory)");
        return ExitCode::from(1);
    }

    // Get cwd for repo resolution
    let cwd = match std::env::current_dir() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: cannot determine current directory: {}", e);
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

    // Connect to daemon
    let mut client = match DaemonClient::new() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };

    // Send docs_extract request with repo path
    let params = serde_json::json!({ "repo": repo_path });
    match client.request("docs_extract", Some(params)) {
        Ok(result) => {
            if json_mode {
                // Raw JSON output
                match serde_json::to_string_pretty(&result) {
                    Ok(json) => {
                        println!("{}", json);
                        ExitCode::SUCCESS
                    }
                    Err(e) => {
                        eprintln!("error: failed to serialize result: {}", e);
                        ExitCode::from(2)
                    }
                }
            } else {
                // Human-readable output
                match serde_json::from_value::<DocsExtractResponse>(result) {
                    Ok(response) => {
                        print!("{}", response.render_human());
                        ExitCode::SUCCESS
                    }
                    Err(e) => {
                        eprintln!("error: failed to parse response: {}", e);
                        ExitCode::from(2)
                    }
                }
            }
        }
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::from(2)
        }
    }
}
