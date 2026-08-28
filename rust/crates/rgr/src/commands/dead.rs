//! Dead command.
//!
//! Dead-code detection surface (currently disabled).
//!
//! # Boundary rules
//!
//! This module owns dead command behavior:
//! - command handler
//! - dead-specific DTOs
//! - dead output shaping
//!
//! This module does **not** own:
//! - shared infrastructure (lives in `crate::cli`)
//! - dead-node queries (lives in `repo-graph-storage`)
//! - trust assessment (lives in `repo-graph-trust`)

use std::process::ExitCode;

use crate::commands::dead_render;
use crate::daemon_client::DaemonClient;

/// CLI output DTO for dead-code results with per-result trust.
///
/// Wraps the storage DeadNodeResult and adds a local trust section.
/// Every dead result carries explicit confidence — no Option A hiding.
///
/// NOTE: Struct is kept for reintroduction of the dead-code surface.
/// The `dead` command is currently disabled; see run_dead() comment.
#[allow(dead_code)]
#[derive(serde::Serialize)]
struct DeadNodeOutput {
    stable_key: String,
    symbol: String,
    kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    subtype: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    line: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    line_count: Option<i64>,
    is_test: bool,
    /// Per-result dead-code confidence.
    trust: repo_graph_trust::DeadResultTrust,
}

pub fn run_dead(_args: &[String]) -> ExitCode {
    // ══════════════════════════════════════════════════════════════════
    // DELIBERATELY DISABLED — 2026-04-27 (disable decision + exit code 2 FROZEN).
    //
    // The `dead` command refuses because current signal quality produces
    // 85-95% false-positive rates on real codebases; a misleading "dead"
    // label is worse than no label. That decision is NOT re-litigated here.
    //
    // DEAD-CAUSES-1 (2026-08-28): what the refusal SAYS about *why* the
    // signal is unreliable was stale — it transcribed a 2026-04 static list
    // ("Missing framework detectors (Spring, React, …)") that the reader's
    // OWN snapshot often disproves (glamCRM carries 212 React + 14 Spring
    // liveness inferences). The "Root causes" section is now DERIVED from the
    // snapshot via the `dead_causes` daemon arm; when it cannot be derived
    // (daemon down / repo not indexed / read failure) a generic list is shown
    // under an explicit "not derived" label with the reason.
    //
    // The underlying substrate is preserved (storage::find_dead_nodes,
    // trust::assess_dead_confidence). Reintroduction as `orphans` + a
    // stronger evidence-backed `dead` is future work (see docs/TECH-DEBT.md).
    // ══════════════════════════════════════════════════════════════════

    eprintln!("error: `rmap dead` is disabled");
    eprintln!();
    eprintln!("Dead-code detection is not available in rmap because current");
    eprintln!("signal quality produces high false-positive rates (85-95% on");
    eprintln!("real codebases). Using this output would mislead agents into");
    eprintln!("investigating or deleting live code.");
    eprintln!();

    // Derive the "Root causes" from the reader's snapshot; fall back to a
    // labelled generic list (with the reason) when it cannot be derived.
    match fetch_dead_causes() {
        Ok(facts) => eprint!("{}", dead_render::render_derived(&facts)),
        Err(reason) => eprint!("{}", dead_render::render_generic(&reason)),
    }

    eprintln!();
    eprintln!("Alternative discovery commands that work:");
    eprintln!("  rmap callers  - trace who calls a symbol");
    eprintln!("  rmap callees  - trace what a symbol calls");
    eprintln!("  rmap imports  - trace file imports");
    eprintln!("  rmap orient   - repo overview with trust signals");
    eprintln!("  rmap trust    - detailed reliability report");
    eprintln!();
    eprintln!("Dead-code surface will be reintroduced when framework-liveness,");
    eprintln!("entrypoint, and coverage evidence are wired into deadness scoring");
    eprintln!("(such evidence may already exist in the snapshot but is not yet");
    eprintln!("consumed by any deadness verdict).");

    ExitCode::from(2)
}

/// Resolve the cwd's repo and ask the daemon for the derived cause facts.
///
/// Returns the parsed facts, or a reader-facing reason string (daemon unreachable,
/// repo not indexed, read failure, malformed payload) for the generic fallback. No
/// result is defaulted: every failure carries its reason (STANDING HONESTY RULE 1).
fn fetch_dead_causes() -> Result<dead_render::DeadCausesFacts, String> {
    let cwd = std::env::current_dir().map_err(|e| format!("cannot determine cwd: {e}"))?;
    let repo_path = cwd
        .canonicalize()
        .map_err(|e| format!("cannot canonicalize cwd: {e}"))?
        .to_string_lossy()
        .to_string();

    let mut client = DaemonClient::new().map_err(|e| e.to_string())?;
    let params = serde_json::json!({ "repo": repo_path });
    let result = client
        .request("dead_causes", Some(params))
        .map_err(|e| e.to_string())?;
    let facts: dead_render::DeadCausesFacts =
        serde_json::from_value(result).map_err(|e| format!("unexpected daemon response: {e}"))?;
    // Cross-field invariant serde cannot express (a zero-inference snapshot must carry
    // its empty-state message) → generic labelled fallback with the reason, never a
    // silently-defaulted cause line.
    facts.validate()?;
    Ok(facts)
}
