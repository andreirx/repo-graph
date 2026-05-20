//! Churn command.
//!
//! RS-MS-2: Query-time per-file git churn for indexed files.
//! No persistence. Git is the authoritative history source.
//!
//! # CLI-OUT-6 Output Contract
//!
//! - Human output by default
//! - `--json` for machine mode (raw JSON)
//! - Deterministic ordering (by lines_changed desc, then path asc)
//! - Full output, no truncation
//!
//! # Legacy Contract Exception
//!
//! This command uses legacy direct-storage contract (explicit db_path/repo_uid),
//! not REG-1 daemon contract.
//!
//! # Boundary rules
//!
//! This module owns churn command behavior:
//! - `run_churn` handler
//! - `parse_churn_args` (also used by risk command)
//! - `ChurnRow` DTO
//!
//! This module does **not** own:
//! - shared infrastructure (lives in `crate::cli`)
//! - git churn extraction (belongs in `repo-graph-git`)

use std::path::Path;
use std::process::ExitCode;

use crate::cli::{build_envelope, open_storage, resolve_repo_root};

// ── churn command ────────────────────────────────────────────────

/// Output row for churn command.
#[derive(serde::Serialize)]
pub(super) struct ChurnRow {
    file_path: String,
    commit_count: u64,
    lines_changed: u64,
}

pub fn run_churn(args: &[String]) -> ExitCode {
    // Parse args: <db_path> <repo_uid> [--since <expr>] [--json]
    // Default --since: 90.days.ago
    let parsed = match parse_since_args(args) {
        Ok(p) => p,
        Err(e) => {
            match e {
                SinceArgsError::MissingArgs => {
                    eprintln!("usage: rmap churn <db_path> <repo_uid> [--since <expr>] [--json]");
                }
                SinceArgsError::SinceMissingValue => {
                    eprintln!("error: --since requires a value");
                }
                SinceArgsError::UnknownArgument(arg) => {
                    eprintln!("error: unknown argument: {}", arg);
                }
            }
            return ExitCode::from(1);
        }
    };

    let db_path = parsed.db_path;
    let repo_uid = parsed.repo_uid;
    let since = parsed.since;
    let json_mode = parsed.json_mode;

    let storage = match open_storage(db_path) {
        Ok(s) => s,
        Err(msg) => {
            eprintln!("error: {}", msg);
            return ExitCode::from(2);
        }
    };

    let snapshot = match storage.get_latest_snapshot(repo_uid) {
        Ok(Some(snap)) => snap,
        Ok(None) => {
            eprintln!("error: no snapshot found for repo '{}'", repo_uid);
            return ExitCode::from(2);
        }
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };

    // Get repo for root_path (needed to invoke git)
    use repo_graph_storage::types::RepoRef;
    let repo = match storage.get_repo(&RepoRef::Uid(repo_uid.to_string())) {
        Ok(Some(r)) => r,
        Ok(None) => {
            eprintln!("error: repo not found: {}", repo_uid);
            return ExitCode::from(2);
        }
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };

    // Get indexed files for filtering
    let indexed_files = match storage.get_files_by_repo(repo_uid) {
        Ok(files) => files,
        Err(e) => {
            eprintln!("error: failed to read indexed files: {}", e);
            return ExitCode::from(2);
        }
    };

    let indexed_paths: std::collections::HashSet<&str> =
        indexed_files.iter().map(|f| f.path.as_str()).collect();

    // Call git crate for churn
    use repo_graph_git::{get_file_churn, ChurnWindow};
    let window = ChurnWindow::new(&since);

    // Resolve repo root relative to DB location (cwd-independent)
    let repo_path = match resolve_repo_root(db_path, &repo.root_path) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };

    let raw_churn = match get_file_churn(&repo_path, &window) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: git churn failed: {}", e);
            return ExitCode::from(2);
        }
    };

    // Filter to indexed files only, preserving git crate ordering
    let results: Vec<ChurnRow> = raw_churn
        .into_iter()
        .filter(|entry| indexed_paths.contains(entry.file_path.as_str()))
        .map(|entry| ChurnRow {
            file_path: entry.file_path,
            commit_count: entry.commit_count,
            lines_changed: entry.lines_changed,
        })
        .collect();

    // Build envelope with extra `since` field
    let count = results.len();
    let mut extra = serde_json::Map::new();
    extra.insert(
        "since".to_string(),
        serde_json::Value::String(since.clone()),
    );

    let output = match build_envelope(
        &storage,
        "churn",
        repo_uid,
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

    if json_mode {
        // Raw JSON output
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
    } else {
        // Human-readable output
        use crate::presentation::churn::ChurnResponse;

        match serde_json::from_value::<ChurnResponse>(output) {
            Ok(response) => {
                print!("{}", response.render_human());
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("error: failed to parse response for rendering: {}", e);
                ExitCode::from(2)
            }
        }
    }
}

/// Typed parse error for since-window commands.
///
/// Allows callers to provide their own usage strings without
/// string inspection.
#[derive(Debug, Clone, PartialEq)]
pub enum SinceArgsError {
    /// Missing required positional args (db_path, repo_uid).
    MissingArgs,
    /// --since flag without value.
    SinceMissingValue,
    /// Unknown argument encountered.
    UnknownArgument(String),
}

/// Parsed result from since-window commands.
pub struct SinceArgs<'a> {
    pub db_path: &'a Path,
    pub repo_uid: &'a str,
    pub since: String,
    pub json_mode: bool,
}

/// Parse args for commands with `<db_path> <repo_uid> [--since <expr>] [--json]` signature.
///
/// Used by churn and risk commands (same arg signature).
/// Returns typed errors so each caller can provide its own usage string.
pub fn parse_since_args(args: &[String]) -> Result<SinceArgs<'_>, SinceArgsError> {
    if args.len() < 2 {
        return Err(SinceArgsError::MissingArgs);
    }

    let db_path = Path::new(&args[0]);
    let repo_uid = &args[1];

    // Default window
    let mut since = "90.days.ago".to_string();
    let mut json_mode = false;

    // Parse optional flags
    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--since" => {
                if i + 1 >= args.len() {
                    return Err(SinceArgsError::SinceMissingValue);
                }
                since = args[i + 1].clone();
                i += 2;
            }
            "--json" => {
                json_mode = true;
                i += 1;
            }
            _ => {
                return Err(SinceArgsError::UnknownArgument(args[i].clone()));
            }
        }
    }

    Ok(SinceArgs {
        db_path,
        repo_uid,
        since,
        json_mode,
    })
}
