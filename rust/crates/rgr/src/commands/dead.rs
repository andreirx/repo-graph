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
    // DELIBERATELY DISABLED — 2026-04-27
    //
    // The `dead` command is removed from the CLI surface because:
    //
    // 1. Current signal quality produces 85-95% false positive rates
    //    on real-world codebases (smoke-run validation 2026-04-27).
    //
    // 2. Missing framework detectors (Spring, React, Axum, FastAPI)
    //    cause all runtime-owned symbols to appear dead.
    //
    // 3. Missing entrypoint declarations cause all entry symbols to
    //    appear dead.
    //
    // 4. A misleading "dead" label is worse than no label — it directs
    //    agents toward the wrong investigation frontier.
    //
    // The underlying substrate is preserved:
    // - storage::find_dead_nodes() still works
    // - trust::assess_dead_confidence() still works
    // - tests pinning current behavior remain
    //
    // This surface will be reintroduced as TWO separate commands:
    //
    // 1. `rmap orphans` — structural graph orphans, no deadness claim
    // 2. `rmap dead` — coverage-backed + framework-liveness-backed,
    //    much stronger evidence required
    //
    // See docs/TECH-DEBT.md for the full rationale and reintroduction
    // criteria.
    // ══════════════════════════════════════════════════════════════════

    eprintln!("error: `rmap dead` is disabled");
    eprintln!();
    eprintln!("Dead-code detection is not available in rmap because current");
    eprintln!("signal quality produces high false-positive rates (85-95% on");
    eprintln!("real codebases). Using this output would mislead agents into");
    eprintln!("investigating or deleting live code.");
    eprintln!();
    eprintln!("Root causes:");
    eprintln!("  - Missing framework detectors (Spring, React, Axum, FastAPI)");
    eprintln!("  - Missing entrypoint declarations");
    eprintln!("  - No coverage-backed evidence");
    eprintln!();
    eprintln!("Alternative discovery commands that work:");
    eprintln!("  rmap callers  - trace who calls a symbol");
    eprintln!("  rmap callees  - trace what a symbol calls");
    eprintln!("  rmap imports  - trace file imports");
    eprintln!("  rmap orient   - repo overview with trust signals");
    eprintln!("  rmap trust    - detailed reliability report");
    eprintln!();
    eprintln!("Dead-code surface will be reintroduced when:");
    eprintln!("  - Framework entrypoint detection is mature, OR");
    eprintln!("  - Coverage-backed evidence is available");

    ExitCode::from(2)
}
