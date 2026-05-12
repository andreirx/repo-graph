//! Enrichment command family.
//!
//! Resolves receiver types for unresolved edges using language-specific
//! resolvers (rust-analyzer for Rust, tsserver for TypeScript, etc.).
//!
//! The CLI is the composition root: it instantiates concrete resolver
//! adapters and registers them with the pipeline. The pipeline owns
//! language grouping and dispatch logic.

use std::path::Path;
use std::process::ExitCode;

use enrichment::{EnrichmentConfig, EnrichmentLanguage, EnrichmentPipeline, ResolverRegistry};
use jdtls_resolver::{JdtlsConfig, JdtlsResolver};
use rust_analyzer_resolver::RustAnalyzerResolver;
use serde::Serialize;
use tsserver_resolver::TsServerResolver;

use crate::cli::open_storage;

/// Run the `rmap enrich` command.
///
/// Usage: `rmap enrich <db_path> <repo_uid> [options]`
///
/// Options:
///   --snapshot <uid>     Use specific snapshot (default: latest)
///   --language <lang>    Filter to specific language(s): rust, typescript, java
///   --limit <n>          Maximum edges to process
///   --promote            Promote enriched edges to resolved graph
///   --force              Re-enrich already enriched edges
///   --jdtls-path <path>  Path to jdtls executable (required for Java)
///
/// Environment:
///   JDTLS_PATH           Fallback path to jdtls if --jdtls-path not provided
///
/// Exit codes:
/// - 0: success
/// - 1: usage error
/// - 2: runtime error (DB error, missing repo/snapshot, no resolvers)
pub fn run_enrich(args: &[String]) -> ExitCode {
    let parsed = match parse_args(args) {
        Ok(p) => p,
        Err(msg) => {
            eprintln!("error: {}", msg);
            print_usage();
            return ExitCode::from(1);
        }
    };

    // Open storage
    let storage = match open_storage(Path::new(&parsed.db_path)) {
        Ok(s) => s,
        Err(msg) => {
            eprintln!("error: {}", msg);
            return ExitCode::from(2);
        }
    };

    // Get and validate snapshot (latest or specified)
    let snapshot_uid = match &parsed.snapshot_uid {
        Some(uid) => {
            // Explicit snapshot: validate existence, ownership, and status
            match storage.get_snapshot(uid) {
                Ok(Some(snap)) => {
                    if snap.repo_uid != parsed.repo_uid {
                        eprintln!(
                            "error: snapshot '{}' belongs to repo '{}', not '{}'",
                            uid, snap.repo_uid, parsed.repo_uid
                        );
                        return ExitCode::from(2);
                    }
                    if snap.status != "ready" {
                        eprintln!(
                            "error: snapshot '{}' is not ready (status: {})",
                            uid, snap.status
                        );
                        return ExitCode::from(2);
                    }
                    snap.snapshot_uid
                }
                Ok(None) => {
                    eprintln!("error: snapshot '{}' not found", uid);
                    return ExitCode::from(2);
                }
                Err(e) => {
                    eprintln!("error: failed to get snapshot: {}", e);
                    return ExitCode::from(2);
                }
            }
        }
        None => {
            // No explicit snapshot: use latest ready snapshot for repo
            match storage.get_latest_snapshot(&parsed.repo_uid) {
                Ok(Some(snap)) => {
                    if snap.status != "ready" {
                        eprintln!(
                            "error: latest snapshot for '{}' is not ready (status: {})",
                            parsed.repo_uid, snap.status
                        );
                        return ExitCode::from(2);
                    }
                    snap.snapshot_uid
                }
                Ok(None) => {
                    eprintln!("error: no snapshot found for repo '{}'", parsed.repo_uid);
                    return ExitCode::from(2);
                }
                Err(e) => {
                    eprintln!("error: failed to get latest snapshot: {}", e);
                    return ExitCode::from(2);
                }
            }
        }
    };

    // Build resolver registry
    // CLI instantiates concrete adapters - this is the composition root
    let mut registry = ResolverRegistry::new();
    let mut available_languages = Vec::new();

    // Register Rust resolver if not filtered out
    let should_register_rust =
        parsed.languages.is_empty() || parsed.languages.contains(&EnrichmentLanguage::Rust);

    if should_register_rust {
        // RustAnalyzerResolver::new() doesn't fail - it defers session creation to resolve_batch
        let resolver = RustAnalyzerResolver::new();
        registry.register(Box::new(resolver));
        available_languages.push(EnrichmentLanguage::Rust);
    }

    // Register TypeScript resolver if not filtered out
    let should_register_typescript =
        parsed.languages.is_empty() || parsed.languages.contains(&EnrichmentLanguage::TypeScript);

    if should_register_typescript {
        // TsServerResolver::new() doesn't fail - it defers session creation to resolve_batch
        let resolver = TsServerResolver::new();
        registry.register(Box::new(resolver));
        available_languages.push(EnrichmentLanguage::TypeScript);
    }

    // Register Java resolver if not filtered out
    // jdtls requires explicit path: --jdtls-path flag or JDTLS_PATH env var
    let should_register_java =
        parsed.languages.is_empty() || parsed.languages.contains(&EnrichmentLanguage::Java);

    if should_register_java {
        // Resolve jdtls path: CLI flag takes precedence over env var
        let jdtls_path = parsed
            .jdtls_path
            .clone()
            .or_else(|| std::env::var("JDTLS_PATH").ok());

        if let Some(path) = jdtls_path {
            let config = JdtlsConfig {
                jdtls_path: Some(path),
                ..Default::default()
            };
            let resolver = JdtlsResolver::with_config(config);
            registry.register(Box::new(resolver));
            available_languages.push(EnrichmentLanguage::Java);
        } else if parsed.languages.contains(&EnrichmentLanguage::Java) {
            // User explicitly requested Java but no jdtls path configured
            eprintln!("error: --language java requires --jdtls-path or JDTLS_PATH env var");
            return ExitCode::from(1);
        }
        // If no explicit --language java and no jdtls configured, silently skip Java
    }

    // Check that we have at least one resolver for requested languages
    if available_languages.is_empty() {
        eprintln!("error: no resolvers available for requested languages");
        return ExitCode::from(2);
    }

    // Build config
    let mut config = EnrichmentConfig::new();

    if let Some(limit) = parsed.limit {
        config = config.with_limit(limit);
    }

    if !parsed.languages.is_empty() {
        config = config.with_languages(parsed.languages.clone());
    }

    if parsed.force {
        config = config.with_force();
    }

    if parsed.promote {
        config = config.with_promotion();
    }

    if parsed.dry_run {
        config = config.with_dry_run();
    }

    // Run pipeline
    let mut pipeline = EnrichmentPipeline::with_registry(storage, registry);

    let report = match pipeline.run(&parsed.repo_uid, &snapshot_uid, &config) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: enrichment pipeline failed: {}", e);
            return ExitCode::from(2);
        }
    };

    // Build output
    let output = EnrichOutput {
        command: "enrich".to_string(),
        repo_uid: parsed.repo_uid.clone(),
        snapshot_uid: snapshot_uid.clone(),
        promote: parsed.promote,
        eligible_count: report.eligible_count,
        enriched_count: report.enriched_count,
        failed_count: report.failed_count,
        attempted_persist_count: report.attempted_persist_count(),
        persisted_count: report.persisted_count.unwrap_or(0),
        has_storage_discrepancy: report.has_storage_discrepancy(),
        enrichment_rate: report.enrichment_rate,
        promotion: report.promotion.as_ref().map(|p| PromotionOutput {
            candidates: p.candidates,
            promoted: p.promoted,
            persisted_count: p.persisted_count,
        }),
        by_language: report
            .by_language
            .iter()
            .map(|(lang, stats)| {
                (
                    format!("{:?}", lang).to_lowercase(),
                    LanguageStats {
                        eligible: stats.eligible,
                        enriched: stats.enriched,
                        failed: stats.failed,
                        rate: stats.rate,
                    },
                )
            })
            .collect(),
        top_failure_reasons: report
            .top_failure_reasons
            .iter()
            .take(10)
            .map(|fc| (fc.reason.clone(), fc.count))
            .collect(),
        top_types: report
            .top_types
            .iter()
            .take(10)
            .map(|tc| TypeOutput {
                type_name: tc.type_name.clone(),
                is_external: tc.is_external,
                count: tc.count,
            })
            .collect(),
        available_resolvers: available_languages
            .iter()
            .map(|l| format!("{:?}", l).to_lowercase())
            .collect(),
    };

    // JSON to stdout
    match serde_json::to_string_pretty(&output) {
        Ok(json) => {
            println!("{}", json);
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: failed to serialize output: {}", e);
            ExitCode::from(2)
        }
    }
}

