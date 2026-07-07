//! Enrichment command family.
//!
//! Resolves receiver types for unresolved edges using language-specific resolvers
//! (rust-analyzer for Rust, tsserver for TypeScript, jdtls for Java) and promotes the safe subset to
//! resolved call-graph edges.
//!
//! # REG-1 contract (ENRICH-LIFECYCLE-1 §3.6)
//!
//! Like every other command, `rmap enrich` resolves the repo from the current working directory via
//! the daemon registry — the daemon owns storage and runs the pipeline (CLAUDE.md architecture rule
//! #8: clients never open storage directly). Auto-enrichment already runs after every index/refresh;
//! this manual form is the on-demand top-up (and now legal — the identifiers REG-1 hid are no longer
//! required). The legacy positional `rmap enrich <db_path> <repo_uid>` form keeps working for
//! compatibility but is out of `--help`.

use std::process::ExitCode;

use crate::daemon_client::{DaemonClient, DaemonClientError};

/// Per-read stall timeout for the (potentially long, LSP-backed) enrich op, in seconds. Progress
/// frames reset this deadline. Shares the index/refresh override env var for one consistent knob.
const ENRICH_READ_TIMEOUT_SECS: u64 = 300;

fn enrich_read_timeout_secs() -> u64 {
    match std::env::var("RMAP_LONG_OP_READ_TIMEOUT_SECS") {
        Ok(v) => v.parse::<u64>().unwrap_or(ENRICH_READ_TIMEOUT_SECS).max(1),
        Err(_) => ENRICH_READ_TIMEOUT_SECS,
    }
}

/// Run the `rmap enrich` command.
///
/// Usage: `rmap enrich [options]` (repo resolved from cwd) — or the legacy
/// `rmap enrich <db_path> <repo_uid> [options]` for compatibility.
///
/// Exit codes:
/// - 0: success (or the op is progressing detached after a client read-timeout)
/// - 1: usage error
/// - 2: runtime error (daemon unavailable, repo not indexed, enrichment failure)
pub fn run_enrich(args: &[String]) -> ExitCode {
    let parsed = match parse_args(args) {
        Ok(p) => p,
        Err(msg) => {
            eprintln!("error: {}", msg);
            print_usage();
            return ExitCode::from(1);
        }
    };

    // Build the request params. Either the REG-1 `repo` (cwd path — the registry resolves it,
    // including aliases) or the legacy positional identifiers.
    let mut params = serde_json::Map::new();
    match &parsed.target {
        Target::Cwd => {
            let cwd = match std::env::current_dir().and_then(|p| p.canonicalize()) {
                Ok(p) => p.to_string_lossy().to_string(),
                Err(e) => {
                    eprintln!("error: cannot resolve current directory: {}", e);
                    return ExitCode::from(2);
                }
            };
            params.insert("repo".to_string(), serde_json::json!(cwd));
        }
        Target::Legacy { db_path, repo_uid } => {
            params.insert("db_path".to_string(), serde_json::json!(db_path));
            params.insert("repo_uid".to_string(), serde_json::json!(repo_uid));
        }
    }
    if let Some(uid) = &parsed.snapshot_uid {
        params.insert("snapshot_uid".to_string(), serde_json::json!(uid));
    }
    if !parsed.languages.is_empty() {
        params.insert("languages".to_string(), serde_json::json!(parsed.languages));
    }
    if let Some(limit) = parsed.limit {
        params.insert("limit".to_string(), serde_json::json!(limit));
    }
    if parsed.promote {
        params.insert("promote".to_string(), serde_json::json!(true));
    }
    if parsed.force {
        params.insert("force".to_string(), serde_json::json!(true));
    }
    if parsed.dry_run {
        params.insert("dry_run".to_string(), serde_json::json!(true));
    }
    if let Some(path) = &parsed.jdtls_path {
        params.insert("jdtls_path".to_string(), serde_json::json!(path));
    }

    let mut client = match DaemonClient::new() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(2);
        }
    };

    // Enrich emits coarse phase progress ("initializing"/"resolving"/"complete") — render it so a
    // long LSP-backed pass is not a silent block, mirroring index/refresh.
    let mut on_progress = |frame: &serde_json::Value| {
        if let Some(phase) = frame.get("phase").and_then(|v| v.as_str()) {
            eprintln!("  {phase}...");
        }
    };

    match client.request_with_progress(
        "enrich",
        Some(serde_json::Value::Object(params)),
        enrich_read_timeout_secs(),
        &mut on_progress,
    ) {
        Ok(result) => match serde_json::to_string_pretty(&result) {
            Ok(json) => {
                println!("{}", json);
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("error: failed to serialize result: {}", e);
                ExitCode::from(2)
            }
        },
        // Detached completion (INDEX-DISCONNECT-1 semantics): a client read-timeout does NOT abort
        // the enrich — the daemon runs it to completion. Report honestly and point at doctor.
        Err(DaemonClientError::Timeout { timeout_secs }) => {
            eprintln!(
                "note: enrich is still running on the daemon (client read timed out after {timeout_secs}s);"
            );
            eprintln!("      it continues detached — check `rmap doctor` for the result.");
            ExitCode::SUCCESS
        }
        Err(DaemonClientError::DaemonError { code, message, .. }) => {
            if code == "RepoNotFound" {
                eprintln!("error: repo not indexed");
                eprintln!("hint: run 'rmap index .' to index this repo");
            } else {
                eprintln!("error: {}: {}", code, message);
            }
            ExitCode::from(2)
        }
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::from(2)
        }
    }
}