fn print_usage() {
    eprintln!("usage: rmap enrich <db_path> <repo_uid> [options]");
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

struct ParsedArgs {
    db_path: String,
    repo_uid: String,
    snapshot_uid: Option<String>,
    languages: Vec<EnrichmentLanguage>,
    limit: Option<usize>,
    promote: bool,
    force: bool,
    dry_run: bool,
    jdtls_path: Option<String>,
}

fn parse_args(args: &[String]) -> Result<ParsedArgs, String> {
    if args.len() < 2 {
        return Err("missing required arguments".to_string());
    }

    let db_path = args[0].clone();
    let repo_uid = args[1].clone();

    let mut snapshot_uid = None;
    let mut languages = Vec::new();
    let mut limit = None;
    let mut promote = false;
    let mut force = false;
    let mut dry_run = false;
    let mut jdtls_path = None;

    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--snapshot" => {
                i += 1;
                if i >= args.len() {
                    return Err("--snapshot requires a value".to_string());
                }
                snapshot_uid = Some(args[i].clone());
            }
            "--language" => {
                i += 1;
                if i >= args.len() {
                    return Err("--language requires a value".to_string());
                }
                let lang = parse_language(&args[i])?;
                if !languages.contains(&lang) {
                    languages.push(lang);
                }
            }
            "--limit" => {
                i += 1;
                if i >= args.len() {
                    return Err("--limit requires a value".to_string());
                }
                limit = Some(
                    args[i]
                        .parse()
                        .map_err(|_| format!("invalid limit: {}", args[i]))?,
                );
            }
            "--jdtls-path" => {
                i += 1;
                if i >= args.len() {
                    return Err("--jdtls-path requires a value".to_string());
                }
                jdtls_path = Some(args[i].clone());
            }
            "--promote" => {
                promote = true;
            }
            "--force" => {
                force = true;
            }
            "--dry-run" => {
                dry_run = true;
            }
            other => {
                return Err(format!("unknown option: {}", other));
            }
        }
        i += 1;
    }

    Ok(ParsedArgs {
        db_path,
        repo_uid,
        snapshot_uid,
        languages,
        limit,
        promote,
        force,
        dry_run,
        jdtls_path,
    })
}

fn parse_language(s: &str) -> Result<EnrichmentLanguage, String> {
    match s.to_lowercase().as_str() {
        "rust" | "rs" => Ok(EnrichmentLanguage::Rust),
        "typescript" | "ts" | "javascript" | "js" => Ok(EnrichmentLanguage::TypeScript),
        "java" => Ok(EnrichmentLanguage::Java),
        other => Err(format!("unknown language: {}", other)),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Output types
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct EnrichOutput {
    command: String,
    repo_uid: String,
    snapshot_uid: String,
    promote: bool,
    eligible_count: usize,
    enriched_count: usize,
    failed_count: usize,
    attempted_persist_count: usize,
    persisted_count: usize,
    has_storage_discrepancy: bool,
    enrichment_rate: f64,
    promotion: Option<PromotionOutput>,
    by_language: Vec<(String, LanguageStats)>,
    top_failure_reasons: Vec<(String, usize)>,
    top_types: Vec<TypeOutput>,
    available_resolvers: Vec<String>,
}

#[derive(Debug, Serialize)]
struct PromotionOutput {
    candidates: usize,
    promoted: usize,
    persisted_count: Option<usize>,
}

#[derive(Debug, Serialize)]
struct LanguageStats {
    eligible: usize,
    enriched: usize,
    failed: usize,
    rate: f64,
}

#[derive(Debug, Serialize)]
struct TypeOutput {
    type_name: String,
    is_external: bool,
    count: usize,
}