fn print_usage() {
    eprintln!("usage: rmap enrich [options]");
    eprintln!();
    eprintln!("Resolves receiver types for unresolved edges and promotes the safe subset to");
    eprintln!(
        "resolved call-graph edges. Repository is resolved from the current working directory."
    );
    eprintln!("(Auto-enrichment runs after every index/refresh; this is the on-demand top-up.)");
    eprintln!();
    eprintln!("options:");
    eprintln!("  --snapshot <uid>     Use specific snapshot (default: latest)");
    eprintln!("  --language <lang>    Filter to language: rust, typescript, java");
    eprintln!("  --limit <n>          Maximum edges to process");
    eprintln!("  --promote            Promote enriched edges to resolved graph");
    eprintln!("  --force              Re-enrich already enriched edges");
    eprintln!("  --dry-run            Resolve types but do not persist to database");
    eprintln!("  --jdtls-path <path>  Path to jdtls executable (or set JDTLS_PATH env var)");
}

// ─────────────────────────────────────────────────────────────────────────────
// Argument parsing
// ─────────────────────────────────────────────────────────────────────────────

/// Which repo the enrich targets: cwd (REG-1) or the legacy positional identifiers.
enum Target {
    Cwd,
    Legacy { db_path: String, repo_uid: String },
}

struct ParsedArgs {
    target: Target,
    snapshot_uid: Option<String>,
    /// Canonical lowercase language tokens (`rust` / `typescript` / `java`).
    languages: Vec<String>,
    limit: Option<usize>,
    promote: bool,
    force: bool,
    dry_run: bool,
    jdtls_path: Option<String>,
}

fn parse_args(args: &[String]) -> Result<ParsedArgs, String> {
    let mut positionals: Vec<String> = Vec::new();
    let mut snapshot_uid = None;
    let mut languages: Vec<String> = Vec::new();
    let mut limit = None;
    let mut promote = false;
    let mut force = false;
    let mut dry_run = false;
    let mut jdtls_path = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--snapshot" => {
                i += 1;
                snapshot_uid = Some(arg_value(args, i, "--snapshot")?);
            }
            "--language" => {
                i += 1;
                let lang = canonical_language(&arg_value(args, i, "--language")?)?;
                if !languages.contains(&lang) {
                    languages.push(lang);
                }
            }
            "--limit" => {
                i += 1;
                let v = arg_value(args, i, "--limit")?;
                limit = Some(v.parse().map_err(|_| format!("invalid limit: {}", v))?);
            }
            "--jdtls-path" => {
                i += 1;
                jdtls_path = Some(arg_value(args, i, "--jdtls-path")?);
            }
            "--promote" => promote = true,
            "--force" => force = true,
            "--dry-run" => dry_run = true,
            flag if flag.starts_with("--") => {
                return Err(format!("unknown option: {}", flag));
            }
            other => positionals.push(other.to_string()),
        }
        i += 1;
    }

    let target = match positionals.len() {
        0 => Target::Cwd,
        2 => Target::Legacy {
            db_path: positionals[0].clone(),
            repo_uid: positionals[1].clone(),
        },
        1 => {
            return Err(
                "a single positional is ambiguous: run `rmap enrich` from the repo (cwd), or pass the legacy `<db_path> <repo_uid>`".to_string(),
            )
        }
        n => return Err(format!("too many positional arguments ({n})")),
    };

    Ok(ParsedArgs {
        target,
        snapshot_uid,
        languages,
        limit,
        promote,
        force,
        dry_run,
        jdtls_path,
    })
}

fn arg_value(args: &[String], i: usize, flag: &str) -> Result<String, String> {
    args.get(i)
        .cloned()
        .ok_or_else(|| format!("{flag} requires a value"))
}

/// Normalize a language token to the canonical lowercase form the daemon accepts.
fn canonical_language(s: &str) -> Result<String, String> {
    match s.to_lowercase().as_str() {
        "rust" | "rs" => Ok("rust".to_string()),
        "typescript" | "ts" | "javascript" | "js" => Ok("typescript".to_string()),
        "java" => Ok("java".to_string()),
        other => Err(format!("unknown language: {}", other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cwd_form_takes_no_positionals() {
        let p = parse_args(&["--promote".to_string()]).unwrap();
        assert!(matches!(p.target, Target::Cwd));
        assert!(p.promote);
    }

    #[test]
    fn legacy_form_takes_two_positionals() {
        let p = parse_args(&[
            "/db/x.db".to_string(),
            "repo-uid".to_string(),
            "--force".to_string(),
        ])
        .unwrap();
        match p.target {
            Target::Legacy { db_path, repo_uid } => {
                assert_eq!(db_path, "/db/x.db");
                assert_eq!(repo_uid, "repo-uid");
            }
            _ => panic!("expected legacy form"),
        }
        assert!(p.force);
    }

    #[test]
    fn one_positional_is_a_usage_error() {
        assert!(parse_args(&["justone".to_string()]).is_err());
    }

    #[test]
    fn language_tokens_are_canonicalized_and_deduped() {
        let p = parse_args(&[
            "--language".to_string(),
            "rs".to_string(),
            "--language".to_string(),
            "rust".to_string(),
            "--language".to_string(),
            "ts".to_string(),
        ])
        .unwrap();
        assert_eq!(
            p.languages,
            vec!["rust".to_string(), "typescript".to_string()]
        );
    }
}
